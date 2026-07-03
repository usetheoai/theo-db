# TheoDB vector benchmark — 2026-07-03

- **commit:** `8d1d20c` · **seed:** 2026 · **dataset:** m40-carrier-headhead (n=50000 dim=64 metric=l2) · **k:** 10 · **runs (best-of-N):** 3
- recall@k is distance-thresholded (ANN-Benchmarks semantics); ground-truth is exact float32 brute-force ground-truth.
- `mean`/`std` are per-query latency dispersion within the timed sample (ms), not run-to-run variance; QPS is best-of-N over the runs.

| index | params | recall@k | QPS | p50 ms | p95 ms | p99 ms | mean ms | std ms | build ms | index bytes |
|---|---|---|---|---|---|---|---|---|---|---|
| theodb_hnsw | ef_search=10 | 0.3132 | 1216.6 | 0.851 | 1.280 | 1.788 | 0.885 | 0.249 | 44399.2 | 25296896 |
| theodb_hnsw | ef_search=40 | 0.6166 | 566.5 | 1.757 | 2.701 | 3.326 | 1.808 | 0.499 | 44399.2 | 25296896 |
| theodb_hnsw | ef_search=100 | 0.8092 | 329.3 | 3.164 | 4.103 | 4.485 | 3.152 | 0.604 | 44399.2 | 25296896 |
| theodb_hnsw | ef_search=200 | 0.9110 | 214.1 | 5.075 | 6.387 | 7.145 | 4.870 | 1.086 | 44399.2 | 25296896 |
| theodb_ivfflat | probes=1 | 0.0634 | 4302.2 | 0.215 | 0.434 | 0.520 | 0.262 | 0.111 | 6154.8 | 14442496 |
| theodb_ivfflat | probes=4 | 0.1658 | 2781.5 | 0.477 | 0.999 | 1.600 | 0.513 | 0.285 | 6154.8 | 14442496 |
| theodb_ivfflat | probes=16 | 0.3946 | 2274.2 | 0.393 | 0.854 | 1.146 | 0.502 | 0.211 | 6154.8 | 14442496 |
| theodb_ivfflat | probes=44 | 0.6648 | 1249.9 | 0.763 | 1.784 | 2.216 | 0.958 | 0.420 | 6154.8 | 14442496 |
| theodb_ivfflat | probes=100 | 0.9024 | 642.8 | 1.595 | 4.023 | 4.785 | 1.988 | 0.924 | 6154.8 | 14442496 |
