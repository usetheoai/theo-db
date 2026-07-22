# TheoDB vector benchmark — 2026-06-27

- **commit:** `c421550` · **seed:** 42 · **dataset:** glove-25-angular (n=50000 dim=25 metric=cosine) · **k:** 10 · **runs (best-of-N):** 3
- recall@k is distance-thresholded (ANN-Benchmarks semantics); ground-truth is exact brute-force.

| index | params | recall@k | QPS | p50 ms | p95 ms | p99 ms | build ms | index bytes |
|---|---|---|---|---|---|---|---|---|
| hnsw | ef_search=40 | 0.9836 | 2778.5 | 0.371 | 0.569 | 0.685 | 11061.0 | 20553728 |
| hnsw | ef_search=100 | 0.9958 | 1495.0 | 0.676 | 1.112 | 1.390 | 11061.0 | 20553728 |
| diskann | sls=100,rescore=100 | 0.6104 | 446.4 | 2.210 | 3.332 | 4.005 | 123378.9 | 22773760 |
| diskann | sls=500,rescore=500 | 0.8630 | 128.7 | 7.590 | 11.303 | 12.428 | 123378.9 | 22773760 |
| diskann | sls=1000,rescore=1000 | 0.9334 | 74.5 | 14.158 | 20.688 | 25.286 | 123378.9 | 22773760 |
| diskann | sls=2000,rescore=1000 | 0.9334 | 52.4 | 18.839 | 32.035 | 41.952 | 123378.9 | 22773760 |
