# M83 — pg_scann v5 storage-separated IVF-AQ: D3 spike (SIFT1M, in-Postgres A/B)

**Date:** 2026-07-11 · **Dataset:** SIFT1M (full 1M base, 200 queries, official GT) · **Verdict:** `GO` (build M84)

The measurement-first gate of Roadmap v7. Tests the **one lever ADR-0037 (M82) named**: separating the AQ
codes from the f32 vectors onto distinct page ranges, so the scan reads only the compact codes to prune
(Stage 1) and random-reads f32 only for the rerank survivors (Stage 2). Measured **in the real Access Method**
(not in-memory — the M82 lesson: the in-memory M75 gain did not survive the page path).

## Method (M46 same-data A/B)

- Two tables with **identical** 1M SIFT data:
  - `sift5` → **v5** `WITH (lists=1000, pq_subspaces=32, aq_threshold=2000, separate_storage=1)` (codes/f32 on distinct pages)
  - `sift4` → **v4** `WITH (lists=1000, pq_subspaces=32, aq_threshold=2000)` (interleaved — the M82 layout)
- `recall@10` vs official SIFT GT; `QPS` = best-of-3 over 200 index-scan queries per probe.
- `buffers_per_query` = avg `(shared hit + read)` from `EXPLAIN (ANALYZE, BUFFERS)` over 30 sample queries — the I/O-model proof.
- DO 8 vCPU / 16 GB, single-threaded, pg17, warm cache.

## Correctness (gate: recall-neutral + no regression)

- **238 pg_tests GREEN** (236 existing + 2 new v5: `ambuild_ivf_pq_subspaces_v5_split_scans_high_recall`, `ivf_aq_v5_folds_post_build_inserts`), **0 failed** — zero regression.
- **recall@10 byte-identical between v5 and v4 at every probe** — the split layout is **lossless** vs interleaved. This is the correctness anchor: v5 changes only *where* bytes live, not *which* rows are returned.

## Results — the layout A/B

| probes | recall (v5 = v4) | **v5 QPS** | v4 QPS | **speedup** | v5 buffers | v4 buffers | buffer reduction |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 8 | 0.7265 | 553.9 | 203.1 | **2.7×** | 448 | 1525 | 3.4× |
| 16 | 0.7755 | 508.6 | 122.7 | **4.1×** | 512 | 2786 | 5.4× |
| 32 | 0.7945 | **431.2** | 69.5 | **6.2×** | 640 | 5278 | 8.2× |
| 64 | 0.7970 | 336.7 | 38.2 | **8.8×** | 881 | 9937 | 11.3× |
| 128 | 0.7940 | 236.1 | 20.0 | **11.8×** | 1346 | 18846 | 14.0× |

Build: v5 = 1716s, v4 = 1715s (identical — both dominated by the shared IVF Lloyd k-means). Size: v5 532 MB, v4 528 MB.

## Findings

1. **GATE CRUSHED — 2.7× to 11.8× at matched recall.** The speedup grows **monotonically** with nprobe, exactly
   as the deep research predicted: Stage-1 (codes-only scan) scales with the candidate count while the top-64
   rerank stays pinned. At probes=32 the shipped v5 index is **6.2×** the v4 interleaved index; at probes=128, **11.8×**.
2. **Mechanism proven — 3.4× to 14× fewer buffer accesses.** Even fully **warm/cached**, v5 touches far fewer
   shared buffers, because the Stage-1 scan pages in `[ids][codes]` (~24 B/vec) instead of `[ids][f32][codes]`
   (~536 B/vec). The QPS win comes from **less page-access + memcpy CPU** — precisely the cost that dominates
   in-Postgres vector search (**arXiv:2603.23710, SIGMOD 2026**: *84.4% of ScaNN's CPU cycles inside Postgres are
   system overhead — page access and data handling*), and precisely the cost M82 was bound by (ADR-0037).
3. **Recovers (and exceeds) the layout bucket the research estimated.** The deep-research dossier estimated the
   recoverable layout+I/O bucket at ~4-6×; measured 6.2-11.8× at matched recall on the buffer-access axis.

## Honest caveats

- **Recall ceiling (open, orthogonal to the A/B).** This run's absolute recall plateaued at **~0.795** (probes≥32) —
  below M82's 0.9995 at the nominal-identical config. Because v5 and v4 share this ceiling **exactly**
  (recall-matched), it does **not** bias the layout A/B. But the cross-run discrepancy is a real reproducibility
  question (codebook quality) for M84. Note the rerank pool is **hardcoded at 64** (`over_fetch().max(64)`,
  `MAX_OVER_FETCH=64`) and not GUC-tunable — a candidate cause and a first M84 lever (widen the rerank pool +
  re-measure at recall 0.99). We did **not** confirm the speedup specifically at recall 0.985 in this run; the A/B
  is recall-matched so the speedup is recall-neutral, but the high-recall confirmation is an explicit M84 item.
- **Warm-cache (page-cache confound, flagged pre-spike).** At 1M the f32 (512 MB) fits in 16 GB RAM, so the win is
  from fewer **buffer accesses** (logical page-handling CPU), not less physical disk I/O. This is representative of
  production (working set cached) and is a **lower bound** — at billion-scale (f32 spills to disk, M88) the win
  compounds with physical-I/O savings. `buffers_per_query` is the cache-independent mechanism proof.
- Single-threaded, L2, SIFT1M only. Multi-client + billion-scale are M88.

## Verdict

**GO** → build **M84** (production v5 layout: WAL-safe write, VACUUM/fold of the two regions, `amcostestimate`
v5-aware, + investigate the recall ceiling / rerank-pool cap). The storage-separation lever the ADR-0037 seed
named is **measured, decisive, and recall-neutral**: 6.2-11.8× QPS at matched recall, 8-14× fewer buffer
accesses. This is the **class-AlloyDB-in-Postgres** lever — it does NOT beat the ScaNN library (the paradigm
MVCC/WAL/heap tax remains, per M73/ADR-0035), and any claim awaits the M84 high-recall + billion-scale re-measure.

## Reproduction

```
# pg17 + theodb_rs installed, m83 db, SIFT in $SIFT:
M83_N=1000000 M83_NQ=200 python3 m83_split_bench.py   # 2-table same-data A/B: build v5+v4 → sweep probes + buffers
```

See also: `docs/research/scann-storage-separation-2026-07.md` (the deep research), `docs/adr/0037-m82-am-ivf-aq-measured-verdict.md` (the lever seed), `docs/benchmarks/m82-pgscann-headtohead.md` (the v4 baseline).
