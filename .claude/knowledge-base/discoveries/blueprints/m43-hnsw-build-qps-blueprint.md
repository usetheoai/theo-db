# Blueprint: theodb_hnsw build-time optimization (M43)

**Slug:** `m43-hnsw-build`
**milestone_id:** M43
**Created:** 2026-07-03
**Rigor:** measurement-first; oracle = A/B build-time + recall PARITY (the graph changes slightly under SIMD
rounding, so the gate is parity within tolerance, NOT byte-identical). Baseline = M42 (build 1440 s @ 1M).

## Context

M42 (SIFT1M carrier verdict) proved `theodb_hnsw`'s SCAN wins on real data, but exposed its weakness: **build is
1440 s (24 min) at 1M** vs pgvector hnsw 473 s and theodb_ivfflat 86 s. M41 optimized the SCAN, not the BUILD.
This closes the build-time gap.

## Coverage Corner 1 — Integration Tests

The build is exercised by `benchmarks/tests/test_index_am.py` (8 tests, incl. `test_hnsw_am_persists_pushes_down_and_recalls`)
and the `ann/hnsw.rs` `#[pg_test]` (`neighbor_slice_matches_in_memory_graph_every_layer`). Correctness gate: those
tests stay green AND recall on a SIFT subset is PARITY (within tolerance) before/after — the graph is approximate,
so SIMD-vs-scalar ULP differences may flip a few near-tie neighbor selections; recall must not regress.

## Coverage Corner 2 — Dependencies

None new. Reuse the existing M31b SIMD kernel `crate::vec::simd_x86::l2_sq` (AVX2+FMA) via a byte-reinterpret cast
of the `&[f32]` operand — zero new dependency (parsimony rung 4).

## Coverage Corner 3 — Tools

A/B: build `theodb_hnsw` over a SIFT subset (200k for a fast ~1-2 min build; then 1M to confirm) on the baseline
(`theo-db:m41`, scalar build) vs the optimized build, comparing build wall-clock + recall@10. ≥3 build samples
mean±std (build-time has less run-to-run variance than QPS, but still sample it).

## Coverage Corner 4 — Techniques

### The bottleneck (from `theodb_rs/src/ann/hnsw.rs` + `vec.rs`)

The build (`HnswIndex::build` → `insert` → `search_layer`/`select_from`) computes distance via `self.metric.dist`
at `ann/hnsw.rs:88,107,114,136,155,181` — all routing to `crate::vec::l2_distance` (`vec.rs:35`), which is a
**scalar** `a.iter().zip(b).map(|(x,y)|(x-y)*(x-y)).sum()`. Meanwhile the SCAN uses the **SIMD** `l2_dist_from_bytes`
(`vec.rs:167` → `simd_x86::l2_sq`, AVX2+FMA 8-lane). So the build does billions of SCALAR 128-dim distance
computations (N × ef_construction × avg_degree) while the equivalent scan work is vectorized. The build is
dominated by `search_layer` (almost entirely distance + heap ops).

### The fix (parsimony rung 4 — reuse the existing SIMD kernel)

Add `crate::vec::l2_distance_simd(a: &[f32], b: &[f32]) -> f64` that reuses `simd_x86::l2_sq(a, <b as bytes>)`
(the same AVX2+FMA core the scan uses), with the scalar `l2_distance` as the fallback (non-x86 / no-AVX). Route the
`theodb_hnsw` BUILD's L2 distance to it (the persisted AM is L2-only per ADR 0010, so the build is always L2).

**Do NOT change `l2_distance`** (it is the pgvector-parity function used by the operators + scan rerank + knn; its
exact-parity oracle tests must stay exact). The new `l2_distance_simd` is a separate, build-only path.

### Correctness: parity, not byte-identical

SIMD FMA (`d*d+acc` one rounding, 8 parallel lanes horizontally summed) differs from scalar (two roundings,
sequential sum) in the last ULPs. On near-tie neighbor selections this can flip a choice → a slightly different
graph. So recall is PARITY (measured), not identical. Bonus consistency argument: today the graph is BUILT with
scalar distance but SEARCHED with SIMD — aligning both to SIMD is arguably more correct (build/scan use the same
metric), and may even improve recall marginally.

## Cross-cutting Comparison

| Aspect | scan (fast) | build (slow, today) | M43 fix |
|---|---|---|---|
| distance kernel | SIMD `l2_dist_from_bytes` | scalar `l2_distance` | SIMD `l2_distance_simd` (reuse kernel) |
| operand | `&[f32]` × `&[u8]` (page) | `&[f32]` × `&[f32]` | `&[f32]` × `&[f32]` (byte-cast to reuse kernel) |
| build/scan metric consistency | — | INCONSISTENT (scalar build, SIMD scan) | CONSISTENT (both SIMD) |

## ADRs

### D1 — SIMD the build distance via a new build-only `l2_distance_simd`; do NOT touch `l2_distance`

**Decision:** Add `l2_distance_simd` reusing `simd_x86::l2_sq`; route only the `theodb_hnsw` build to it. Keep
`l2_distance` (pgvector-parity) untouched.

**Rationale:** the build is the slow path and is approximate (a slightly different graph is fine if recall holds);
the operators/scan-rerank need exact pgvector parity and must not change. Separating the two keeps the parity
contract intact while speeding the build.

**Alternatives considered:** (a) make `l2_distance` itself SIMD — rejected (changes the pgvector-parity function +
recall of every path, high blast radius); (b) reduce ef_construction — rejected (trades recall for build-time; the
M42 recall is a hard-won asset); (c) parallel build — rejected (the in-proc build is single-thread by ADR; adding
threads is a bigger, riskier change — YAGNI until SIMD is measured).

### D2 — Benchmark-gated (measurement-first)

**Decision:** merges only if the A/B shows build-time drops meaningfully at recall parity (recall within tolerance,
e.g. ±0.01, on a SIFT subset). If the build-time win is marginal (distance wasn't the bottleneck — the M36 risk) or
recall regresses, revert honestly.

## Recommendations for the project

1. Add `l2_distance_simd` (reuse the kernel); route the theodb_hnsw build L2 distance to it.
2. Keep the AM tests green + measure recall parity on a SIFT subset.
3. A/B build-time at 200k (fast) then 1M; record honestly.

## Blocked questions (if any)

(none — the scalar-vs-SIMD build distance is clear from the code; the A/B build-time + recall parity is the oracle.)

## Related

- Baseline: `docs/benchmarks/sift1m-carrier-verdict.md` (build 1440 s @ 1M)
- Hot path: `theodb_rs/src/ann/hnsw.rs:88,107,114,136,155,181` (build distance); `theodb_rs/src/vec.rs:35` (scalar l2_distance)
- SIMD kernel to reuse: `theodb_rs/src/vec.rs:96` (`simd_x86`), `:167` (`l2_dist_from_bytes`)
