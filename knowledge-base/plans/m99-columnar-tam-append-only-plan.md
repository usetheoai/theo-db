---
slug: m99-columnar-tam-append-only
milestone_id: M99
created_at: 2026-07-14
goal: Ship an own-code append-only columnar TableAM (theodb_columnar) whose seqscan+aggregate result-matches a row-store table byte-for-byte, proven by pg_tests + MVCC isolation permutations + crash-safety replay + a scan benchmark vs heap.
---

# M99 — Own-code append-only columnar Table Access Method (`theodb_columnar`)

## Goal

Ship an own-code append-only columnar `TableAmRoutine` (`theodb_columnar`) whose `SELECT`/aggregate over a columnar
table is result-identical to the same data in a row-store heap table — proven by result-equivalence pg_tests, green
`pg_isolation_regress` MVCC permutation specs, a crash-safety WAL-replay test, and a columnar-vs-heap scan benchmark
(`docs/benchmarks/m99-columnar-tam.{md,json}`). Metric: **the full M99 test set (equivalence + isolation + crash) GREEN
on pg17 + a measured compression/skip scan artifact**.

## Context

M98 (v0.85.0) proved pgrx 0.19 + DataFusion 54 + Arrow 58 coexist in one cdylib. M99 builds the **storage substrate**
the single-planner pillar needs: a native in-Postgres columnar TableAM that M100's DataFusion CustomScan can push
scans into. This is the storage half of the seam; M100 is the execution half.

