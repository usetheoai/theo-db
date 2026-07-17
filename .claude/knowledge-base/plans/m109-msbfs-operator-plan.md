---
slug: m109-msbfs-operator
milestone_id: M109
created_at: 2026-07-16
goal: Ship a batched multi-source BFS operator `theodb.graph_expand_multi` whose per-seed-set reachable set is byte-identical (set-hash) to single-source `expand`, with a measured N-seeds-parallel vs N-sequential throughput benchmark in docs/benchmarks/.
---

# M109 — Vectorized Multi-Source BFS operator

## Goal

Ship `theodb.graph_expand_multi(edge_rel, seed_sets, max_hops)` — a batched multi-source BFS over the
persisted CSR — whose per-seed-set reachable set is byte-identical (per-lane set-hash) to looping
single-source `expand`, with a measured N-seeds-parallel-vs-N-sequential throughput benchmark
persisted to `docs/benchmarks/m109-msbfs.{md,json}`.

## Context

Consumes the M109 discovery blueprint (`knowledge-base/discoveries/blueprints/m109-msbfs-operator-blueprint.md`,
verdict SHIPPABLE_WITH_CAVEATS). Builds on M108's persisted CSR (`theodb.graph_csr` catalog + `CSR_CACHE`
+ `Csr::expand`). Decisions resolved in discovery: bitwise-OR source-parallel core (auto-vectorized plain
Rust, NOT `ah.rs` pshufb — ADR-1); scalar u64-mask W=64 v1 (ADR-2); distinct batched operator, `expand`
unchanged (ADR-3); top-down ANP only (ADR-4). SOTA: Then et al. VLDB'14, DuckPGQ `iterativelength.cpp` (MIT).

## Baseline Context

### Files that will be touched
| File | LoC today | Last touch | Why it exists |
|---|---|---|---|
| `theodb_rs/src/graph.rs` | 471 | 6bc4d8b 2026-07-16 | M108 persisted CSR + `expand`; M109 adds `Csr::expand_multi` + `graph_expand_multi` pg_extern + wrapper + tests |
| `docs/benchmarks/m109-msbfs.md` | 0 (NEW) | — | measured N-seeds benchmark artifact (Rule 5) |
| `docs/benchmarks/m109-msbfs.json` | 0 (NEW) | — | raw numbers for the benchmark |
| `CHANGELOG.md` | — | — | Unbreakable Rule 6 `[Unreleased] § Added` |

### Current callers / symbols
- `Csr` struct + `Csr::expand` (`graph.rs:45,91`) — internal; called by `graph_expand` pg_extern (`graph.rs` theodb_rs mod). `expand_multi` is a sibling method; `expand` is NOT modified (ADR-3).
- `CSR_CACHE` (`graph.rs:22`) — `graph_expand_multi` resolves the cached `Rc<Csr>` exactly as `graph_expand` does (same oid+built_at epoch cheap resolve).
- Set-hash oracle `bit_xor(hashint8(node))` (`graph.rs:425`) — reused per-lane in tests.

### Glossary
- **seed-set / lane** — one independent BFS source-set; bit `i` in a `u64` mask = "lane i reached this vertex".
- **W** — batch width = 64 (one u64) for v1. Tiling loops 64-lane tiles when > 64 seed-sets.
- **ANP** — Aggregated Neighbor Processing: Pass-1 pure-OR push, Pass-2 single `seen` sweep (Then §4.1.1).
- **reachable set** — nodes within ≤ max_hops of any seed in the lane's set (undirected), theo-rag semantics.

### Architecture boundaries
- All in `theodb_rs/src/graph.rs` (the graph module). No new module, no cross-layer import. pgrx
  `#[pg_extern]` in the `theodb_rs` schema-mod + `theodb.*` SQL wrapper (M108 pattern). No new crate.

## Prior Art & Related Work
- Internal: M108 blueprint + `graph.rs` (the CSR + `expand` this extends); set-hash oracle discipline.
- External (cited in blueprint): Then et al. VLDB'14 (MS-BFS + ANP); DuckPGQ `iterativelength.cpp` MIT
  (the exact double-buffered bitset frontier to mirror); NetworkX `bfs_layers` BSD (correctness ref).

