# analytical/crossover/row-count on theodb

**Status:** VALID · **Profile:** nightly · **Run:** `20260822T003027Z-analytical-crossover-row-count-theodb-7cbd4d78`

> This result is **not publishable evidence**: the profile it ran under does not freeze methodology or datasets.

## Results

| Configuration | Throughput/s | Recall | p50 ms | p95 ms | p99 ms | Stable |
|---|---|---|---|---|---|---|
| total_rows via row @ 10000 linhas | 1,753 | _not measured_ | 0.6094 | 0.6291 | 0.6302 | **no** |
| sum_amount via row @ 10000 linhas | 1,380 | _not measured_ | 0.7403 | 0.8104 | 0.8132 | **no** |
| group_by_category via row @ 10000 linhas | 572.6 | _not measured_ | 1.775 | 1.92 | 1.937 | **no** |
| filtered_sum via row @ 10000 linhas | 1,187 | _not measured_ | 0.8521 | 0.9569 | 0.9667 | **no** |
| total_rows via columnar @ 10000 linhas | 1,244 | _not measured_ | 0.8652 | 1.017 | 1.026 | **no** |
| sum_amount via columnar @ 10000 linhas | 1,185 | _not measured_ | 0.892 | 1.013 | 1.014 | **no** |
| group_by_category via columnar @ 10000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 10000 linhas | 435 | _not measured_ | 2.387 | 2.582 | 2.598 | **no** |
| total_rows via parquet @ 10000 linhas | 38.08 | _not measured_ | 28.93 | 29.04 | 29.05 | **no** |
| sum_amount via parquet @ 10000 linhas | 31.07 | _not measured_ | 32.39 | 32.6 | 32.61 | **no** |
| group_by_category via parquet @ 10000 linhas | 32.36 | _not measured_ | 33.85 | 33.94 | 33.94 | **no** |
| filtered_sum via parquet @ 10000 linhas | 35.64 | _not measured_ | 29.26 | 30.85 | 30.99 | **no** |
| total_rows via row @ 50000 linhas | 377.8 | _not measured_ | 2.749 | 2.854 | 2.864 | yes |
| sum_amount via row @ 50000 linhas | 291.1 | _not measured_ | 3.464 | 3.509 | 3.51 | **no** |
| group_by_category via row @ 50000 linhas | 119.7 | _not measured_ | 8.412 | 8.486 | 8.496 | yes |
| filtered_sum via row @ 50000 linhas | 236.7 | _not measured_ | 4.448 | 4.479 | 4.482 | yes |
| total_rows via columnar @ 50000 linhas | 968.7 | _not measured_ | 1.121 | 1.321 | 1.339 | **no** |
| sum_amount via columnar @ 50000 linhas | 583.4 | _not measured_ | 1.756 | 1.793 | 1.796 | yes |
| group_by_category via columnar @ 50000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 50000 linhas | 130.4 | _not measured_ | 8.008 | 8.19 | 8.198 | yes |
| total_rows via parquet @ 50000 linhas | 6.857 | _not measured_ | 147.6 | 148 | 148.1 | yes |
| sum_amount via parquet @ 50000 linhas | 6.205 | _not measured_ | 162.5 | 163 | 163.1 | yes |
| group_by_category via parquet @ 50000 linhas | 5.896 | _not measured_ | 169.9 | 170.5 | 170.6 | yes |
| filtered_sum via parquet @ 50000 linhas | 6.465 | _not measured_ | 155.3 | 156.9 | 157 | yes |
| total_rows via row @ 100000 linhas | 180.9 | _not measured_ | 5.598 | 5.651 | 5.656 | yes |
| sum_amount via row @ 100000 linhas | 145.2 | _not measured_ | 6.943 | 7.01 | 7.016 | **no** |
| group_by_category via row @ 100000 linhas | 60.33 | _not measured_ | 16.82 | 16.9 | 16.92 | yes |
| filtered_sum via row @ 100000 linhas | 119.2 | _not measured_ | 8.616 | 8.669 | 8.686 | yes |
| total_rows via columnar @ 100000 linhas | 716.4 | _not measured_ | 1.409 | 1.623 | 1.652 | **no** |
| sum_amount via columnar @ 100000 linhas | 424.2 | _not measured_ | 2.47 | 2.567 | 2.575 | **no** |
| group_by_category via columnar @ 100000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 100000 linhas | 72.44 | _not measured_ | 14 | 14.25 | 14.27 | yes |
| total_rows via parquet @ 100000 linhas | 3.423 | _not measured_ | 294 | 298 | 298 | yes |
| sum_amount via parquet @ 100000 linhas | 3.124 | _not measured_ | 320.4 | 325.1 | 325.6 | yes |
| group_by_category via parquet @ 100000 linhas | 2.969 | _not measured_ | 337.6 | 339.7 | 339.9 | yes |
| filtered_sum via parquet @ 100000 linhas | 3.218 | _not measured_ | 310.9 | 312.1 | 312.2 | yes |
| total_rows via row @ 500000 linhas | 47.58 | _not measured_ | 21.13 | 21.3 | 21.31 | **no** |
| sum_amount via row @ 500000 linhas | 42.22 | _not measured_ | 23.98 | 24.42 | 24.46 | **no** |
| group_by_category via row @ 500000 linhas | 24.6 | _not measured_ | 41.25 | 41.68 | 41.74 | yes |
| filtered_sum via row @ 500000 linhas | 37.19 | _not measured_ | 27.1 | 27.27 | 27.29 | **no** |
| total_rows via columnar @ 500000 linhas | 261.3 | _not measured_ | 3.88 | 4.038 | 4.045 | yes |
| sum_amount via columnar @ 500000 linhas | 108.1 | _not measured_ | 9.329 | 9.37 | 9.374 | yes |
| group_by_category via columnar @ 500000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 500000 linhas | 16.3 | _not measured_ | 61.5 | 61.99 | 62.03 | yes |
| total_rows via parquet @ 500000 linhas | 0.7016 | _not measured_ | 1,435 | 1,438 | 1,438 | yes |
| sum_amount via parquet @ 500000 linhas | 0.6379 | _not measured_ | 1,581 | 1,597 | 1,598 | yes |
| group_by_category via parquet @ 500000 linhas | 0.6012 | _not measured_ | 1,668 | 1,681 | 1,682 | yes |
| filtered_sum via parquet @ 500000 linhas | 0.6544 | _not measured_ | 1,541 | 1,544 | 1,544 | yes |
| total_rows via row @ 1000000 linhas | 31.09 | _not measured_ | 33.08 | 33.53 | 33.58 | **no** |
| sum_amount via row @ 1000000 linhas | 29.27 | _not measured_ | 34.72 | 35.05 | 35.08 | **no** |
| group_by_category via row @ 1000000 linhas | 14.38 | _not measured_ | 69.63 | 70.37 | 70.43 | yes |
| filtered_sum via row @ 1000000 linhas | 23.85 | _not measured_ | 42.31 | 43.16 | 43.23 | **no** |
| total_rows via columnar @ 1000000 linhas | 134.9 | _not measured_ | 7.62 | 7.693 | 7.696 | yes |
| sum_amount via columnar @ 1000000 linhas | 58.74 | _not measured_ | 17.15 | 17.28 | 17.3 | yes |
| group_by_category via columnar @ 1000000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 1000000 linhas | 8.181 | _not measured_ | 122.6 | 123 | 123 | yes |
| total_rows via parquet @ 1000000 linhas | 0.343 | _not measured_ | 2,946 | 2,947 | 2,947 | yes |
| sum_amount via parquet @ 1000000 linhas | 0.3106 | _not measured_ | 3,221 | 3,229 | 3,229 | yes |
| group_by_category via parquet @ 1000000 linhas | 0.2977 | _not measured_ | 3,380 | 3,400 | 3,402 | yes |
| filtered_sum via parquet @ 1000000 linhas | 0.3218 | _not measured_ | 3,113 | 3,116 | 3,116 | yes |
| total_rows via row @ 2000000 linhas | 22.78 | _not measured_ | 44.64 | 45.01 | 45.04 | **no** |
| sum_amount via row @ 2000000 linhas | 17.56 | _not measured_ | 57 | 57.61 | 57.66 | **no** |
| group_by_category via row @ 2000000 linhas | 8.144 | _not measured_ | 123.5 | 124.3 | 124.4 | yes |
| filtered_sum via row @ 2000000 linhas | 14.45 | _not measured_ | 69.96 | 70.29 | 70.32 | yes |
| total_rows via columnar @ 2000000 linhas | 72.1 | _not measured_ | 14.1 | 14.21 | 14.22 | yes |
| sum_amount via columnar @ 2000000 linhas | 30.67 | _not measured_ | 32.9 | 33.2 | 33.23 | yes |
| group_by_category via columnar @ 2000000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 2000000 linhas | 4.131 | _not measured_ | 243.8 | 244.4 | 244.5 | yes |
| total_rows via parquet @ 2000000 linhas | 0.1714 | _not measured_ | 5,865 | 5,894 | 5,899 | yes |
| sum_amount via parquet @ 2000000 linhas | 0.1553 | _not measured_ | 6,464 | 6,484 | 6,484 | yes |
| group_by_category via parquet @ 2000000 linhas | 0.1468 | _not measured_ | 6,811 | 6,842 | 6,845 | yes |
| filtered_sum via parquet @ 2000000 linhas | 0.16 | _not measured_ | 6,258 | 6,305 | 6,306 | yes |

