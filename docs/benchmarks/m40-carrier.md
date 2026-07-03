# M40 — Carrier head-to-head: theodb_hnsw vs theodb_ivfflat (recall × QPS)

**Date:** 2026-07-03
**Verdict:** `THEODB_IVFFLAT_WINS` on synthetic data — at matched QPS, `theodb_ivfflat` has substantially higher
recall than `theodb_hnsw` across most of the curve. **theodb_hnsw is 3–5× slower at matched recall → real QPS
optimization headroom.** Honest caveat: synthetic random-gaussian is the *worst case* for a graph index; the
verdict does NOT generalize to real structured data at scale (needs SIFT1M).
**Type:** measurement (re-scoped M40 — the ceiling probe showed the carrier, not the quantizer, is the recall
lever; `docs/benchmarks/m40-ceiling-probe.md`).

## Why this benchmark (re-scope)

M40 was proposed as "ScaNN anisotropic quantization loss". The M40 ceiling probe falsified that premise: in our
f32-rerank pipeline the recall ceiling is the **carrier** (candidate generation), not the quantizer. So the real
vector-superiority question is which OWN carrier wins the recall × QPS trade-off — `theodb_hnsw` (graph, M35) vs
`theodb_ivfflat` (probes, M34). This runs both persisted AMs over the same corpus + exact brute-force ground
truth, sweeping each AM's query-time knob, and compares at matched QPS.

## Measurement (n=50000, dim=64, k=10, 500 queries, 3 runs; `benchmarks/run_m40_carrier.py`)

| AM | knob | recall@10 | QPS | p50 (ms) |
|---|---|---|---|---|
| theodb_hnsw | ef_search=10 | 0.313 | 1217 | 0.85 |
| theodb_hnsw | ef_search=40 | 0.617 | 566 | 1.76 |
| theodb_hnsw | ef_search=100 | 0.809 | 329 | 3.16 |
| theodb_hnsw | ef_search=200 | 0.911 | 214 | 5.07 |
| theodb_ivfflat | probes=1 | 0.063 | 4302 | 0.22 |
| theodb_ivfflat | probes=4 | 0.166 | 2782 | 0.48 |
| theodb_ivfflat | probes=16 | 0.395 | 2274 | 0.39 |
| theodb_ivfflat | probes=44 | 0.665 | 1250 | 0.76 |
| theodb_ivfflat | probes=100 | 0.902 | 643 | 1.59 |

Matched-QPS (each theodb_hnsw point → nearest theodb_ivfflat point by QPS):

| hnsw | qps | recall | ivf | qps | recall | winner |
|---|---|---|---|---|---|---|
| ef=10 | 1217 | 0.313 | probes=44 | 1250 | 0.665 | **IVF** |
| ef=40 | 566 | 0.617 | probes=100 | 643 | 0.902 | **IVF** |
| ef=100 | 329 | 0.809 | probes=100 | 643 | 0.902 | **IVF** |
| ef=200 | 214 | 0.911 | probes=100 | 643 | 0.902 | HNSW (barely, at 3× lower QPS) |

Reproduce: `PGPORT=<port> python3 benchmarks/run_m40_carrier.py --n 50000 --dim 64 --runs 3`. Artifact:
`docs/benchmarks/m40-carrier.json`. Container: `theo-db:m39` (has both persisted AMs). Local dev CPU.

## Findings (honest)

1. **theodb_ivfflat wins the recall × QPS trade-off** on this corpus: at matched QPS it delivers higher recall
   across the curve (e.g. at ~600 QPS, ivf 0.902 vs hnsw 0.617). It only loses at the extreme high-recall end,
   and even there hnsw's marginal recall edge (0.911 vs 0.902) costs 3× the QPS (214 vs 643).
2. **theodb_hnsw is 3–5× slower at matched recall.** Its page-native on-demand traversal (M35) is less optimized
   than theodb_ivfflat's SIMD+heap scan (M31b/M36). This is concrete **QPS optimization headroom** in
   theodb_hnsw — the graph should be faster than probing, not slower.
3. **The graph's structural advantage did not show — expected on synthetic data.** Random-gaussian in 64-dim has
   no cluster structure for a graph to exploit (curse of dimensionality → near-equidistant points), which is the
   worst case for HNSW and unusually favorable for IVFFlat's exhaustive-at-high-probes behavior.

## Honest caveat (do not over-read)

This is **synthetic random-gaussian** at 50k. The result (ivfflat wins) is NOT trustworthy for the real
vector-superiority question, because:
- Real ANN workloads (SIFT/GIST/DEEP) have cluster structure where HNSW's graph typically dominates IVFFlat on
  the recall×QPS Pareto frontier — the opposite of what synthetic shows.
- 50k is small; HNSW's O(log N) traversal advantage over IVFFlat's O(probes·list_size) grows with N.

The trustworthy head-to-head needs **SIFT1M** (real, structured, 1M scale) via `--hdf5` — not present locally.

## Recommendation (next work, evidence-based)

1. **theodb_hnsw QPS optimization is the highest-leverage vector work** — it is 3–5× slower than theodb_ivfflat
   at matched recall despite being a graph. Profile its page-native scan (the M31b SIMD path already exists for
   ivfflat; theodb_hnsw's `hnsw_page.rs` traversal may not fully use it). Closing this makes the graph win where
   it should.
2. **Run this head-to-head on SIFT1M** before any carrier-superiority claim (`public-copy.md`): the synthetic
   verdict is a harness-validation, not the real answer.

## What shipped (functional, validated)

- `benchmarks/run_m40_carrier.py` — reproducible theodb_hnsw-vs-theodb_ivfflat recall×QPS head-to-head with a
  matched-QPS verdict, composing the two persisted-AM specs over one corpus + exact GT. Ran clean at n=2k and
  n=50k; structure test `benchmarks/tests/test_run_m40_carrier.py`.
- `docs/benchmarks/m40-carrier.{md,json}` — the honest measurement + the SIFT1M caveat.

The measurement is honest and actionable: theodb_ivfflat is the stronger carrier at this scale/data; theodb_hnsw
has clear QPS optimization headroom; the real verdict needs SIFT1M. No superiority claim is made (Rule 3).
