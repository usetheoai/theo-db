# TheoDB vector benchmark — 2026-07-03

- **commit:** `66c05b9` · **seed:** 2026 · **dataset:** m40-carrier-headhead (n=50000 dim=64 metric=l2) · **k:** 10 · **runs (best-of-N):** 3
- recall@k is distance-thresholded (ANN-Benchmarks semantics); ground-truth is exact float32 brute-force ground-truth.
- `mean`/`std` are per-query latency dispersion within the timed sample (ms), not run-to-run variance; QPS is best-of-N over the runs.

| index | params | recall@k | QPS | p50 ms | p95 ms | p99 ms | mean ms | std ms | build ms | index bytes |
|---|---|---|---|---|---|---|---|---|---|---|
| theodb_hnsw | ef_search=10 | 0.3132 | 1652.0 | 0.592 | 1.070 | 1.792 | 0.626 | 0.285 | 38281.0 | 25296896 |
| theodb_hnsw | ef_search=40 | 0.6166 | 1053.3 | 0.920 | 1.811 | 2.318 | 1.061 | 0.380 | 38281.0 | 25296896 |
| theodb_hnsw | ef_search=100 | 0.8092 | 582.4 | 1.644 | 2.884 | 3.631 | 1.780 | 0.525 | 38281.0 | 25296896 |
| theodb_hnsw | ef_search=200 | 0.9110 | 353.4 | 2.810 | 4.874 | 5.448 | 3.014 | 0.746 | 38281.0 | 25296896 |
| theodb_ivfflat | probes=1 | 0.0634 | 5465.1 | 0.179 | 0.387 | 0.472 | 0.219 | 0.086 | 4130.2 | 14442496 |
| theodb_ivfflat | probes=4 | 0.1658 | 4766.6 | 0.207 | 0.404 | 0.521 | 0.236 | 0.079 | 4130.2 | 14442496 |
| theodb_ivfflat | probes=16 | 0.3946 | 2366.7 | 0.378 | 0.745 | 0.887 | 0.447 | 0.152 | 4130.2 | 14442496 |
| theodb_ivfflat | probes=44 | 0.6648 | 1288.0 | 0.710 | 1.379 | 1.689 | 0.803 | 0.256 | 4130.2 | 14442496 |
| theodb_ivfflat | probes=100 | 0.9024 | 653.6 | 1.505 | 2.967 | 3.530 | 1.724 | 0.574 | 4130.2 | 14442496 |