Unstable points are reported, not removed. Their repetitions disagree by more than the declared threshold, so the median below is a weaker claim than it looks:

- `total_rows via row @ 10000 linhas`: throughput_per_second cv=0.052
- `sum_amount via row @ 10000 linhas`: latency_p95_ms cv=0.064; latency_p99_ms cv=0.069
- `group_by_category via row @ 10000 linhas`: latency_p50_ms cv=0.059; throughput_per_second cv=0.053
- `filtered_sum via row @ 10000 linhas`: latency_p50_ms cv=0.054; latency_p99_ms cv=0.051; throughput_per_second cv=0.059
- `total_rows via columnar @ 10000 linhas`: latency_p50_ms cv=0.066; throughput_per_second cv=0.085
- `sum_amount via columnar @ 10000 linhas`: latency_p50_ms cv=0.087; latency_p95_ms cv=0.080; latency_p99_ms cv=0.088; throughput_per_second cv=0.072
- `filtered_sum via columnar @ 10000 linhas`: latency_p95_ms cv=0.104; latency_p99_ms cv=0.110
- `total_rows via parquet @ 10000 linhas`: latency_p50_ms cv=0.050; latency_p95_ms cv=0.053; latency_p99_ms cv=0.053
- `sum_amount via parquet @ 10000 linhas`: latency_p50_ms cv=0.057; throughput_per_second cv=0.064
- `group_by_category via parquet @ 10000 linhas`: throughput_per_second cv=0.051
- `filtered_sum via parquet @ 10000 linhas`: latency_p99_ms cv=0.051
- `sum_amount via row @ 50000 linhas`: latency_p50_ms cv=0.050; latency_p95_ms cv=0.056; latency_p99_ms cv=0.057; throughput_per_second cv=0.074
- `total_rows via columnar @ 50000 linhas`: latency_p50_ms cv=0.056; throughput_per_second cv=0.077
- `sum_amount via row @ 100000 linhas`: latency_p50_ms cv=0.075; latency_p95_ms cv=0.100; latency_p99_ms cv=0.103
- `total_rows via columnar @ 100000 linhas`: latency_p50_ms cv=0.129; latency_p95_ms cv=0.112; latency_p99_ms cv=0.113; throughput_per_second cv=0.068
- `sum_amount via columnar @ 100000 linhas`: latency_p50_ms cv=0.057; latency_p95_ms cv=0.058; latency_p99_ms cv=0.058
- `total_rows via row @ 500000 linhas`: latency_p50_ms cv=0.066; latency_p95_ms cv=0.124; latency_p99_ms cv=0.129; throughput_per_second cv=0.066
- `sum_amount via row @ 500000 linhas`: latency_p50_ms cv=0.063; latency_p95_ms cv=0.141; latency_p99_ms cv=0.147; throughput_per_second cv=0.051
- `filtered_sum via row @ 500000 linhas`: latency_p95_ms cv=0.126; latency_p99_ms cv=0.133; throughput_per_second cv=0.053
- `total_rows via row @ 1000000 linhas`: latency_p50_ms cv=0.105; throughput_per_second cv=0.133
- `sum_amount via row @ 1000000 linhas`: latency_p50_ms cv=0.085; latency_p95_ms cv=0.208; latency_p99_ms cv=0.218; throughput_per_second cv=0.089
- `filtered_sum via row @ 1000000 linhas`: latency_p50_ms cv=0.052; latency_p95_ms cv=0.052; latency_p99_ms cv=0.052; throughput_per_second cv=0.060
- `total_rows via row @ 2000000 linhas`: latency_p50_ms cv=0.128; latency_p95_ms cv=0.137; latency_p99_ms cv=0.137; throughput_per_second cv=0.123
- `sum_amount via row @ 2000000 linhas`: latency_p50_ms cv=0.086; latency_p95_ms cv=0.084; latency_p99_ms cv=0.084; throughput_per_second cv=0.083

