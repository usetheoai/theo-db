---
slug: m109-msbfs-operator
milestone_id: M109
date: 2026-07-16
cycle: implement
---

# M109 — Multi-Source BFS operator — implementation summary

## Outcome

Shipped `theodb.graph_expand_multi` (per-seed reachable sets), `graph_expand_multi_card` (per-seed
reach-count, traversal isolated) and `graph_expand_card` (single-source reach-count) — all in
`theodb_rs/src/graph.rs`, reusing the M108 persisted CSR + per-backend cache. The batched MS-BFS advances
up to 64 independent BFS lanes per CSR sweep via per-vertex `u64` source-masks (frontier-driven).

## The research-driven course-correction (the substance of M109)

1. **ROADMAP misframing corrected (ADR-1).** Deep research (council-research-adr + council-performance-simd,
   R0 web evidence: Then et al. VLDB'14, DuckPGQ CIDR'23/VLDB'23) proved "reuse `vec/ah.rs` kernels" is a
   category error: MS-BFS vectorizes the *source* dimension (bitwise-OR of masks, auto-vectorized `vpor`),
   orthogonal to `ah.rs`'s *candidate*-dimension int8-LUT `pshufb`. We reuse the *discipline* (dispatch +
   scalar-oracle + release-asserts), the kernel is new plain safe Rust.
2. **First benchmark showed a spurious LOSS (0.44–0.62×).** Honest investigation + a second research pass
   found the cause: the benchmark timed `count(*)` over ~1.28M returned node rows (64 lanes × ~20k) — it
   measured SQL row-streaming, not traversal, plus an intermediate-`Vec` double-materialization bug.
3. **Fixed: lazy iterator (no intermediate Vec) + traversal-only measurement** (`graph_expand_multi_card` /
   `graph_expand_card`, count-in-Rust, N rows out). The confound-free crossover sweep reverses the verdict.

## Measured result (`docs/benchmarks/m109-msbfs.{md,json}`)

Traversal-only, hub graph 40k/200k, ≤3 hops, mean±std over 3 runs, oracle PASS at every N:

| N | batched ms (±std) | seq count-in-Rust ms (±std) | pure_speedup | naive_speedup |
|---:|---:|---:|---:|---:|
| 1 | 1.63 ±0.08 | 2.77 ±0.81 | 1.70× | 2.71× |
| 16 | 8.49 ±0.59 | 61.40 ±8.18 | 7.24× | 19.16× |
| 64 | 33.04 ±1.26 | 213.75 ±14.78 | **6.47×** | 12.60× |
| 256 | 145.23 ±9.62 | 723.22 ±12.11 | 4.98× | 10.41× |
| 512 | 183.06 ±14.85 | 1430.11 ±5.39 | 7.81× | 17.09× |

Crossover N=1; pure_speedup ~5–8× across N≥16 (Then et al. edge-sharing, confirmed empirically). **Topology
floor:** a uniform-random graph at N=64 gives ~10.2× — the win is robust across topologies, not hub-gamed.

## DoD verification

| DoD (ROADMAP M109) | Status | Evidence |
|---|---|---|
| (1) MS-BFS own-code, reusing `vec/ah.rs` | ✅ (scope corrected, ADR-1) | `Csr::expand_multi`; reuses ah.rs *discipline*, own bitwise kernel |
| (2) bounded ≤H semantics == theo-rag, set-hash (not count+sum) | ✅ | per-lane set-hash oracle `m109_expand_multi_matches_expand_per_lane` (H=1,2,3) |
| (3) benchmark N-seeds | ✅ | crossover sweep N=1..512, mean of 3, `docs/benchmarks/m109-msbfs` |
| (4) integration with M108 AM | ✅ | reuses `load_cached_csr` (M108 cache), same catalog |
| GATE: oracle set-hash + throughput measured + N-seeds gain quantified | ✅ | oracle PASS every N; pure_speedup 6–7×; regression gate `>2×` |

## Tests (7 M109, all GREEN; 337 total, 0 regression)

`m109_expand_multi_matches_expand_per_lane`, `m109_expand_multi_multiseed_lane`,
`m109_expand_multi_lanes_independent`, `m109_expand_multi_tiling_65_sets`,
`m109_expand_multi_without_build_errors`, `m109_expand_multi_length_mismatch_errors`,
`m109_bench_crossover_sweep`.

## Honest boundary

- Speedup is topology-dependent (comes from lanes sharing hub traversals).
- For the pure UNION neighborhood (all seeds → one set), M108 `expand(all_seeds)` already suffices (HippoRAG
  joint-neighborhood). M109 is the faster primitive when *per-seed* separation is needed (per-entity signals,
  future per-seed PPR / M112). This is documented, not hidden.
