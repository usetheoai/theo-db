# TheoDB vector benchmark — 2026-07-03

- **commit:** `a79ef70` · **seed:** 2026 · **dataset:** m40-carrier-headhead (n=50000 dim=64 metric=l2) · **k:** 10 · **runs (best-of-N):** 3
- recall@k is distance-thresholded (ANN-Benchmarks semantics); ground-truth is exact float32 brute-force ground-truth.
- `mean`/`std` are per-query latency dispersion within the timed sample (ms), not run-to-run variance; QPS is best-of-N over the runs.

| index | params | recall@k | QPS | p50 ms | p95 ms | p99 ms | mean ms | std ms | build ms | index bytes |
|---|---|---|---|---|---|---|---|---|---|---|
| theodb_hnsw | ef_search=10 | 0.3132 | 3538.5 | 0.271 | 0.524 | 0.676 | 0.308 | 0.108 | 36222.4 | 25296896 |
| theodb_hnsw | ef_search=40 | 0.6166 | 1675.4 | 0.604 | 1.068 | 1.286 | 0.675 | 0.204 | 36222.4 | 25296896 |
| theodb_hnsw | ef_search=100 | 0.8092 | 865.0 | 1.162 | 2.068 | 2.782 | 1.288 | 0.412 | 36222.4 | 25296896 |
| theodb_hnsw | ef_search=200 | 0.9110 | 510.5 | 2.025 | 3.485 | 4.373 | 2.246 | 0.669 | 36222.4 | 25296896 |
| theodb_ivfflat | probes=1 | 0.0634 | 5168.9 | 0.203 | 0.372 | 0.473 | 0.232 | 0.082 | 5055.9 | 14442496 |
| theodb_ivfflat | probes=4 | 0.1658 | 4757.5 | 0.205 | 0.407 | 0.475 | 0.223 | 0.070 | 5055.9 | 14442496 |
| theodb_ivfflat | probes=16 | 0.3946 | 2812.3 | 0.345 | 0.462 | 0.695 | 0.362 | 0.086 | 5055.9 | 14442496 |
| theodb_ivfflat | probes=44 | 0.6648 | 1444.9 | 0.693 | 1.469 | 1.913 | 0.799 | 0.295 | 5055.9 | 14442496 |
| theodb_ivfflat | probes=100 | 0.9024 | 608.8 | 1.867 | 3.618 | 4.319 | 2.220 | 0.857 | 5055.9 | 14442496 |