### Repetitions

Every repetition is retained:

- `total_rows via row @ 10000 linhas` latency_p50_ms: 0.6094, 0.617, 0.5913
- `total_rows via row @ 10000 linhas` latency_p95_ms: 0.625, 0.6291, 0.642
- `total_rows via row @ 10000 linhas` latency_p99_ms: 0.6264, 0.6302, 0.6465
- `total_rows via row @ 10000 linhas` throughput_per_second: 1,688, 1,753, 1,869
- `sum_amount via row @ 10000 linhas` latency_p50_ms: 0.7784, 0.7403, 0.7401
- `sum_amount via row @ 10000 linhas` latency_p95_ms: 0.8104, 0.8417, 0.7412
- `sum_amount via row @ 10000 linhas` latency_p99_ms: 0.8132, 0.8507, 0.7413
- `sum_amount via row @ 10000 linhas` throughput_per_second: 1,365, 1,380, 1,395
- `group_by_category via row @ 10000 linhas` latency_p50_ms: 1.931, 1.725, 1.775
- `group_by_category via row @ 10000 linhas` latency_p95_ms: 1.978, 1.92, 1.881
- `group_by_category via row @ 10000 linhas` latency_p99_ms: 1.982, 1.937, 1.89
- `group_by_category via row @ 10000 linhas` throughput_per_second: 529.1, 586.3, 572.6
- `filtered_sum via row @ 10000 linhas` latency_p50_ms: 0.9312, 0.8471, 0.8521
- `filtered_sum via row @ 10000 linhas` latency_p95_ms: 0.9724, 0.9569, 0.8859
- `filtered_sum via row @ 10000 linhas` latency_p99_ms: 0.9761, 0.9667, 0.8889
- `filtered_sum via row @ 10000 linhas` throughput_per_second: 1,074, 1,187, 1,199
- `total_rows via columnar @ 10000 linhas` latency_p50_ms: 0.9144, 0.8014, 0.8652
- `total_rows via columnar @ 10000 linhas` latency_p95_ms: 1.017, 1.003, 1.052
- `total_rows via columnar @ 10000 linhas` latency_p99_ms: 1.026, 1.021, 1.069
- `total_rows via columnar @ 10000 linhas` throughput_per_second: 1,138, 1,350, 1,244
- `sum_amount via columnar @ 10000 linhas` latency_p50_ms: 1.006, 0.8528, 0.892
- `sum_amount via columnar @ 10000 linhas` latency_p95_ms: 1.013, 1.096, 0.935
- `sum_amount via columnar @ 10000 linhas` latency_p99_ms: 1.014, 1.118, 0.9388
- `sum_amount via columnar @ 10000 linhas` throughput_per_second: 1,044, 1,185, 1,187
- `filtered_sum via columnar @ 10000 linhas` latency_p50_ms: 2.398, 2.387, 2.239
- `filtered_sum via columnar @ 10000 linhas` latency_p95_ms: 2.582, 2.844, 2.309
- `filtered_sum via columnar @ 10000 linhas` latency_p99_ms: 2.598, 2.885, 2.315
- `filtered_sum via columnar @ 10000 linhas` throughput_per_second: 418.9, 435, 447.3
- `total_rows via parquet @ 10000 linhas` latency_p50_ms: 26.48, 28.94, 28.93
- `total_rows via parquet @ 10000 linhas` latency_p95_ms: 26.94, 29.04, 29.87
- `total_rows via parquet @ 10000 linhas` latency_p99_ms: 26.99, 29.05, 29.95
- `total_rows via parquet @ 10000 linhas` throughput_per_second: 38.47, 38.08, 38.05
- `sum_amount via parquet @ 10000 linhas` latency_p50_ms: 29.42, 32.39, 32.6
- `sum_amount via parquet @ 10000 linhas` latency_p95_ms: 32.02, 32.6, 32.88
- `sum_amount via parquet @ 10000 linhas` latency_p99_ms: 32.25, 32.61, 32.91
- `sum_amount via parquet @ 10000 linhas` throughput_per_second: 34.44, 31.07, 30.74
- `group_by_category via parquet @ 10000 linhas` latency_p50_ms: 33.85, 31.28, 34.18
- `group_by_category via parquet @ 10000 linhas` latency_p95_ms: 33.94, 33.53, 34.91
- `group_by_category via parquet @ 10000 linhas` latency_p99_ms: 33.94, 33.73, 34.98
- `group_by_category via parquet @ 10000 linhas` throughput_per_second: 29.58, 32.36, 32.39
- `filtered_sum via parquet @ 10000 linhas` latency_p50_ms: 28.2, 29.8, 29.26
- `filtered_sum via parquet @ 10000 linhas` latency_p95_ms: 28.49, 31.25, 30.85
- `filtered_sum via parquet @ 10000 linhas` latency_p99_ms: 28.52, 31.38, 30.99
- `filtered_sum via parquet @ 10000 linhas` throughput_per_second: 35.64, 35.69, 35.22
- `total_rows via row @ 50000 linhas` latency_p50_ms: 2.749, 2.665, 2.852
- `total_rows via row @ 50000 linhas` latency_p95_ms: 2.854, 2.841, 2.908
- `total_rows via row @ 50000 linhas` latency_p99_ms: 2.864, 2.857, 2.913
- `total_rows via row @ 50000 linhas` throughput_per_second: 377.8, 390.4, 359.9
- `sum_amount via row @ 50000 linhas` latency_p50_ms: 3.494, 3.185, 3.464
- `sum_amount via row @ 50000 linhas` latency_p95_ms: 3.509, 3.191, 3.537
- `sum_amount via row @ 50000 linhas` latency_p99_ms: 3.51, 3.192, 3.543
- `sum_amount via row @ 50000 linhas` throughput_per_second: 289.6, 329.4, 291.1
- `group_by_category via row @ 50000 linhas` latency_p50_ms: 8.412, 8.37, 8.809
- `group_by_category via row @ 50000 linhas` latency_p95_ms: 8.476, 8.486, 8.873
- `group_by_category via row @ 50000 linhas` latency_p99_ms: 8.482, 8.496, 8.878
- `group_by_category via row @ 50000 linhas` throughput_per_second: 119.7, 123.6, 113.8
- `filtered_sum via row @ 50000 linhas` latency_p50_ms: 4.477, 4.266, 4.448
- `filtered_sum via row @ 50000 linhas` latency_p95_ms: 4.548, 4.342, 4.479
- `filtered_sum via row @ 50000 linhas` latency_p99_ms: 4.555, 4.349, 4.482
- `filtered_sum via row @ 50000 linhas` throughput_per_second: 223.6, 237.6, 236.7
- `total_rows via columnar @ 50000 linhas` latency_p50_ms: 1.121, 1.172, 1.047
- `total_rows via columnar @ 50000 linhas` latency_p95_ms: 1.321, 1.35, 1.29
- `total_rows via columnar @ 50000 linhas` latency_p99_ms: 1.339, 1.366, 1.312
- `total_rows via columnar @ 50000 linhas` throughput_per_second: 968.7, 875, 1,019
- `sum_amount via columnar @ 50000 linhas` latency_p50_ms: 1.721, 1.756, 1.762
- `sum_amount via columnar @ 50000 linhas` latency_p95_ms: 1.816, 1.789, 1.793
- `sum_amount via columnar @ 50000 linhas` latency_p99_ms: 1.824, 1.791, 1.796
- `sum_amount via columnar @ 50000 linhas` throughput_per_second: 582.8, 593.5, 583.4
- `filtered_sum via columnar @ 50000 linhas` latency_p50_ms: 8.008, 8.093, 7.65
- `filtered_sum via columnar @ 50000 linhas` latency_p95_ms: 8.331, 8.19, 7.843
- `filtered_sum via columnar @ 50000 linhas` latency_p99_ms: 8.36, 8.198, 7.86
- `filtered_sum via columnar @ 50000 linhas` throughput_per_second: 130.4, 125, 133.8
- `total_rows via parquet @ 50000 linhas` latency_p50_ms: 147.6, 147.9, 147.2
- `total_rows via parquet @ 50000 linhas` latency_p95_ms: 148, 148, 148
- `total_rows via parquet @ 50000 linhas` latency_p99_ms: 148.1, 148, 148.1
- `total_rows via parquet @ 50000 linhas` throughput_per_second: 6.857, 6.951, 6.846
- `sum_amount via parquet @ 50000 linhas` latency_p50_ms: 162.5, 164.1, 161.2
- `sum_amount via parquet @ 50000 linhas` latency_p95_ms: 163, 165.1, 161.7
- `sum_amount via parquet @ 50000 linhas` latency_p99_ms: 163.1, 165.2, 161.7
- `sum_amount via parquet @ 50000 linhas` throughput_per_second: 6.157, 6.226, 6.205
- `group_by_category via parquet @ 50000 linhas` latency_p50_ms: 169.9, 170, 169.9
- `group_by_category via parquet @ 50000 linhas` latency_p95_ms: 170.5, 170.9, 170.5
- `group_by_category via parquet @ 50000 linhas` latency_p99_ms: 170.5, 171, 170.6
- `group_by_category via parquet @ 50000 linhas` throughput_per_second: 5.892, 5.896, 5.916
- `filtered_sum via parquet @ 50000 linhas` latency_p50_ms: 155.3, 156.1, 154.7
- `filtered_sum via parquet @ 50000 linhas` latency_p95_ms: 156.9, 159, 156.5
- `filtered_sum via parquet @ 50000 linhas` latency_p99_ms: 157, 159.2, 156.7
- `filtered_sum via parquet @ 50000 linhas` throughput_per_second: 6.465, 6.437, 6.49
- `total_rows via row @ 100000 linhas` latency_p50_ms: 5.56, 5.598, 5.603
- `total_rows via row @ 100000 linhas` latency_p95_ms: 5.613, 5.651, 5.674
- `total_rows via row @ 100000 linhas` latency_p99_ms: 5.617, 5.656, 5.68
- `total_rows via row @ 100000 linhas` throughput_per_second: 180.9, 180.3, 182.3
- `sum_amount via row @ 100000 linhas` latency_p50_ms: 6.854, 6.943, 7.828
- `sum_amount via row @ 100000 linhas` latency_p95_ms: 6.989, 7.01, 8.291
- `sum_amount via row @ 100000 linhas` latency_p99_ms: 7, 7.016, 8.332
- `sum_amount via row @ 100000 linhas` throughput_per_second: 147, 145.2, 143.1
- `group_by_category via row @ 100000 linhas` latency_p50_ms: 16.82, 16.62, 16.95
- `group_by_category via row @ 100000 linhas` latency_p95_ms: 16.82, 16.9, 16.96
- `group_by_category via row @ 100000 linhas` latency_p99_ms: 16.82, 16.92, 16.96
- `group_by_category via row @ 100000 linhas` throughput_per_second: 60.33, 60.76, 59.68
- `filtered_sum via row @ 100000 linhas` latency_p50_ms: 8.478, 8.616, 8.626
- `filtered_sum via row @ 100000 linhas` latency_p95_ms: 8.669, 8.744, 8.665
- `filtered_sum via row @ 100000 linhas` latency_p99_ms: 8.686, 8.756, 8.668
- `filtered_sum via row @ 100000 linhas` throughput_per_second: 119.2, 116.9, 120.4
- `total_rows via columnar @ 100000 linhas` latency_p50_ms: 1.409, 1.663, 1.297
- `total_rows via columnar @ 100000 linhas` latency_p95_ms: 1.486, 1.852, 1.623
- `total_rows via columnar @ 100000 linhas` latency_p99_ms: 1.493, 1.868, 1.652
- `total_rows via columnar @ 100000 linhas` throughput_per_second: 716.4, 679.8, 777.3
- `sum_amount via columnar @ 100000 linhas` latency_p50_ms: 2.47, 2.601, 2.319
- `sum_amount via columnar @ 100000 linhas` latency_p95_ms: 2.567, 2.609, 2.341
- `sum_amount via columnar @ 100000 linhas` latency_p99_ms: 2.575, 2.61, 2.343
- `sum_amount via columnar @ 100000 linhas` throughput_per_second: 424.2, 401.6, 440.7
- `filtered_sum via columnar @ 100000 linhas` latency_p50_ms: 13.95, 14, 14.32
- `filtered_sum via columnar @ 100000 linhas` latency_p95_ms: 14.12, 14.25, 14.41
- `filtered_sum via columnar @ 100000 linhas` latency_p99_ms: 14.13, 14.27, 14.42
- `filtered_sum via columnar @ 100000 linhas` throughput_per_second: 73.4, 72.44, 71.03
- `total_rows via parquet @ 100000 linhas` latency_p50_ms: 291.6, 294, 297.8
- `total_rows via parquet @ 100000 linhas` latency_p95_ms: 295.9, 303.5, 298
- `total_rows via parquet @ 100000 linhas` latency_p99_ms: 296.3, 304.3, 298
- `total_rows via parquet @ 100000 linhas` throughput_per_second: 3.442, 3.423, 3.378
- `sum_amount via parquet @ 100000 linhas` latency_p50_ms: 320.1, 325.3, 320.4
- `sum_amount via parquet @ 100000 linhas` latency_p95_ms: 320.4, 328.9, 325.1
- `sum_amount via parquet @ 100000 linhas` latency_p99_ms: 320.4, 329.2, 325.6
- `sum_amount via parquet @ 100000 linhas` throughput_per_second: 3.141, 3.092, 3.124
- `group_by_category via parquet @ 100000 linhas` latency_p50_ms: 336.8, 340.2, 337.6
- `group_by_category via parquet @ 100000 linhas` latency_p95_ms: 337, 344.5, 339.7
- `group_by_category via parquet @ 100000 linhas` latency_p99_ms: 337, 344.9, 339.9
- `group_by_category via parquet @ 100000 linhas` throughput_per_second: 2.969, 2.973, 2.965
- `filtered_sum via parquet @ 100000 linhas` latency_p50_ms: 309.9, 312.3, 310.9
- `filtered_sum via parquet @ 100000 linhas` latency_p95_ms: 311.6, 312.4, 312.1
- `filtered_sum via parquet @ 100000 linhas` latency_p99_ms: 311.8, 312.4, 312.2
- `filtered_sum via parquet @ 100000 linhas` throughput_per_second: 3.229, 3.21, 3.218
- `total_rows via row @ 500000 linhas` latency_p50_ms: 21.72, 21.13, 19.14
- `total_rows via row @ 500000 linhas` latency_p95_ms: 24.58, 21.3, 19.26
- `total_rows via row @ 500000 linhas` latency_p99_ms: 24.84, 21.31, 19.27
- `total_rows via row @ 500000 linhas` throughput_per_second: 46.7, 47.58, 52.66
- `sum_amount via row @ 500000 linhas` latency_p50_ms: 25.76, 23.98, 22.71
- `sum_amount via row @ 500000 linhas` latency_p95_ms: 29.89, 24.42, 23.04
- `sum_amount via row @ 500000 linhas` latency_p99_ms: 30.26, 24.46, 23.07
- `sum_amount via row @ 500000 linhas` throughput_per_second: 39.83, 42.22, 44.09
- `group_by_category via row @ 500000 linhas` latency_p50_ms: 42.54, 41, 41.25
- `group_by_category via row @ 500000 linhas` latency_p95_ms: 42.75, 41.68, 41.65
- `group_by_category via row @ 500000 linhas` latency_p99_ms: 42.77, 41.74, 41.69
- `group_by_category via row @ 500000 linhas` throughput_per_second: 23.73, 24.63, 24.6
- `filtered_sum via row @ 500000 linhas` latency_p50_ms: 28.15, 27.1, 25.51
- `filtered_sum via row @ 500000 linhas` latency_p95_ms: 32.71, 27.27, 25.86
- `filtered_sum via row @ 500000 linhas` latency_p99_ms: 33.12, 27.29, 25.89
- `filtered_sum via row @ 500000 linhas` throughput_per_second: 35.69, 37.19, 39.6
- `total_rows via columnar @ 500000 linhas` latency_p50_ms: 3.96, 3.88, 3.834
- `total_rows via columnar @ 500000 linhas` latency_p95_ms: 4.038, 4.046, 3.969
- `total_rows via columnar @ 500000 linhas` latency_p99_ms: 4.045, 4.061, 3.981
- `total_rows via columnar @ 500000 linhas` throughput_per_second: 261.1, 263.2, 261.3
- `sum_amount via columnar @ 500000 linhas` latency_p50_ms: 9.457, 8.912, 9.329
- `sum_amount via columnar @ 500000 linhas` latency_p95_ms: 9.486, 8.959, 9.37
- `sum_amount via columnar @ 500000 linhas` latency_p99_ms: 9.489, 8.963, 9.374
- `sum_amount via columnar @ 500000 linhas` throughput_per_second: 108.1, 112.9, 107.9
- `filtered_sum via columnar @ 500000 linhas` latency_p50_ms: 62.23, 61.44, 61.5
- `filtered_sum via columnar @ 500000 linhas` latency_p95_ms: 62.36, 61.51, 61.99
- `filtered_sum via columnar @ 500000 linhas` latency_p99_ms: 62.37, 61.52, 62.03
- `filtered_sum via columnar @ 500000 linhas` throughput_per_second: 16.22, 16.45, 16.3
- `total_rows via parquet @ 500000 linhas` latency_p50_ms: 1,433, 1,440, 1,435
- `total_rows via parquet @ 500000 linhas` latency_p95_ms: 1,438, 1,463, 1,436
- `total_rows via parquet @ 500000 linhas` latency_p99_ms: 1,438, 1,465, 1,436
- `total_rows via parquet @ 500000 linhas` throughput_per_second: 0.7007, 0.7018, 0.7016
- `sum_amount via parquet @ 500000 linhas` latency_p50_ms: 1,568, 1,581, 1,600
- `sum_amount via parquet @ 500000 linhas` latency_p95_ms: 1,570, 1,597, 1,620
- `sum_amount via parquet @ 500000 linhas` latency_p99_ms: 1,570, 1,598, 1,622
- `sum_amount via parquet @ 500000 linhas` throughput_per_second: 0.6379, 0.6391, 0.6317
- `group_by_category via parquet @ 500000 linhas` latency_p50_ms: 1,664, 1,668, 1,716
- `group_by_category via parquet @ 500000 linhas` latency_p95_ms: 1,674, 1,681, 1,725
- `group_by_category via parquet @ 500000 linhas` latency_p99_ms: 1,675, 1,682, 1,726
- `group_by_category via parquet @ 500000 linhas` throughput_per_second: 0.6012, 0.6077, 0.5928
- `filtered_sum via parquet @ 500000 linhas` latency_p50_ms: 1,533, 1,542, 1,541
- `filtered_sum via parquet @ 500000 linhas` latency_p95_ms: 1,540, 1,548, 1,544
- `filtered_sum via parquet @ 500000 linhas` latency_p99_ms: 1,540, 1,548, 1,544
- `filtered_sum via parquet @ 500000 linhas` throughput_per_second: 0.6598, 0.6494, 0.6544
- `total_rows via row @ 1000000 linhas` latency_p50_ms: 33.08, 34.91, 28.36
- `total_rows via row @ 1000000 linhas` latency_p95_ms: 33.53, 35.04, 32.82
- `total_rows via row @ 1000000 linhas` latency_p99_ms: 33.58, 35.05, 33.22
- `total_rows via row @ 1000000 linhas` throughput_per_second: 31.09, 28.97, 37.3
- `sum_amount via row @ 1000000 linhas` latency_p50_ms: 39.37, 34.72, 33.61
- `sum_amount via row @ 1000000 linhas` latency_p95_ms: 48.51, 35.05, 33.86
- `sum_amount via row @ 1000000 linhas` latency_p99_ms: 49.33, 35.08, 33.88
- `sum_amount via row @ 1000000 linhas` throughput_per_second: 25.51, 29.27, 30.34
- `group_by_category via row @ 1000000 linhas` latency_p50_ms: 71.96, 69.63, 68.3
- `group_by_category via row @ 1000000 linhas` latency_p95_ms: 72.23, 70.37, 69.42
- `group_by_category via row @ 1000000 linhas` latency_p99_ms: 72.26, 70.43, 69.52
- `group_by_category via row @ 1000000 linhas` throughput_per_second: 13.98, 14.38, 14.72
- `filtered_sum via row @ 1000000 linhas` latency_p50_ms: 45.53, 42.31, 41.19
- `filtered_sum via row @ 1000000 linhas` latency_p95_ms: 45.71, 43.16, 41.21
- `filtered_sum via row @ 1000000 linhas` latency_p99_ms: 45.72, 43.23, 41.21
- `filtered_sum via row @ 1000000 linhas` throughput_per_second: 22.04, 23.85, 24.85
- `total_rows via columnar @ 1000000 linhas` latency_p50_ms: 7.62, 7.661, 7.576
- `total_rows via columnar @ 1000000 linhas` latency_p95_ms: 7.652, 7.693, 7.717
- `total_rows via columnar @ 1000000 linhas` latency_p99_ms: 7.655, 7.696, 7.73
- `total_rows via columnar @ 1000000 linhas` throughput_per_second: 134.4, 135.1, 134.9
- `sum_amount via columnar @ 1000000 linhas` latency_p50_ms: 17.11, 17.18, 17.15
- `sum_amount via columnar @ 1000000 linhas` latency_p95_ms: 17.13, 17.31, 17.28
- `sum_amount via columnar @ 1000000 linhas` latency_p99_ms: 17.13, 17.32, 17.3
- `sum_amount via columnar @ 1000000 linhas` throughput_per_second: 58.74, 58.34, 58.98
- `filtered_sum via columnar @ 1000000 linhas` latency_p50_ms: 122.3, 122.6, 122.8
- `filtered_sum via columnar @ 1000000 linhas` latency_p95_ms: 122.7, 123.7, 123
- `filtered_sum via columnar @ 1000000 linhas` latency_p99_ms: 122.7, 123.8, 123
- `filtered_sum via columnar @ 1000000 linhas` throughput_per_second: 8.223, 8.181, 8.165
- `total_rows via parquet @ 1000000 linhas` latency_p50_ms: 2,902, 2,946, 2,974
- `total_rows via parquet @ 1000000 linhas` latency_p95_ms: 2,908, 2,947, 2,979
- `total_rows via parquet @ 1000000 linhas` latency_p99_ms: 2,909, 2,947, 2,980
- `total_rows via parquet @ 1000000 linhas` throughput_per_second: 0.346, 0.343, 0.3365
- `sum_amount via parquet @ 1000000 linhas` latency_p50_ms: 3,190, 3,248, 3,221
- `sum_amount via parquet @ 1000000 linhas` latency_p95_ms: 3,197, 3,252, 3,229
- `sum_amount via parquet @ 1000000 linhas` latency_p99_ms: 3,197, 3,252, 3,229
- `sum_amount via parquet @ 1000000 linhas` throughput_per_second: 0.3153, 0.309, 0.3106
- `group_by_category via parquet @ 1000000 linhas` latency_p50_ms: 3,352, 3,410, 3,380
- `group_by_category via parquet @ 1000000 linhas` latency_p95_ms: 3,375, 3,421, 3,400
- `group_by_category via parquet @ 1000000 linhas` latency_p99_ms: 3,377, 3,421, 3,402
- `group_by_category via parquet @ 1000000 linhas` throughput_per_second: 0.2986, 0.2938, 0.2977
- `filtered_sum via parquet @ 1000000 linhas` latency_p50_ms: 3,102, 3,140, 3,113
- `filtered_sum via parquet @ 1000000 linhas` latency_p95_ms: 3,109, 3,160, 3,116
- `filtered_sum via parquet @ 1000000 linhas` latency_p99_ms: 3,109, 3,161, 3,116
- `filtered_sum via parquet @ 1000000 linhas` throughput_per_second: 0.3227, 0.3186, 0.3218
- `total_rows via row @ 2000000 linhas` latency_p50_ms: 54.47, 43.31, 44.64
- `total_rows via row @ 2000000 linhas` latency_p95_ms: 55.54, 43.48, 45.01
- `total_rows via row @ 2000000 linhas` latency_p99_ms: 55.63, 43.5, 45.04
- `total_rows via row @ 2000000 linhas` throughput_per_second: 18.43, 23.21, 22.78
- `sum_amount via row @ 2000000 linhas` latency_p50_ms: 65.58, 57, 56.34
- `sum_amount via row @ 2000000 linhas` latency_p95_ms: 65.71, 57.61, 56.51
- `sum_amount via row @ 2000000 linhas` latency_p99_ms: 65.72, 57.66, 56.53
- `sum_amount via row @ 2000000 linhas` throughput_per_second: 15.35, 17.56, 17.96
- `group_by_category via row @ 2000000 linhas` latency_p50_ms: 131.1, 123.1, 123.5
- `group_by_category via row @ 2000000 linhas` latency_p95_ms: 131.4, 124.3, 123.7
- `group_by_category via row @ 2000000 linhas` latency_p99_ms: 131.4, 124.4, 123.7
- `group_by_category via row @ 2000000 linhas` throughput_per_second: 7.678, 8.144, 8.178
- `filtered_sum via row @ 2000000 linhas` latency_p50_ms: 74.57, 69.96, 68.32
- `filtered_sum via row @ 2000000 linhas` latency_p95_ms: 74.95, 70.29, 68.67
- `filtered_sum via row @ 2000000 linhas` latency_p99_ms: 74.98, 70.32, 68.7
- `filtered_sum via row @ 2000000 linhas` throughput_per_second: 13.67, 14.45, 14.96
- `total_rows via columnar @ 2000000 linhas` latency_p50_ms: 14.11, 13.71, 14.1
- `total_rows via columnar @ 2000000 linhas` latency_p95_ms: 14.22, 14.04, 14.21
- `total_rows via columnar @ 2000000 linhas` latency_p99_ms: 14.23, 14.07, 14.22
- `total_rows via columnar @ 2000000 linhas` throughput_per_second: 72.1, 74.57, 71.82
- `sum_amount via columnar @ 2000000 linhas` latency_p50_ms: 33.39, 32.69, 32.9
- `sum_amount via columnar @ 2000000 linhas` latency_p95_ms: 33.64, 33.11, 33.2
- `sum_amount via columnar @ 2000000 linhas` latency_p99_ms: 33.67, 33.15, 33.23
- `sum_amount via columnar @ 2000000 linhas` throughput_per_second: 30.32, 30.93, 30.67
- `filtered_sum via columnar @ 2000000 linhas` latency_p50_ms: 239.5, 243.8, 244.8
- `filtered_sum via columnar @ 2000000 linhas` latency_p95_ms: 241.1, 244.4, 246.8
- `filtered_sum via columnar @ 2000000 linhas` latency_p99_ms: 241.3, 244.5, 246.9
- `filtered_sum via columnar @ 2000000 linhas` throughput_per_second: 4.229, 4.131, 4.114
- `total_rows via parquet @ 2000000 linhas` latency_p50_ms: 5,843, 5,865, 5,894
- `total_rows via parquet @ 2000000 linhas` latency_p95_ms: 5,894, 5,874, 5,912
- `total_rows via parquet @ 2000000 linhas` latency_p99_ms: 5,899, 5,875, 5,913
- `total_rows via parquet @ 2000000 linhas` throughput_per_second: 0.1723, 0.1714, 0.1707
- `sum_amount via parquet @ 2000000 linhas` latency_p50_ms: 6,440, 6,482, 6,464
- `sum_amount via parquet @ 2000000 linhas` latency_p95_ms: 6,474, 6,484, 6,513
- `sum_amount via parquet @ 2000000 linhas` latency_p99_ms: 6,477, 6,484, 6,517
- `sum_amount via parquet @ 2000000 linhas` throughput_per_second: 0.1555, 0.1545, 0.1553
- `group_by_category via parquet @ 2000000 linhas` latency_p50_ms: 6,811, 6,844, 6,801
- `group_by_category via parquet @ 2000000 linhas` latency_p95_ms: 6,840, 6,846, 6,842
- `group_by_category via parquet @ 2000000 linhas` latency_p99_ms: 6,842, 6,846, 6,845
- `group_by_category via parquet @ 2000000 linhas` throughput_per_second: 0.1468, 0.1463, 0.1472
- `filtered_sum via parquet @ 2000000 linhas` latency_p50_ms: 6,291, 6,258, 6,241
- `filtered_sum via parquet @ 2000000 linhas` latency_p95_ms: 6,305, 6,268, 6,307
- `filtered_sum via parquet @ 2000000 linhas` latency_p99_ms: 6,306, 6,268, 6,313
- `filtered_sum via parquet @ 2000000 linhas` throughput_per_second: 0.1594, 0.16, 0.1603

