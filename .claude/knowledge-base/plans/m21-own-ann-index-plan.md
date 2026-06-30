---
slug: m21-own-ann-index
milestone_id: M21
created_at: 2026-06-30
goal: Ship own Rust HNSW + IVFFlat ANN search (SQL-callable) at recall@k parity with pgvector, proven by a reproducible benchmark gate.
---

# Plan: M21 — Own ANN index (HNSW + IVFFlat) in Rust (SQL-callable, recall-gated)

> **Version 1.1** (edge-cases absorbed: EC-1 NULL rows skipped, EC-2 param upper-caps, EC-3 cosine zero-norm, EC-4 N=1, EC-5 empty queries, EC-6 id_col integer) — Implement TheoDB's own HNSW and IVFFlat approximate-nearest-neighbour algorithms in Rust
> (`theodb_rs`), exposed as SQL set-returning functions that build an index over a table's vector column and
> answer top-k `<->`/`<#>`/`<=>` queries, reusing M20's f32-parity distance kernel. A reproducible benchmark
> proves recall@k parity with pgvector's `hnsw`/`ivfflat` indexes (tolerance band, measurement-first). Scope is
> the **SQL-callable** algorithm + recall gate (per the locked scope decision 2026-06-30); the planner-integrated
> `CREATE INDEX … USING` access-method wrapper is explicitly deferred to a follow-up (M21b). Full coexistence:
> pgvector, `theodb.embed/hybrid/import`, and existing HNSW/IVFFlat indexes are untouched.

## Goal

> Enable TheoDB to answer top-k vector search with its own Rust HNSW + IVFFlat algorithms so that own-index
> recall@k reaches parity with pgvector, measured by `benchmarks/tests/test_ann_index.py` passing the parity gate
> (own recall@10 ≥ pgvector recall@10 − tolerance across an ef_search/probes sweep) against the container.

## Context

M21 (`ROADMAP-v2.md:116`) requires an own ANN index (HNSW + IVFFlat) in Rust, substituting pgvector **only** when
recall@k parity is proven on the M2/M9 harness — else an honest ADR keeps pgvector (anti-sunk-cost,
`ROADMAP-v2.md:124`). The discovery blueprint
(`.claude/knowledge-base/discoveries/blueprints/m21-own-ann-index-blueprint.md`, SHIPPABLE_WITH_CAVEATS 89) locked
**coexistence + measurement-first**. The implementation scope was further locked on 2026-06-30 to the
**SQL-callable** algorithm form (real HNSW/IVFFlat build+search over a column, recall-gated) — the lowest-risk path
that delivers a 100%-functional own index + reproducible benchmark this cycle; the on-disk planner AM wrapper is a
separate follow-up. M20 shipped the f32-parity distance kernel (`theodb_rs/src/vec.rs`) this depends on.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `theodb_rs/src/vec.rs` | 133 | `da8239d` (2026-06-30) | M20 f32-parity distance kernel (`l2_distance`/`inner_product`/`cosine_distance`, `pub(crate)`) | Signatures stay `pub(crate) fn (&[f32],&[f32])->f64`; f32-accum parity unchanged |
| `theodb_rs/src/ann.rs` (NEW) | 0 | — | (to create) own HNSW + IVFFlat algorithm core | — |
| `theodb_rs/src/lib.rs` | 491 | `da8239d` (2026-06-30) | crate root: `pg_module_magic`, `#[pg_schema] mod theodb_rs`, `#[pg_extern]`s, `extension_sql!` blocks | Existing externs/SQL (embed/nl/hybrid/import/vec) unchanged; only ADD `mod ann;` + new externs + new extension_sql block |
| `theodb_rs/src/pg.rs` | 56 | `6f5a01a` (2026-06-30) | typed-error helpers (`err_input`/`err_external`/`err_unsupported`, all `-> !`) | Reuse `err_input` for invalid args (SQLSTATE 22023); no signature change |
| `benchmarks/theodb_bench/recall.py` | — | (M2/M9 harness) | `recall_at_k` (`:61`), `brute_force_ground_truth` (`:41`) | REUSED read-only — not modified |
| `benchmarks/theodb_bench/db.py` | — | (M2/M9 harness) | `build_index(ddl)`/`query_topk`/`load_vectors` (`id INTEGER PK, embedding vector`) | REUSED; may add a thin `theodb_ann_knn()` helper (additive) |
| `benchmarks/bench_ann_index.py` (NEW) | 0 | — | (to create) parity benchmark own vs pgvector | — |
| `benchmarks/tests/test_ann_index.py` (NEW) | 0 | — | (to create) integration + parity gate vs container | — |
| `docs/benchmarks/m21-ann-index-parity.md` (NEW) | 0 | — | (to create) reproducible benchmark record | — |
| `CHANGELOG.md` | — | — | public contract | `[Unreleased]` gets one Added entry |

### Current callers / dependents

- **Symbol:** `l2_distance` / `inner_product` / `cosine_distance` in `theodb_rs/src/vec.rs` — callers (production): `theodb_rs/src/lib.rs` (the M20 `_vec_*` externs). Callers (tests): `theodb_rs/src/vec.rs` `#[pg_test]` mod. External public API: no. M21 ADDS `theodb_rs/src/ann.rs` as a new caller (intra-crate) — no existing caller changes.
- **Symbol:** `err_input` in `theodb_rs/src/pg.rs` — callers: `vec.rs:check_dims`, nl/hybrid/embed. M21 adds `ann.rs` as a caller. No change to the function.
- `theodb_rs/src/lib.rs` `extension_sql!` blocks — consumed by `cargo pgrx` SQL generation. M21 appends ONE new block; existing blocks unchanged.

### Domain glossary

