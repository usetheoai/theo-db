# TheoDB vector benchmark — 2026-07-03

- **commit:** `c4999ac` · **seed:** 42 · **dataset:** m32-scale-sift1m (n=1000000 dim=128 metric=l2) · **k:** 10 · **runs (best-of-N):** 3
- recall@k is distance-thresholded (ANN-Benchmarks semantics); ground-truth is exact GT recomputed from the ANN-Benchmarks `neighbors` ids in the float32 contract (neighbors-GT).
- `mean`/`std` are per-query latency dispersion within the timed sample (ms), not run-to-run variance; QPS is best-of-N over the runs.

| index | params | recall@k | QPS | p50 ms | p95 ms | p99 ms | mean ms | std ms | build ms | index bytes |
|---|---|---|---|---|---|---|---|---|---|---|
| hnsw | ef_search=40 | 0.9260 | 132.8 | 6.962 | 14.888 | 23.189 | 7.871 | 3.487 | 472666.5 | 820174848 |
| hnsw | ef_search=100 | 0.9765 | 73.8 | 13.671 | 22.738 | 31.107 | 14.258 | 4.850 | 472666.5 | 820174848 |
| ivfflat | probes=1 | 0.3810 | 1136.7 | 0.911 | 1.978 | 2.376 | 0.994 | 0.494 | 92789.2 | 550559744 |
| ivfflat | probes=10 | 0.8620 | 170.6 | 5.833 | 9.845 | 11.202 | 6.106 | 1.896 | 92789.2 | 550559744 |
| ivfflat | probes=1000 | 1.0000 | 3.1 | 311.800 | 420.618 | 828.476 | 334.070 | 90.320 | 92789.2 | 550559744 |
| theodb_ivfflat | fixed | 0.9845 | 28.7 | 36.112 | 60.245 | 72.398 | 38.254 | 11.496 | 85740.0 | 532938752 |
| theodb_hnsw | fixed [q=200] | 0.9595 | 277.9 | 3.502 | 5.341 | 5.790 | 3.606 | 0.947 | 1440190.1 | 759250944 |
