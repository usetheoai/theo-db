---
slug: columnar-minmax-zonemap
created_at: 2026-07-19
goal: Admit min(col)/max(col) in the columnar CustomScan byte-identical to PostgreSQL, with a zone-map directory fast-path for the scalar-no-filter case.
---

# Plan: columnar `min(col)`/`max(col)` aggregate + zone-map directory fast-path

## Goal

Make the M100 columnar `CustomScan` admit `min(col)`/`max(col)` on native ordered types (int2/4/8, float4/8, bool,
timestamp/date) **byte-identical** to the native plan, and add a **directory-only fast-path** that answers scalar
`min/max` with no WHERE by folding the zone-map `min_bits`/`max_bits` already written per (chunk_group, col) — never
decoding a column chunk. Proven by `benchmarks/columnar_minmax_ab.py` at 1M rows.

## Context

The CustomScan today does `count/sum/avg` (`columnar_agg.rs:315-352`); `min`/`max` fall to the native plan. The zone-map
directory (`ChunkDirEntry.min_bits/max_bits/has_minmax`, `columnar_codec.rs:103-158`) is written per (chunk_group, col)
and already READ by the WHERE skip-pruning consumer (`chunk_can_match`, `columnar.rs:729`) — but never used to answer an
aggregate. The blueprint (`knowledge-base/discoveries/blueprints/columnar-minmax-zonemap-blueprint.md`), verified by
`council-index-storage`, established that a directory fold is MVCC-byte-identical under the append-only + stripe-atomic
visibility model, gated by 7 conditions (the load-bearing one: `max(float)` must fall back because `compute_minmax`
skips NaN while PG `max` returns NaN).

## Baseline Context (deep review of current state)

Git sha at plan time: `30592bc`.

### Files that will be touched

| File | LoC today | Role today | Change |
|---|---|---|---|
| `theodb_rs/src/am/df_executor.rs` | 637 | AggSpec + DataFusion agg exec + `arrow_value_to_datum` (`:518`) | Add `AggSpec::MinCol/MaxCol(name,typoid)`; DataFusion `min()/max()`; emit via `arrow_value_to_datum`; the Phase-B directory fold helper |
| `theodb_rs/src/am/columnar_agg.rs` | 1067 | admit + custom_private encode/decode (`:315-352`, `:732`) | Admit `min`/`max` bare-Var on ordered kinds → new kinds; encode/decode; wire the Phase-B gate + fold call |
| `theodb_rs/src/am/columnar.rs` | 1637 | `minmax_kind_of` (`:480`), `read_visible_stripes` (`:117`), pending `WRITE_STATES` (`:48`), directory read (`:1073-1085`) | Expose a `directory_minmax(rel, col, want_max)` fold over visible stripes' `ChunkDirEntry` + a pending-rows min/max scan |

### Current callers / dependents

- `minmax_kind_of(typid) -> MinMaxKind` (`columnar.rs:480`) — the ordered-type gate (I2/I4/I8/F4/F8/Bool/temporal), reused by the WHERE zone-map.
- `arrow_value_to_datum(arr,row,typoid)` (`df_executor.rs:518`) — build_arrow reverse; already maps every ordered Arrow value → native PG datum. Reused for Phase-A emit.
- `read_visible_stripes(rel_oid)` (`columnar.rs:117`) — the visible stripe set; the fast-path folds only these.
- `WRITE_STATES` pending rows (`columnar.rs:48,754`) — same-xact uncommitted; the fast-path scans+folds these.
- `compute_minmax` (`columnar_codec.rs:293`) — NaN-skip + F4-widen + i64→u64 store (the domain the fold must mirror).

### Domain glossary
- **chunk group** — the flush unit a `ChunkDirEntry` describes (per column).
- **directory fold** — computing min/max from the per-chunk-group `min_bits/max_bits` without decoding chunks.
- **Phase A / Phase B** — always-correct DataFusion scan / directory-only fast-path.

### Architecture boundaries affected
Admission + planner glue in `columnar_agg.rs`; execution in `df_executor.rs`; storage/visibility/domain in `columnar.rs`
+ `columnar_codec.rs`. The fast-path helper lives in `columnar.rs` (storage layer owns visibility + the directory).

## Prior Art & Related Work
- Internal: the numeric-output slice (`AggSpec` extension pattern), the zone-map skip-pruning consumer
  (`chunk_can_match` — same `min_bits/max_bits` domain), M114 GROUP BY (`arrow_value_to_datum` reverse).
- Blueprint `columnar-minmax-zonemap-blueprint.md` (this cycle) — verified by `council-index-storage`.
- External: PG 17 float8 btree NaN-greatest ordering; `min/max` over empty/all-NULL = NULL (SQL standard).

## Objective
Two aggregates (`min`, `max`) pushed down byte-identically on ordered native types, with a directory fast-path for the
scalar-no-WHERE case, honest per-shape path reporting in the A/B, and no regression to `count/sum/avg`.