- **ANN** — approximate nearest neighbour: trade exactness for speed; quality measured by recall@k.
- **HNSW** — Hierarchical Navigable Small World: layered proximity graph; search params `M` (neighbours/node), `ef_construction` (build candidate list), `ef_search` (query candidate list).
- **IVFFlat** — Inverted-File Flat: k-means centroids partition vectors into `lists`; query scans the `probes` nearest lists.
- **recall@k** — fraction of the true top-k neighbours (by distance, eps-thresholded) the index returns (`recall.py:61`).
- **tolerance band** — parity is `own_recall ≥ pgvector_recall − tol`, NOT identical result sets (HNSW is build-order non-deterministic; blueprint Q5).
- **coexistence** — own functions live alongside pgvector; they read vectors via `::real[]`, never redefine pgvector's type/operators/AMs.

### Architecture boundaries affected

Per `rules/architecture.md`: `ann.rs` is **domain** (pure algorithm, no pg I/O except via the injected distance kernel). `lib.rs` externs are the **interface/composition root** (Spi table read + SRF emission + arg validation at the boundary). The distance kernel (`vec.rs`) is the **shared inner dependency** (DIP: `ann.rs` depends on the in-crate distance functions, not on pgvector). No new cross-layer import direction is introduced.

## Prior Art & Related Work

- **Internal blueprint** — `.claude/knowledge-base/discoveries/blueprints/m21-own-ann-index-blueprint.md` (SHIPPABLE_WITH_CAVEATS 89). Consumed: ADR D1 (coexistence), D2 (distance reuse + tolerance-band parity), D3 (measurement-first + anti-sunk-cost), D4 (pgrx 0.16.1 pattern); Coverage Corner 4 (HNSW/IVFFlat algorithm details with pgvector `path:line`); Coverage Corner 1 (recall-harness reuse sketch).
- **Reference (algorithm)** — pgvector C: HNSW build/search (`.claude/knowledge-base/references/pgvector/src/hnswutils.c:115,249` layer math; `:826-849` heaps; `:1063-1163` neighbour heuristic), IVFFlat (`.claude/knowledge-base/references/pgvector/src/ivfkmeans.c:19-88` kmeans++; `.claude/knowledge-base/references/pgvector/src/ivfscan.c:133-135` probes). Defaults M=16/ef_construction=64/ef_search=40 (`hnsw.h:50,53,56`), lists=100/probes=1 (`ivfflat.h:51,54`).
- **Reference (test shape)** — pgvectorscale distance-thresholded recall test (`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/build.rs:1363-1396`).
- **Internal prior milestone** — M20 distance kernel `theodb_rs/src/vec.rs` (reused, not reinvented — Rule 9).
- **External literature** — Malkov & Yashunin, "Efficient and robust ANN using HNSW" (arXiv:1603.09320) — the algorithm M21 implements.

## Objective

- [ ] Own HNSW build+search in Rust (`ann.rs`) — deterministic given a seed, reusing `vec.rs` distances.
- [ ] Own IVFFlat build+search in Rust (`ann.rs`) — kmeans++ lists + probes, reusing `vec.rs` distances.
- [ ] SQL surface `theodb.hnsw_knn(...)` + `theodb.ivfflat_knn(...)` reading a table column via Spi, REVOKEd from PUBLIC.
- [ ] Input validation at the boundary: typed errors (22023) for bad args / dim mismatch / missing table-column.
- [ ] Reproducible recall@k benchmark own vs pgvector in `docs/benchmarks/`, mean±std ≥3 runs, with a parity-gate test.
- [ ] Migration decision (coexistence) documented; "retain pgvector" remains valid if parity not reached (anti-sunk-cost).

## ADRs

### D1 — SQL-callable build+search (one call builds once, answers the query batch); planner AM deferred

**Decision:** expose `theodb.hnsw_knn(src_table regclass, embed_col text, queries vector[], k int, m int, ef_construction int, ef_search int, metric text, id_col text, seed bigint)` (and `ivfflat_knn` with `lists`/`probes`) as set-returning functions. Each call reads `(id_col, embed_col)` from `src_table` via Spi, builds the index in-memory ONCE, then answers every query in `queries`, emitting `(query_idx, id, distance)` rows ordered by distance per query. No cross-call persistence.

**Rationale:** the recall gate needs "build once, query many" within a fair comparison; a batch SRF gives exactly that with zero cross-call state and zero serialization (KISS, parsimony rung 6). Matches the locked measurement-first scope.

**Alternatives considered:** (a) persisted index serialized to `bytea` + separate `build`/`search` functions — rejected (serialization complexity, YAGNI for the recall gate). (b) full planner `CREATE INDEX … USING` AM — rejected for THIS milestone (PhD-level/multi-week, high risk; deferred to M21b per the scope decision). (c) build-per-single-query — rejected (rebuilds the graph per query → unfair latency + wasteful).

**Consequences:** the deliverable is a real own ANN algorithm + recall measurement; usability as a drop-in index awaits M21b. The benchmark calls the batch function once per (algorithm, ef_search) point.

### D2 — Reuse M20 distance kernel; recall parity is a tolerance band

**Decision:** `ann.rs` computes all distances through `crate::vec::{l2_distance,inner_product,cosine_distance}` (M20). Parity is `own_recall@k ≥ pgvector_recall@k − tolerance` (default tol=0.02) across an ef_search sweep, NOT identical neighbour sets.

**Rationale:** Rule 9 (reuse M20, no new distance code, no SIMD dep); HNSW is build-order non-deterministic (blueprint Q5, `hnswutils.c:249`) so a band is the only physically correct contract (mirrors M20 ADR D3).

**Alternatives considered:** new SIMD distance (rejected — perf, M22, YAGNI); exact result-set match (rejected — physically impossible across builds).

**Consequences:** the gate is a benchmark sweep with a documented tolerance; the M20 kernel is the shared distance core.

### D3 — Deterministic seeded RNG via std (no `rand` dependency)

**Decision:** HNSW layer assignment + IVFFlat kmeans++ seeding use a small in-module SplitMix64/xorshift PRNG seeded by the `seed` arg (default 42). No `rand` crate.

**Rationale:** parsimony rung 2/5 — a 5-line `std`-only PRNG suffices for layer levels + centroid seeding; reproducibility makes tests deterministic (`rules/testing.md` §6 — inject the RNG). Adding `rand` would be a redundant dep (parsimony rung 4 inverse).

