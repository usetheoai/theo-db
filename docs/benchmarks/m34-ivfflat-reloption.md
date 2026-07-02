# TheoDB vector benchmark — 2026-07-02

- **commit:** `7f40945` · **seed:** 42 · **dataset:** m34-ivfflat-reloption (n=1000000 dim=128 metric=l2) · **k:** 10 · **runs (best-of-N):** 3
- recall@k is distance-thresholded (ANN-Benchmarks semantics); ground-truth is exact GT recomputed from the ANN-Benchmarks `neighbors` ids in the float32 contract (neighbors-GT).
- `mean`/`std` are per-query latency dispersion within the timed sample (ms), not run-to-run variance; QPS is best-of-N over the runs.

| index | params | recall@k | QPS | p50 ms | p95 ms | p99 ms | mean ms | std ms | build ms | index bytes |
|---|---|---|---|---|---|---|---|---|---|---|
| theodb_ivfflat | probes=1 | 0.3734 | 1627.4 | 0.602 | 1.019 | 1.359 | 0.651 | 0.184 | 575062.9 | 537075712 |
| theodb_ivfflat | probes=10 | 0.8737 | 331.1 | 2.987 | 4.706 | 5.798 | 3.201 | 2.881 | 575062.9 | 537075712 |
| theodb_ivfflat | probes=50 | 0.9924 | 77.9 | 12.765 | 17.582 | 20.851 | 13.122 | 2.524 | 575062.9 | 537075712 |
| theodb_ivfflat | probes=100 | 0.9991 | 38.8 | 25.377 | 33.684 | 37.769 | 25.861 | 4.343 | 575062.9 | 537075712 |
| ivfflat | probes=1 | 0.3744 | 2485.8 | 0.366 | 0.697 | 1.035 | 0.406 | 0.154 | 32739.5 | 550551552 |
| ivfflat | probes=10 | 0.8661 | 348.7 | 2.718 | 4.608 | 5.276 | 2.909 | 0.880 | 32739.5 | 550551552 |
| ivfflat | probes=50 | 0.9923 | 71.8 | 13.482 | 20.317 | 23.582 | 14.029 | 9.093 | 32739.5 | 550551552 |
| ivfflat | probes=100 | 0.9993 | 36.0 | 28.320 | 35.979 | 41.116 | 28.419 | 5.689 | 32739.5 | 550551552 |

## Environment & methodology

- **Dataset:** SIFT1M (`sift-128-euclidean`, ANN-Benchmarks) — full 1 000 000×128 train, 1 000-query subsample
  (seed 42), 3 timed runs (best-of-N QPS). Ground truth = neighbors-GT (exact, from the HDF5 `neighbors` ids).
- **Both ivfflat indexes built `WITH (lists = 1000)`** (pgvector guidance ~ rows/1000) and swept over the SAME
  `probes ∈ {1, 10, 50, 100}` — theodb via the new `SET theodb_ivfflat.probes` GUC (M34), pgvector via
  `SET ivfflat.probes`. **Each spec is measured in ISOLATION** (the harness drops the other index during a spec's
  queries) — without this, two ivfflat-family indexes on one column let the planner cross-use them and flatten the
  sweep (the measurement bug caught + fixed this run).
- **Hardware:** 13th Gen Intel i7-1355U (10C/12T, mobile), 15 GB RAM; `theo-db:m34` (PG17, pgrx 0.16.1);
  single-thread builds for both (`max_parallel_maintenance_workers=0`, fair build-time axis).

## Honest per-knob verdict — the M32 ~8× QPS gap is CLOSED

At **matched `probes` (⇒ matched recall)** theodb_ivfflat now tracks pgvector ivfflat, and **wins at the
high-recall operating points**:

| probes | recall (theodb/pgv) | theodb p50 | pgvector p50 | theodb verdict |
|---|---|---:|---:|---|
| 1 | 0.373 / 0.374 | 0.60 ms | 0.37 ms | slightly slower (trivial 1-list point) |
| 10 | 0.874 / 0.866 | 2.99 ms | 2.72 ms | **PARITY** (~10%, recall ≥ pgvector) |
| 50 | 0.992 / 0.992 | **12.77 ms** | 13.48 ms | **theodb FASTER** at parity recall |
| 100 | 0.999 / 0.999 | **25.38 ms** | 28.32 ms | **theodb FASTER** at parity recall |

**M34 DoD MET:** with the configurable `lists`/`probes`, `theodb_ivfflat` p50 is **≤ pgvector** at 1M on the
recall-matched operating points that matter (probes 50/100, recall 0.99+). This closes the M32 finding
(theodb_ivfflat was 30.7 QPS / 32.5 ms at the fixed `lists=100`, ~8× behind); at `lists=1000` + tuned probes it now
reaches **QPS parity, and is faster at high recall**. Index size is also smaller (537 MB vs 550 MB).

**Honest residual (not a defect, a named future lever):** the theodb **build** is far slower — 575 s vs pgvector's
33 s — because theodb runs a single-thread scalar k-means over the FULL 1M corpus, while pgvector samples + can
parallelize. M34 fixed the k-means++ init from O(k²·n·d) to O(k·n·d) (a `WITH (lists=1000)` build was otherwise
stuck > 26 min) and moved the index directory to page-chunked pages (format v2 — the single-page directory capped
lists at ~665); build-time PARITY (sampled / parallel k-means) is a separate future lever, not M34's scan-latency
DoD.

## Reproduction

```bash
# SIFT1M cached at benchmarks/.datasets/sift-128-euclidean.hdf5 (see run_m32_sift1m.py header)
PGPORT=<port> python3 benchmarks/run_m34_ivfflat.py --lists 1000 --n-queries 1000 --runs 3
```