**Decision resolved during grilling/discovery (2026-07-14, council-index-storage + council-research-adr reading the
real Hydra/Citus/cstore_fdw sources):** M99 is **own-code only** (ADR-0042). Hydra columnar is AGPLv3 (not Apache-2.0
as the M98 amendment mislabeled) — studied as *design literature* (Rule 9), never copied/linked. The MVCC-correctness
trick (delegate stripe visibility to a heap catalog row's own MVCC) is reused as a *design idea*, which means we do
NOT re-implement MVCC — the single highest-risk thing a TAM could do.

## Baseline Context

### Files that will be touched

| File | LoC today | Role | Change |
|---|---|---|---|
| `theodb_rs/src/am/mod.rs` | ~290 (customscan/datafusion_probe mods present) | AM registration idiom (`make_amroutine` for IndexAmRoutine) | ADD `mod columnar;` + a `table_am_handler` `#[pg_extern]` |
| `theodb_rs/src/am/columnar.rs` | (NEW) | the `TableAmRoutine` + callbacks | NEW — the milestone core |
| `theodb_rs/src/am/columnar/meta.rs` | (NEW) | stripe/chunk_group/chunk catalog structs + DDL + snapshot-scoped lookups | NEW |
| `theodb_rs/src/am/columnar/writer.rs` | (NEW) | write-state, chunk-group accumulation, zstd per-column compression, flush | NEW |
| `theodb_rs/src/am/columnar/reader.rs` | (NEW) | scan, projection, min/max chunk-group skip-pruning, decompress | NEW |
| `theodb_rs/src/am/page.rs` | ~1900 | GenericXLog/WAL/extend/reinit/read block primitives | REUSE (no change expected) |
| `theodb_rs/src/am/tid.rs` | (small) | TID codec | REUSE + add synthetic row_number↔TID variant |
| `theodb_rs/Cargo.toml` | ~60 | deps | ADD `zstd` (or reuse arrow-rs zstd codec — parsimony rung 4) |
| `test/isolation/` (NEW dir) | (NEW) | `pg_isolation_regress` .spec + expected | NEW — MVCC proof |
| `docs/benchmarks/m99-columnar-tam.{md,json}` | (NEW) | scan benchmark artifact | NEW |

### Current callers / prior art in this repo (reuse, not greenfield)

- `am/mod.rs:75-115` `make_amroutine` — the AM-registration FFI idiom; `TableAmRoutine` is the sibling of the
  already-shipped `IndexAmRoutine`. The `table_am_handler` returns a `PgBox<TableAmRoutine>` with
  `.type = NodeTag::T_TableAmRoutine`.
- `am/page.rs` — `write_blob`/`extend_page_with_item` (`:21,73`), `write_meta_page` full-image (`:459`),
  `reinit_page_with_items` (`:415`), `read_page_item_at`/`read_page_item_into` (`:1790,1828`). These are the 1:1
  analogues of Hydra's `columnar_storage.c` `WriteToBlock`/`ReadFromBlock`/metapage-init — the WAL/crash layer is
  ALREADY DONE.
- `am/build.rs:124,279,315,396,425` — the unwind-boundary discipline: `#[pg_guard] extern "C-unwind"` + `Result<_,
  String>` matched to `pg_sys::error!` on Err. Every one of the ~37 TAM callbacks copies this.
- `am/customscan.rs` (present) — M100's seam; M99 must not depend on it.

### Glossary

- **Stripe** — a batch of rows (default 150k) flushed as one unit; the MVCC/visibility granule.
- **Chunk group** — a stripe sub-batch (default 10k rows); the skip-pruning granule.
- **Chunk** — one column's data within one chunk group; carries min/max + null-bitmap streams.
- **Synthetic TID** — a fake `(block,offset)` computed bijectively from a monotonic `row_number` so indexes/executor
  can address columnar rows (`row_number_to_tid`/`tid_to_row_number`).
- **MVCC-via-catalog** — a stripe is visible to a scan iff its `columnar.stripe` heap-catalog row is visible under the
  scan's snapshot; the TAM writes no custom xmin/xmax on column data.

### Architecture boundaries

Per `rules/architecture.md`: the TAM callbacks are the *interface* layer (called from C executor); `meta.rs` is the
persistence adapter (heap catalog); `writer.rs`/`reader.rs` are the storage engine. No callback panics across C
(`error-handling.md` + Rule 8) — every fallible path returns a typed `Err` → `pg_sys::error!`.

## Prior Art & Related Work

- **Discovery dossiers (2026-07-14):** council-index-storage (TableAmRoutine callback map, on-disk layout, TID scheme,
  MVCC-via-catalog, WAL mapping onto `page.rs`, unwind boundary, 4-phase decomposition) + council-research-adr
  (columnar TAM landscape + licenses, storage-format literature, MVCC pitfalls, isolation tooling, ADR-grade
  decisions). Both read the real `hydra/columnar/**`, `citus/src/backend/columnar/**`, `cstore_fdw/**`.
- **AGPL study-only precedent:** `[[vectorchord-agpl-study-only]]` memory + the own-code `public.vector` (M60/M69).
- **Reference code (design literature only, Rule 9):** `.claude/knowledge-base/references/hydra/columnar/src/backend/
  columnar/{columnar_tableam.c,columnar_metadata.c,columnar_storage.c,columnar_reader.c,columnar_writer.c}`;
  `.claude/knowledge-base/references/citus/src/backend/columnar/sql/columnar--9.5-1--10.0-1.sql`;
  `.claude/knowledge-base/references/citus/src/test/regress/spec/columnar_write_concurrency.spec`.
- **Permissive reuse:** `cstore_fdw` (Apache-2.0), `arrow-rs` zstd/lz4 codecs (Apache-2.0, already vendored per M98).
- **Postgres authoritative:** TableAM required-callback set = `GetTableAmRoutine` asserts
  (`postgres/src/backend/access/table/tableamapi.c:45-98`); isolation tooling = `src/test/isolation/README`.

## ADRs

### D1 — Own-code TAM, AGPL as design literature only (license gate)

**Decision:** build `theodb_columnar` from scratch; study Hydra/Citus (AGPLv3) as design literature; copy no source,
link no AGPL library. Recorded in `docs/adr/0042-m99-own-code-columnar-tam.md`.
**Alternatives:** (A) adopt Hydra/Citus columnar directly — REJECTED, AGPLv3 barred by D1 (ADR-0041's finding). (B)
keep DEFER (0041) — REJECTED, leaves M100-M103 with no native substrate. **Rationale:** algorithms/layouts are not
copyrightable; clean-room Rust reimplementation is Rule-9-compliant (same posture as the vector pillar).

### D2 — Metadata in heap catalog tables (delegate MVCC), not custom pages

**Decision:** `columnar.stripe`/`chunk_group`/`chunk` are ordinary heap tables in a `columnar` schema; stripe
visibility = catalog-row visibility under the scan snapshot (`systable_beginscan_ordered(..., snapshot, ...)`).
**Alternatives:** custom metadata pages with hand-rolled visibility — REJECTED: re-implements MVCC (forbidden by the
parsimony ladder + Rule 9; discards the entire correctness trick). **Rationale:** delegates snapshot isolation, WAL,
crash recovery, pg_dump/replication to Postgres for free (council dossiers, `hydra/columnar/README.md:26-37`).

### D3 — Bespoke PG-fork stripe framing + arrow-rs codecs (not Parquet-file-per-stripe)

**Decision:** stripe/chunk bytes live in the relation's storage fork via `page.rs` GenericXLog (crash-safe,
PG-native); per-column compression reuses arrow-rs/zstd codecs (parsimony rung 4).
**Alternatives:** Parquet-file-per-stripe via arrow-rs — DEFERRED (not rejected): trades PG-native WAL/crash
integration for format reuse (the pg_duckdb/pg_mooncake choice); recorded for a possible later milestone.
**Rationale:** crash-safety must be Postgres-native (WAL replay); compression is a solved codec problem to reuse.

### D4 — Append-only surface; UPDATE/DELETE/parallel/bitmap/sample are typed-ERROR stubs

**Decision:** implement real behavior only for INSERT/multi_insert/seqscan/aggregate/ANALYZE/relation-lifecycle;
every other required callback is a non-NULL `pg_sys::error!("... not supported (M99 is append-only)")` stub.
**Alternatives:** implement Hydra `row_mask` soft-delete DML — REJECTED for M99: its only reference impl is AGPL
(clean-room needed) + advisory-lock serialization is a concurrency minefield (a whole later bet). **Rationale:**
smallest surface that ships a *correct* TAM; every ERROR path is honest (Citus base surface). "Updatable columnar
HTAP" would be over-claiming (M73/M97 discipline).

## Dependency Graph

```
Phase A (register + metapage + lifecycle)  ── gates ──▶ Phase B (write path)
                                                          │
Phase B (write path, single-xact append)  ── gates ──▶  Phase C (read + MVCC + pruning)
                                                          │
Phase C ── gates ──▶ Phase D (concurrency/isolation proofs + crash-safety + benchmark)
```

Phases are strictly sequential (each de-risks the next). Within a phase, tasks are ordered.

## Phase A — Register the TAM + metapage + relation lifecycle

### Task A1 — `theodb_columnar` TableAM registers and `CREATE TABLE ... USING theodb_columnar` loads

#### Why this step
A TAM whose ~37 required callbacks are all non-NULL is the precondition for anything else — an assert-build crashes
otherwise (`tableamapi.c:45-98`). We fill the scan/insert/lifecycle slots with real fns (implemented in later tasks)
and every other required slot with a typed-ERROR stub (D4). This is the M26 IndexAM registration idiom applied to a
`TableAmRoutine` (`am/mod.rs:75-115`).

#### Files to edit
- `theodb_rs/src/am/mod.rs` — add `mod columnar;` + re-export the handler.
- `theodb_rs/src/am/columnar.rs` (NEW) — `table_am_handler` `#[pg_extern]` returning `PgBox<TableAmRoutine>`; the
  `CREATE ACCESS METHOD theodb_columnar TYPE TABLE HANDLER ...` DDL via `extension_sql!`.

#### TDD
- RED: `test_columnar_am_creates_table` — `CREATE TABLE t (a int, b text) USING theodb_columnar;` succeeds; `\d t`
  reports the AM. Assert via `Spi::run` + a catalog query on `pg_am`/`pg_class.relam`. Fails before the handler
  exists.
- GREEN: register all required callbacks (real for lifecycle; ERROR-stub for the rest); implement
  `relation_set_new_filelocator` to init the metapage (reuse `page.rs:459` full-image write), `relation_size`,
  `relation_needs_toast_table`=false.
- REFACTOR: extract the callback table into a `make_columnar_amroutine()` mirroring `make_amroutine`.

#### Concurrency tests
(none — single-threaded) — registration + DDL is not concurrent; MVCC concurrency is Phase D.

#### Acceptance criteria
- `CREATE TABLE ... USING theodb_columnar` succeeds; `DROP TABLE` succeeds; metapage initialized (block 0 readable).
- Every required callback is non-NULL (assert-build does not crash on create).

#### DoD
- `cargo pgrx test pg17 columnar_am_creates` GREEN on the droplet.

### Task A2 — the `columnar.stripe`/`chunk_group`/`chunk` catalog + row_number/stripe-id reservation

#### Why this step
The metadata catalog is the MVCC substrate (D2). Reserving `row_number`/`stripe_id` off the metapage under the
extension lock is the append-serialization primitive (`columnar_storage.c:399-438`) — a classic race site, so it is
its own task with its own test.

#### Files to edit
- `theodb_rs/src/am/columnar/meta.rs` (NEW) — Rust structs + `extension_sql!` DDL for the three catalog tables (own
  DDL, not copied); `reserve_row_number(rel)` / `reserve_stripe_id(rel)` bumping metapage counters under
  `LockRelationForExtension`.

#### TDD
- RED: `test_reserve_row_number_monotonic` — 1000 sequential `reserve_row_number` calls return strictly increasing,
  gap-free ids; survives a metapage round-trip (write → read → continue). Fails before reservation exists.
- GREEN: metapage counter read-modify-write under extension lock; catalog DDL install.
- REFACTOR: encapsulate metapage layout in a `Metapage` struct with `reserved_row_number`/`reserved_stripe_id`.

#### Concurrency tests
`#### Concurrency tests` — deferred to Phase D task D1 (two concurrent inserters must get non-overlapping row_number
ranges). Single-thread test here proves the codec; the race proof is the isolation permutation.

#### Acceptance criteria
- Catalog tables created under the `columnar` schema; reservation monotonic + durable across metapage reload.

#### DoD
- `cargo pgrx test pg17 reserve_row_number` GREEN.

## Phase B — Write path (single-xact append)

### Task B1 — write-state + chunk-group accumulation + per-column zstd + flush to a stripe

#### Why this step
INSERT is the producer side; without it there is nothing to scan. The write-state accumulates rows, splits into
chunk groups (10k), compresses each column chunk (zstd, D3), computes per-chunk min/max, writes the byte streams via
`page.rs` GenericXLog, and inserts the `columnar.stripe`/`chunk_group`/`chunk` catalog rows.

#### Files to edit
- `theodb_rs/src/am/columnar/writer.rs` (NEW) — `WriteState`, `accumulate_row`, `flush_stripe`; wired into the
  `tuple_insert`/`multi_insert` callbacks in `columnar.rs`.
- `theodb_rs/Cargo.toml` — reuse arrow-rs zstd codec if reachable, else add `zstd` (parsimony rung 4 check first).

#### TDD
- RED: `test_insert_then_seqscan_roundtrip` — INSERT 25k rows (2 chunk groups + a partial), seqscan returns all 25k
  rows with identical values + column order + NULLs. Fails before the writer/reader exist (reader is B2/C1; this test
  gates the whole write→read loop and stays RED until C1).
- RED: `test_flush_at_stripe_limit` — INSERT 150k+ rows produces ≥2 stripes in `columnar.stripe`.
- GREEN: minimal write-state → compress → `page.rs` write → catalog insert; flush on stripe limit + on xact commit
  (xact callback).
- REFACTOR: separate compression codec behind a `ColumnCodec` trait (SRP); min/max computation per PG type.

#### Concurrency tests
(none — single-threaded) — single-xact append; cross-xact flush ordering is Phase D.

#### Acceptance criteria
- INSERT of N rows creates ⌈N/150000⌉ stripes; each column chunk carries min/max + null bitmap; commit flushes the
  pending stripe.

#### DoD
- `cargo pgrx test pg17 insert_flush` GREEN (round-trip test may stay RED until C1 — noted).

## Phase C — Read path + MVCC + chunk-group pruning

### Task C1 — `scan_getnextslot` (projection + decompress) + synthetic TID + result-equivalence

#### Why this step
The consumer side. Decompress only projected columns (late materialization), assign synthetic row_number→TID, emit
virtual tuples (`ExecStoreVirtualTuple`). This closes the write→read loop and delivers the headline DoD:
result-equivalence vs a row-store.

#### Files to edit
- `theodb_rs/src/am/columnar/reader.rs` (NEW) — `ColumnarScan`, `getnextslot`; wired into `scan_begin`/
  `scan_getnextslot`/`scan_end` in `columnar.rs`.
- `theodb_rs/src/am/tid.rs` — add `row_number_to_tid`/`tid_to_row_number` (synthetic, bijective).

#### TDD
- RED: `test_columnar_matches_rowstore` — same 100k-row dataset inserted into a `theodb_columnar` table and a heap
  table; `SELECT count(*), sum(a), avg(a), min(b), max(b), array_agg(a ORDER BY a)` are IDENTICAL. This is the
  result-equivalence GATE. Fails before the reader exists.
- RED: `test_synthetic_tid_bijective` — `tid_to_row_number(row_number_to_tid(n)) == n` for n in a boundary set (0, 1,
  MaxOffset-1, MaxOffset, large). Unit test.
- GREEN: decompress projected columns; emit virtual tuples; assign synthetic TIDs.
- REFACTOR: projection pushdown (only touch scanned columns).

#### Concurrency tests
(none — single-threaded) — snapshot visibility is C2; raw scan is single-reader here.

#### Acceptance criteria
- Aggregates + ordered array_agg over columnar == row-store, byte-identical, for ≥100k rows across all supported
  column types (int, bigint, text, float8, bool, nullable).

#### DoD
- `cargo pgrx test pg17 columnar_matches_rowstore` + `synthetic_tid` GREEN.

### Task C2 — MVCC visibility (catalog-snapshot delegation) + chunk-group min/max skip-pruning + ANALYZE

#### Why this step
Visibility delegated to the catalog row's snapshot (D2) is what makes concurrent readers correct; min/max pruning is
the columnar performance lever; ANALYZE gives the planner stats. `tuple_satisfies_snapshot` =
`FindStripeByRowNumber(rel, rowNumber, snapshot) != NULL`.

#### Files to edit
- `theodb_rs/src/am/columnar/meta.rs` — `find_stripe_by_row_number(rel, row_number, snapshot)` via
  `systable_beginscan_ordered` on the `(storage_id, first_row_number)` index with the scan snapshot.
- `theodb_rs/src/am/columnar/reader.rs` — chunk-group skip using per-chunk min/max vs scan quals;
  `scan_analyze_next_block`/`scan_analyze_next_tuple`.

#### TDD
- RED: `test_uncommitted_stripe_invisible` — an in-progress (unflushed/uncommitted) stripe is not returned to a
  concurrent reader's snapshot (single-process simulation via snapshot manipulation; the true cross-xact proof is
  Phase D). Fails before snapshot-scoped lookup exists.