## Validation

| Check | Outcome | Required | Detail |
|---|---|---|---|
| sut_alive | PASS | yes |  |
| run_not_refused | PASS | yes |  |
| within_time_budget | PASS | yes |  |
| client_alive | PASS | yes |  |
| operation_count | PASS | yes |  |
| repetitions_completed | PASS | yes |  |
| timeout_rate | PASS | yes |  |
| error_rate | PASS | yes |  |
| result_integrity | PASS | yes |  |
| warmup_policy | PASS | yes |  |
| process_containment | PASS | yes |  |
| cpu_limit | PASS | yes |  |
| memory_limit | PASS | yes |  |
| no_oom | PASS | yes |  |
| telemetry_complete | PASS | no |  |
| quality_reported | PASS | yes |  |
| clean_source_tree | PASS | no |  |
| regression_baseline | UNAVAILABLE | no | ProfileName.NIGHTLY gates on regression and no baseline was supplied: unavailable: no baseline supplied for this run. No regression detection was performed |

## Environment

- Host: theo-bench-20260822T000650Z
- CPU: Intel(R) Xeon(R) Platinum 8280 CPU @ 2.70GHz (16 logical, 16 physical)
- SMT: False · Governor: _unavailable_
- Memory: 67424505856 bytes
- Kernel: 6.8.0-124-generic · Runner: theodb-bench 0.6.0
- Benchmark commit: 8c5ca32e22e528484b488970de4dc0b8bcac8aea (dirty: False)

Fields shown in italics were not available on this host and are recorded as absent rather than as zero.