**Alternatives considered:** `rand`/`rand_chacha` (rejected — redundant dep for a 5-line need); unseeded RNG (rejected — non-deterministic tests are flaky, forbidden).

**Consequences:** tests assert exact recall on a fixed seed; the PRNG is documented as not cryptographic (it is not security-sensitive).

### D4 — Boundary validation with typed errors; coexistence preserved

**Decision:** the SQL externs validate args at the boundary (Unbreakable Rule 8) with BOTH lower and upper bounds
(EC-2 — prevent unbounded-allocation DoS): `1≤k`, `2≤m≤100`, `k≤ef_construction≤1000`, `k≤ef_search≤1000`,
`1≤lists≤32768`, `1≤probes≤32768` (ranges mirror pgvector `hnsw.h:50-58`/`ivfflat.h:51-54`), `metric ∈
{l2,cosine,ip}`, `id_col` must be an integer-typed column (EC-6), dim consistency across rows; NULL-vector rows are
SKIPPED (EC-1 — matches pgvector index semantics, not indexed); failures raise `err_input` (SQLSTATE 22023). The
functions live in the `theodb` schema, are REVOKEd from PUBLIC, and read vectors via `embed_col::real[]` — never
touching pgvector's type/operators/indexes or `embed/hybrid/import`.

**Rationale:** fail-fast typed errors (`rules/error-handling.md`); coexistence is the blueprint D1 decision; REVOKE matches M20's least-privilege posture.

**Alternatives considered:** trust inputs (rejected — Rule 8); PUBLIC execute (rejected — M20 precedent REVOKEs).

