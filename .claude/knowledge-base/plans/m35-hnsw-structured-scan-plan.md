---
slug: m35-hnsw-structured-scan
milestone_id: M35
created_at: 2026-07-02
goal: Replace theodb_hnsw's O(N) blob scan with a page-native structured graph (meta + element/neighbor tuples) traversed on demand, so the scan reads O(ef·M) pages not O(N), measured by a re-run of the M32 SIFT1M harness showing theodb_hnsw QPS ≥ 50 at 1M with recall preserved and pages-read flat in N.
---

# M35 — theodb_hnsw page-native structured scan (partial-read, à la M31 for the graph)

## Goal

Replace `theodb_hnsw`'s single-blob O(N) scan (deserializes the whole ~6.5 GB graph per query at 1M —
`ann/hnsw.rs:243`, `am/scan.rs:168`) with a **page-native structured** persistence (meta + element tuples +
neighbor tuples) and an **on-demand traversal** that reads only visited nodes → **O(ef·M) pages, flat in N** —
measured by a re-run of the M32 SIFT1M harness showing `theodb_hnsw` **QPS ≥ 50 at 1M with recall preserved** and
**pages-read flat in N** (`docs/benchmarks/m35-hnsw-structured-scan.{md,json}`).

## Context

M32 measured `theodb_hnsw` impractical at 1M (1.6 QPS — the O(N) blob scan). M35 is the graph analogue of what
M31 did for `theodb_ivfflat` (structured pages + partial read). The discovery blueprint
(`.claude/knowledge-base/discoveries/blueprints/m35-hnsw-structured-scan-blueprint.md`) established the pattern
(mirror pgvector `HnswElementTupleData`/`HnswNeighborTupleData`/`HnswMetaPageData`) and — the load-bearing
KISS scope-cut — that theodb's INSERT-via-pending + VACUUM-full-rebuild make the graph **immutable between
rebuilds**, so M35 needs only a page codec + an on-demand read path + one GUC (NOT pgvector's on-disk incremental
insert / tombstone / version-stale machinery).

## Baseline Context

### Files that will be touched

| File | LoC | git sha (last) | Why |
|---|---|---|---|
| `theodb_rs/src/am/hnsw_page.rs` | (NEW) | — | the structured HNSW codec: pack graph → element/neighbor pages + meta; read meta + read one tuple by (blkno,offno). New module (page.rs is already 649 LoC — SRP + file-size budget) |
| `theodb_rs/src/am/scan.rs` | 218 | 61e64db | add `scan_hnsw_structured` (on-demand traversal) + dispatch arm on `HNSW_STRUCT_MAGIC`; wire the pages-read profiler |
| `theodb_rs/src/am/build.rs` | 248 | (M34) | `ambuild_hnsw` + VACUUM HNSW rebuild write the structured layout instead of `to_bytes`→`write_blob` |
| `theodb_rs/src/am/page.rs` | 649 | (M34) | add `read_page_item_at(rel,blkno,offno)`; add an `HNSW_STRUCT` arm to `main_index_pages` (pending-region location) + `peek_magic` recognition |
| `theodb_rs/src/am/guc.rs` | 36 | (M34) | add `theodb_hnsw.ef_search` GUC (clone the `probes` pattern) |
| `theodb_rs/src/am/mod.rs` | 212 | (M34) | register the new module + init the ef_search GUC in `_PG_init` |
| `theodb_rs/src/ann/hnsw.rs` | 346 | (M26) | expose the graph internals the codec needs (levels, neighbors, entry, ids, vectors) via `pub(crate)` accessors — no algorithm change |
| `benchmarks/run_m35_hnsw.py` | (NEW) | — | operator driver: re-run the M32 harness for theodb_hnsw at 1M + ef_search sweep + pages-read flat-in-N proof → the artifact |
| `docs/benchmarks/m35-hnsw-structured-scan.{md,json}` | (NEW) | — | the DoD benchmark artifact |

### Current callers / dependents