- RED: `test_chunk_group_pruned_on_range` — a `WHERE a > K` that excludes whole chunk-groups reads fewer groups
  (assert via a decompression counter/log) while returning the correct rows.
- GREEN: snapshot-scoped catalog scan for visibility; min/max prune; ANALYZE sampling.
- REFACTOR: a `PruneDecision` helper per PG type comparator.

#### Concurrency tests
`#### Concurrency tests` — the real cross-xact visibility proof is Phase D (`pg_isolation_regress`). This task's tests
are single-process snapshot simulations; they do NOT prove race-freedom on their own (noted honestly).

#### Failure scenarios
`## Failure scenarios` — corrupt metapage magic / out-of-range row_number → typed `pg_sys::error!` (the
`ErrorIfInvalidRowNumber` analogue), never a panic across C; test with a hand-corrupted metapage fixture.

#### Acceptance criteria
- Uncommitted stripe invisible to another snapshot; range predicate skips ≥1 chunk group with correct results;
  ANALYZE populates `pg_statistic`.

#### DoD
- `cargo pgrx test pg17 columnar_mvcc` + `chunk_group_prune` GREEN.

## Phase D — Concurrency proofs + crash-safety + benchmark (the correctness GATE)

### Task D1 — `pg_isolation_regress` MVCC permutation specs (the non-optional proof)

