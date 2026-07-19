# E2 — FastScan 1-bit SIMD sign kernel for `theodb_symqg`: SIFT1M verdict

**Date:** 2026-07-18 · **Module:** `theodb_rs/src/vec/ah.rs` (`build_sign_lut16` + `sign_estimate_block`),
`theodb_rs/src/am/page/symqg.rs` (v3 block32-nibble layout), `theodb_rs/src/am/scan.rs` (D5 dispatch),
`theodb_rs/src/am/guc.rs` (`theodb.symqg_fastscan` A/B kill-switch). Own-code, clean-room from arXiv:2411.12229
(SymphonyQG) + arXiv:2405.12497 (RaBitQ FastScan) — the NTUITIVE reference is study-only, never copied (D1).

**What is measured** (all on ONE dedicated **c-8** droplet, 8 vCPU CPU-Optimized, **no CPU steal**):

1. **Primary A/B** — `theodb_symqg` v3 (FastScan) vs `theodb_hnsw`, matched recall@10, SIFT1M.
2. **Ablation** — FastScan **ON** vs **OFF** (`SET theodb.symqg_fastscan`) on the **SAME** built index — isolates the
   kernel's effect. The v2-vs-v3 comparison across the two different boxes was **confounded by the box change**;
   the same-index ablation is the only honest FastScan measurement.

**Method:** PostgreSQL 17.10 + pgrx 0.19.0, `shared_buffers=2GB`, N=1,000,000, 200 queries × best-of-3 (warm),
recall@10 vs official GT, `degree_bound=32`. Reproduce: `benchmarks/e2_symqg_inpg.py` (primary),
`benchmarks/e2_symqg_fastscan_ablation.py` (ablation). Raw: `e2-symqg-fastscan-verdict.json`.

**Gate:** `theodb_symqg QPS ≥ 1.5× theodb_hnsw at matched recall@10 ≥ 0.95`. **Result: NOT MET.**

---

## Result 1 — The FastScan kernel is correct and RECALL-NEUTRAL (D3 gate passed)

The ablation runs the FastScan and scalar (`estimate_sign`) paths on the **same index**. Recall is **identical**
within 0.1 pp across the whole sweep — the int8 LUT requant preserves the ranking exactly:

| ef | recall (FastScan) | recall (scalar) | Δ |
|---:|---:|---:|---:|
| 80 | 0.9465 | 0.9455 | +0.10 pp |
| 160 | 0.9810 | 0.9805 | +0.05 pp |
| 640 | 0.9995 | 0.9995 | 0.00 |

The off-PG arithmetic proof (standalone) already bounded the dequantized-dot error; this confirms it end-to-end on
real SIFT1M through the actual scan. **A QPS win bought by a recall regression did not happen — recall holds.**

## Result 2 — The FastScan speedup is MODEST: 1.07–1.22× (honest, same-index)

| ef | FastScan qps | scalar qps | **speedup** |
|---:|---:|---:|---:|
| 40 | 206.9 | 193.0 | 1.07× |
| 80 | 194.5 | 168.5 | **1.15×** |
| 160 | 160.7 | 136.7 | 1.18× |
| 320 | 126.7 | 104.8 | 1.21× |
| 640 | 88.6 | 72.5 | **1.22×** |

The speedup grows with `ef` (more neighbours scored per hop ⇒ more of the batched kernel's benefit). **This is far
below the 2.4–2.8× a naive v2-vs-v3 comparison suggested** — that number was almost entirely the **box change**
(the earlier v2 ran on a steal-heavy shared droplet; `theodb_hnsw` itself went 287→712 QPS purely from moving to
the dedicated box). Quoting the cross-box number as "the FastScan speedup" would be dishonest; the same-index
ablation is the only valid measurement.

**Why only ~1.2× (mechanism):** the per-hop estimate (32 scalar sign-dots) is **not** the sole bottleneck. Row
decode (`rot` 512 B + ordinals + signs + `nr`/`w`), the beam-search heap ops, the `HashSet` visited-set, and the
page reads together dominate more than the estimate. Making the estimate ~4–8× cheaper yields ~1.2× overall
(Amdahl) — the same "the assumed bottleneck isn't the whole story" lesson as E1 (WARM Stage-2 wasn't the bind) and
E2 (the page tax).

## Result 3 — Gate NOT met: `theodb_hnsw` remains 2.1–3.5× faster (parity only at 0.999)

Primary A/B, same box, matched recall:

| matched recall@10 | theodb_symqg v3 FastScan | theodb_hnsw | hnsw faster |
|---|---:|---:|---:|
| ~0.95 | ef=80 → 204.5 qps | ef=40 → 711.8 qps | ~3.5× |
| ~0.98 | ef=160 → 166.0 qps | ef=80 → 430.7 qps | ~2.6× |
| ~0.994 | ef=320 → 129.7 qps | ef=160 → 271.6 qps | ~2.1× |
| ~0.999 | ef=640 → 90.3 qps | ef=640 → 93.6 qps | ~1.04× (parity) |

The gap narrows monotonically with recall and reaches parity only at recall 0.999 — but the gate (`≥ 1.5× hnsw at
recall ≥ 0.95`) is **not met**.

---

## Verdict (honest)

- **The FastScan 1-bit kernel works and is recall-neutral.** `build_sign_lut16` + `sign_estimate_block` reuse the
  tested `ah_score_block` LUT16-pshufb kernel (Rule 9); recall is identical to the scalar path (int8 requant
  preserves ranking); the D5 eligibility dispatch falls back to scalar for `dim > 1032` (protecting 1536-dim
  OpenAI embeddings from an int16-accumulator overflow).
- **Its measured speedup is modest (~1.2×), not the confounded cross-box ~2.8×.** The estimate is not the sole
  per-hop bottleneck; decode + heap + page-read costs dominate more than expected.
- **The gate is NOT met.** `theodb_hnsw` is 2.1–3.5× faster at matched recall (0.95–0.994), converging to parity
  only at recall 0.999. FastScan narrows the intra-symqg cost but does not close the gap to the mature HNSW AM.
- **This is the measurement-first honest-negative** (plan D4): the deliverable is the measured number, and the
  number says a permissive-PG co-located quantized graph — even with FastScan — does not beat our own HNSW AM on
  warm SIFT1M. **No symqg QPS-win claim is made** (`public-copy.md`, rule 5). Consistent with the E1/E2/M73/M74
  line: the vector-superiority pillar remains measured-as-not-achieved for this lever.

## Caveats

- Warm (in-`shared_buffers`) regime only; a genuine out-of-RAM run would re-weight the costs (more page-read,
  which FastScan does not address).
- Build is single-graph HNSW-adjacency-bound (~880 s at 1M on this box); not optimized (out of scope).
- L2-only; degree `32` (the `degree_bound` default and the A/B config). `degree > 32` chunks `⌈degree/32⌉` block32
  blocks (EC-3); `dim > 1032` uses the scalar fallback (EC-1).
- Further levers not pursued here (would need their own measured slice): copy-free row reads, a bitset visited-set,
  f32 heaps — each attacks a different per-hop cost the ablation shows the estimate does not dominate.
