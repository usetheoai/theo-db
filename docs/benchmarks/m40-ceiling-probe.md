# M40 — Ceiling probe: is the recall gap the quantizer or the carrier? (measurement-first)

**Date:** 2026-07-03
**Verdict:** the recall ceiling is the **IVFFlat carrier (probes)**, NOT the quantizer → **anisotropic quantization
loss (the proposed M40) targets the wrong bottleneck in our f32-rerank pipeline.** Re-scope before building.
**Type:** discover-phase measurement (cycle-discover); falsifies the M40 premise before any implementation.

## Why this probe ran

M39 concluded PQ ≈ SBQ at recall 0.77 and named "ScaNN anisotropic loss" as the next recall lever. Anisotropic
loss improves the **quantizer's** ranking. Before building it, measurement-first asks: **in our pipeline (IVFFlat
carrier → quantized rank → exact f32 rerank), is the recall actually limited by the quantizer, or by the
carrier?** If the carrier (candidate generation) is the ceiling, a better quantizer cannot help.

## Measurement (n=2000, dim=64, m=8, bits=4, nq=100, runs=2; `benchmarks/run_m39_pq.py`)

| config | probes | over_fetch | PQ recall@10 | SBQ recall@10 |
|---|---|---|---|---|
| baseline | 16 | 16 | 0.770 | 0.769 |
| more_probes | 44 | 16 | **0.944** | 0.943 |
| max_probes | 100 | 16 | 0.944 | 0.943 |
| more_overfetch | 16 | 64 | 0.787 | 0.787 |
| both_max | 100 | 64 | **1.000** | 0.996 |

## Findings (honest)

1. **Probes (carrier) dominate recall.** 16→44 probes jumps recall +17 points (0.77→0.944). More over_fetch alone
   (16→64) barely moves it (+1.7 points). The candidate-generation stage — not the quantizer — is the ceiling.
2. **PQ and SBQ are indistinguishable at every operating point** (0.770/0.769, 0.944/0.943, 1.000/0.996). The
   quantizer choice does not change recall — because the exact f32 rerank equalizes any quantizer ranking as long
   as the true neighbors are in the retrieved candidate set (the carrier's job).
3. **At full probing both reach ~1.0.** The recall gap vs f32 that M39 measured (0.77) was an artifact of low
   probes (16), not a fundamental quantizer limit.

## Architectural conclusion

In a pipeline with an exact f32 rerank, the quantizer only decides which candidates survive the `k·over_fetch`
truncation; the rerank corrects the ranking. So **quantizer ranking quality (what anisotropic loss improves) is
not the recall bottleneck — carrier candidate quality (probes / index recall) is.** This holds regardless of
dataset: the f32 rerank is the equalizer. Anisotropic loss would matter only in a **no-rerank** regime
(memory-constrained, quantized distance is final) — which is not our pipeline (f32 always available via
coexistence).

## Honest caveat

This probe uses a random-gaussian corpus (no cluster structure). On structured real data (SIFT), a quantizer's
ranking could matter marginally more. But the load-bearing finding — probes move recall ~20 points while the
quantizer choice moves ~0, because of the f32 rerank — is architectural, not dataset-specific.

## Recommendation (re-scope)

Do **not** build anisotropic quantization loss (M40 as specified) — it optimizes the wrong component. The real
recall/QPS lever is the **carrier**: candidate generation quality at a given probe budget. Concretely:
- The `theodb_hnsw` graph AM (M35) already does better candidate generation than IVFFlat probes — the head-to-head
  recall×QPS of `theodb_hnsw` vs `theodb_ivfflat` at matched QPS is the real vector-superiority question.
- OR tune the IVFFlat `probes`/`lists` recall×QPS curve honestly (more probes = higher recall, lower QPS).

The vector pillar's recall is **not** quantizer-limited; it is carrier/probe-limited. That is where the P0
(recall×QPS superiority vs AlloyDB/ScaNN) must be fought.
