# M82 — pg_scann v4 IVF-AQ+AH Access Method: final measured head-to-head (SIFT1M, in-Postgres)

**Date:** 2026-07-11 · **Dataset:** SIFT1M (full 1,000,000 base vectors, 200 queries, official ground-truth — valid at full 1M) · **Verdict:** `HONEST_NEGATIVE_FINAL`

This is the terminal measurement of the pg_scann track (M75→M82): the ScaNN algorithm
(IVF partition + Anisotropic Vector Quantization + Asymmetric-Hashing batched-LUT SIMD +
exact rerank) shipped as a first-class PostgreSQL Access Method (`theodb_ivfflat` v4), measured
end-to-end **inside Postgres** at true 1M scale against its own exact-f32 IVF baseline and the
M33 reference points.

## Method (M46 same-data A/B rigor)

- COPY the full 1,000,000 SIFT base vectors into ONE table.
- Build BOTH indexes **on the same data** (not cross-container — the M46 learning):
  - **v4** — `theodb_ivfflat WITH (lists=1000, pq_subspaces=32, aq_threshold=2000)` → batched-AH IVF-AQ scan path (`scan_ivf_aq`).
  - **v3** — `theodb_ivfflat WITH (lists=1000)` → exact-f32 IVF scan path (byte-identical to the pre-M77 baseline).
- `recall@10` vs the official SIFT ground-truth (valid because the full 1M base is loaded).
- `QPS` = best-of-3 over 200 sequential `ORDER BY e <-> q LIMIT 10` index-scan queries per probe level; `enable_seqscan=off`.
- Hardware: DO droplet 8 vCPU / 16 GB, single-threaded scan, pg17.

## Index sizes — v4 is a real v4 index

| Index | Size | Note |
|---|---|---|
| `sift_v4` (IVF-AQ) | **528 MB** | f32 vectors + AQ codes |
| `sift_v3` (f32 IVF) | 512 MB | f32 vectors only |
| **delta** | **16 MB** | = `m=32 → 16 bytes/code × 1M` — the AQ codes. Confirms the v4 path built + persisted the quantized codes (not a silent v3 fallback). |

## Build time

| Index | Build (s) |
|---|---|
| v4 IVF-AQ | 1599.2 |
| v3 f32 IVF | 1588.3 |

v4 is only **+0.7%** slower despite training the AVQ codebook + encoding + block32-packing 1M
vectors — the M82 `AQ_TRAIN_SAMPLE=50k` deterministic-stride sampling made the 1M codebook
train tractable (the naive full-N train was the M75 super-linear blocker). Both builds are
dominated by the shared single-threaded IVF Lloyd k-means (`lists=1000`).

## Results — recall@10 × QPS

| probes | recall (v4 = v3) | v4 QPS | v3 QPS |
|---:|---:|---:|---:|
| 1 | 0.3765 | 513.1 | 509.0 |
| 4 | 0.7175 | 318.4 | 322.9 |
| 8 | 0.8510 | 221.0 | 222.7 |
| 16 | 0.9355 | 136.5 | 134.1 |
| 32 | 0.9850 | 78.5 | 79.2 |
| 64 | 0.9980 | 41.7 | 43.0 |
| 128 | 0.9995 | 22.6 | 22.7 |
| 256 | 0.9995 | 11.5 | 11.5 |

**Reference (M33, SIFT1M):** ScaNN 1920 QPS @ recall 0.99 · pgvector f32 IVF 78 QPS @ recall 0.99.

## Findings (honest)

1. **Functionally correct.** The v4 IVF-AQ+AH index's `recall@10` is **byte-identical** to the
   exact-f32 IVF at every probe level. The AH candidate pruning + exact rerank is **lossless** at
   these settings — the shipped index returns exactly what the exact IVF returns.
2. **Zero measured QPS advantage.** v4 and v3 track each other within best-of-3 timing noise
   (v4 faster at some probes, v3 at others). The AH quantization buys **no** speedup in the AM.
3. **f32-IVF class, ~24× below ScaNN.** At recall 0.985 the v4 index measures **78.5 QPS** — the
   same class as M33's pgvector f32 IVF (78 QPS), and ~24× below ScaNN (1920 QPS).

### Root cause — why the M75 in-memory 5-7× vanished

The M75 spike measured a ~5-7× QPS gain for IVF-AQ+AH **in-memory**, with an explicit caveat:
*"in-memory single-thread, no pgrx page/WAL tax (M76+)"*. M82 confirms that caveat was
load-bearing.

In the current v4 page layout the AQ codes are **interleaved** with the f32 vectors in the same
per-list pages (`[ids][f32][codes]`). Reading the codes to score by AH therefore also pages in
the f32 vectors, so the scan pays the **full f32 page I/O** regardless. The AH LUT scoring only
saves the exact-distance **compute** — and compute is **not** the bottleneck. The AM scan is
**I/O + centroid-probe bound**, exactly the "system-level overheads" documented by
**arXiv:2603.23710 (SIGMOD 2026)**. The in-memory speedup does not survive the page-based AM.

### Future lever (honest seed — NOT shipped, NOT a claim)

To realize the AH speedup in the AM, the codes must live in **separate pages** from the f32
vectors, so the scan reads only the compact code pages for the full list, prunes to top-K, then
random-reads f32 **only** for the K survivors (the FastScan/ScaNN storage separation). This is a
layout redesign beyond M82's benchmark+verdict scope. Recorded as a next-discovery seed.

## North Star verdict

M82 **confirms and extends** the M73 verdict ([ADR-0035](../adr/0035-m73-northstar-vector-verdict.md)):
permissive-PostgreSQL-extension vector **QPS superiority over ScaNN/AlloyDB is paradigmatically
unreachable**. M82 adds the specific, measured finding that even the AH-quantized in-memory
speedup (M75) **does not survive** the page-based AM's I/O/probe binding.

Permitted positioning (per `.claude/rules/public-copy.md` + [ADR-0035](../adr/0035-m73-northstar-vector-verdict.md)):
**"recall parity + billion-scale memory + AI-native/HTAP/open"** — **never** "faster than
AlloyDB on vectors". Full ADR: [0037-m82-am-ivf-aq-measured-verdict.md](../adr/0037-m82-am-ivf-aq-measured-verdict.md).

## Caveats

- Single-threaded scan; multi-client QPS not measured here (M72 covered the multi-client 128d regime).
- L2 metric, SIFT1M only.
- The null result is specific to the current interleaved `[ids][f32][codes]` layout; the code/f32
  page separation is an **untested future lever**, not a measured claim.

## Reproduction

```
# on the build host (pg17, extension installed, m82 db):
#   sift_base.fvecs / sift_query.fvecs / sift_groundtruth.ivecs in $SIFT
M82_N=1000000 M82_NQ=200 python3 m82_bench.py   # harness: COPY → build v4+v3 → sweep probes
```