**Consequences:** negative-case tests assert the specific 22023; existing pgvector workflows are provably untouched.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Own HNSW may not reach pgvector recall parity (algorithm subtlety: neighbour heuristic, entry-point) | High | Tolerance band (D2); anti-sunk-cost fallback — if parity not reached, ADR keeps pgvector and the milestone ships the *measurement* of the gap (DoD, `ROADMAP-v2.md:124`) | impl |
| Build-per-call has O(N·log N) build cost folded into the benchmark call | Medium | Benchmark separates build time from query time; one build per (alg, ef_search) point; documented honestly in the benchmark doc | impl |
| Spi read of a large table into memory could be heavy | Medium | Benchmark uses bounded corpus sizes (e.g., 1k–50k); function documents the in-memory build (not for production-scale tables — that is M21b's on-disk AM) | impl |
| SQL-callable form is not a planner `USING` index (literal "access method" deferred) | Medium | Scope decision logged (2026-06-30); M21b tracked for the AM wrapper; coexistence decision applies to both | owner |
| f32 determinism across SIMD on the distance path | Low | Reuses M20 kernel whose parity is already proven (~1e-6 rel); recall is distance-thresholded with eps, absorbing low-bit noise | impl |

## Unresolved Questions

- Q1 — What tolerance (`tol`) and ef_search sweep points make the parity gate both fair and meaningful? (resolved at plan time: tol=0.02 default, ef_search ∈ {10,40,100,200}, k=10 — tunable in the benchmark; documented in the doc.)
- Q2 — Does the corpus need a real embedding dataset, or is a deterministic synthetic corpus sufficient for the parity gate? (resolved: synthetic deterministic corpus like `theodb_bench` uses elsewhere is sufficient — recall is relative own-vs-pgvector on the SAME corpus; a real dataset is a nice-to-have, not a gate requirement.)
- Q3 — Should `metric='ip'` (inner product, non-metric) be in the first cut? (resolved: yes for l2 + cosine as the gated metrics; ip supported in the API but gated only on l2+cosine to keep the benchmark focused.)

## Dependencies

### Existing — use as-is

| Package | Version | Ecosystem | Why |
|---|---|---|---|
| `pgrx` | `0.16.1` | rust | PostgreSQL extension framework — already the theodb_rs core (ADR D4); provides `#[pg_extern]`, `Spi`, `TableIterator`, `pg_module_magic` |
| `serde_json` | `1` | rust | already declared (`theodb_rs/Cargo.toml:30`) — NOT used by M21 (no serialization needed, ADR D1); listed for completeness |
| `psycopg2` / `numpy` | (harness) | python | already in `benchmarks/requirements.txt` — used by the recall benchmark + integration test |

### New — to be introduced

| Package | Version | Ecosystem | Rule 9 rationale (libs evaluated) | Why this one |
|---|---|---|---|---|
| (none) | — | — | Evaluated: `rand`/`rand_chacha` (rejected — a 5-line `std` SplitMix64 covers the only RNG need, parsimony rung 2; redundant dep, ADR D3); `simdeez`/`half` (rejected — SIMD/quantization is perf, deferred to M22, YAGNI); `bincode`/`serde` for index serialization (rejected — batch SRF needs no persistence, ADR D1). | M21 needs ZERO new deps: `pgrx` + `std` + the M20 `vec.rs` kernel suffice (Rule 9 / parsimony-ladder rungs 2–4). |

### Removed

| Package | Last version | Why removed |
|---|---|---|
| (none) | | |

## Dependency Graph

```
Phase 1 (ann.rs algorithm core + unit tests)
   │
   ▼
Phase 2 (SQL surface hnsw_knn/ivfflat_knn + Spi + REVOKE + integration test)
   │
   ▼
Phase 3 (recall@k parity benchmark vs pgvector + docs + parity-gate test)
   │
   ▼
Final Phase (integration validation)
```

All phases are sequential (each depends on the prior). No parallelism.

---

## Phase 1: HNSW + IVFFlat algorithm core in Rust

**Objective:** implement the two ANN algorithms as pure in-crate Rust over an in-memory corpus, reusing the M20 distance kernel, deterministic under a seed — verified by unit tests on a known corpus.

### T1.1 — HNSW build + search

#### Objective
Implement `HnswIndex::build(corpus: &[(i64, Vec<f32>)], m, ef_construction, metric, seed)` and `.search(query, k, ef_search) -> Vec<(i64, f64)>` in `theodb_rs/src/ann.rs`.

#### Why this step (action + reasoning)
1. **What this step does** — adds the HNSW graph (layered adjacency + entry point), build via greedy insert + neighbour heuristic, and search via the candidate/visited heaps, returning top-k `(id, distance)`.
2. **Why now** — it is the algorithmic core the SQL surface (Phase 2) and the recall gate (Phase 3) depend on; it must exist and be unit-correct before any wiring (ADR D1/D2; blueprint Coverage Corner 4 cites the exact pgvector algorithm at `hnswutils.c:115,249,826-849,1063-1163`).

#### Evidence
pgvector layer math `level=(int)(-log(rand)*ml), ml=1/log(M)` (`.claude/knowledge-base/references/pgvector/src/hnswutils.c:115,249`); two-heap search (`hnswutils.c:826-849`); neighbour heuristic (`hnswutils.c:1063-1163`); defaults M=16/ef_construction=64 (`.claude/knowledge-base/references/pgvector/src/hnsw.h:50,53`).

#### Files to edit
```
theodb_rs/src/ann.rs (NEW) — HnswIndex struct + build + search + SplitMix64 PRNG + #[pg_test] unit tests
theodb_rs/src/lib.rs — add `mod ann;`
```

#### Deep file dependency analysis
- `ann.rs` (new): depends on `crate::vec` (distance) + `crate::pg::err_input` (validation). No pgvector dependency.
- `lib.rs`: add one `mod ann;` line (Baseline row: only ADD, existing modules unchanged).

#### Deep Dives
- Data structures: `HnswIndex { nodes: Vec<Node>, entry: Option<usize>, max_level: usize, m, ef_construction, metric, vectors: Vec<Vec<f32>>, ids: Vec<i64> }`; `Node { neighbors: Vec<Vec<usize>> }` (per-layer adjacency).
- Layer level: `level = (-(prng_f64().ln()) * (1.0/(m as f64).ln())) as usize`.
- Search layer: min-heap candidates by distance, max-heap results size ef, visited `Vec<bool>` (or `HashSet`).
- Neighbour heuristic: keep a candidate only if it is closer to the query than to any already-selected neighbour (pgvector `SelectNeighbors`).
- Invariants: distances via `crate::vec::*` (preserve M20 kernel); all corpus vectors same dim (validated upstream).
- Edge cases: empty corpus → empty result; k > N → return N; single element; ef_search < k → clamp to k.

#### Pseudo-code / Signatures
```rust
pub(crate) enum Metric { L2, Cosine, Ip }
pub(crate) struct HnswIndex { /* fields above */ }
impl HnswIndex {
  pub(crate) fn build(corpus: &[(i64, Vec<f32>)], m: usize, ef_c: usize, metric: Metric, seed: u64) -> Self;
  pub(crate) fn search(&self, q: &[f32], k: usize, ef_search: usize) -> Vec<(i64, f64)>; // sorted by distance asc
}
// Example: build([(0,[0,0]),(1,[1,0]),(2,[5,5])], m=16, ef_c=64, L2, 42).search([0,0], k=2, 40)
//   -> [(0, 0.0), (1, 1.0)]
```

#### Tasks
1. Add `mod ann;` to `lib.rs`.
2. Implement `Metric` + `dist(metric,a,b)` dispatch over `crate::vec`.
3. Implement SplitMix64 PRNG (seeded, std-only).
4. Implement `HnswIndex::build` (insert each element: assign level, search-descend, select neighbours, link bidirectionally, update entry).
5. Implement `HnswIndex::search` (greedy descend ef=1 to layer 1, then ef_search at layer 0, return top-k sorted).

#### TDD
```
RED: hnsw_returns_exact_on_tiny_corpus() — build [(0,[0,0]),(1,[1,0]),(2,[5,5])], search [0,0] k=2 → ids [0,1], dists [0.0,1.0]
RED: hnsw_recall_high_on_random_corpus() — 200 random 8-d vecs (seeded), ef_search=100, recall@10 vs brute-force ≥ 0.95
RED: hnsw_deterministic_same_seed() — two builds same seed → identical search result; different seed allowed to differ
RED: hnsw_k_greater_than_n_returns_n() — corpus of 3, k=10 → 3 results
RED: hnsw_single_element() — corpus of 1, k=10 → exactly 1 result, no infinite loop (EC-4)
RED: cosine_zero_norm_does_not_panic() — corpus incl. [0,0,…] with metric=cosine → no panic, zero vector sorts last (EC-3)
GREEN: implement build+search
REFACTOR: extract shared heap/search-layer helper if duplicated with IVF; else none
VERIFY: cargo pgrx test --features pg17 (ann tests) OR cargo test
```

#### Concurrency tests

(none — single-threaded) — the index build + search runs entirely in-memory within a single backend call; no shared mutable state, no threads. The optional rayon-style parallel build is explicitly out of scope (M22).

#### Acceptance Criteria
- [ ] All RED tests green — `cargo pgrx test --features pg17` exits 0 on the `ann` tests.
- [ ] `hnsw_recall_high_on_random_corpus` ≥ 0.95 at ef_search=100.
- [ ] Pass: lint — `cargo clippy --release --features pg17 -- -D warnings` clean on `ann.rs`.
- [ ] Pass: size — `ann.rs` ≤ 500 lines.

#### DoD
- [ ] Tests passing (`cargo pgrx test`/`cargo test`), clippy clean, `mod ann;` wired.

### T1.2 — IVFFlat build + search

#### Objective
Implement `IvfflatIndex::build(corpus, lists, metric, seed)` (kmeans++ centroids + list assignment) and `.search(query, k, probes) -> Vec<(i64,f64)>` in `ann.rs`.

#### Why this step (action + reasoning)
1. **What this step does** — adds kmeans++ centroid training, assigns each vector to its nearest centroid list, and searches the `probes` nearest lists, returning top-k.
2. **Why now** — IVFFlat is the second DoD algorithm and shares the distance kernel + corpus-read with HNSW; building it right after HNSW maximizes reuse (ADR D2; blueprint Coverage Corner 4 cites `ivfkmeans.c:19-88`, `ivfscan.c:133-135`).

#### Evidence
kmeans++ init + Lloyd (`.claude/knowledge-base/references/pgvector/src/ivfkmeans.c:19-88`); probes loop (`.claude/knowledge-base/references/pgvector/src/ivfscan.c:133-135`); defaults lists=100/probes=1 (`.claude/knowledge-base/references/pgvector/src/ivfflat.h:51,54`).

#### Files to edit
```
theodb_rs/src/ann.rs — add IvfflatIndex + kmeans++ + #[pg_test] tests
```

#### Deep file dependency analysis
- `ann.rs`: adds `IvfflatIndex` reusing `Metric`/`dist`/PRNG from T1.1. No new external dependency.

#### Deep Dives
- Data structures: `IvfflatIndex { centroids: Vec<Vec<f32>>, lists: Vec<Vec<usize>>, vectors, ids, metric }`.
- kmeans++: seed first centroid uniformly; each next with prob ∝ squared distance; then a few Lloyd iterations (cap, e.g., 10) — bounded.
- Assignment: each vector → argmin distance to centroid.
- Search: rank centroids by distance to query, scan tuples in the `probes` nearest lists, collect + sort by distance, return top-k.
- Edge cases: lists > N → clamp lists to N; empty list; probes > lists → clamp.

#### Pseudo-code / Signatures
```rust
pub(crate) struct IvfflatIndex { /* fields above */ }
impl IvfflatIndex {
  pub(crate) fn build(corpus: &[(i64, Vec<f32>)], lists: usize, metric: Metric, seed: u64) -> Self;
  pub(crate) fn search(&self, q: &[f32], k: usize, probes: usize) -> Vec<(i64, f64)>;
}
```

#### Tasks
1. Implement kmeans++ seeding + bounded Lloyd iterations.
2. Implement list assignment.
3. Implement search over `probes` nearest lists, sort, top-k.

#### TDD
```
RED: ivfflat_returns_exact_on_tiny_corpus() — 3 vecs, lists=2, probes=2 → top-k matches brute force
RED: ivfflat_recall_with_enough_probes() — 200 random 8-d, lists=16, probes=16 → recall@10 vs brute ≥ 0.95
RED: ivfflat_probes_one_is_partial() — probes=1 returns a subset (recall < 1.0 but > 0), no crash
RED: ivfflat_clamps_lists_gt_n() — lists=1000 on corpus of 5 → no panic, correct results
GREEN: implement build+search
REFACTOR: share dist/heap with HNSW; else none
VERIFY: cargo pgrx test --features pg17
```

#### Concurrency tests

(none — single-threaded) — the index build + search runs entirely in-memory within a single backend call; no shared mutable state, no threads. The optional rayon-style parallel build is explicitly out of scope (M22).

#### Acceptance Criteria
- [ ] All RED tests green; `ivfflat_recall_with_enough_probes` ≥ 0.95 — `cargo pgrx test --features pg17` exits 0.
- [ ] Pass: lint — `cargo clippy --release --features pg17 -- -D warnings` exits 0 on `ann.rs`.
- [ ] Pass: size — `ann.rs` ≤ 500 lines (split into `ann/hnsw.rs` + `ann/ivf.rs` if exceeded).

#### DoD
- [ ] Tests passing, clippy clean.

---

## Phase 2: SQL surface + Spi table read + REVOKE

**Objective:** expose `theodb.hnsw_knn` and `theodb.ivfflat_knn` as set-returning functions reading a table column via Spi, with boundary validation + typed errors + REVOKE — proven by a Python integration test against the container.

### T2.1 — `theodb.hnsw_knn` + `theodb.ivfflat_knn` externs + extension_sql

#### Objective
Add `#[pg_extern]` set-returning functions that read `(id_col, embed_col::real[])` from `src_table` via Spi, build the index (Phase 1), answer each query, and emit `(query_idx int, id bigint, distance float8)`; declare them in `theodb` schema via `extension_sql!`, REVOKE from PUBLIC.

#### Why this step (action + reasoning)
1. **What this step does** — wires the Phase-1 algorithms to SQL: Spi read → build → per-query search → `TableIterator` rows; validates args at the boundary (ADR D4).
2. **Why now** — the recall gate (Phase 3) drives the algorithms through SQL; this is the caller (wiring triad pillar a) that makes the algorithm reachable end-to-end (`rules/cycle-implement.md`).

#### Evidence
Existing extern + `extension_sql!` pattern in `theodb_rs/src/lib.rs:158-238` (M20 `_vec_*` + REVOKE); pgrx `TableIterator` SRF + `Spi::connect` reads; harness table shape `id INTEGER PK, embedding vector` (`benchmarks/theodb_bench/db.py:85`).

#### Files to edit
```
theodb_rs/src/lib.rs — add hnsw_knn + ivfflat_knn #[pg_extern]s (TableIterator SRF) + one extension_sql! block (CREATE FUNCTION wrappers in theodb schema + REVOKE FROM PUBLIC)
theodb_rs/src/ann.rs — add a thin pub(crate) helper if needed to map (regclass,col) Spi rows → corpus
```

#### Deep file dependency analysis
- `lib.rs`: ADD two externs + one extension_sql block; existing externs/SQL unchanged (Baseline invariant). The new externs call `crate::ann::*` (new caller) + `crate::pg::err_input` (validation).
- Spi: read `format!("SELECT {id_col}, {embed_col}::real[] FROM {src_table}")` — `src_table` is `regclass` (already validated by PG cast), `id_col`/`embed_col` validated against an identifier allowlist regex to prevent injection (boundary validation, Rule 8).

#### Deep Dives
- Signature (SQL): `theodb.hnsw_knn(src_table regclass, embed_col text, queries vector[], k int default 10, m int default 16, ef_construction int default 64, ef_search int default 40, metric text default 'l2', id_col text default 'id', seed bigint default 42) RETURNS TABLE(query_idx int, id bigint, distance float8)`.
- `queries vector[]` → each `vector` cast to `real[]` → `Vec<f32>`.
- Validation (raise 22023 via `err_input`): `embed_col`/`id_col` not a valid identifier (allowlist regex `^[A-Za-z_][A-Za-z0-9_]*$`); `k<1`; `m<2 || m>100`; `ef_construction<k || >1000`; `ef_search<k || >1000`; `lists<1 || >32768`; `probes<1 || >32768`; `metric` not in set; `id_col` not an integer-typed column (EC-6); corpus rows with differing dim; query dim ≠ corpus dim.
- NULL-vector rows are skipped in `spi_read` (EC-1 — `if v.is_none() { continue; }`), matching pgvector index semantics (NULLs are not indexed).
- Empty `queries` array → early-return 0 rows BEFORE the Spi read/build (EC-5 — no wasted build).
- Build once, loop queries, emit rows ordered by distance.
- REVOKE: `REVOKE ALL ON FUNCTION theodb.hnsw_knn(...) FROM PUBLIC;` (+ ivfflat).

#### Pseudo-code / Signatures
```rust
#[pg_extern] fn hnsw_knn(src_table: pgrx::PgRelation, embed_col: &str, queries: Vec<...>, k: i32, ...)
  -> TableIterator<'static, (name!(query_idx,i32), name!(id,i64), name!(distance,f64))> {
    validate(...);                         // err_input on bad args (22023)
    let corpus = spi_read(src_table, id_col, embed_col);  // Vec<(i64,Vec<f32>)>
    let idx = HnswIndex::build(&corpus, m, ef_c, metric, seed);
    let mut out = vec![];
    for (qi, q) in queries.iter().enumerate() {
       for (id, d) in idx.search(q, k, ef_search) { out.push((qi as i32, id, d)); }
    }
    TableIterator::new(out)
}
```

#### Tasks
1. Implement `spi_read(regclass,id_col,embed_col) -> Vec<(i64,Vec<f32>)>` with identifier validation.
2. Implement `hnsw_knn` extern (validate → read → build → search → emit).
3. Implement `ivfflat_knn` extern (same shape, lists/probes args).
4. Add `extension_sql!` block: nothing extra needed if `#[pg_extern(schema="theodb"...)]` — match the M20 pattern (wrappers + REVOKE).

#### TDD
```
RED (Rust #[pg_test]): hnsw_knn_smoke() — CREATE TEMP TABLE t(id int, e vector(2)); insert 3 rows; SELECT * FROM theodb.hnsw_knn('t','e', ARRAY['[0,0]']::vector[], 2) → 2 rows, id 0 then 1
RED (#[pg_test], error): hnsw_knn_bad_metric_raises_22023() — metric='nope' → 22023
RED (#[pg_test], error): hnsw_knn_dim_mismatch_raises_22023() — query dim ≠ table dim → 22023
RED (#[pg_test]): ivfflat_knn_smoke() — analogous
RED (#[pg_test]): knn_empty_queries_returns_zero_rows() — ARRAY[]::vector[] → 0 rows, no build (EC-5)
RED (#[pg_test]): knn_skips_null_vector_rows() — table with a NULL embedding row → that row excluded, no panic (EC-1)
RED (#[pg_test], error): knn_param_over_cap_raises_22023() — ef_construction=2_000_000 → 22023 (EC-2)
RED (#[pg_test], error): knn_non_integer_id_col_raises_22023() — id_col on a text/uuid column → 22023 (EC-6)
GREEN: implement externs + extension_sql
REFACTOR: factor the shared validate+spi_read; else none
VERIFY: cargo pgrx test --features pg17
```

#### Concurrency tests

(none — single-threaded) — the index build + search runs entirely in-memory within a single backend call; no shared mutable state, no threads. The optional rayon-style parallel build is explicitly out of scope (M22).

#### Acceptance Criteria
- [ ] All RED `#[pg_test]`s green.
- [ ] `has_function_privilege('public','theodb.hnsw_knn(...)','execute')` is false (REVOKE) — asserted in the integration test (Phase 3).
- [ ] Pass: lint — clippy `-D warnings` clean.

#### DoD
- [ ] `cargo pgrx test` green; functions installed; CHANGELOG `[Unreleased]` Added entry.

### T2.2 — Python integration test: top-k correctness vs brute force (container)

#### Objective
`benchmarks/tests/test_ann_index.py` builds a table on the container, calls `theodb.hnsw_knn`/`ivfflat_knn`, and asserts top-k correctness vs a brute-force ground truth + the 22023 negative cases + REVOKE.

#### Why this step (action + reasoning)
1. **What this step does** — proves the SQL surface works end-to-end on the real container (the wiring-triad integration test).
2. **Why now** — `#[pg_test]`s do not run in CI (M18-M20 limitation); the container integration test is the always-on proof (blueprint Coverage Corner 1).

#### Evidence
M20's `benchmarks/tests/test_vector_ops.py` container-integration pattern; harness `db.py` connection + `load_vectors`.

#### Files to edit
```
benchmarks/tests/test_ann_index.py (NEW) — pytest integration: correctness + negative (22023) + REVOKE
```

#### Deep file dependency analysis
- New test; uses `psycopg2` + PG* env like `test_vector_ops.py`. Reuses `theodb_bench.recall.brute_force_ground_truth` for ground truth.

#### Deep Dives
- Build a deterministic corpus (numpy seeded), insert into a temp table, call the function, compare returned ids/distances to brute force (recall@k should be high at large ef_search/probes).
- Negative: bad metric / dim mismatch → assert `pgcode == '22023'`.
- REVOKE: assert `has_function_privilege('public', ..., 'execute') is False`.

#### Tasks
1. Fixture: connect, create temp table, load seeded corpus.
2. Test: hnsw_knn high recall vs brute force at ef_search=100.
3. Test: ivfflat_knn high recall at probes=16.
4. Test: 22023 negative cases.
5. Test: REVOKE from public.

#### TDD
```
RED: test_hnsw_knn_recall_high_vs_bruteforce — recall@10 ≥ 0.90 at ef_search=100
RED: test_ivfflat_knn_recall_high_vs_bruteforce — recall@10 ≥ 0.90 at probes=16
RED: test_knn_bad_metric_raises_22023 / test_knn_dim_mismatch_raises_22023
RED: test_knn_revoked_from_public
GREEN: (functions from T2.1 already implemented) — make the container green
VERIFY: pytest benchmarks/tests/test_ann_index.py -v (against the theo-db image)
```

#### Concurrency tests

(none — single-threaded) — the index build + search runs entirely in-memory within a single backend call; no shared mutable state, no threads. The optional rayon-style parallel build is explicitly out of scope (M22).

#### Acceptance Criteria
- [ ] All integration tests green — `pytest benchmarks/tests/test_ann_index.py` exits 0 against the container.
- [ ] Negative cases assert exact `22023` (psycopg2 `exc.pgcode == '22023'`).

#### DoD
- [ ] `pytest benchmarks/tests/test_ann_index.py` green on the built image.

## Failure scenarios (external I/O — Spi table reads)

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| `src_table` (Spi read) | table/column does not exist | call with a bogus `embed_col` | typed error surfaced (PG undefined_column / `err_input` 22023), no panic, no partial output |
| `src_table` rows | inconsistent vector dimension across rows | insert two rows of different dim | `err_input` 22023 ("inconsistent vector dimension"), fail-fast before search |
| `queries` arg | query dim ≠ corpus dim | pass a mismatched query vector | `err_input` 22023, no crash |
| `src_table` | empty table (0 rows) | call on an empty table | returns 0 rows (no panic); documented as valid |
| `src_table` rows | NULL vectors in `embed_col` | insert a row with NULL embedding | NULL rows SKIPPED (EC-1, pgvector semantics); no panic; other rows indexed |
| args | unbounded param (ef_construction/lists huge) | pass ef_construction=2_000_000 | capped → `err_input` 22023 (EC-2); no unbounded allocation/OOM |
| `id_col` | non-integer id column | `id_col` on a `text`/`uuid` column | `err_input` 22023 (EC-6) before cast; no raw Spi panic |
| identifier args | injection attempt via `embed_col`/`id_col` | pass `e; DROP TABLE` | rejected by identifier-allowlist validation (22023), no SQL executed |

---

## Phase 3: recall@k parity benchmark vs pgvector + docs

**Objective:** produce a reproducible benchmark (mean±std ≥3 runs) comparing own HNSW/IVFFlat recall@k against pgvector's indexes across an ef_search/probes sweep, gate parity, and record it in `docs/benchmarks/`.

### T3.1 — `bench_ann_index.py` + parity gate + benchmark doc

#### Objective
`benchmarks/bench_ann_index.py` builds a corpus, computes brute-force ground truth, measures recall@k for (a) pgvector `hnsw`/`ivfflat` indexes and (b) `theodb.hnsw_knn`/`ivfflat_knn`, across an ef_search/probes sweep over ≥3 runs, and writes `docs/benchmarks/m21-ann-index-parity.md`; a gate test asserts the tolerance band.

#### Why this step (action + reasoning)
1. **What this step does** — the measurement-first deliverable: the recall@k parity proof (DoD).
2. **Why now** — it is the milestone's acceptance metric (Goal); it reuses `theodb_bench.recall_at_k`/`brute_force_ground_truth` (Rule 9 — no harness rebuild; blueprint Coverage Corner 1).

#### Evidence
`benchmarks/theodb_bench/recall.py:41,61` (ground truth + recall@k); M20's `benchmarks/bench_vector_ops.py` + `docs/benchmarks/m20-vector-ops-parity.md` pattern; pgvector index DDL `CREATE INDEX … USING hnsw (embedding vector_l2_ops)`.

#### Files to edit
```
benchmarks/bench_ann_index.py (NEW) — parity (recall@k sweep) + perf (build/query latency mean±std ≥3 runs), --tolerance, --write-doc
benchmarks/tests/test_ann_index.py — add test_recall_parity_gate (own ≥ pgvector − tol across the sweep)
docs/benchmarks/m21-ann-index-parity.md (NEW) — recorded results + methodology + migration decision
```

#### Deep file dependency analysis
- `bench_ann_index.py`: imports `theodb_bench.recall` (`recall_at_k`, `brute_force_ground_truth`), `theodb_bench.db` (connection, load_vectors, build_index, query_topk). Calls own functions via SQL and pgvector via index.
- gate test: asserts `own_recall ≥ pg_recall − tol` at each sweep point; writes machine-readable JSON next to the doc.

#### Deep Dives
- Corpus: seeded synthetic (e.g., 5k × 64-d) + held-out queries (e.g., 100). Ground truth via `brute_force_ground_truth(corpus, queries, k=10, metric)`.
- pgvector arm: `CREATE INDEX … USING hnsw`, `SET hnsw.ef_search`, `query_topk` per query → distances → `recall_at_k`.
- own arm: `theodb.hnsw_knn(... ef_search=...)` batch → per-query distances → `recall_at_k`.
- Sweep: ef_search ∈ {10,40,100,200}; probes ∈ {1,8,16,32}. ≥3 runs → mean±std.
- Gate: tol=0.02 default; report PASS/FAIL per point + overall; honest "retain pgvector" verdict if FAIL (anti-sunk-cost).

#### Pseudo-code / Signatures
```python
def parity(con, corpus, queries, k, sweep, runs, tol) -> dict:
    gt_idx, gt_d = brute_force_ground_truth(corpus, queries, k, metric)
    for ef in sweep:
        own = [recall_at_k(gt_d, own_knn(con, ef), k) for _ in range(runs)]
        pg  = [recall_at_k(gt_d, pgvector_knn(con, ef), k) for _ in range(runs)]
        assert mean(own) >= mean(pg) - tol   # gate (or record FAIL honestly)
```

#### Tasks
1. Implement corpus/query generation (seeded) + ground truth.
2. Implement pgvector arm (build index, sweep ef_search/probes, recall).
3. Implement own arm (call theodb functions, same sweep).
4. Aggregate mean±std ≥3 runs; `--write-doc` writes the md + JSON.
5. Add `test_recall_parity_gate` asserting the tolerance band.

#### TDD
```
RED: test_recall_parity_gate — for HNSW and IVFFlat, own mean recall@10 ≥ pgvector mean − 0.02 at the matched sweep point (≥1 strong point each); FAIL records an honest "retain pgvector" verdict instead of a green lie
GREEN: implement bench_ann_index.py arms + aggregation
REFACTOR: dedupe the two arms behind a small runner; else none
VERIFY: python benchmarks/bench_ann_index.py --write-doc && pytest benchmarks/tests/test_ann_index.py::test_recall_parity_gate -v
```

#### Concurrency tests

(none — single-threaded) — the index build + search runs entirely in-memory within a single backend call; no shared mutable state, no threads. The optional rayon-style parallel build is explicitly out of scope (M22).

#### Acceptance Criteria
- [ ] `docs/benchmarks/m21-ann-index-parity.md` exists with recall sweep (mean±std ≥3 runs), methodology, exact repro commands, and the migration decision.
- [ ] `test_recall_parity_gate` green (parity reached) OR an explicit honest "retain pgvector" verdict recorded in the doc + an ADR (anti-sunk-cost) if not.
- [ ] Numbers carry units + methodology (`rules/analysis-golden-rule.md` rigor; `rules/public-copy.md`).

#### DoD
- [ ] Benchmark reproducible from a single command; doc + JSON written; gate test green or honest-fallback documented.

---

## Coverage Matrix

| # | Gap / Requirement | Task(s) | Resolution |
|---|---|---|---|
| 1 | Own HNSW builds + answers `<->`/`<=>`/`<#>` | T1.1, T2.1 | Rust HnswIndex + SQL `theodb.hnsw_knn` |
| 2 | Own IVFFlat builds + answers | T1.2, T2.1 | Rust IvfflatIndex + SQL `theodb.ivfflat_knn` |
| 3 | recall@k parity measured vs pgvector, reproducible in docs/benchmarks/ | T3.1 | `bench_ann_index.py` sweep + `docs/benchmarks/m21-ann-index-parity.md` + gate test |
| 4 | Anti-sunk-cost: honest ADR keeps pgvector if parity not reached | T3.1 (verdict), ADR D2/D3 | Gate records honest "retain pgvector" verdict on FAIL |
| 5 | Coexistence: pgvector/embed/hybrid/import untouched | T2.1 (ADR D4) | `theodb` schema funcs read via `::real[]`; no pgvector redefinition; REVOKE |
| 6 | Reuse M20 distance kernel (Rule 9) | T1.1, T1.2 (ADR D2) | `crate::vec::*` used by `ann.rs` |
| 7 | Boundary validation + typed errors (22023) | T2.1 (ADR D4), Failure scenarios | `err_input` on bad args/dim/identifier |
| 8 | Migration decision documented (DoD) | T3.1, ADR D1/D3 | Doc + ADRs record coexistence + SQL-form scope + M21b deferral |

**Coverage: 8/8 gaps covered (100%)**

## Global Definition of Done

- [ ] All phases completed
- [ ] All tests passing — `cargo pgrx test --features pg17` (Rust) + `pytest benchmarks/tests/test_ann_index.py` (container) green
- [ ] Zero lint warnings — `cargo clippy --release --features pg17 -- -D warnings`
- [ ] File-size budget respected (`ann.rs` ≤ 500 lines or split into `ann/`; `rules/architecture.md`)
- [ ] CHANGELOG.md updated under `[Unreleased] § Added`
- [ ] Backward compatibility preserved — pgvector `vector`, operators, indexes, and `theodb.embed/hybrid/import` unchanged (coexistence)
- [ ] Plan-specific: reproducible recall@k benchmark vs pgvector in `docs/benchmarks/m21-ann-index-parity.md` (mean±std ≥3 runs) + parity-gate test green OR honest "retain pgvector" ADR (anti-sunk-cost)
- [ ] Runtime-metric proof — the recall@k gate runs against the real container (not just compiles); own-vs-pgvector numbers observed
- [ ] Plan archived after `/review` READY_TO_MERGE + PR merged

## Final Phase: Integration Validation (MANDATORY)

**Objective:** validate the own ANN index works in a real container workload and the recall parity gate holds.

### Execution
```
cargo pgrx test --features pg17                      # Rust algorithm + SQL #[pg_test]
cargo clippy --release --features pg17 -- -D warnings # zero warnings
docker build / run the theo-db image                  # ship the new functions
pytest benchmarks/tests/test_ann_index.py -v          # integration + parity gate vs container
python benchmarks/bench_ann_index.py --write-doc      # reproducible benchmark + doc
```

### Acceptance Criteria
- [ ] Rust + container suites green — `cargo pgrx test --features pg17` exits 0 AND `pytest benchmarks/tests/test_ann_index.py` exits 0
- [ ] Zero clippy warnings — `cargo clippy --release --features pg17 -- -D warnings` exits 0
- [ ] Recall parity gate green — `pytest benchmarks/tests/test_ann_index.py::test_recall_parity_gate` exits 0 (own ≥ pgvector − tol) OR `docs/benchmarks/m21-ann-index-parity.md` records a FAIL verdict + an anti-sunk-cost ADR retaining pgvector
- [ ] Failure scenarios exercised — `pytest benchmarks/tests/test_ann_index.py` covers the 22023 negatives, empty table, and injection-rejection rows
- [ ] Benchmark doc written with methodology + repro commands

### If Validation Fails
1. Separate plan-caused failures from pre-existing.
2. Fix all plan-caused failures; re-run the chain.
3. If recall parity genuinely cannot be reached, record the honest anti-sunk-cost verdict (keep pgvector) — the milestone still ships the *measurement* (DoD), which is a PASS for M21, not a failure.
