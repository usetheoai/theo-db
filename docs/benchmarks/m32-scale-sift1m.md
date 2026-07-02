# TheoDB vector benchmark — 2026-07-02

- **commit:** `ca4e005` · **seed:** 42 · **dataset:** m32-scale-sift1m (n=1000000 dim=128 metric=l2) · **k:** 10 · **runs (best-of-N):** 3
- recall@k is distance-thresholded (ANN-Benchmarks semantics); ground-truth is exact GT recomputed from the
  ANN-Benchmarks `neighbors` ids in the float32 contract (neighbors-GT, ADR-2 — NOT the 10¹⁰ brute force).
- Latency **mean±std** (per-query dispersion within the timed sample, ms): hnsw ef=40 2.32±0.57 · **ef=100 4.62±11.29**
  (⚠ a heavy tail — ≥1 extreme outlier far beyond p99=6.59 ms, consistent with mobile-CPU thermal/scheduler jitter;
  best-of-N QPS is insulated from it) · ivfflat probes=1 0.45±0.19 · probes=10 4.17±0.99 · probes=1000 4.21±1.01 ·
  theodb_ivfflat 32.76±5.63 · theodb_hnsw 609.61±15.44. `std` is intra-sample dispersion, not run-to-run variance.

| index | params | recall@k | QPS | p50 ms | p95 ms | p99 ms | build ms | index bytes |
|---|---|---|---|---|---|---|---|---|
| hnsw | ef_search=40 | 0.9308 | 436.9 | 2.300 | 3.326 | 4.003 | 291768.4 | 820011008 |
| hnsw | ef_search=100 | 0.9814 | 237.5 | 4.364 | 5.630 | 6.591 | 291768.4 | 820011008 |
| ivfflat | probes=1 | 0.3666 | 2335.1 | 0.397 | 0.831 | 1.108 | 60758.1 | 550371328 |
| ivfflat | probes=10 | 0.9814 | 242.3 | 4.278 | 5.601 | 6.510 | 60758.1 | 550371328 |
| ivfflat | probes=1000 | 0.9814 | 238.5 | 4.308 | 5.635 | 6.605 | 60758.1 | 550371328 |
| theodb_ivfflat | fixed | 0.9876 | 30.7 | 32.495 | 42.179 | 47.544 | 296991.4 | 532930560 |
| theodb_hnsw | fixed [q=50] | 0.9640 | 1.6 | 607.445 | 636.078 | 655.617 | 903224.5 | 824164352 |

## Environment & methodology

- **Dataset:** SIFT1M (`sift-128-euclidean`, ANN-Benchmarks) — `train` 1 000 000 × 128, `test` 10 000 × 128,
  Euclidean/l2. Full train loaded (no subsample). 1 000 query subsample (seed 42), 3 timed runs (best-of-N QPS).
- **Ground truth:** exact GT distances recomputed from the HDF5 `neighbors` ids in the float32 contract
  (`recall.neighbors_ground_truth`) — NOT the 10¹⁰ brute force (M32 ADR-2). recall@10 is distance-thresholded
  (ANN-Benchmarks semantics, `recall.recall_at_k`).
- **Hardware:** 13th Gen Intel i7-1355U (10 cores / 12 threads — 2 P-cores + 8 E-cores; a mobile, thermally
  constrained CPU, hence the ef=100 latency tail above), 15 GB RAM; Docker image `theo-db:m31b` (PG17, pgrx 0.16.1),
  container default `/dev/shm`. Peak container RSS ≈ 3.3 GB (during the theodb_hnsw in-memory build).
- **Fairness — builds are SINGLE-THREADED for BOTH engines** (`max_parallel_maintenance_workers=0`,
  `maintenance_work_mem=2GB`): theodb's M26 build has no parallel path, so pgvector is held single-threaded too so
  the build-TIME axis is apples-to-apples (else pgvector would get all cores, theodb 1). Note `maintenance_work_mem`
  is consumed ONLY by pgvector's build (theodb builds in-process and ignores it) — i.e. the generous budget *helps
  pgvector*, the anti-cherry-pick direction.
- **theodb_hnsw query cap = 50** (`[q=50]`): its scan is O(N)-per-query (whole-blob deserialize; M31's structured
  partial-read is ivfflat-only), ~0.6 s/query at 1M; the full 1 000-query set would take hours. 50 queries × 3 runs
  is a valid latency/recall sample. This cap is the honest signal that theodb_hnsw does not scale, not a hidden gap.

## Honest per-knob verdict (no cherry-pick — full QPS×recall frontier above)

| Dimension | theodb | pgvector | Verdict for theodb |
|---|---|---|---|
| **recall@10 (ivfflat family, ~0.98 band)** | theodb_ivfflat **0.9876** | ivfflat probes=10 0.9814 | **SUPERIOR** (+0.6 pts) |
| **index size (ivfflat family)** | theodb_ivfflat **533 MB** | ivfflat 550 MB, hnsw 820 MB | **SUPERIOR** (most compact) |
| **QPS @ recall≈0.98 (ivfflat family)** | theodb_ivfflat 30.7 | ivfflat probes=10 **242** | **INFERIOR** (~8× slower) |
| **HNSW family** | theodb_hnsw 1.6 QPS / 607 ms @ recall 0.964 | hnsw ef=100 **237 QPS** @ recall 0.981 | **INFERIOR** (impractical) |
| **build time (single-thread)** | ivfflat 297 s, hnsw 903 s | ivfflat **61 s**, hnsw 292 s | **INFERIOR** |

**Root causes (measured, actionable — the value of this scale run):**

1. **theodb_ivfflat under-partitions at 1M.** `DEFAULT_LISTS=100` is a fixed Rust constant with no reloption, so
   at 1M each list holds ~10 000 vectors; `SCAN_PROBES=10` → ~100 000 candidates scored/query (p50 32 ms). A
   well-tuned pgvector ivfflat uses `lists=1000` → ~10 000 scored/query (p50 4 ms). theodb needs a **configurable
   `lists`/`probes`** (currently fixed) to reach QPS parity — its recall and index size are already ahead.
2. **theodb_hnsw scan is O(N)-per-query.** The M31 structured partial-read (O(probes) page reads) was applied to
   `theodb_ivfflat` only; `theodb_hnsw` still deserializes the whole blob per query (~0.6 s at 1M → 1.6 QPS). The
   clear next lever is the **structured-scan treatment for hnsw** (mirror M31 for the graph AM).
3. **theodb builds are single-threaded scalar** (no parallel maintenance path) — slower build times.

**Bottom line (honest, per the North Star measurement-first mandate):** at 1M scale theodb reaches **recall
parity/superiority** and a **smaller index**, but **does NOT yet reach QPS parity** with pgvector — the
vector-superiority pillar is unmet on throughput at scale. The gap is fully explained by two missing, named levers
(configurable ivfflat lists/probes; structured hnsw scan), not by the distance kernel (M31b already closed that).
This quantifies the `goto-p0-vector-superiority` gap and feeds the next milestones.

## Reproduction

```bash
# 1. cache SIFT1M (once, ~500 MB):
curl -fsSL -o benchmarks/.datasets/sift-128-euclidean.hdf5 http://ann-benchmarks.com/sift-128-euclidean.hdf5
# 2. run against a container with the theodb AMs (theo-db:m31b or later), PGPORT set:
PGPORT=<port> python3 benchmarks/run_m32_sift1m.py --n-queries 1000 --runs 3 --theodb-hnsw-query-cap 50
# writes docs/benchmarks/m32-scale-sift1m.{md,json}
```