- `am/scan.rs:61-72` — `amrescan` dispatches `IVF_STRUCT_MAGIC` → `scan_ivf_structured`, else `scan_blob` (the O(N) HNSW path at `:168`). M35 adds an `HNSW_STRUCT_MAGIC` arm → `scan_hnsw_structured`.
- `am/index.rs:9,55` — `SCAN_EF=64` const feeds `HnswIndex::search_merged`; M35's structured scan reads `ef_search` from the new GUC (clamped) instead.
- `am/build.rs:76` `ambuild_hnsw` + `:143` VACUUM rebuild — today `HnswIndex::to_bytes`→`page::write_blob`; M35 routes HNSW to the structured writer.
- `am/page.rs:327` `main_index_pages` — locates the pending region; today knows blob + `IVF_STRUCT`; M35 adds `HNSW_STRUCT`.
- `ann/hnsw.rs:196` `HnswIndex::search` — the in-memory reference the structured traversal must match bit-for-bit (recall preserved). Reused UNCHANGED as the round-trip oracle in tests.

### Domain glossary

- **element tuple** — one per node: `[tag u8, level u8, tid i64, nbr_blkno u32, nbr_offno u16, dim u16, f32×dim]`. The node's own address is its `(blkno,offno)`.
- **neighbor tuple** — one per node, fixed max degree: `[tag u8, count u16, (blkno u32,offno u16)×((level+2)·m)]`; per-layer slice `start=(level-lc)·m`, len `lm=(lc==0?m0:m)` (pgvector `hnswutils.c:784`).
- **meta page** (block 0) — `[magic, version, metric, dim, m, m0, ef_construction, entry_blkno, entry_offno, entry_level i16, node_count, elem_first, elem_npages, nbr_first, nbr_npages]`; `entry_level=-1` = empty.
- **on-demand traversal** — mirror `HnswSearchLayer`: expand a candidate → read its neighbor tuple (1 page); each unvisited neighbor → read its element tuple (1 page) + SIMD-score off the bytes; visited-set keyed by `(blkno,offno)` loads each node ≤ once.
- **pages-read** — the wiring-triad runtime metric: count of `ReadBuffer`s per scan; must be O(ef·M), flat in N.

### Architecture boundaries affected

All within the `theodb_rs` extension AM layer (`am/`), reusing `ann/hnsw.rs` (algorithm) + `vec.rs` (SIMD) +
`page.rs` (WAL/buffer primitives). No new external boundary. DIP: the codec depends on `HnswIndex` accessors
(the domain graph), not vice-versa. Per `rules/architecture.md` (composition at the AM handler; no god module —
the new codec is its own file).

## Prior Art & Related Work

- Blueprint (this cycle): `.claude/knowledge-base/discoveries/blueprints/m35-hnsw-structured-scan-blueprint.md`.
- In-repo mirror: M31 structured IVFFlat pages (`am/page.rs:327` meta, `am/scan.rs:78` partial-read scan, the
  M31b SIMD `vec.rs:167` from-bytes scorer, the `THEODB_SCAN_PROFILE` counter `scan.rs:105-164`).
- Reference implementation: pgvector `src/hnsw{.h,scan.c,utils.c,build.c}` under `.claude/knowledge-base/references/pgvector/`.

## ADRs

### ADR-1 — immutable-between-rebuilds graph: keep pending + VACUUM-rebuild (do NOT port pgvector insert/repair)
**Decision:** M35 = page codec + on-demand read path + ef_search GUC only. Keep INSERT-via-pending
(`page.rs:113-208`) and VACUUM-full-rebuild (`build.rs:143-216`).
**Rationale:** the built graph is immutable between rebuilds → intra-graph refs always valid → pgvector's on-disk
incremental insert (`hnswinsert.c`), tombstones, and version-stale-detection (`hnswutils.c:777`) are
UNNECESSARY. Halves the milestone; removes the concurrency-hazard surface.
**Alternatives rejected:** (a) port pgvector incremental insert (huge, concurrency-hard; pending already gives
correct INSERT semantics — YAGNI); (b) mutable on-disk graph (needs version/repair — not required by the DoD).

### ADR-2 — analytic element addresses + deterministic in-memory packer, single WAL flush (no placeholder/overwrite)
**Decision:** element tuple size is FIXED (dim fixed) → each node's element `(blkno,offno)` is computed
analytically (`elem_first=1`, items-per-page constant). Neighbor tuples (variable size by level) are packed by a
pure deterministic in-memory packer producing `(addrs, page_images)`. Element tuples are then built with
`neighbortid = nbr_addr` and both ranges are flushed via the WAL scaffold in one pass — **no placeholder tuple,
no `PageIndexTupleOverwrite`**.
**Rationale:** the whole graph is already in memory (`HnswIndex`), so all addresses are computable before any
I/O; a pure packer is unit-testable without a DB and avoids the FFI overwrite surface (blueprint R3).
**Alternatives rejected:** pgvector's placeholder + `PageIndexTupleOverwrite` two-pass (needs a new FFI port +
two packers to agree; unnecessary when the graph is in memory).

