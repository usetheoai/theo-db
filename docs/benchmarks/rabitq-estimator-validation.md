# Extended multi-bit RaBitQ — own-code estimator validation (E1 core)

**Date:** 2026-07-17 · **Module:** `theodb_rs/src/vec/rabitq.rs` (own-code, arXiv:2409.09913 algorithm, Apache-2.0
reimplemented — the RaBitQ vendored tree was deleted ADR-0046). **Method:** hermetic Monte-Carlo (droplet-free,
`rustc --test`): 200 random 128-d data vectors × 40 random queries, estimate ‖q−x‖² from the f32-FREE code
(integer-weighted dot `⟨q_r,u⟩/W`, no raw vector touched) vs the true distance, per bit-depth.

## Measured estimator error vs bit-depth

| bits/dim | mean relative error | mean signed bias |
|---:|---:|---:|
| 1 | 7.16% | 0.94% |
| 3 | 1.91% | ~0.0001 |
| 5 | 0.38% | ~-0.0001 |
| **7** | **0.09%** | **0.00%** |

## Findings

1. **Monotone tightening + unbiased** — error falls 7.16% → 0.09% as bits 1→7; bias ≈ 0 at every depth ≥ 3 (the
   RaBitQ ratio-estimator property under a random orthogonal rotation). Confirmed by
   `rabitq_estimator_unbiased_and_tightens_with_bits`.
2. **7-bit is f32-free-capable** — 0.09% relative error, zero bias → accurate enough to be the FINAL ranking with
   NO raw-vector rerank. This is the load-bearing claim (paper: 5–7 bit → 95/99% recall without reranking) and it
   is now VALIDATED own-code.
3. **1-bit explains the M74 negative** — 7.16% error is too coarse for recall 0.99, which is exactly why the M74
   1-bit RaBitQ spike (ADR-0036) required an f32 rerank that ate the win. Multi-bit (5–7) removes that need — a
   different algorithm, the un-measured lever.

## Status
Quantizer CREATED + math-VALIDATED (this note). NOT yet wired into the AM scan. Next (E1, needs droplet + SIFT):
dedicated rerank-code page → f32-free Stage-2 in `scan_ivf_aq_split` → SIFT1M same-data A/B vs v5 (gate: ≥2× QPS
@ 0.99 with measured f32-buffer-access drop). Blueprint: `.claude/knowledge-base/discoveries/blueprints/vec-f32free-rerank-blueprint.md`.
