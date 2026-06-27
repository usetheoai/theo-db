# TheoDB vector benchmark — 2026-06-27

- **commit:** `0297a97` · **seed:** 42 · **dataset:** n=5000 dim=128 metric=cosine · **k:** 10 · **runs (best-of-N):** 3
- recall@k is distance-thresholded (ANN-Benchmarks semantics); ground-truth is exact brute-force.

| index | params | recall@k | QPS | p50 ms | p95 ms | p99 ms | build ms | index bytes |
|---|---|---|---|---|---|---|---|---|
| hnsw | ef_search=40 | 0.7100 | 1290.7 | 0.849 | 1.476 | 1.945 | 2239.6 | 4169728 |
| hnsw | ef_search=100 | 0.9440 | 587.2 | 1.666 | 2.707 | 3.505 | 2239.6 | 4169728 |
| diskann | sls=100 | 0.6290 | 430.9 | 2.265 | 3.867 | 5.436 | 5323.5 | 2433024 |
| diskann | sls=500 | 0.9160 | 178.4 | 5.108 | 11.343 | 14.974 | 5323.5 | 2433024 |
| diskann | sls=1000 | 0.9160 | 141.0 | 7.709 | 15.336 | 17.259 | 5323.5 | 2433024 |