### ADR-3 — new format magic + REINDEX gate for the old blob (mirror the M31 IVF v1→v2 gate)
**Decision:** `HNSW_STRUCT_MAGIC` ("THSS"); the old blob `HNSW_MAGIC` ("THNS") is recognized on read and rejected
with a clear REINDEX error (mirror `page.rs:340-344`).
**Rationale:** pre-1.0 engine — a clean format break beats a dual-read path.
**Alternatives rejected:** silent dual-format read (complexity for a pre-1.0 index nobody has in production).

### ADR-4 — fixed M per layer / fixed-size neighbor tuple; cap max level to fit one page
**Decision:** fixed max degree (m upper, m0 ground), single neighbor tuple per node, assert it fits one page;
cap the build max level so it always does.
**Rationale:** pgvector confirms fixed-M is standard (`HnswGetLayerM`, `hnsw.h:112`,`:118`); makes the slice math
trivial and packing deterministic. **Alternatives rejected:** variable-degree lists (no benefit; complicates the
codec).

## Dependencies

### Existing — use as-is

| Package | Version | Ecosystem | Why |
|---|---|---|---|
| `pgrx` | `=0.16.1` | Rust | `pg_sys` FFI (buffers/WAL/pages), GUC, `#[pg_extern]` — already the extension's only PG binding |
| (std) | — | Rust | packing, byte codec |

### New — to be introduced

| Package | Version | Ecosystem | Rule 9 rationale | Why this one |
|---|---|---|---|---|
| (none) | | | The codec is `pg_sys` FFI + std; `read_page_item_at` reuses existing page macros; no crate solves "PG index-page graph codec" | mirrors M31 which added zero deps |

### Removed

| Package | Last version | Why removed |
|---|---|---|
| (none) | | |

## Dependency graph

```
Phase 1 (structured codec: pack + write + read-one + meta; round-trip == in-memory search)
   ──▶ Phase 2 (wire build+vacuum+scan dispatch + ef_search GUC + pages-read profiler; on-demand traversal)
        ──▶ Phase 3 (1M benchmark: QPS≥50 recall-preserved + pages-read flat-in-N + ef_search sweep)
```

## Phase 1 — the structured codec (build + read primitives)

### T1.1 — pack the in-memory graph into element/neighbor page images (pure, unit-tested)

#### Why this step
The correctness core: turn `HnswIndex` (node indices, levels, neighbors) into element + neighbor tuples with
resolved `(blkno,offno)` pointers, deterministically and WITHOUT I/O, so it is unit-testable in isolation before
touching Postgres pages (blueprint R1/R3). A pure packer is the cheapest place to catch the address math.

#### Files to edit
- `theodb_rs/src/am/hnsw_page.rs` (NEW), `theodb_rs/src/ann/hnsw.rs` (add `pub(crate)` accessors: `levels()`, `neighbors()`, `entry()`, `ids()`, `vectors()`, `params()`)

#### TDD
- RED: `test_pack_addresses_are_consistent` — build a small `HnswIndex`; pack it; assert (a) element addrs are the
  analytic `(1 + i/ipp, 1 + i%ipp)`; (b) every neighbor tuple's pointers resolve to a real element addr in range;
  (c) the per-layer slice for a multi-level node returns exactly `neighbors[node][lc]` remapped to addrs, for
  every `lc ∈ 0..=level`. Given a 5-node graph with a level-2 node, when packed and its neighbor tuple sliced per
  layer, then each layer's decoded pointers equal the in-memory `neighbors[node][lc]`.
- GREEN: `hnsw_page::pack(idx) -> PackedGraph { meta, elem_images: Vec<PageImage>, nbr_images: Vec<PageImage>, node_addrs: Vec<(u32,u16)> }` — analytic elem addrs; deterministic neighbor packer; element tuples carry `neighbortid`.
- REFACTOR: share the tuple byte layout (offsets) as `const`s + `encode_element`/`encode_neighbor`/`decode_*` helpers; no magic offsets inline.

#### Concurrency tests
(none — single-threaded) — pure function, no shared state