## ADRs
- **ADR-MM1 — ship both phases.** Phase A (DataFusion scan) is the correctness floor for all shapes/types incl.
  `max(float)`-with-NaN and GROUP BY/WHERE; Phase B (directory fold) is the scalar-no-filter speedup. Rejected: Phase B
  only (leaves GROUP BY/WHERE/float-max on the native plan).
- **ADR-MM2 — conservative kind-gate for `max(float)`** (fall back to Phase A) rather than a `has_nan` page-format bit.
  Rejected: `has_nan` bit — a STRIPE_VERSION bump + rewrite story for a case Phase A already handles. KISS, no format risk.
- **ADR-MM3 — reuse the two proven decoders** (`arrow_value_to_datum` for Phase A; the `columnar_agg` const-domain
  decode for Phase B) instead of a bespoke emitter. Rejected: hand-rolled per-type emitter (DRY/Rule 9 violation).

## Drawbacks & Risks
| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Fast-path returns a non-visible value under some MVCC path | HIGH | Fold only `read_visible_stripes` + pending scan; verified by council-index-storage (append-only + stripe-atomic). A/B includes same-xact pending + empty/all-NULL cases. | impl |
| `max(float)` NaN mismatch (directory skips NaN) | HIGH | Gate: `max` on float kind → Phase A. A/B includes a NaN-row float column asserting `max`=NaN via the scan path. | impl |
| Domain decode bug (u64-vs-i64, F4 re-narrow, wrong output type) | MEDIUM | Reuse `columnar_agg.rs:133-139` domain logic; A/B compares as native type incl. negatives + float4 + temporal. | impl |
| Silent hidden fallback masking a Phase-B bug | MEDIUM | A/B reports which path fired per shape (fast-path vs scan); pg_test asserts fast-path fires for a clean int column. | impl |

## Unresolved Questions
(none — the 7 gating conditions and the two-phase split are fully resolved by the verified blueprint.)

## Failure scenarios
- **Empty / all-NULL visible + empty pending** → emit SQL NULL (both phases). A/B asserts.
- **Same-xact uncommitted rows** (pending, no directory entry) → fast-path scans+folds them; A/B does `INSERT; SELECT max()` in one xact.
- **Float column containing NaN** → `max` falls back to Phase A and returns NaN; `min` returns the smallest non-NaN. A/B asserts both.
- **Bare-Var gate** → `min(col+1)` / `min(cast(col))` must decline the fast-path (directory is pre-projection); admit still allows Phase A only if the arg is a bare column of an ordered type, else native plan.

## Dependency Graph
Phase 1 (Phase A: admit + DataFusion min/max + emit) → Phase 2 (Phase B: directory fold + gate + pending scan) → Phase 3 (A/B).

## Phase 1: Phase A — admit min/max + DataFusion scan (always correct)

### T1.1 — AggSpec MinCol/MaxCol + DataFusion min/max + emit

#### Why this step
Establishes byte-identical min/max for ALL shapes/types via the proven decoded-scan path (same as sum/avg), so Phase B
is a pure optimization with a correct floor to fall back to (ADR-MM1).

