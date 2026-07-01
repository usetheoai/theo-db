---
slug: m26-vector-index-am
milestone_id: M26
created_at: 2026-07-01
goal: Promote the in-memory rebuild-per-query ANN to a persisted Postgres Index Access Method (theodb_hnsw + theodb_ivfflat) with planner pushdown, proven by benchmarks/tests/test_index_am.py passing (CREATE INDEX persisted + EXPLAIN uses the index + recall@k parity + incremental INSERT/DELETE/VACUUM), all green against the container.
---

# M26 — Vector Index Access Method (in-memory function → persisted index engine)

## Goal

Promote TheoDB's in-memory rebuild-per-query ANN into a **persisted Postgres Index Access Method**
(`theodb_hnsw` + `theodb_ivfflat`) with planner pushdown, measured by a single observable metric:
**`benchmarks/tests/test_index_am.py` passing** (CREATE INDEX persists to pages · `EXPLAIN` shows an Index Scan
for `ORDER BY embedding <-> $1 LIMIT k` · recall@k ≥ parity with the current function · incremental
INSERT/DELETE/VACUUM keep the index correct) **plus** `docs/benchmarks/m26-index-am.md` recording recall + latency
(persisted-scan vs full-scan+rebuild), all green in the Docker image.

## Context

M26 (`ROADMAP.md § M26`) closes the single architectural HIGH of the audit: today the ANN is a **function that
rebuilds the index on every query** — `ann_query.rs:153` reads the whole corpus into a `Vec<(i64, Vec<f32>)>` and
`:159`/`:167` calls `HnswIndex::build` / `IvfflatIndex::build` **per call**. That is O(N·build) per query and has
no persistence, no planner integration — the structural gap vs pgvector/pgvectorscale/vectorchord (all real AMs).

The prior-art investigation is **already done** and reused (no re-work): the SHIPPABLE_WITH_CAVEATS (89) blueprint
`.claude/knowledge-base/discoveries/blueprints/m21-own-ann-index-blueprint.md` (M26 absorbs the old M21b deferral).
It confirmed the feasibility linchpin: **pgrx 0.16 can register a Rust index AM** — pgvectorscale's
`amhandler(_fcinfo) -> PgBox<pg_sys::IndexAmRoutine>` (`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/mod.rs:45`)
sets every hook and emits `CREATE ACCESS METHOD diskann TYPE INDEX HANDLER diskann_amhandler` via
`#[pg_extern(sql=…)]`. The opclass that drives pushdown: `CREATE OPERATOR CLASS vector_l2_ops … USING <am> AS
OPERATOR 1 <-> (vector, vector) FOR ORDER BY float_ops, FUNCTION 1 …` (pgvectorscale
`sql/vectorscale--0.0.2--0.9.0.sql:134-137`; pgvector `sql/vector--0.4.4--0.5.0.sql:31`).