#### Failure scenarios
- A node whose neighbor tuple would exceed one page (level too high) → the packer returns a typed `Err` (asserted), and the build caps max level so it cannot happen in practice (ADR-4).

#### Acceptance criteria
- `cargo test -p theodb_rs hnsw_page::` green (pack determinism + slice correctness + oversize Err).
- No `unsafe` in the packer (pure byte math); `cargo clippy` clean.

#### DoD
- Packer is I/O-free (grep: no `pg_sys` in the pack path); tuple layout constants single-sourced.

### T1.2 — write the packed images to WAL-logged pages + read meta + read-one-tuple; full round-trip

#### Why this step
Persist the packed images crash-safely (reuse the M31 WAL scaffold) and provide the on-demand read primitive
(`read_page_item_at`) + `read_hnsw_meta`, then prove a structured graph traversed on-demand reproduces
`HnswIndex::search` bit-for-bit (recall preserved) — the milestone's central correctness gate, BEFORE any
benchmark (blueprint recommendation #1).

#### Files to edit
- `theodb_rs/src/am/hnsw_page.rs`, `theodb_rs/src/am/page.rs` (add `read_page_item_at`), `theodb_rs/src/am/scan.rs` (a `traverse_structured(rel,q,ef)` helper used by both the test and the scan)

#### TDD
- RED: `hnsw_structured_roundtrip_reproduces_search` (`#[pg_test]`) — build a corpus, write structured pages to a
  temp index relation, traverse on-demand with ef, and assert the `(id,dist)` results EQUAL
  `HnswIndex::search(q,k,ef)` on the same corpus/seed (the in-memory oracle). Given the M26 `corpus()` fixture,
  when persisted structured and traversed, then results == in-memory `search` for several queries.
- RED: `hnsw_structured_empty_and_single_node` — empty graph (`entry_level=-1`) → `[]`; single node → returns it,
  no neighbor-tuple deref.
- RED (negative): `hnsw_structured_rejects_corrupt` — bad meta magic / truncated element page / neighbor pointer
  out of range → typed `Err` surfaced as a PG error, never a panic across `C-unwind`.
- GREEN: `write_structured(rel, fork, &PackedGraph)`; `read_hnsw_meta(rel)`; `read_page_item_at(rel,blkno,offno)`;
  `traverse_structured` (the pseudocode from the blueprint: meta entry → greedy upper (ef=1) → ground (ef) with
  visited-set + SIMD from-bytes scoring).
- REFACTOR: the traversal's candidate/result heaps mirror `HnswIndex::search_layer` — factor the shared heap
  logic if it reduces duplication without coupling.

#### Concurrency tests
(none — single-threaded). The traversal is a share-locked snapshot (`index_shared`, `scan.rs:56`); VACUUM exclusivity is the existing `lock.rs` contract, unchanged — no new shared mutable state

#### Failure scenarios
- Corrupt page mid-traversal (bad magic / OOB pointer / short page) → typed `Err` → `pg_sys::error!`, no panic.
- Entry node with fewer live layers than `entry_level` → guarded (`lc > node.level` skip), no infinite loop / OOB (mirror pgvector `hnswutils.c:947`).

#### Acceptance criteria
- `cargo pgrx test` (the new `#[pg_test]`s) green: round-trip == in-memory search; empty/single; corrupt → Err.
- Recall of the structured traversal == in-memory `search` on the fixture (exact equality on small data).

#### DoD
- `read_page_item_at` share-locks + copies out (no dangling buffer); round-trip test is the recall-preserved gate.

## Phase 2 — wire build/vacuum/scan + ef_search GUC + pages-read profiler

### T2.1 — route theodb_hnsw build + VACUUM to the structured writer; scan dispatch to on-demand; ef_search GUC + pages-read metric

#### Why this step
Make the structured path the real `theodb_hnsw` behavior end-to-end (the wiring triad: caller = build/scan,
integration test = SQL CREATE INDEX + ORDER BY, runtime metric = pages-read counter), replacing the O(N) blob
path, with `ef_search` tunable (GUC) — so a real SQL query on a real index reads O(ef·M) pages.

#### Files to edit
- `theodb_rs/src/am/build.rs` (`ambuild_hnsw` + VACUUM HNSW arm → `hnsw_page::write_structured`), `theodb_rs/src/am/scan.rs` (dispatch `HNSW_STRUCT_MAGIC` → `scan_hnsw_structured` folding pending; pages-read counter), `theodb_rs/src/am/guc.rs` (+`theodb_hnsw.ef_search`), `theodb_rs/src/am/mod.rs` (init GUC), `theodb_rs/src/am/page.rs` (`main_index_pages` HNSW arm + `peek_magic`)

#### TDD
- RED: `test_theodb_hnsw_sql_order_by_returns_knn` (`#[pg_test]`, SQL) — `CREATE INDEX … USING theodb_hnsw`;
  `SELECT … ORDER BY col <-> $q LIMIT k` returns the k nearest (recall preserved vs a seqscan on small data);
  a second query after `INSERT` includes the new row (pending fold); after `DELETE`+`VACUUM` the deleted row is
  gone. Given a small table + theodb_hnsw index, when ORDER BY <-> q LIMIT k, then the k true-nearest ids return.
- RED: `test_ef_search_guc_monotone` — `SET theodb_hnsw.ef_search` higher → recall ≥ lower (on a fixture where
  they differ); default (64) preserved when unset; out-of-range rejected.
- RED: `test_old_blob_hnsw_rejected_with_reindex` — an index in the old blob format is rejected with the REINDEX
  message (ADR-3), not misparsed.
- GREEN: the wiring above; `scan_hnsw_structured` reads `guc::ef_search()` clamped; increments a pages-read
  counter logged under `THEODB_SCAN_PROFILE=1` (the runtime metric).
- REFACTOR: DRY the ef_search GUC against the probes GUC (shared `define_int_guc` shape); remove `SCAN_EF` const
  usage on the structured path.

#### Concurrency tests
(none — single-threaded). Reads take `index_shared`; the VACUUM structured rewrite takes the existing exclusive lock (`lock.rs`), an unchanged contract — no new shared mutable state introduced by M35

#### Failure scenarios
- `ORDER BY col <-> NULL` → empty scan (existing `SK_ISNULL` guard `scan.rs:51`, unchanged).
- Scan of an unbuilt/empty index → `peek_magic==0` early return (existing).
- VACUUM interrupted mid-rewrite → WAL replay leaves the prior committed structured index (crash-safety test:
  build → simulate restart → identical scan).

#### Acceptance criteria
- `cargo pgrx test` green (SQL kNN + INSERT-fold + DELETE/VACUUM + ef_search GUC + blob-reject).
- `cargo pgrx install --release` 0 warnings; `CREATE EXTENSION` smoke OK in the container image.
- M20–M22 + ann + recall coexistence suites stay green (theodb_ivfflat untouched).

#### DoD
- Wiring triad present: caller (build+scan), integration test (SQL), runtime metric (pages-read counter).
- CHANGELOG `[Unreleased]` updated with the format break (REINDEX theodb_hnsw) per Rule 6.

## Phase 3 — 1M benchmark (the DoD gate)

### T3.1 — re-run the M32 harness for theodb_hnsw at 1M: QPS≥50 recall-preserved + pages-read flat-in-N + ef_search sweep

#### Why this step
The milestone's evidence (standing directive: DADOS E VALIDAÇÃO EM BENCHMARK): prove the O(N)→O(ef·M) win at
1M with a reproducible artifact, honest verdict, same harness/methodology as M32/M34.

#### Files to edit
- `benchmarks/run_m35_hnsw.py` (NEW), `docs/benchmarks/m35-hnsw-structured-scan.{md,json}` (NEW), `CHANGELOG.md`

#### TDD
(none — measurement artifact over the container, like M32/M34; the codec correctness is gated by the Phase-1/2 `#[pg_test]`s)

#### Concurrency tests
(none — single-threaded) — measurement benchmark

#### Failure scenarios
- If QPS < 50 at 1M OR recall regresses vs the pre-M35 blob baseline → honest FAIL surfaced in the artifact; do
  NOT ship a passing claim without the number (the DoD is a hard gate, not a target to spin).

#### Acceptance criteria
- `docs/benchmarks/m35-hnsw-structured-scan.json`: theodb_hnsw at 1M×128 — QPS ≥ 50, recall@10 preserved (≥ the
  M32 blob recall at matched ef), ef_search sweep (recall↑/QPS↓ monotone), and a **pages-read flat-in-N** table
  (pages-read at N vs 2N for the same query — grows with ef_search, not with N).
- `.md` carries hardware (CPU/cores/RAM), methodology, repro command, honest verdict + the O(N)→O(ef·M) delta vs
  the M32 blob number (1.6 QPS).

#### DoD
- Artifact reproducible via `python3 benchmarks/run_m35_hnsw.py`; CHANGELOG links it.

## Coverage Matrix

| Goal / DoD item | Task(s) |
|---|---|
| theodb_hnsw persists structured pages (meta + element + neighbor), not a blob; VACUUM/INSERT/DELETE intact | T1.1, T1.2, T2.1 |
| Scan reads O(ef·M) pages not O(N) (on-demand traversal) | T1.2, T2.1 (pages-read metric), T3.1 (flat-in-N proof) |
| QPS ≥ ~50 at 1M, recall preserved, validated by re-run of the M32 harness | T3.1 |
| Graph integrity: entry-point fallback, corrupt refs → typed Err, no recall regression | T1.2 (round-trip + negative), T2.1 |
| ef_search configurable (reuse M34 GUC infra) | T2.1 |
| Coexistence M20–M22 green; benchmark reproducible | T2.1, T3.1 |
| No new dependency | Dependencies (none) |
| CHANGELOG (Rule 6) + format-break REINDEX documented | T2.1, T3.1 |

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| On-demand traversal must match the in-memory graph bit-for-bit at 1M (pointer-chased graph, per-layer slice) — the single hardest part (~3-4× M31) | HIGH | codec-first: the T1.2 round-trip `#[pg_test]` (structured == in-memory search) is a hard gate BEFORE the benchmark; faithful port of `HnswSearchLayer`; immutable graph removes race complexity; reuse M31b SIMD scorer | paulohenriquevn |
| Off-by-one in per-layer neighbor slice / level cap → silent recall loss (no crash) | HIGH | unit-test the slice for every `lc` on multi-level fixtures (T1.1); recall@10 vs blob baseline as a hard gate (T3.1); assert neighbor tuple ≤ one page at build | paulohenriquevn |
| New page writer / read_page_item_at FFI → WAL/page corruption | MEDIUM | reuse the exact WAL scaffold from `extend_page_with_item`; pure packer (no FFI) tested in isolation; crash-safety test (build → WAL replay → identical scan); corrupt-page → typed Err | paulohenriquevn |
| Format break invalidates existing theodb_hnsw indexes | MEDIUM | ADR-3 REINDEX gate + CHANGELOG `Changed` (BREAKING) entry; pre-1.0 engine, no production indexes | paulohenriquevn |
| Benchmark shows QPS < 50 (traversal overhead) | MEDIUM | measurement-first; if under target, the pages-read metric localizes the bottleneck (reads vs score vs sort — the existing profiler pattern); honest artifact either way | paulohenriquevn |

## Unresolved Questions

- Element+neighbor page co-location for cache locality (pgvector `hnswbuild.c:204`) — deferred (YAGNI); if the
  benchmark shows read-latency-bound scans, it becomes a follow-up. Resolved for v1: two separate page ranges.
- Whether to bound the graph max-level analytically vs empirically so a neighbor tuple always fits one page —
  resolved: cap at build (ADR-4), assert at pack (T1.1).

## Failure scenarios

- **Corrupt page mid-traversal** (bad magic / OOB pointer / short page) → typed `Err` → `pg_sys::error!`, no panic across the C boundary. (T1.2)
- **Old blob-format index** → REINDEX error, not misparse. (T2.1, ADR-3)
- **ORDER BY <-> NULL / empty index** → empty scan (existing guards, unchanged). (T2.1)
- **VACUUM interrupted** → WAL replay leaves the prior committed structured index. (T2.1)
- **Benchmark under target** → honest FAIL in the artifact, pages-read localizes the cause. (T3.1)

## Final Phase — Integration Validation

- `cargo pgrx test` green: codec round-trip == in-memory search; empty/single/corrupt; SQL kNN + INSERT-fold +
  DELETE/VACUUM; ef_search GUC monotone; old-blob rejected.
- `cargo pgrx install --release` 0 warnings; `CREATE EXTENSION` + coexistence suites (M20–M22, ann, recall) green
  in the container.
- `docs/benchmarks/m35-hnsw-structured-scan.{md,json}` committed: QPS ≥ 50 at 1M, recall preserved, pages-read
  flat-in-N, ef_search sweep, hardware + repro + honest verdict.
- CHANGELOG `[Unreleased]` updated (Added: structured scan; Changed: BREAKING format — REINDEX theodb_hnsw).
