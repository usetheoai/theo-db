# M43 — theodb_hnsw build-time optimization: ~2.2–2.9× via SIMD build distance, recall parity

**Date:** 2026-07-03
**Verdict:** **WIN** — the `theodb_hnsw` graph build is **~2.2× faster** (rigorous 3-sample A/B @ 200k, std bands
separated) at **identical recall**, and **8.4 min @ 1M** (down from 24 min, ~2.9× vs the M42 baseline). Closes the
build-time weakness M42 exposed.
**Type:** A/B benchmark (build-time + recall parity — the honest oracle). Recall is PARITY, not byte-identical
(SIMD FMA rounds differently → the graph may differ in a few near-tie selections; measured identical at the tested
scales).

## What changed

The `theodb_hnsw` in-memory graph build (`ann/hnsw.rs`) computed L2 distance via the **scalar** `crate::vec::l2_distance`
(`a.iter().zip(b).map(|(x,y)|(x-y)²).sum()`) — billions of scalar 128-dim distances (N × ef_construction × degree),
while the SCAN already used the **SIMD** AVX2+FMA kernel. M43 adds `crate::vec::l2_distance_simd` (reuses the M31b
kernel `simd_x86::l2_sq` via a zero-copy f32→bytes reinterpret) and routes the build's L2 distance to it
(`Metric::dist_simd`). This also **aligns** the build's metric with the scan's (both SIMD → consistent; the graph
was previously built with scalar distance but searched with SIMD).

`l2_distance` (the pgvector-parity function used by operators / scan rerank / knn) is **unchanged** — only the
approximate graph build/search uses the SIMD variant.

## A/B build-time @ 200k (rigorous — 3 alternating samples, mean±std, SIFT subset, exact GT)

| build | mean±std | recall@10 |
|---|---|---|
| m41 scalar build | 200.1 ± 23.2 s | 0.9825 |
| **m43 SIMD build** | **90.8 ± 2.7 s** | **0.9825** |

**Build speedup: 2.20×** — std bands widely separated (200±23 vs 91±3 → decisively significant). **Recall
IDENTICAL** (0.9825 = 0.9825, Δ +0.0000) — the SIMD-vs-scalar ULP differences did not flip any recall@10 selection
at this scale. The m43 build is also more *consistent* (±2.7 s vs ±23.2 s). Query QPS is unchanged within variance
(the scan code is identical between m41 and m43 — only the build changed).

## Build-time @ 1M (target scale)

| build | wall-clock | recall@10 |
|---|---|---|
| M42 baseline (scalar, run_m32_sift1m harness) | 1440 s (24 min) | ~0.96 |
| **m43 SIMD build** | **503.7 s (8.4 min)** | **0.9725** |

**~2.86×** at 1M — directionally consistent with (and slightly above) the controlled 200k 2.20×, plausibly because
the distance fraction grows with graph density at scale. Honest caveat: the 1M baseline (1440 s) was measured in a
prior run via the full harness (different thermal/measurement conditions), so the 200k **2.20×** is the rigorous
controlled number; the 1M figure confirms the build dropped from 24 min → 8.4 min at recall parity (0.9725 ≈ M42's
0.96, the small gap being id-overlap vs distance-threshold recall methodology).

## Correctness gate (recall parity)

- `benchmarks/tests/test_index_am.py` — **8/8 pass** on `theo-db:m43` (incl. `test_hnsw_am_persists_pushes_down_and_recalls`).
- Recall @ 200k IDENTICAL (0.9825); @ 1M 0.9725 (parity with M42). The graph is internally consistent (the
  `neighbor_slice_matches_in_memory_graph_every_layer` pg_test still holds — build + persistence use the same graph).

## Safety (unsafe reinterpret cast)

`l2_distance_simd` does `std::slice::from_raw_parts(b.as_ptr() as *const u8, b.len()*4)` — a read-only reinterpret
of an f32 slice as its own LE bytes (an f32 slice IS its bytes on x86_64 LE), living only for the
`l2_dist_from_bytes` call, which re-asserts `raw.len() == a.len()*4`. Bounds are enforced by `check_dims` + the
assert. No new allocation, no aliasing (read-only), no lifetime escape.

Reproduce: alternate a 200k SIFT-subset `CREATE INDEX ... USING theodb_hnsw` on `theo-db:m41` (scalar) vs
`theo-db:m43` (SIMD), ≥3 samples each, compare mean±std + recall@10. Dataset `benchmarks/.datasets/sift-128-euclidean.hdf5`.

## Honest bottom line

A real, recall-preserving **~2.2× (controlled) to ~2.9× (@1M)** build-time win — the theodb_hnsw build drops from
24 min to 8.4 min at 1M, narrowing the last gap the SIFT1M carrier verdict (M42) exposed. Combined with M41 (scan)
and M42 (real-data superiority signal), the theodb_hnsw carrier is now competitive on build, scan, AND recall×QPS.

## Next (evidence-based)

- Cosine/Ip build distance still scalar (no SIMD kernel yet) — the persisted AM is L2-only (ADR 0010), so
  build-inert today; add if a cosine AM ships.
- Build is still single-thread (ADR); parallel build is a separate, larger lever (YAGNI until needed).