**Parsimony insight (the plan's spine):** the ANN *algorithms* already exist and are correct
(`ann/hnsw.rs`, `ann/ivf.rs` — pure build+search, proven by M21/M22 recall tests). M26 does **not** touch the
algorithm; it adds the **persistence + AM-plumbing layer**: serialize the built index into index pages (`ambuild`,
WAL-logged), read it back for scans (`ambeginscan`/`amrescan`/`amgettuple`), wire the opclass + `amcostestimate`
for pushdown, and maintain it incrementally. Reusing the proven algorithm is the biggest risk-reducer.

## Baseline Context

### Files that will be touched

| File | LoC today | git sha (last touch) | Why it exists / role in M26 |
|---|---:|---|---|
| `theodb_rs/src/ann_query.rs` | 176 | `d89083f` (M25 tree) | Current rebuild-per-query orchestration (`read_corpus`, `knn`). Stays (coexistence); the AM reuses `read_corpus`+algo. |
| `theodb_rs/src/ann/mod.rs` | 249 | `d89083f` | `Metric`, `HnswIndex`, `IvfflatIndex` re-exports + `Metric::dist` (M25 `pub(crate)`). Reused by the AM. |
| `theodb_rs/src/ann/hnsw.rs` | 212 | `d89083f` | In-memory HNSW build+search. Add `serialize`/`deserialize` (bincode-free, manual) for page persistence. |
| `theodb_rs/src/ann/ivf.rs` | 167 | `d89083f` | In-memory IVFFlat build+search. Add `serialize`/`deserialize`. |
| `theodb_rs/src/am/mod.rs` | 0 (NEW) | — | The `amhandler` (IndexAmRoutine registration) + `CREATE ACCESS METHOD` + opclass `extension_sql!`. |
| `theodb_rs/src/am/build.rs` | 0 (NEW) | — | `ambuild`/`ambuildempty`/`aminsert` — build index, serialize to WAL-logged pages; incremental pending-buffer insert. |
| `theodb_rs/src/am/scan.rs` | 0 (NEW) | — | `ambeginscan`/`amrescan`/`amgettuple`/`amendscan` — deserialize from pages, search main+pending, return TIDs by distance. |
| `theodb_rs/src/am/vacuum.rs` | 0 (NEW) | — | `ambulkdelete`/`amvacuumcleanup` — drop dead TIDs, fold pending, opportunistic compaction. |
| `theodb_rs/src/am/cost.rs` | 0 (NEW) | — | `amcostestimate` + `amoptions` (reloptions: m/ef_construction/lists) + `amvalidate`. |
| `theodb_rs/src/am/page.rs` | 0 (NEW) | — | Page layout helpers: meta page + serialized-blob pages via `GenericXLog` (WAL), buffer read/write guards. |
| `theodb_rs/src/lib.rs` | 92 | `d89083f` | Add `mod am;`. |
| `benchmarks/tests/test_index_am.py` | 0 (NEW) | — | The Goal-metric integration suite (persistence, EXPLAIN pushdown, recall parity, maintenance). |
| `docs/benchmarks/m26-index-am.md` | 0 (NEW) | — | Reproducible recall + latency evidence. |

### Current callers / dependents

- `ann_query::knn` is called by the `#[pg_extern] _hnsw_knn`/`_ivfflat_knn` in `api.rs` (M25 split). These SQL-callable
  functions STAY (coexistence, DoD #6). The AM is an ADDITION reached via `CREATE INDEX … USING theodb_hnsw`.
- `ann::{HnswIndex,IvfflatIndex,Metric}` are used by `ann_query.rs` and `sbq.rs`. The AM reuses them read-only + adds
  `serialize`/`deserialize` methods (additive).

### Domain glossary

- **AM (Access Method):** a Postgres index engine implementing `IndexAmRoutine` (the `am*` C callbacks).
- **opclass / opfamily:** binds an operator (`<->`) to the AM as an ORDER-BY operator so the planner can pushdown.
- **amgettuple:** returns the next matching TID (heap tuple id) in order; the executor fetches the heap row.
- **GenericXLog:** the Postgres WAL API for custom AMs to make page writes crash-safe without a custom rmgr.
- **pending buffer:** an overflow region for tuples inserted after build; scans merge main+pending; VACUUM folds it.

### Architecture boundaries affected

Per `rules/architecture.md`: `ann/` stays the pure domain (no pg types); `am/` is the new infrastructure/interface
adapter that depends on `pg_sys` + `ann/`. `am/` MUST NOT leak `pg_sys` types into `ann/`. Composition via `lib.rs`.

## Prior Art & Related Work

- **Blueprint** `m21-own-ann-index-blueprint.md` (SHIPPABLE 89) — the investigation; its four decisions (coexistence,
  measurement-first, anti-sunk-cost, deps clean). This plan implements its top recommendations.
- **pgvectorscale** (Rust/pgrx DiskANN AM) — `src/access_method/{mod,build,scan,cost_estimate,vacuum}.rs` — the
  scaffolding template (amhandler PgBox pattern, page storage, cost estimate).
- **pgvector** (C) — `src/{hnsw.c,ivfflat.c}` + opclass SQL — the recall@k parity target + canonical opclass shape.
- **vectorchord** (Rust/pgrx) — 2nd datapoint for the amhandler + sqllogictest recall pattern.

## ADRs

### ADR-1 — Persist by serializing the built index into WAL-logged index pages (not a bespoke on-disk graph)

**Decision:** `ambuild` builds the existing in-memory `HnswIndex`/`IvfflatIndex` (reuse the proven algorithm) and
serializes it into the index relation's pages via `GenericXLog` (crash-safe). Scans deserialize (cached in scan
state) and run the existing search. **Rationale:** parsimony + measurement-first (the blueprint's anti-sunk-cost stance) — the algorithm
is proven; the risk is the page/WAL plumbing, which a serialize-blob approach minimizes. **Rejected alternatives:**
(a) a bespoke on-disk DiskANN-style graph with per-node pages (pgvectorscale's approach) — materially larger,
higher FFI risk, not needed to meet the DoD; deferred. (b) storing the index in a side heap table — violates
"persisted in pages" and bypasses the AM contract.

### ADR-2 — Incremental maintenance via a pending buffer + VACUUM fold (not in-place graph mutation)

**Decision:** `aminsert` appends `(TID, vector)` to a pending region (no total rebuild — DoD #4); scans search the
main serialized index AND linearly scan the (bounded) pending region, merging by distance. `ambulkdelete`/
`amvacuumcleanup` drop dead TIDs and fold pending into the main index (rebuild-from-pages when pending exceeds a
threshold). **Rationale:** correct + genuinely incremental, uniform for HNSW & IVFFlat, far less FFI-risky than
in-place page-level graph mutation. It is a recognized design (write-buffer + background merge). **Rejected:**
(a) in-place HNSW graph insert in pages — pgvector-HNSW-grade effort, highest risk. (b) rebuild-on-every-insert —
violates DoD #4. **Drawback (documented):** scan degrades if pending grows unbounded without VACUUM — bounded by a
reloption threshold + autovacuum; recorded in the benchmark doc.

### ADR-3 — Ship both theodb_ivfflat and theodb_hnsw AMs; IVFFlat is the de-risk-first target

**Decision:** implement the AM generically; register two AMs (`theodb_ivfflat`, `theodb_hnsw`) sharing the same
plumbing (serialize/deserialize is format-agnostic). Phase order builds+validates **IVFFlat first** (simpler
search, easier to reason about pending merge), then HNSW reuses the identical page/scan/maintenance layer.
**Rationale:** pgvector itself shipped IVFFlat before HNSW; measurement-first. Both meet the DoD (bullet 2 names
`theodb_hnsw` — delivered). **Rejected:** HNSW-only first (higher risk, no incremental de-risk step).

### ADR-4 — Phase 0 is a mandatory de-risk spike gate

**Decision:** before any real build code, Phase 0 registers a **no-op AM** (amhandler with minimal hooks that
build an empty index + a scan returning nothing) and proves end-to-end: `CREATE ACCESS METHOD` loads, `CREATE INDEX
… USING theodb_ivfflat` succeeds, `EXPLAIN` shows the planner *can* select it. **Rationale:** the ROADMAP tags
FFI/longjmp/WAL as ALTO risk "não exercitada"; proving the pgrx 0.16 AM FFI works on THIS toolchain BEFORE the
large build is the anti-sunk-cost guard. If Phase 0 hits a pgrx/pg_sys wall, we surface it honestly (Rule 3) before
investing in the full build. **Rejected:** diving straight into `ambuild` (discovers FFI walls after large sunk cost).

## Dependency Graph

```
Phase 0 (de-risk spike: register no-op AM + CREATE INDEX + EXPLAIN)   ← GATE: must pass before Phase 1
   ↓
Phase 1 (page.rs: meta + blob pages via GenericXLog; ann serialize/deserialize)
   ↓
Phase 2 (ambuild: build+serialize IVFFlat to pages; ambuildempty; amoptions/amvalidate)
   ↓
Phase 3 (scan: ambeginscan/amrescan/amgettuple/amendscan — deserialize + search + TID order)  ← depends on 1,2
   ↓
Phase 4 (opclass + amcanorderbyop + amcostestimate → EXPLAIN Index Scan pushdown proven)      ← depends on 3
   ↓
Phase 5 (aminsert pending-buffer + ambulkdelete/amvacuumcleanup + VACUUM fold)                ← depends on 2,3
   ↓
Phase 6 (theodb_hnsw AM reusing the same layer)                                               ← depends on 2-5
   ↓
Phase 7 (benchmark: recall parity + latency; test_index_am.py green) — Final Integration Validation
```

## Dependencies

### Existing — use as-is

| Package | Version | Ecosystem | Why |
|---|---|---|---|
| `pgrx` | 0.16.1 | Rust | Already the extension framework; exposes `pg_sys` for the AM FFI. No new dep. |
| `pgrx::pg_sys` | (pgrx 0.16.1) | Rust | `IndexAmRoutine`, `GenericXLog*`, `ReadBufferExtended`, `PageInit`, buffer/page APIs — all in pg_sys. |

### New — to be introduced

(none — the AM is built on pgrx + std only, matching the blueprint Corner 2 finding that no new dep is required for
the MVP. Serialization is hand-rolled f32/usize little-endian into page bytes — no `bincode`/`serde` dep, KISS.)

### Removed

(none — coexistence.)

## Phase 0 — De-risk spike (GATE)

### T0.1 — Register a minimal no-op theodb_ivfflat AM and prove CREATE INDEX + EXPLAIN

#### Why this step
The ROADMAP flags the low-level pgrx AM surface (FFI/longjmp/WAL) as ALTO risk, "not yet exercised" in this repo.
Per ADR-4, we prove the FFI path works on THIS pgrx 0.16.1 + PG17 toolchain before the large build — anti-sunk-cost.
A no-op AM that registers, accepts `CREATE INDEX`, and is visible to the planner is the smallest end-to-end proof.

#### Files to edit
- `theodb_rs/src/am/mod.rs` (NEW) — `amhandler` returning `PgBox<IndexAmRoutine>` with all hooks set to minimal
  stubs (ambuild → empty `IndexBuildResult`, ambeginscan/amrescan/amendscan no-op, amgettuple → false, aminsert →
  false, ambulkdelete/amvacuumcleanup → passthrough, amcostestimate → constant, amoptions → null, amvalidate → true)
  + `extension_sql!` for `CREATE FUNCTION theodb_ivfflat_amhandler(internal) RETURNS index_am_handler … CREATE
  ACCESS METHOD theodb_ivfflat TYPE INDEX HANDLER …` + a minimal opclass so CREATE INDEX parses.
- `theodb_rs/src/lib.rs` — add `mod am;`.

#### TDD
- `benchmarks/tests/test_index_am.py::test_create_access_method_and_index_loads` (RED first):
  Given the extension, When `CREATE ACCESS METHOD theodb_ivfflat` exists in `pg_am` AND
  `CREATE INDEX ON t USING theodb_ivfflat (embedding vector_l2_ops)` succeeds,
  Then `SELECT amname FROM pg_am WHERE amname='theodb_ivfflat'` returns 1 row AND the index exists in `pg_class`.
- Assert `EXPLAIN SELECT … ORDER BY embedding <-> '[…]' LIMIT 5` mentions the index name OR (no-op stage) at least
  the planner does not error — the pushdown proof itself lands in Phase 4.

#### Concurrency tests
(none — single-threaded) — the Rust in each AM callback is single-threaded; cross-backend safety of concurrent scan/insert/vacuum rides on Postgres buffer content locks (every page write under a pinned+locked buffer in a `GenericXLog` span), exercised at integration by the concurrent-insert-during-scan test in T5.1.

#### Acceptance criteria
- `CREATE ACCESS METHOD theodb_ivfflat` + `CREATE INDEX … USING theodb_ivfflat` succeed in the image.
- `cargo pgrx` builds the extension with the AM FFI (no link/compile error) — proves pgrx 0.16 exposes the routine.
- `SELECT amname FROM pg_am WHERE amname = 'theodb_ivfflat'` returns exactly 1 row in the running image (and if any pgrx/pg_sys wall blocks registration, the build fails loudly and we STOP before Phase 1 — Rule 3).

#### DoD
- `pytest test_index_am.py::test_create_access_method_and_index_loads` green in the Docker image.
- Full image builds (`cargo pgrx install` regenerates SQL with the AM).

## Phase 1 — Page layout + index (de)serialization

### T1.1 — Meta page + WAL-logged blob pages; `HnswIndex`/`IvfflatIndex` serialize/deserialize
#### Why this step
Persistence (DoD #2) requires the built index to live in the index relation's pages, crash-safely. `GenericXLog`
is the sanctioned WAL API for custom AMs. Serialization must be exact (f32 bit-faithful) so a deserialized index
reproduces the same search results (recall parity, DoD #5).
#### Files to edit
- `theodb_rs/src/am/page.rs` (NEW) — meta page (magic, version, algo, metric, dim, nlists/m, pending-count) + a
  chained blob-page writer/reader using `GenericXLogStart`/`GenericXLogRegisterBuffer`/`GenericXLogFinish` and
  `ReadBufferExtended`/`PageInit`. Fail-fast typed errors (Rule 8) on short reads / magic mismatch.
- `theodb_rs/src/ann/ivf.rs`, `theodb_rs/src/ann/hnsw.rs` — add `to_bytes(&self) -> Vec<u8>` / `from_bytes(&[u8])
  -> Result<Self,String>` (little-endian f32/usize; no serde dep — KISS/YAGNI).
#### TDD
- Rust unit tests (`#[pg_test]`): `ivf_roundtrip_bytes` / `hnsw_roundtrip_bytes` — build a small index, `to_bytes`
  then `from_bytes`, assert the deserialized index returns identical `search` results (bit-faithful).
- Negative: `from_bytes` on truncated / bad-magic input returns `Err` (typed), not panic.
#### Concurrency tests
(none — single-threaded) — the Rust in each AM callback is single-threaded; cross-backend safety of concurrent scan/insert/vacuum rides on Postgres buffer content locks (every page write under a pinned+locked buffer in a `GenericXLog` span), exercised at integration by the concurrent-insert-during-scan test in T5.1.

#### Failure scenarios
- Short page read / corrupt blob → `from_bytes` returns typed `Err` → AM raises a clean error, never UB.
- `GenericXLogFinish` failure path → propagate as a pg error (fail-loud), page not half-written.
#### Acceptance criteria / DoD
- Round-trip unit tests green; blob pages readable back after write; WAL records emitted (verified via a crash-free
  write + re-read in the same test session). File ≤ 500 LoC each.

## Phase 2 — ambuild (IVFFlat): build + serialize to pages

### T2.1 — `ambuild`/`ambuildempty` build IVFFlat from the heap and persist; `amoptions`/`amvalidate`
#### Why this step
DoD #2: `CREATE INDEX` must persist (build once, not per query). `ambuild` scans the heap via
`table_index_build_scan` (the pg callback), collects `(TID, vector)`, builds `IvfflatIndex` (reuse `ann/ivf.rs`),
serializes to pages (Phase 1). `amoptions` parses reloptions (`lists`); `amvalidate` checks the opclass.
#### Files to edit
- `theodb_rs/src/am/build.rs` (NEW) — `ambuild` (heap scan callback → collect → build → serialize), `ambuildempty`
  (empty index for unlogged), `amoptions`, `amvalidate`.
- `theodb_rs/src/am/cost.rs` (NEW) — `amoptions` reloptions table (stub cost until Phase 4).
#### TDD
- `test_index_am.py::test_build_persists_not_rebuild`: build an index over N rows; assert (a) index size in pages
  > 0 (`pg_relation_size`), (b) two identical `ORDER BY <-> LIMIT k` queries each run an Index Scan (Phase 4) with
  NO per-query full-corpus read (proven by latency: 2nd query not O(N·build)). In Phase 2 (pre-pushdown) assert the
  index build succeeds + `pg_relation_size(idx) > 0` + rebuild is not triggered on read (log/counter).
#### Concurrency tests
(none — single-threaded) — the Rust in each AM callback is single-threaded; cross-backend safety of concurrent scan/insert/vacuum rides on Postgres buffer content locks (every page write under a pinned+locked buffer in a `GenericXLog` span), exercised at integration by the concurrent-insert-during-scan test in T5.1.

#### Acceptance criteria / DoD
- `CREATE INDEX` over a real table persists to pages; `pg_relation_size(idx) > 0`; build reuses `ann/ivf.rs` (no
  algorithm fork). File ≤ 500 LoC.

## Phase 3 — Scan (IVFFlat): ambeginscan / amrescan / amgettuple / amendscan

### T3.1 — Deserialize from pages, search, return TIDs in distance order
#### Why this step
The scan is how the persisted index answers `ORDER BY embedding <-> $1 LIMIT k`. `ambeginscan` allocates scan
state; `amrescan` receives the order-by key (the query vector) and deserializes the index (once) + searches
(reusing `ann/ivf.rs` search + probes); `amgettuple` returns TIDs in ascending distance; `amendscan` frees state.
#### Files to edit
- `theodb_rs/src/am/scan.rs` (NEW) — the four scan hooks + a scan-state struct holding the deserialized index +
  the result TID iterator. Read the order-by datum (query vector) via the scan keys.
#### TDD
- `test_index_am.py::test_index_scan_matches_bruteforce_recall`: build index; for K queries compare index results
  to a brute-force sequential scan (`enable_indexscan=off`), assert recall@10 ≥ parity threshold (reuse
  `theodb_bench` recall harness). Negative: empty index → 0 rows, no crash.
#### Concurrency tests
(none — single-threaded) — the Rust in each AM callback is single-threaded; cross-backend safety of concurrent scan/insert/vacuum rides on Postgres buffer content locks (every page write under a pinned+locked buffer in a `GenericXLog` span), exercised at integration by the concurrent-insert-during-scan test in T5.1.

#### Failure scenarios
- Query dim ≠ index dim → typed error (fail-fast), not a bad read.
- Scan over a concurrently-vacuumed index → returns only live TIDs (executor rechecks heap visibility anyway).
#### Acceptance criteria / DoD
- Index Scan returns correct neighbors; recall@10 ≥ parity vs the current function on the same corpus. ≤ 500 LoC.

## Phase 4 — Planner pushdown: opclass + amcanorderbyop + amcostestimate

### T4.1 — Opclass ORDER-BY binding + cost estimate so the planner picks the index
#### Why this step
DoD #3: `EXPLAIN` must show an Index Scan for `ORDER BY <-> LIMIT k`. That needs (a) the opclass declaring the
distance op `FOR ORDER BY float_ops`, (b) `amcanorderbyop=true` (set in Phase 0's amhandler), (c) `amcostestimate`
returning a cost lower than the seq-scan+sort so the planner chooses it.
#### Files to edit
- `theodb_rs/src/am/mod.rs` — `extension_sql!`: `CREATE OPERATOR CLASS theodb_ivfflat_l2_ops FOR TYPE vector USING
  theodb_ivfflat AS OPERATOR 1 <-> (vector,vector) FOR ORDER BY float_ops, FUNCTION 1 …` (+ cosine `<=>`, ip `<#>`).
- `theodb_rs/src/am/cost.rs` — real `amcostestimate` (index selectivity ~ probes/lists · N; startup + per-tuple).
#### TDD
- `test_index_am.py::test_explain_uses_index_scan`: `EXPLAIN (FORMAT JSON) SELECT … ORDER BY embedding <-> $1
  LIMIT 5` — assert an `Index Scan` node using the theodb_ivfflat index (with `enable_seqscan` default). Negative:
  a query with no LIMIT / no order-by op still works via seq scan (coexistence).
#### Concurrency tests
(none — single-threaded) — the Rust in each AM callback is single-threaded; cross-backend safety of concurrent scan/insert/vacuum rides on Postgres buffer content locks (every page write under a pinned+locked buffer in a `GenericXLog` span), exercised at integration by the concurrent-insert-during-scan test in T5.1.

#### Acceptance criteria / DoD
- `EXPLAIN` shows Index Scan on the theodb_ivfflat index for the ORDER BY <-> LIMIT k query. Proven in the image.

## Phase 5 — Incremental maintenance: aminsert + ambulkdelete + amvacuumcleanup

### T5.1 — Pending-buffer insert; vacuum drops dead TIDs + folds pending
#### Why this step
DoD #4: INSERT/DELETE must maintain the index without a total rebuild, and VACUUM must clean it. Per ADR-2:
`aminsert` appends to the pending region (WAL-logged); scans already merge main+pending (Phase 3 extended);
`ambulkdelete` marks dead TIDs; `amvacuumcleanup` folds pending into main + compacts when the threshold trips.
#### Files to edit
- `theodb_rs/src/am/build.rs` — `aminsert` (append to pending page, update meta count).
- `theodb_rs/src/am/vacuum.rs` (NEW) — `ambulkdelete` (callback dead-TID predicate), `amvacuumcleanup` (fold+compact).
- `theodb_rs/src/am/scan.rs` — extend search to merge main + pending by distance.
#### TDD
- `test_index_am.py::test_incremental_insert_delete_vacuum`:
  build index over N rows; INSERT M new rows → assert a new query finds an inserted row (pending searched) WITHOUT
  a full rebuild (latency/counter); DELETE some rows + `VACUUM t` → assert deleted TIDs no longer returned; assert
  recall stays ≥ parity after maintenance.
#### Concurrency tests
(none — single-threaded) — the Rust in each AM callback is single-threaded; cross-backend safety of concurrent scan/insert/vacuum rides on Postgres buffer content locks (every page write under a pinned+locked buffer in a `GenericXLog` span), exercised at integration by the concurrent-insert-during-scan test in T5.1.

#### Acceptance criteria / DoD
- Incremental INSERT visible in results without rebuild; DELETE+VACUUM removes dead TIDs; recall ≥ parity post-maint.

## Phase 6 — theodb_hnsw AM (reuse the layer)

### T6.1 — Register theodb_hnsw sharing page/scan/maintenance; HNSW build+search
#### Why this step
DoD bullet 2 names `theodb_hnsw`. The page/scan/vacuum layer is algorithm-agnostic (serialize-blob); HNSW plugs in
by building `HnswIndex` in `ambuild` and searching it in the scan — the meta page's `algo` field selects the path.
#### Files to edit
- `theodb_rs/src/am/mod.rs` — second `amhandler` + `CREATE ACCESS METHOD theodb_hnsw` + hnsw opclasses.
- `theodb_rs/src/am/build.rs`, `scan.rs` — branch on `algo` (hnsw vs ivfflat) reusing `ann/hnsw.rs`.
#### TDD
- `test_index_am.py::test_hnsw_am_recall_and_explain`: same persistence + EXPLAIN Index Scan + recall parity as
  IVFFlat, for `USING theodb_hnsw`.
#### Concurrency tests
(none — single-threaded) — the Rust in each AM callback is single-threaded; cross-backend safety of concurrent scan/insert/vacuum rides on Postgres buffer content locks (every page write under a pinned+locked buffer in a `GenericXLog` span), exercised at integration by the concurrent-insert-during-scan test in T5.1.

#### Acceptance criteria / DoD
- `CREATE INDEX … USING theodb_hnsw` persists, pushes down (EXPLAIN), recall@10 ≥ parity, maintenance works.

## Phase 7 — Final Integration Validation + benchmark

### T7.1 — Reproducible benchmark + full suite green
#### Why this step
Measurement-first (the blueprint's anti-sunk-cost stance, TheoDB rule 5): a performance/persistence claim needs reproducible data.
#### Files to edit
- `docs/benchmarks/m26-index-am.md` (NEW) — recall@k (index vs brute-force) for both AMs + latency: persisted
  Index Scan vs the current full-scan+rebuild function, mean±std over ≥3 runs, with the reproduction commands.
- confirm coexistence: run the M20–M22 suites (test_vector_ops / test_ann_index / test_sbq_index) — still green.
#### Concurrency tests
(none — single-threaded) — the Rust in each AM callback is single-threaded; cross-backend safety of concurrent scan/insert/vacuum rides on Postgres buffer content locks (every page write under a pinned+locked buffer in a `GenericXLog` span), exercised at integration by the concurrent-insert-during-scan test in T5.1.

#### Acceptance criteria / DoD
- Full `test_index_am.py` green in the Docker image; recall ≥ parity; latency table recorded; M20–M22 suites green
  (coexistence, DoD #6); `cargo pgrx test` unit tests compile; clippy -D warnings clean; CHANGELOG updated.

## Coverage Matrix

| # | M26 DoD item | Task(s) |
|---|---|---|
| 1 | `IndexAmRoutine` registered (ambuild/aminsert/ambeginscan/amgettuple/amendscan/ambulkdelete/amvacuumcleanup/amcostestimate) | T0.1 (skeleton) + T2.1 + T3.1 + T4.1 + T5.1 (real impls) |
| 2 | `CREATE INDEX … USING theodb_hnsw` persisted in pages (not rebuild-per-query) | T1.1 + T2.1 (ivfflat) + T6.1 (hnsw) |
| 3 | Planner pushdown `ORDER BY <-> LIMIT k` via amcanorderbyop + amcostestimate, proven by EXPLAIN | T4.1 + T6.1 |
| 4 | Incremental INSERT/DELETE maintain the index (no total rebuild); VACUUM cleans | T5.1 |
| 5 | Reproducible benchmark: recall@k ≥ parity + latency persisted vs rebuild | T3.1 (recall) + T7.1 (benchmark doc) |
| 6 | Coexistence with the SQL-callable function (no M20–M22 break) | T7.1 (regression) + ann_query.rs untouched |

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| pgrx 0.16 / pg_sys FFI wall (longjmp across Rust, memory context, WAL API not cleanly exposed) | HIGH | Phase 0 de-risk spike GATE proves it before any large build; pgvectorscale/vectorchord are existence proofs on pgrx | paulohenriquevn |
| Page/buffer/WAL misuse → corruption / crash (UB) | HIGH | Use `GenericXLog` (sanctioned WAL API), every write under a pinned+locked buffer; pg_regress + crash-free re-read tests; `#[pg_guard]` on all callbacks | paulohenriquevn |
| Pending buffer unbounded → scan degradation | MEDIUM | reloption threshold + amvacuumcleanup fold + benchmark records the degradation curve | paulohenriquevn |
| HNSW incremental in pending-buffer loses graph quality vs in-place insert | MEDIUM | Acceptable for MVP (recall parity is vs the current function, itself a from-scratch build); documented; in-place graph insert is a follow-up | paulohenriquevn |
| Scope: full production AM is large | HIGH | Phased; IVFFlat-first; each phase independently validated in Docker; honest BLOCKED over false PASS if a phase can't reach evidence | paulohenriquevn |

## Failure scenarios

- **Heap scan yields NULL / wrong-dim vectors** → `ambuild` skips NULL (pgvector index semantics), fail-fast typed
  error on dim mismatch (reuse `read_corpus` validation shape).
- **Corrupt/truncated index page** → `from_bytes` typed `Err` → AM raises clean error, never UB.
- **GenericXLog failure mid-write** → propagate as pg error; buffer released; no half-written page.
- **Concurrent VACUUM during scan** → executor heap-recheck drops dead TIDs; scan returns live only.
- **Query with no ORDER BY op / no LIMIT** → planner uses seq scan (coexistence), AM not forced.

## Unresolved Questions

- Does pgrx 0.16.1 expose `GenericXLog*` + `table_index_build_scan` in `pg_sys` cleanly, or is a thin `extern "C"`
  shim needed? — **Resolved empirically by Phase 0/T1.1 spike** (the honest first evidence; if a shim is needed it
  is added there with a note, not assumed).
- Exact `amcostestimate` constants for the planner to prefer the index at small N — tuned in Phase 4 against EXPLAIN
  (may need `enable_seqscan=off` documented as the small-N caveat if the planner won't pick it at tiny scale).

## Global DoD

- [ ] `benchmarks/tests/test_index_am.py` green in the Docker image (all phases' tests).
- [ ] `EXPLAIN` proves Index Scan pushdown for both AMs.
- [ ] recall@10 ≥ parity vs the current function; latency table in `docs/benchmarks/m26-index-am.md` (mean±std ≥3 runs).
- [ ] Incremental INSERT/DELETE/VACUUM correctness tests green.
- [ ] M20–M22 suites still green (coexistence).
- [ ] `cargo pgrx` builds; clippy -D warnings clean; every changed file ≤ 500 LoC.
- [ ] CHANGELOG `[Unreleased]` updated; no `Co-Authored-By`.
