# M31b — SIMD vector distance (AVX2+FMA): profile + latency

**Milestone:** M31b · **Date:** 2026-07-01 · **Plan:** `.claude/knowledge-base/plans/m31b-simd-distance-plan.md`

## Phase 0 — profile-before-optimize (the flamegraph/criterion tip)

Standalone micro-bench of the scan hot-loop compute (N=65 000 candidates, dim=128), portable build (SSE2 baseline,
matching the extension — no `target-cpu=native`), per query, mean over 60 iters:

| Component | Cost | Share |
|---|---:|---:|
| **decode** (page bytes → f32 scratch, `from_le_bytes`) | 1.96 ms | **45%** |
| **distance** (scalar l2) | 2.34 ms | **55%** |
| total hot-loop compute | 4.30 ms | 100% |

**Decisive finding:** the distance is only 55% of the compute — **AVX2 on the distance alone would NOT reach
≤ pgvector** (halving 55% ≈ 26% total win). The profile (thanks to the flamegraph tip — measurement-first applied
to the optimization) prevented mis-targeting the SIMD effort.

**Design (informed by the profile):** the AVX2 distance reads f32 DIRECTLY from the entry's page bytes via
`_mm256_loadu_ps` (unaligned load), **fusing decode + distance** into one SIMD pass — eliminating BOTH the 45%
decode and the 55% scalar distance (no scratch buffer). Repro: `/tmp/hotloop_profile.rs` (rustc -O --edition 2021).

## Latency — (filled by Phase 3)

_theodb_ivfflat Index Scan p50: before (M31 scalar) / after (M31b SIMD) / pgvector — n≥100k dim128._
