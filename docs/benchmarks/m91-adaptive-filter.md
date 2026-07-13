# M91 — adaptive filtered vector search: the real axis is PROBES, not strategy (SIFT1M, MEASURED)

**Date:** 2026-07-13 · **Host:** DO 8-vCPU (Intel Xeon Platinum 8280, 15 GB) · **Dataset:** SIFT1M (real neighbors, 100 real queries)

M91 set out to build an *adaptive strategy selector* (INLINE / POST / PRE) that dominates each fixed strategy
across label selectivity. The measurement **re-scoped the milestone**: on real data there is **no strategy
crossover** — the M90 INLINE (v7) strategy dominates the M87 POST (v5) strategy at every selectivity. The real
adaptive lever is **the probe count, driven by the filter's selectivity**.

## Finding 1 — no strategy crossover (INLINE dominates POST everywhere)

Fixed `probes=64`, recall@10 vs exact seqscan-filtered top-10:

| selectivity | INLINE (v7) recall | INLINE QPS | POST (v5, M87) recall | POST QPS |
|---:|---:|---:|---:|---:|
| 0.01% | **0.741** | 147 | 0.328 | 2.1 |
| 0.1% | 0.886 | 285 | 0.788 | 3.7 |
| 0.5% | 0.951 | 293 | 0.700 | 8.7 |
| 1% | 0.951 | 288 | 0.679 | 12.6 |
| 5% | 0.912 | 282 | 0.579 | 53 |
| 10% | 0.903 | 285 | 0.569 | 103 |
| 30% | 0.848 | 265 | 0.575 | 275 |

INLINE beats POST on recall at **every** point (and on QPS everywhere except 30%, where POST's high QPS comes with
0.575 recall vs INLINE's 0.848). **A 2-way INLINE⇄POST adaptive switch is falsified** — POST never wins. The
original M91 premise ("different fixed strategies win in different regimes") does not hold for this pair on real data.

## Finding 2 — the synthetic "collapse" was a tie-density artifact (measurement-first caught a phantom)

An earlier synthetic sweep (500k, 200 well-separated clusters, scale 20) showed a dramatic INLINE recall **collapse**
at loose selectivity (0.088 @ 30%) — which looked like a real crossover case begging for an adaptive switch. It was
**100% a measurement artifact**: with 2500 points per cluster the true top-10 are hundreds of near-ties, so recall@10
is a coin-flip. Proof: base **unfiltered** recall stuck at **0.024 even at probes=200 (all lists)** — probing
everything didn't help because the "neighbors" were tied. On SIFT (real neighbor structure) the collapse **vanishes**:
INLINE holds 0.85–0.95 across 0.5%–30%. **We nearly built an adaptive strategy to fix a phantom; the SIFT re-measure
stopped it.** This is exactly what measurement-first exists for.

## Finding 3 — cranking probes recovers ultra-selective recall (the real adaptive axis)

INLINE's only genuine weakness is **ultra-selective** (≤0.1%): the true filtered neighbors hide in lists the default
probes never visit. Cranking probes recovers it, decisively:

| selectivity | probes=64 | 128 | 256 | 500 | 1000 |
|---:|---:|---:|---:|---:|---:|
| **0.01%** (100 rows) | 0.741 | 0.789 | 0.969 | **1.000** | 1.000 |
| **0.1%** (1k rows) | 0.886 | 0.974 | 0.996 | 0.996 | 0.996 |
| **1%** (10k rows) | 0.951 | 0.963 | 0.964 | 0.964 | 0.965 |

(QPS at 0.01%: 151 → 95 as probes 64 → 500 — the correct trade: a filtered search returning 74% of the right answers
is broken; 95 QPS at 100% recall is correct.) At 1%+ (already ≥0.95) probes barely helps → no cost where none is needed.

## The M91 design the data mandates

**Selectivity-adaptive probing on the INLINE (v7) path.** The v7 scan today probes a fixed `.take(probes)` lists.
The change: **when a label filter is active, keep probing nearest lists past the default until the accumulated
*matching-candidate* count reaches the rerank target** (bounded by the total list count). Self-tuning on the measured
match count — no threshold GUC, no plan-time plumbing, no new page format, no new strategy:

- **Selective filter** → few matches per list → the scan probes more lists → the true NN are found → recall recovers.
- **Loose filter** → the default probes already yield enough matches → the loop breaks immediately → no extra I/O.
- **No filter** → path is byte-identical to today (break at `probed >= probes`).

### Why the existing M87 iterative doesn't already do this

The M87 IVF iterative re-search grows probes only when the heap **underflows** (< LIMIT candidates emitted). At
ultra-selective the probed lists still yield ≥ LIMIT *non-optimal* matches, so `LIMIT 10` is satisfied with a **wrong**
top-10 and the growth trigger never fires — recall sticks at 0.741. M91 triggers on the **matching-candidate count**
inside the scan, not on heap underflow, closing exactly this gap.

## Honest boundary

- Real SIFT data (recall meaningful); the synthetic collapse is retained only as the tie-density counter-example.
- At **extreme** selectivity (matches ≪ rerank pool) adaptive probing degenerates toward a near-full-list scan
  (O(lists) reads) — bounded and correct (there is no cheaper way to find those few rows' NN), but it tensions the
  partial-read invariant; a GUC ceiling is a follow-up only if a gate shows it's needed.
- **NOT a QPS-superiority claim vs ScaNN/AlloyDB** — the paradigm ceiling (M73/M82) stands. This is a
  **recall-stable-across-the-whole-selectivity-range** result via self-tuning probes.

## Verdict

The measurement re-scoped M91 from "adaptive strategy selector" to **"selectivity-adaptive probing"** — the honest,
data-driven realization of adaptive filtered search. The implementation gate: re-run this sweep with the adaptive v7
and show it **rides the recall envelope** (recovers ultra-selective to ~1.0) while matching the loose-selectivity QPS
(no regression where the default probes already suffice).

## Provenance

- Harness: `benchmarks/m91_filter_bench.py` (SIFT sweep + probe-recovery). Raw: `m91_sift.log`, `m91_ultra.log`.
- Blueprint: `knowledge-base/discoveries/blueprints/adaptive-filter-strategy-blueprint.md`. Raw JSON: `docs/benchmarks/m91-adaptive-filter.json`.
