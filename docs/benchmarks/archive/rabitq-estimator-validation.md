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

## SIFT1M recall — the E1 make-or-break, MEASURED on the real benchmark (2026-07-17)

Standalone harness on the real SIFT1M (ann-benchmarks `sift-128-euclidean`, 1M base × 128, 300 queries, official
ground-truth). Candidate pool = exact top-200 (isolates rerank quality); rerank the pool by (a) exact f32
[ceiling] vs (b) RaBitQ f32-free code. Two conditions: c=0 (full-vector residual, PESSIMISTIC) and IVF residuals
(the real AM condition: r = x − nearest-of-1000-k-means-centroids).

| bits/dim | recall@10 (c=0, pessimistic) | recall@10 (IVF residuals — real AM) | rerank bytes/cand |
|---:|---:|---:|---:|
| 1 | 0.046 | — | 24 |
| 5 | 0.876 | **0.951** | 88 |
| **7** | 0.969 | **0.988** | 120 |
| exact (ceiling) | 0.9997 | 0.9997 | 512 (f32) |

**Verdict: E1's core hypothesis is MEASURED-VALIDATED.** RaBitQ-7bit residual rerank = **0.988 recall@10,
f32-FREE** (no raw vector touched), ~1% below the exact ceiling — near-lossless, and the residual condition
(real AM) lifts it +1.9 pts over the pessimistic full-vector case (a better-trained k-means than this 50k-sample
12-iter Lloyd would tighten further). Rerank cost **120 B/cand vs 512 (f32) = 4.3× fewer bytes AND zero random
f32 page reads** — removing the exact Stage-2 bind measured in M82/v5. The residual pipeline is the AM's native
condition (`am/scan.rs` already carries IVF residuals + `dir` centroids), so this transfers directly.

**Remaining ~1% to a hard 0.99:** close with 8-bit codes OR a tiny hybrid (exact-f32 rerank of only the top ~20,
reintroducing 20 random reads instead of 200 — still a large QPS win). **Next (the QPS proof):** wire the RaBitQ
residual code into a dedicated page + f32-free Stage-2 in `scan_ivf_aq_split`, build in-PG, SIFT A/B vs v5 —
gate ≥2× QPS @ 0.99 with a measured f32-buffer-access drop.