#### Why this step
Per ADR-0042 + both dossiers: "without isolation permutations green, 'MVCC-correct columnar' is over-claiming." MVCC
bugs only appear under concurrency. This wires the `isolationtester` harness (blueprint Corner 3 tooling gap) and
authors own permutation specs (studying Citus `columnar_write_concurrency.spec` as ideas, re-authored — the specs are
not AGPL code).

#### Files to edit
- `test/isolation/` (NEW) — `specs/columnar_write_concurrency.spec`, `columnar_reader_vs_writer.spec`,
  `columnar_abort_vs_reader.spec` + `expected/` outputs; a `Makefile`/script `check-isolation` target (standalone,
  Citus-style — MEMORY `[[m46-measurement-learnings]]`: CI does not run `cargo pgrx test`, so a standalone harness).

#### TDD
- RED: the three specs fail (or the harness is unwired) before the visibility logic + harness exist.
- GREEN: wire `pg_isolation_regress`; specs assert: (a) two concurrent inserters produce non-overlapping row_number
  ranges + both sets visible after commit; (b) a REPEATABLE READ reader does NOT see a stripe committed after its
  snapshot; (c) an aborted mid-flush stripe is invisible to a concurrent reader.
- REFACTOR: shared setup teardown across specs.

#### Concurrency tests
`#### Concurrency tests` — this task IS the concurrency proof: 3 permutation specs covering inserter/inserter,
reader/writer under RR, and abort/reader. Race-aware by construction (isolationtester serializes declared
permutations).