## ADRs
Inherited verbatim from the blueprint (ADR-1..4). Summary + alternatives:
- **ADR-1** MS-BFS core = auto-vectorized plain Rust bitwise-OR, not `ah.rs` pshufb. *Rejected:* bending
  the int8-LUT kernel into reachability-OR (wrong mechanism; DuckPGQ VLDB'23 confirms no intrinsics).
- **ADR-2** scalar u64 W=64 v1; widen to `[u64;K]` only on measured lane-starvation. *Rejected:* v1
  hand-rolled AVX-512 (premature; MS-BFS is memory-bound, gather dominates — measure first, per `ah.rs` L269).
- **ADR-3** distinct batched `graph_expand_multi`; `expand` unchanged. *Rejected:* replacing `expand`
  (regresses single-seed path — MS-BFS collapses to ~1× at one source).
- **ADR-4** top-down ANP only. *Rejected:* Beamer direction-optimizing (frontier never saturates at
  bounded ≤H + few seeds; DuckPGQ ships uni-directional and wins).

## Dependency Graph
Phase 1 (kernel + oracle) → Phase 2 (pg surface + tiling) → Phase 3 (benchmark) → Phase 4 (integration validation).
Phase 3 depends on Phase 2 (needs the SQL surface). Phase 1 tests are pure-Rust/pg_test on `Csr`.

## Phase 1 — `Csr::expand_multi` kernel + per-lane set-hash oracle

### Task T1.1 — `Csr::expand_multi(seed_sets: &[Vec<i64>], max_hops) -> Vec<Vec<i64>>`
#### Why this step
Action: implement the top-down ANP MS-BFS over ≤64 lanes as a pure method on `Csr`, returning one
reachable-node `Vec<i64>` per input seed-set. Reasoning: this is the traversal primitive (ADR-3); a pure
method is unit-testable without SQL and lets the oracle compare lane-for-lane against `expand` (ADR-1
mechanism = plain bitwise-OR, ADR-2 scalar u64).
#### Files to edit
- `theodb_rs/src/graph.rs` (add `impl Csr { fn expand_multi }`; ≤ +90 LoC, keeps file < 600)
#### Deep file dependency analysis
Reads `offsets`/`adj` (immutable). Allocates 3 × `Vec<u64>` len nnodes (visit/visit_next/seen). Skips
seeds ≥ nnodes (as `expand`, `graph.rs:100`). No change to `expand` or serialization.
#### TDD
- `test m109_expand_multi_matches_expand_per_lane`: build the M108 test graph; pick K=5 distinct
  single-seed sets; assert `expand_multi(sets, H)[i]` as a **sorted set** == `expand(sets[i], H)` sorted,
  for H=1,2,3. (Given a CSR and K seed-sets, When expand_multi runs, Then each lane's reachable set
  equals the single-source expand.)
- `test m109_expand_multi_multiseed_lane`: a lane with 2 seeds == union of the two single-seed expands.
- `test m109_expand_multi_seed_out_of_range_skipped`: seed ≥ nnodes contributes nothing, no panic.
- `test m109_expand_multi_empty_lane`: empty seed-set lane → empty result, other lanes unaffected.
#### Concurrency tests
(none — single-threaded; `expand_multi` owns its 3 mask Vecs, no shared state — per blueprint risk-a analysis).
#### Acceptance criteria
- All T1.1 tests GREEN. `expand` byte-identical (unchanged). File < 600 LoC.

### Task T1.2 — tiling for > 64 seed-sets
#### Why this step
Action: when `seed_sets.len() > 64`, loop 64-lane tiles reusing the same `Csr`. Reasoning: W=64 is the
v1 mask width (ADR-2); tiling keeps memory O(nnodes) per tile (blueprint risk-b) and correctness must
hold across the tile boundary.
#### Files to edit
- `theodb_rs/src/graph.rs` (tiling loop inside/around `expand_multi`)
#### Deep file dependency analysis
Outer chunk over `seed_sets.chunks(64)`; each tile writes into the correct global result index via a
`lane_to_set` offset. Same cached CSR (no re-deserialize).
#### TDD
- `test m109_expand_multi_tiling_65_sets`: 65 seed-sets (forces 2 tiles); every lane's set == its
  single-source `expand`. Catches tile-boundary index bugs.
#### Concurrency tests
(none — single-threaded).
#### Acceptance criteria
- 65-set tiling test GREEN; result vector length == input length; lane→set mapping correct.

## Phase 2 — pg surface `graph_expand_multi` + wrapper

### Task T2.1 — `#[pg_extern] graph_expand_multi` + `theodb.graph_expand_multi` wrapper
#### Why this step
Action: expose the primitive as SQL: resolve cached `Rc<Csr>` (M108 path), call `expand_multi`, return
`SETOF (set_id int, node bigint)`. Reasoning: the operator must be callable from SQL for the benchmark
and for theo-rag (ADR-3 surface); reuse M108's oid+built_at resolve + REVOKE-from-PUBLIC.
#### Files to edit
- `theodb_rs/src/graph.rs` (pg_extern in `theodb_rs` mod + `extension_sql!` wrapper + REVOKE)
#### Deep file dependency analysis
Input `seed_sets bigint[][]` → `Vec<Vec<i64>>`. Reuses the exact cache-resolve block from `graph_expand`.
`SetOfIterator<(i32, i64)>`. Wrapper `theodb.graph_expand_multi` calls `theodb_rs.graph_expand_multi`.
#### TDD (pg_test)
- `test m109_graph_expand_multi_sql_matches_expand`: `graph_build` then compare, per set_id, the SQL
  `graph_expand_multi` reachable set (aggregated with `bit_xor(hashint8(node))`) against per-seed
  `graph_expand` set-hash. Set-hash oracle, NOT count (M108 discipline).
- `test m109_graph_expand_multi_without_build_errors`: no persisted CSR → typed error (M108 parity).
#### Concurrency tests
(none — single-threaded; per-backend cache, same as M108).
#### Acceptance criteria
- SQL surface returns correct per-set reachable sets (set-hash == `graph_expand`); REVOKE'd from PUBLIC.

## Phase 3 — Benchmark (measurement gate)

### Task T3.1 — N-seeds-parallel MS-BFS vs N-sequential `expand` benchmark
#### Why this step
Action: measure, on a realistic graph, batched `graph_expand_multi(N seeds)` wall-clock vs looping
`graph_expand` N times; ≥3 runs mean±std; write `docs/benchmarks/m109-msbfs.{md,json}`. Reasoning: the
ROADMAP GATE + ADR-3 — the batched speedup is UNBENCHMARKED and no claim is allowed without the artifact
(Rule 5). Honest-negative is an acceptable, informative outcome.
#### Files to edit
- `theodb_rs/src/graph.rs` (a `#[pg_test] m109_bench_msbfs_vs_sequential` writing the artifact)
- `docs/benchmarks/m109-msbfs.md`, `docs/benchmarks/m109-msbfs.json` (NEW)
#### Deep file dependency analysis
Reuses the M108 bench graph builder (hub topology). Runs N=64 seed-sets; times batched vs sequential;
computes speedup; **set-hash oracle asserts identical reachable sets** before reporting any timing.
#### TDD
- `test m109_bench_msbfs_vs_sequential`: oracle PASS (per-lane set-hash batched == sequential) is the
  hard assertion; timing is recorded. Test fails if the sets diverge (correctness gates the number).
#### Failure scenarios
- SPI/DB read of the persisted CSR bytea fails → propagate M108's typed error (no silent empty result).
#### Concurrency tests
(none — single-threaded).
#### Acceptance criteria
- Benchmark artifact written with mean±std over ≥3 runs; oracle PASS; honest speedup number (may be
  ≥1× or ~1× — reported as measured, not spun).

## Phase 4 — Integration Validation
- `cargo pgrx test pg17` full suite GREEN (0 regression vs 330).
- All M109 tests GREEN (T1.1–T3.1).
- Benchmark artifact present + oracle PASS.
- CHANGELOG `[Unreleased] § Added` updated.
- `expand` (single-source, M108) unchanged — byte-identical behavior.

## Coverage Matrix
| Goal claim | Task(s) |
|---|---|
| batched `graph_expand_multi` operator | T1.1, T2.1 |
| per-seed-set reachable set == single-source `expand` (set-hash) | T1.1, T1.2, T2.1 |
| tiling correctness > W seeds | T1.2 |
| measured N-seeds throughput benchmark in docs/benchmarks/ | T3.1 |
| bounded ≤H theo-rag semantics | T1.1 (H=1,2,3 differential) |
| integration with M108 AM (cached CSR) | T2.1, Phase 4 |

## Drawbacks & Risks
| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Bitset word-boundary / tail-padding bug → phantom source in a lane | HIGH | per-lane set-hash oracle (RED first); W=64 exact-fit u64 avoids tail padding | impl |
| Batched speedup ~1× in GraphRAG regime (few seed-sets) → operator not justified | MEDIUM | measurement gate reports honest number; ADR-3 documents the UNBENCHMARKED status; honest-negative is acceptable and informs theo-rag batching | impl |
| Dense-graph frontier `O(E·W/64)` edge work at saturation | MEDIUM | W capped at 64/tile; max_hops bounded; measure frontier-per-hop before optimizing (bottom-up deferred, ADR-4) | impl |

## Unresolved Questions
- Does theo-rag actually batch dozens of independent seed-sets per retrieval call? Unknown at plan time;
  the benchmark (T3.1) quantifies the payoff regime so theo-rag integration (M110/M111) can decide. This
  is exactly what the measurement gate resolves — building the primitive + benchmark is the honest arbiter.

## Global DoD
- TDD RED→GREEN→REFACTOR per task; set-hash oracle (not count) as the correctness gate.
- File `graph.rs` < 600 LoC; no new crate (parsimony rung 4).
- Full `cargo pgrx test pg17` GREEN, 0 regression.
- Benchmark artifact in `docs/benchmarks/` (Rule 5) with mean±std ≥3 runs + oracle PASS.
- CHANGELOG updated; commits without Co-Authored-By trailer; work on develop.
