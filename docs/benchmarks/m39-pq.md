# M39 — Product Quantization (PQ+ADC) vs SBQ: honest measurement

**Date:** 2026-07-03
**Verdict:** `SBQ_RETAINED` — PQ does **not** beat SBQ for the P0 vector-superiority (QPS/latency) goal.
**Type:** measurement-first go/no-go (blueprint D3, anti-sunk-cost). No release / no superiority claim.

## TL;DR (honest)

We built a working, tested `theodb.pq_knn` (own std-only Product Quantization + ADC) and measured it
head-to-head against `theodb.sbq_knn` on a deterministic corpus with an exact brute-force ground truth. The
benchmark gate (D3) says **retain SBQ**: at parity recall, PQ is ~5× **slower** than SBQ. PQ trades 4× less
memory for ~5× less QPS — that is a memory/latency trade, **not** the QPS win the P0 requires. The anti-sunk-cost
gate did its job **before** the expensive index-AM page-format integration.

This is the third measurement-first honest-negative of the sequence (M36 distance-not-the-bottleneck, M38
SBQ-regresses + copy-not-the-bottleneck, **M39 PQ-not-a-QPS-win**). The blueprint explicitly anticipated it.

## Numbers (n=2000, dim=64, m=8, bits=4, nq=100, runs=3, mean±std)

| Method | recall@10 | QPS | bytes/vector |
|---|---|---|---|
| **PQ** (`theodb.pq_knn`, m=8, k*=256) | 0.770 ± 0.0 | **352 ± 49** | **8** |
| **SBQ** (`theodb.sbq_knn`, bits=4) | 0.769 ± 0.0 | **1828 ± 30** | 32 |
| full-precision f32 (baseline) | 1.000 (by construction) | — | 256 |

Reproduce: `PGPORT=<port> python3 benchmarks/run_m39_pq.py --n 2000 --dim 64 --m 8 --bits 4 --runs 3 --write-doc`
(raw JSON: `docs/benchmarks/m39-pq.json`). Hardware: local dev CPU (throttled — see M38 variance note).

## Honest reading

1. **Parity recall, not a recall win.** PQ 0.770 vs SBQ 0.769 is a 0.001 gap (noise). Both are capped at ~0.77
   by the IVFFlat candidate generation (probes=16 over ~44 lists) + the hard random-gaussian corpus — **neither
   quantizer beats f32's recall 1.0.** The recall gap that matters is vs f32 (0.23), not PQ-vs-SBQ.
2. **PQ is ~5× slower.** SBQ's ranking primitive is Hamming (XOR/popcount over packed bits) — intrinsically fast.
   PQ's ADC precomputes a per-query LUT (`m·k*` = 2048 sub-distances) and does `m` lookups per candidate, plus the
   standalone path pays a per-call k-means **train** (Lloyd, 25 iters × N × k* × sub_dim) that SBQ's single
   mean/std pass does not. For the P0 (QPS/latency), this is a regression.
3. **Memory win is real but off-goal.** 8 bytes/vector (PQ) vs 32 (SBQ) vs 256 (f32) — a 32× compression vs f32.
   Real, but the P0 is latency/QPS, not footprint.

## Decision (D3, anti-sunk-cost)

Per blueprint D3 and the M38 precedent: **do not merge PQ as a superiority claim; do not cut a release.** The
`theodb.pq_knn` surface + this benchmark are kept as honest measurement (the AM page-format integration is
**not** built — the gate stopped it before the sunk cost). `public-copy.md`: no performance claim is made.

## Next lever (the real gap)

The measured gap is **recall vs f32 (0.77 → 1.0)**, not quantizer speed. Vanilla PQ preserves ranking better than
scalar quantization but does not close the recall gap here. The blueprint's named next lever — **ScaNN's
anisotropic (score-aware) quantization loss over the same PQ codebook skeleton** — targets exactly this: it
penalizes the parallel residual component to preserve top-k ranking, i.e. it improves **recall**, which is the
gap that matters. That is the recommended next-milestone seed (M40 candidate), gated by the same D3 harness.

## What shipped as working code (functional, tested)

- `theodb_rs/src/pq.rs` — `PqQuantizer` (std-only Lloyd k-means per subspace), ADC LUT + distance, `pq_knn`
  pipeline. Deterministic train; quantizer-validity gate (`pq_adc_correlates_with_f32_distance`).
- `theodb.pq_knn` SQL surface (REVOKE FROM PUBLIC, mirror `sbq_knn`) — verified live: correct top-k, ascending
  distance, exact-match at distance 0.
- `benchmarks/run_m39_pq.py` — reproducible PQ-vs-SBQ recall×QPS harness with the honest D3 verdict logic.

The code works; the measurement says it is not the P0 lever. Honest BLOCKED-as-superiority > false win (Rule 3).