#### Acceptance criteria
- `make check-isolation` (or the wired target) GREEN for all three specs on pg17.

#### DoD
- Isolation specs GREEN, committed with `expected/` files.

### Task D2 — crash-safety WAL-replay + columnar-vs-heap scan benchmark

#### Why this step
Crash-safety is a DoD hard item (insert stripes → restart → scan-identical; abort → restart → no partial stripe). The
benchmark is the honest measured artifact (compression + skip gain, ~2-5×, NO vectorized execution yet — that's M100).

#### Files to edit
- `theodb_rs/src/am/columnar/` — crash-safety test (simulated restart via `page.rs` replay path, the M26 pattern).
- `docs/benchmarks/m99-columnar-tam.{md,json}` (NEW) — the scan benchmark harness + measured artifact.

#### TDD
- RED: `test_crash_replay_scan_identical` — insert stripes, simulate restart (WAL replay), scan returns identical
  rows; abort-mid-flush + restart leaves zero partial stripes. Fails before WAL wiring is complete.
- GREEN: rely on `page.rs` GenericXLog full-image WAL + the catalog-row commit ordering; benchmark harness runs.
- REFACTOR: benchmark reproducibility (fixed seed, ≥3 runs, mean±stddev per `council-benchmark` rules).

#### Failure scenarios
`## Failure scenarios` — crash between data flush and catalog commit MUST leave an in-progress/aborted (invisible)
stripe, never a visible stripe pointing at garbage bytes; tested by aborting after data write, before catalog commit.

#### Acceptance criteria
- Crash-replay scan-identical; abort leaves no visible partial stripe; benchmark artifact committed with methodology,
  hardware, ≥3 runs, honest ceiling note (compression/skip only, not vectorized — NOT a superiority claim).

#### DoD
- `cargo pgrx test pg17 crash_replay` GREEN; `docs/benchmarks/m99-columnar-tam.{md,json}` present with measured
  numbers.

## Coverage Matrix

| Requirement (ROADMAP M99 DoD) | Task(s) |
|---|---|
| (1) `theodb_columnar` TAM registrable (`CREATE ACCESS METHOD ... TYPE TABLE`) | A1 |
| catalog + row_number/stripe reservation | A2 |
| write path (stripe/chunk/compression/min-max) | B1 |
| (2) result-equivalence pg_tests vs row-store | C1 |
| synthetic TID | C1 |
| MVCC visibility (catalog-snapshot) + ANALYZE | C2 |
| chunk-group min/max skip-pruning | C2 |
| (3) pg_isolation permutation specs (MVCC) green + isolationtester wire | D1 |
| (4) crash-safety WAL-replay | D2 |
| (5) benchmark columnar vs heap | D2 |
| (6) council-index-storage + council-rust-pgrx sign-off | Review phase (post-implement) |
| honest boundary (append-only, no update/parallel/bitmap/sample) | D4 (ADR) enforced across A1/B1/C1 stubs |

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| MVCC bugs only surface under concurrency; single-thread TDD cannot catch them | HIGH | Phase D `pg_isolation_regress` permutations are a non-optional DoD; "MVCC-correct" is not claimed until green (ADR-0042) | impl |
| row_number/stripe-id reservation race (release extension lock too early) | HIGH | A2 holds `LockRelationForExtension` across the metapage read-modify-write; D1 inserter/inserter permutation proves non-overlap | impl |
| Crash between data flush and catalog commit leaves a visible stripe over garbage bytes | HIGH | D3 ordering: data WAL'd durable before the visibility-granting catalog commit; D2 abort-mid-flush test | impl |
| Rust panic crossing the C boundary in any of ~37 callbacks | MEDIUM | `#[pg_guard] extern "C-unwind"` + `Result→pg_sys::error!` on every callback (the `build.rs` discipline); stubs use `pg_sys::error!`, never `panic!`/`unimplemented!` | impl |
| Scope creep into UPDATE/DELETE (AGPL `row_mask` design) | MEDIUM | D4 ADR: append-only only; DELETE/UPDATE are typed-ERROR stubs; clean-room design deferred to a later milestone | plan |
| Over-claiming performance (compression/skip mistaken for vectorized speed) | MEDIUM | benchmark labels the ceiling: compression+skip only, NOT vectorized (M100); no "faster than X" without `docs/benchmarks/` + `public-copy.md` | impl |
| CI does not run `cargo pgrx test`; isolation harness may not run in CI | LOW | standalone `make check-isolation` (Citus-style), run on the droplet; documented (MEMORY `[[m46-measurement-learnings]]`) | impl |

## Unresolved Questions

- **R0 web-grounding:** the two discovery councils grounded on the real on-disk sources (stronger for correctness) but
  flagged that live WebSearch/WebFetch was unavailable to them; the pillar-level blueprint (`single-planner-columnar-ai`,
  SHIPPABLE 98.8) already did web-grounded SOTA. Accepted: on-disk primary sources + prior web-grounded blueprint are
  sufficient evidence for M99; no new web claim is made that isn't in those artifacts.
- **arrow-rs zstd reuse vs `zstd` crate:** resolved at B1 by a parsimony-rung-4 check (reuse arrow-rs codec if reachable
  without a new dep; else add `zstd`). Not a blocker.
- **PG18 TableAM callback drift:** the repo pins pgrx 0.19 / pg17; confirmed against `tableamapi.c` REL_17. If pg18 is
  targeted later, re-check callback signatures (out of M99 scope).

## Failure scenarios

- **Corrupt metapage / out-of-range row_number** (C2) — typed `pg_sys::error!`, never a panic across C; hand-corrupted
  metapage fixture test.
- **Crash between data flush and catalog commit** (D2) — must leave an invisible in-progress/aborted stripe; abort
  test.
- **Concurrent inserters** (D1) — non-overlapping row_number ranges under the extension lock; isolation permutation.

## Global DoD

- All Phase A–D tasks' `cargo pgrx test pg17` GREEN on the droplet (result-equivalence + MVCC + crash).
- `make check-isolation` (or wired target) GREEN for the 3 MVCC permutation specs.
- `docs/benchmarks/m99-columnar-tam.{md,json}` present with measured numbers, methodology, ≥3 runs, honest ceiling.
- No callback panics across C (every callback `#[pg_guard] extern "C-unwind"` + `pg_sys::error!`).
- CHANGELOG `[Unreleased]` updated; no commits to main; no Co-Authored-By trailer.
- Files respect the ~500 LoC budget per file (split `columnar.rs` into `meta`/`writer`/`reader` submodules).
- Sign-off: council-index-storage + council-rust-pgrx (review phase).

## Final Phase — Integration Validation

- Full `cargo pgrx test pg17` suite GREEN (no regression on the 279 M98 tests + the new M99 tests).
- `make check-isolation` GREEN.
- Benchmark artifact reproducible from a documented command.
- council-index-storage + council-rust-pgrx review = READY_TO_MERGE before `/release`.