#### Files to edit
- `theodb_rs/src/am/df_executor.rs` — `AggSpec::MinCol(String,u32)` / `MaxCol(String,u32)` (name + output typoid); `push_agg_exprs` → `min(col)`/`max(col)` aliased; `ncols()==1`; `agg_datum` MinCol/MaxCol arm → `arrow_value_to_datum(b.column(col), row, typoid)`.
- `theodb_rs/src/am/columnar_agg.rs` — admit: `name=="min"||"max"` + bare-Var arg + `minmax_kind_of(vartype)!=None` → kinds 6/7 (carry attno + vartype); `begin_custom_scan` rebuild 6=>MinCol,7=>MaxCol (typoid from the arg's vartype via `get_atttype`).

#### TDD
`test_columnar_minmax_phase_a_byte_identical`: columnar + heap tables, ordered types int4/float8/timestamp/bool; assert
`SELECT min(c), max(c) FROM t` byte-identical (as text) and `EXPLAIN` shows `theodb_columnar_agg`; assert `min(col+1)`
declines the CustomScan (bare-Var gate); assert `max(f)` over a NaN-containing float column returns `'NaN'` (Phase A).

#### Acceptance criteria
- `min`/`max` on int2/4/8, float4/8, bool, timestamp/date byte-identical to heap, scalar + GROUP BY + WHERE.
- Output datum is the input column's native type (int4→int4, float4→float4, timestamp→timestamp).
- `min(col+1)` / unordered-type `min(text)` → native plan (declined).

#### DoD
`cargo build --release` clean; A/B Phase-A shapes `identical=YES customscan=YES`.

## Phase 2: Phase B — zone-map directory fast-path

### T2.1 — `directory_minmax` fold + pending scan + 7-condition gate

#### Why this step
Delivers the headline speedup: answer scalar `min/max` with no WHERE from the small directory + pending, never decoding a
chunk — reusing the min/max the codec already writes (Rule 9). Gated for byte-identity per the verified blueprint.

#### Files to edit
- `theodb_rs/src/am/columnar.rs` — `pub(crate) unsafe fn directory_minmax(rel, col_idx, typid, want_max) -> Result<Option<(Datum,bool)>,String>`: fold `min_bits`/`max_bits` over `read_visible_stripes` chunk-groups (compare in `i64`/`f64` domain per `minmax_kind_of`); **return None (⇒ caller uses Phase A)** if any visible group has `has_minmax==false`, or (`want_max` && float kind); then scan+fold `WRITE_STATES` pending rows (deform, extract col, min/max in-domain); empty visible+pending ⇒ `Some((0,true))` (SQL NULL). Emit native type via the `columnar_agg` const-domain decode (i64→narrow, F4 `from_bits as f32`, bool `!=0`, temporal typid).
- `theodb_rs/src/am/columnar_agg.rs` — in `begin_custom_scan`, before the DataFusion path: if mode is scalar (no group), `npred==0`, and the single agg is MinCol/MaxCol, call `directory_minmax`; if it returns `Some`, emit that single-row result directly and skip DataFusion; if `None`, proceed to Phase A.

#### TDD
`test_columnar_minmax_fast_path`: (a) clean int4 column, no WHERE → `directory_minmax` returns Some and result byte-identical to heap; (b) same-xact `INSERT; SELECT max(c)` → pending folded (result includes the uncommitted row); (c) `max` on a NaN-containing float → returns None (fallback); (d) all-NULL column → NULL; (e) `WHERE c>0` → fast-path NOT taken (npred>0), result still byte-identical via Phase A.

#### Concurrency tests
(none — single-threaded per-backend; pending is backend-local `WRITE_STATES`, no shared mutable state across backends. Visibility is delegated to PG's snapshot via `read_visible_stripes`.)

#### Acceptance criteria
- Fast-path fires (verified via a debug counter/log line in the A/B) for clean scalar-no-WHERE min/max on ordered kinds.
- `max(float)` and any `has_minmax==false` group → falls back to Phase A, still byte-identical.
- Pending same-xact rows folded; empty → NULL.

#### DoD
A/B `columnar_minmax_ab.py` prints `path=fastpath` for the clean cases and `path=scan` for the gated-out cases, all `identical=YES`.

## Phase 3: Measurement (in-PG A/B)

### T3.1 — `benchmarks/columnar_minmax_ab.py`

#### Why this step
Rule 5: no performance/correctness claim without a reproducible 1M-row A/B in `docs/benchmarks/`.

#### Files to edit
- `benchmarks/columnar_minmax_ab.py` (NEW) — 1M-row columnar vs heap; for each ordered type: `min`/`max` scalar (report path fastpath/scan + identical + latency), GROUP BY, WHERE; plus all-NULL→NULL, empty→NULL, same-xact pending, float-NaN `max`. Emit `MINMAX_VERDICT all_identical=YES`.
- `docs/benchmarks/columnar-minmax-zonemap-verdict.md` + `.json` (NEW) — measured results.

#### TDD
The script IS the integration test; asserts byte-identity + path per shape. (pg_test mirrors the unit-level shapes.)

#### Acceptance criteria
`MINMAX_VERDICT all_identical=YES`; fast-path measured faster than Phase-A scan for the clean scalar case; every shape byte-identical.

#### DoD
Verdict doc committed with the measured table + reproduce command.

## Coverage Matrix
| Goal claim / requirement | Task(s) |
|---|---|
| Admit min/max byte-identical on ordered native types | T1.1 |
| Output = input column native type | T1.1 |
| Bare-Var gate (decline min(col+1)/unordered) | T1.1 |
| GROUP BY + WHERE min/max | T1.1 (Phase A) |
| Directory fast-path (scalar, npred==0) | T2.1 |
| 7 gating conditions incl. max(float) fallback + has_minmax=false fallback | T2.1 |
| Pending same-xact fold | T2.1 |
| Empty/all-NULL → SQL NULL | T1.1 + T2.1 |
| 1M-row byte-identical A/B + path reporting | T3.1 |
| No regression to count/sum/avg | T1.1 (admit only adds min/max arms) + T3.1 |

## Global Definition of Done
- `cargo build --release` clean on the droplet; extension installs; `CREATE EXTENSION` OK.
- A/B `MINMAX_VERDICT all_identical=YES`; fast-path fires for clean scalar min/max; fallback shapes byte-identical.
- `docs/benchmarks/columnar-minmax-zonemap-verdict.{md,json}` committed; CHANGELOG `[Unreleased] § Added`.
- Independent `council-index-storage` / `council-rust-pgrx` review with no BLOCKER/HIGH/MEDIUM.
- File-size budget: each touched file stays within its current order of magnitude (no new file > 500 LoC).

## Final Phase: Integration Validation
Run the full A/B on a real 1M-row PG; confirm every shape byte-identical + the fast-path path-reporting; confirm
`count/sum/avg` still byte-identical (no regression). Independent review clean. Then release.
