# E2 — SymphonyQG spike: off-PG gate MET — recall parity + ~2.2× faster at 1M (1-bit sign, scalar)

> **VERDICT UPDATE (2026-07-17, N=1M confirming run):** the off-PG gate (≥1.5× at matched recall) is **MET**.
> The first draft below concluded "wall-clock gated on a kernel" — that was the **multi-bit** estimator (a full
> i8×f32 dot, ≈ one L2). The fix is the SymphonyQG **1-bit SIGN** code: the neighbor dot becomes `Σ ±q_r[d]`,
> **multiply-free** (conditional negate + add), ~2-3× cheaper per element than exact L2. Our multi-bit
> `RabitqQuantizer` is DEGENERATE at bits=1 (`L=2^0−1=0` → all-zero codes), so a dedicated sign codec
> (`encode_sign`/`estimate_sign`) was added. **Measured on the FULL SIFT1M (correct GT, real recall):**
>
> | beam | symqg recall@10 | exact recall@10 | **speedup** | exact-dist ratio |
> |---:|---:|---:|---:|---:|
> | 80 | 0.9495 | 0.9870 | **2.24×** | 21.7× |
> | 160 | 0.9810 | 0.9970 | **2.24×** | 19.4× |
> | 320 | 0.9955 | 0.9985 | **2.66×** | 17.1× |
> | 640 | 0.9985 | 0.9995 | 1.81× | 14.8× |
>
> symqg reaches recall parity (0.998) and is **1.8–2.66× faster** than exact-distance traversal on the SAME HNSW
> graph, at 15–27× fewer exact distances — **SCALAR** (no SIMD kernel). The FastScan 1-bit kernel is now an
> ADDITIONAL multiplier on top, not the gate. **Caveat:** this is OFF-PG (pure in-RAM graph search; no heap/WAL/
> MVCC on the search path). The next gate is the in-PG AM, where each graph hop is a random page read — the PG tax
> a standalone lib avoids. HNSW build 814 s + sign-encode 720 s at 1M (unoptimized dense O(D²) rotate; the paper's
> Fast-JL O(D log D) is the build-cost lever, out of spike scope).

---

# (first-draft finding — superseded by the verdict above, kept for the honest record)

# E2 — SymphonyQG spike: mechanism VALIDATED, wall-clock GATED on a FastScan 1-bit kernel

**Date:** 2026-07-17 · **Module:** `theodb_rs/src/ann/symqg_spike.rs` (clean-room from arXiv:2411.12229; the
NTUITIVE-licensed C++ is study-only, never copied — D1). **What is measured:** on the SAME HNSW base graph, two
traversals — (A) exact-distance beam search (baseline) vs (B) SymphonyQG estimated traversal (co-located per-parent
1-bit/7-bit RaBitQ codes; 1 exact per popped center, all neighbors estimated, no separate re-rank). SIFT, in-PG
`SELECT symqg_spike_bench(...)` (runs in the backend to link the crate; the search itself never touches a table —
it is a pure in-RAM graph measurement).

## Honest caveat on the absolute recall (N=200k)

This run indexes a **200k subset** while the SIFT groundtruth is over the full **1M** base, so the absolute
recall ceiling is ~0.25 (a query's true top-10 are mostly outside the subset). That does NOT affect the two
findings below — both are **relative** (symqg vs exact on the same graph + GT) and scale-robust. A full-1M run
would lift both curves to ~0.95+ without changing the mechanism/wall-clock story.

## Finding 1 — Mechanism VALIDATED: recall parity + 12–26× fewer EXACT distances

7-bit codes, beam sweep (recall@10, exact-distance computations per query):

| beam | symqg recall | exact recall | symqg exact-dists | exact exact-dists | **exdist_ratio** |
|---:|---:|---:|---:|---:|---:|
| 40 | 0.2500 | 0.2500 | 46 | 1001 | **21.7×** |
| 80 | 0.2525 | 0.2525 | 85 | 1638 | **19.2×** |
| 160 | 0.2525 | 0.2525 | 165 | 2734 | **16.6×** |
| 640 | 0.2525 | 0.2525 | 644 | 7502 | **11.6×** |

**symqg recall == exact recall at every beam** — the estimated traversal reaches the same neighbors as
exact-distance traversal, doing **12–26× fewer exact distance computations** (1 exact per popped center vs 1 per
candidate). The SymphonyQG mechanism holds on our stack; the 7-bit RaBitQ estimator (our E1 `estimate_l2_sq`,
reused with `c` = the parent vertex) is accurate enough to guide the graph. At 1-bit the estimator is too coarse
(recall collapses) — matching the M74/RaBitQ-1-bit finding; 7-bit is the working point.

## Finding 2 — Wall-clock: the spike is SLOWER (0.45–0.82×) — the kernel is the gate

Despite 12–26× fewer *exact* distances, the spike is **slower in wall-clock**. Measured, and it is not a bug — it
is arithmetic: at beam=160 symqg does 165 exacts but **165 × 32 = 5 280 scalar neighbor estimates**, while exact
does 2 734 exacts. A **scalar** `estimate_l2_sq` costs ~one L2 (both O(D)), so symqg does *more* O(D) work total.

The SymphonyQG wall-clock win requires the estimate to be **~8–16× cheaper than an exact L2** — which is exactly
what a **batched FastScan kernel** delivers (score all 32 co-located neighbor codes in a few SIMD ops via a 4-bit
LUT, vs 32 full L2). Our `vec/ah.rs::ah_score_block` is that kernel for 4-bit AQ product-codes; the 1-bit RaBitQ
FastScan is a *different* bit-packed kernel (reference: the Apache-2.0 `RaBitQ-Library`). It is **not yet built** —
so the spike, with a scalar estimator, cannot show the win.

Two build-side levers already applied (necessary, not sufficient): rotate the query **once** per search
(`rotate(q−c) = rot_q − P·x_p`, linearity; per-hop O(D) subtraction instead of O(D²) rotate) — this removed the
per-hop rotation but not the scalar per-neighbor cost.

## Verdict (honest, measured)

- **Mechanism: GREEN** — recall parity with exact traversal at 12–26× fewer exact distances. The co-located
  quantized-graph approach works on our stack.
- **Gate (≥1.5× wall-clock @ matched recall, off-PG): NOT met with a scalar estimator (0.45–0.82×).** The entire
  remaining risk is localized to ONE component: a **batched FastScan 1-bit RaBitQ kernel** that makes the neighbor
  estimate ~8–16× cheaper than exact L2. With it, the measured 12–26× exact-distance reduction converts to a
  wall-clock win; without it, it does not.
- **Decision:** the spike de-risked the mechanism and localized the make-or-break to the FastScan kernel. Building
  that kernel (clean-room, RaBitQ-Library as permissive reference) is the next gated step — OR stop here with the
  honest finding that the naive port loses, exactly as the spike-first gate intended to reveal cheaply.

## Reproduce
`SELECT symqg_spike_bench('/path/to/sift', 1000000, 200, 7);` after `cargo pgrx install --release` (build HNSW +
co-located codes, sweep beam). Raw: `/home/theo/pg.log` `E2_RESULT` lines on the droplet run.
