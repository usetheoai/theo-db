# TheoDB vector benchmark — 2026-06-27

- **commit:** `abf56d3` · **seed:** 42 · **dataset:** n=5000 dim=128 metric=cosine · **k:** 10 · **runs (best-of-N):** 3
- recall@k is distance-thresholded (ANN-Benchmarks semantics); ground-truth is exact brute-force.

| index | params | recall@k | QPS | p50 ms | p95 ms | p99 ms | build ms | index bytes |
|---|---|---|---|---|---|---|---|---|
| hnsw | ef_search=40 | 0.7150 | 2174.0 | 0.590 | 1.046 | 1.445 | 1178.1 | 4169728 |
| hnsw | ef_search=100 | 0.9400 | 1088.4 | 0.889 | 1.483 | 1.920 | 1178.1 | 4169728 |
| diskann | sls=100,rescore=100 | 0.6300 | 667.3 | 1.469 | 2.760 | 3.179 | 3126.1 | 2433024 |
| diskann | sls=500,rescore=500 | 0.9150 | 300.3 | 3.015 | 5.626 | 7.549 | 3126.1 | 2433024 |
| diskann | sls=1000,rescore=1000 | 0.9710 | 167.9 | 5.893 | 8.743 | 11.113 | 3126.1 | 2433024 |
| diskann | sls=2000,rescore=1000 | 0.9710 | 104.1 | 9.244 | 16.100 | 17.425 | 3126.1 | 2433024 |
