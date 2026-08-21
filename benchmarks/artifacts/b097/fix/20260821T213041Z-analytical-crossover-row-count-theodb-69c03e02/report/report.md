# analytical/crossover/row-count on theodb

**Status:** EXPLORATORY · **Profile:** research · **Run:** `20260821T213041Z-analytical-crossover-row-count-theodb-69c03e02`

> This run is **EXPLORATORY**. Research runs may use non-frozen parameters, so the numbers below cannot back a published claim.

## Results

| Configuration | Throughput/s | Recall | p50 ms | p95 ms | p99 ms | Stable |
|---|---|---|---|---|---|---|
| total_rows via row @ 10000 linhas | 1,935 | _not measured_ | 0.5515 | 0.6182 | 0.6216 | **no** |
| sum_amount via row @ 10000 linhas | 1,453 | _not measured_ | 0.6945 | 0.7961 | 0.8056 | **no** |
| group_by_category via row @ 10000 linhas | 589.3 | _not measured_ | 1.754 | 1.754 | 1.76 | yes |
| filtered_sum via row @ 10000 linhas | 1,293 | _not measured_ | 0.8074 | 0.9321 | 0.937 | **no** |
| total_rows via columnar @ 10000 linhas | 1,398 | _not measured_ | 0.7578 | 0.8766 | 0.8872 | **no** |
| sum_amount via columnar @ 10000 linhas | 1,168 | _not measured_ | 0.8764 | 0.9426 | 0.9485 | **no** |
| group_by_category via columnar @ 10000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 10000 linhas | 467.8 | _not measured_ | 2.142 | 2.325 | 2.331 | yes |
| total_rows via parquet @ 10000 linhas | 36.12 | _not measured_ | 28.05 | 28.7 | 28.75 | **no** |
| sum_amount via parquet @ 10000 linhas | 32.31 | _not measured_ | 31.47 | 31.83 | 31.86 | **no** |
| group_by_category via parquet @ 10000 linhas | 30.96 | _not measured_ | 32.92 | 32.94 | 32.94 | yes |
| filtered_sum via parquet @ 10000 linhas | 33.84 | _not measured_ | 29.89 | 30.19 | 30.21 | yes |
| total_rows via row @ 50000 linhas | 419.9 | _not measured_ | 2.482 | 2.671 | 2.677 | yes |
| sum_amount via row @ 50000 linhas | 346.5 | _not measured_ | 2.987 | 3.077 | 3.082 | yes |
| group_by_category via row @ 50000 linhas | 128.1 | _not measured_ | 7.997 | 8.224 | 8.244 | yes |
| filtered_sum via row @ 50000 linhas | 262.2 | _not measured_ | 3.882 | 3.947 | 3.955 | yes |
| total_rows via columnar @ 50000 linhas | 980.2 | _not measured_ | 1.039 | 1.192 | 1.209 | **no** |
| sum_amount via columnar @ 50000 linhas | 597.6 | _not measured_ | 1.707 | 1.741 | 1.744 | **no** |
| group_by_category via columnar @ 50000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 50000 linhas | 145.3 | _not measured_ | 6.915 | 6.984 | 6.986 | yes |
| total_rows via parquet @ 50000 linhas | 6.534 | _not measured_ | 153.9 | 155 | 155.1 | yes |
| sum_amount via parquet @ 50000 linhas | 6.043 | _not measured_ | 167.4 | 168.7 | 168.8 | yes |
| group_by_category via parquet @ 50000 linhas | 5.682 | _not measured_ | 176.5 | 177.9 | 178 | yes |
| filtered_sum via parquet @ 50000 linhas | 6.202 | _not measured_ | 161.9 | 163.4 | 163.5 | yes |
| total_rows via row @ 100000 linhas | 219.1 | _not measured_ | 4.786 | 5.019 | 5.04 | **no** |
| sum_amount via row @ 100000 linhas | 167.7 | _not measured_ | 6.126 | 6.131 | 6.131 | yes |
| group_by_category via row @ 100000 linhas | 65.27 | _not measured_ | 15.76 | 15.82 | 15.82 | yes |
| filtered_sum via row @ 100000 linhas | 128.4 | _not measured_ | 8.003 | 8.037 | 8.04 | **no** |
| total_rows via columnar @ 100000 linhas | 824.5 | _not measured_ | 1.313 | 1.405 | 1.415 | **no** |
| sum_amount via columnar @ 100000 linhas | 423.9 | _not measured_ | 2.374 | 2.453 | 2.456 | yes |
| group_by_category via columnar @ 100000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 100000 linhas | 79.36 | _not measured_ | 12.63 | 12.99 | 13.03 | yes |
| total_rows via parquet @ 100000 linhas | 3.294 | _not measured_ | 306.5 | 312.2 | 312.7 | yes |
| sum_amount via parquet @ 100000 linhas | 3 | _not measured_ | 336 | 337.4 | 337.6 | yes |
| group_by_category via parquet @ 100000 linhas | 2.842 | _not measured_ | 353.3 | 355.7 | 356 | yes |
| filtered_sum via parquet @ 100000 linhas | 3.083 | _not measured_ | 330.6 | 331.4 | 331.4 | yes |
| total_rows via row @ 500000 linhas | 48.26 | _not measured_ | 24.13 | 24.48 | 24.51 | **no** |
| sum_amount via row @ 500000 linhas | 45.38 | _not measured_ | 22.52 | 22.67 | 22.69 | **no** |
| group_by_category via row @ 500000 linhas | 26.03 | _not measured_ | 39.63 | 41.09 | 41.22 | **no** |
| filtered_sum via row @ 500000 linhas | 38.73 | _not measured_ | 26.22 | 26.32 | 26.32 | **no** |
| total_rows via columnar @ 500000 linhas | 263.9 | _not measured_ | 3.812 | 3.972 | 3.986 | yes |
| sum_amount via columnar @ 500000 linhas | 113.7 | _not measured_ | 8.802 | 9.052 | 9.057 | **no** |
| group_by_category via columnar @ 500000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 500000 linhas | 16.87 | _not measured_ | 59.58 | 59.68 | 59.69 | **no** |
| total_rows via parquet @ 500000 linhas | 0.6397 | _not measured_ | 1,563 | 1,569 | 1,570 | yes |
| sum_amount via parquet @ 500000 linhas | 0.5916 | _not measured_ | 1,705 | 1,712 | 1,712 | yes |
| group_by_category via parquet @ 500000 linhas | 0.5582 | _not measured_ | 1,802 | 1,810 | 1,811 | yes |
| filtered_sum via parquet @ 500000 linhas | 0.605 | _not measured_ | 1,665 | 1,679 | 1,680 | yes |
| total_rows via row @ 1000000 linhas | 39.22 | _not measured_ | 26.01 | 29.87 | 30.21 | **no** |
| sum_amount via row @ 1000000 linhas | 32.12 | _not measured_ | 31.86 | 32.52 | 32.58 | yes |
| group_by_category via row @ 1000000 linhas | 15.89 | _not measured_ | 63.24 | 64.07 | 64.14 | **no** |
| filtered_sum via row @ 1000000 linhas | 27.25 | _not measured_ | 37.23 | 38.1 | 38.19 | yes |
| total_rows via columnar @ 1000000 linhas | 149 | _not measured_ | 6.833 | 7.119 | 7.128 | yes |
| sum_amount via columnar @ 1000000 linhas | 60.46 | _not measured_ | 16.87 | 16.92 | 16.92 | yes |
| group_by_category via columnar @ 1000000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 1000000 linhas | 8.66 | _not measured_ | 115.6 | 118 | 118.2 | yes |
| total_rows via parquet @ 1000000 linhas | 0.3285 | _not measured_ | 3,060 | 3,107 | 3,109 | yes |
| sum_amount via parquet @ 1000000 linhas | 0.2997 | _not measured_ | 3,337 | 3,352 | 3,353 | yes |
| group_by_category via parquet @ 1000000 linhas | 0.2838 | _not measured_ | 3,542 | 3,558 | 3,563 | yes |
| filtered_sum via parquet @ 1000000 linhas | 0.3113 | _not measured_ | 3,219 | 3,244 | 3,246 | yes |
| total_rows via row @ 2000000 linhas | 23.7 | _not measured_ | 49.6 | 50.11 | 50.15 | **no** |
| sum_amount via row @ 2000000 linhas | 19.74 | _not measured_ | 51.55 | 53.05 | 53.19 | **no** |
| group_by_category via row @ 2000000 linhas | 8.811 | _not measured_ | 115.4 | 138.1 | 140.1 | **no** |
| filtered_sum via row @ 2000000 linhas | 15.96 | _not measured_ | 64.26 | 65.24 | 65.33 | **no** |
| total_rows via columnar @ 2000000 linhas | 79.94 | _not measured_ | 12.51 | 12.9 | 12.94 | **no** |
| sum_amount via columnar @ 2000000 linhas | 32.29 | _not measured_ | 32.07 | 32.22 | 32.23 | yes |
| group_by_category via columnar @ 2000000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 2000000 linhas | 4.37 | _not measured_ | 230.8 | 231.2 | 231.2 | yes |
| total_rows via parquet @ 2000000 linhas | 0.1789 | _not measured_ | 5,607 | 5,632 | 5,634 | yes |
| sum_amount via parquet @ 2000000 linhas | 0.1619 | _not measured_ | 6,204 | 6,268 | 6,271 | yes |
| group_by_category via parquet @ 2000000 linhas | 0.1531 | _not measured_ | 6,592 | 6,609 | 6,610 | yes |
| filtered_sum via parquet @ 2000000 linhas | 0.1663 | _not measured_ | 6,032 | 6,053 | 6,055 | yes |

Unstable points are reported, not removed. Their repetitions disagree by more than the declared threshold, so the median below is a weaker claim than it looks:

- `total_rows via row @ 10000 linhas`: latency_p50_ms cv=0.050; throughput_per_second cv=0.059
- `sum_amount via row @ 10000 linhas`: latency_p95_ms cv=0.084; latency_p99_ms cv=0.089
- `filtered_sum via row @ 10000 linhas`: latency_p50_ms cv=0.069; latency_p95_ms cv=0.102; latency_p99_ms cv=0.112; throughput_per_second cv=0.071
- `total_rows via columnar @ 10000 linhas`: latency_p50_ms cv=0.128; latency_p95_ms cv=0.094; latency_p99_ms cv=0.091; throughput_per_second cv=0.085
- `sum_amount via columnar @ 10000 linhas`: latency_p50_ms cv=0.064; latency_p95_ms cv=0.052; latency_p99_ms cv=0.052
- `total_rows via parquet @ 10000 linhas`: latency_p50_ms cv=0.064; latency_p95_ms cv=0.161; latency_p99_ms cv=0.169
- `sum_amount via parquet @ 10000 linhas`: latency_p95_ms cv=0.053; latency_p99_ms cv=0.056
- `total_rows via columnar @ 50000 linhas`: latency_p50_ms cv=0.097; latency_p95_ms cv=0.131; latency_p99_ms cv=0.134
- `sum_amount via columnar @ 50000 linhas`: latency_p50_ms cv=0.051; throughput_per_second cv=0.057
- `total_rows via row @ 100000 linhas`: latency_p50_ms cv=0.063; latency_p95_ms cv=0.052; latency_p99_ms cv=0.051; throughput_per_second cv=0.075
- `filtered_sum via row @ 100000 linhas`: latency_p95_ms cv=0.076; latency_p99_ms cv=0.079
- `total_rows via columnar @ 100000 linhas`: throughput_per_second cv=0.052
- `total_rows via row @ 500000 linhas`: latency_p50_ms cv=0.173; latency_p95_ms cv=0.178; latency_p99_ms cv=0.178; throughput_per_second cv=0.126
- `sum_amount via row @ 500000 linhas`: latency_p95_ms cv=0.142; latency_p99_ms cv=0.151
- `group_by_category via row @ 500000 linhas`: latency_p95_ms cv=0.162; latency_p99_ms cv=0.173
- `filtered_sum via row @ 500000 linhas`: latency_p50_ms cv=0.077; latency_p95_ms cv=0.073; latency_p99_ms cv=0.072; throughput_per_second cv=0.083
- `sum_amount via columnar @ 500000 linhas`: latency_p99_ms cv=0.053
- `filtered_sum via columnar @ 500000 linhas`: latency_p95_ms cv=0.228; latency_p99_ms cv=0.245
- `total_rows via row @ 1000000 linhas`: latency_p50_ms cv=0.142; latency_p95_ms cv=0.138; latency_p99_ms cv=0.140; throughput_per_second cv=0.151
- `group_by_category via row @ 1000000 linhas`: latency_p95_ms cv=0.151; latency_p99_ms cv=0.161
- `total_rows via row @ 2000000 linhas`: latency_p50_ms cv=0.149; latency_p95_ms cv=0.147; latency_p99_ms cv=0.147; throughput_per_second cv=0.137
- `sum_amount via row @ 2000000 linhas`: latency_p50_ms cv=0.088; latency_p95_ms cv=0.207; latency_p99_ms cv=0.217; throughput_per_second cv=0.088
- `group_by_category via row @ 2000000 linhas`: latency_p95_ms cv=0.195; latency_p99_ms cv=0.206; throughput_per_second cv=0.055
- `filtered_sum via row @ 2000000 linhas`: latency_p50_ms cv=0.059; latency_p95_ms cv=0.060; latency_p99_ms cv=0.061; throughput_per_second cv=0.058
- `total_rows via columnar @ 2000000 linhas`: latency_p95_ms cv=0.918; latency_p99_ms cv=0.953

### Repetitions

Every repetition is retained:

- `total_rows via row @ 10000 linhas` latency_p50_ms: 0.5807, 0.5251, 0.5515
- `total_rows via row @ 10000 linhas` latency_p95_ms: 0.6182, 0.5816, 0.6263
- `total_rows via row @ 10000 linhas` latency_p99_ms: 0.6216, 0.5866, 0.6329
- `total_rows via row @ 10000 linhas` throughput_per_second: 1,765, 1,977, 1,935
- `sum_amount via row @ 10000 linhas` latency_p50_ms: 0.6893, 0.7231, 0.6945
- `sum_amount via row @ 10000 linhas` latency_p95_ms: 0.7961, 0.8368, 0.7089
- `sum_amount via row @ 10000 linhas` latency_p99_ms: 0.8056, 0.847, 0.7102
- `sum_amount via row @ 10000 linhas` throughput_per_second: 1,481, 1,434, 1,453
- `group_by_category via row @ 10000 linhas` latency_p50_ms: 1.763, 1.754, 1.688
- `group_by_category via row @ 10000 linhas` latency_p95_ms: 1.791, 1.754, 1.754
- `group_by_category via row @ 10000 linhas` latency_p99_ms: 1.793, 1.755, 1.76
- `group_by_category via row @ 10000 linhas` throughput_per_second: 575.4, 589.3, 594.9
- `filtered_sum via row @ 10000 linhas` latency_p50_ms: 0.8772, 0.7654, 0.8074
- `filtered_sum via row @ 10000 linhas` latency_p95_ms: 0.9321, 0.9902, 0.8077
- `filtered_sum via row @ 10000 linhas` latency_p99_ms: 0.937, 1.01, 0.8077
- `filtered_sum via row @ 10000 linhas` throughput_per_second: 1,169, 1,343, 1,293
- `total_rows via columnar @ 10000 linhas` latency_p50_ms: 0.8659, 0.6705, 0.7578
- `total_rows via columnar @ 10000 linhas` latency_p95_ms: 0.9854, 0.8201, 0.8766
- `total_rows via columnar @ 10000 linhas` latency_p99_ms: 0.996, 0.8334, 0.8872
- `total_rows via columnar @ 10000 linhas` throughput_per_second: 1,264, 1,498, 1,398
- `sum_amount via columnar @ 10000 linhas` latency_p50_ms: 0.9699, 0.8764, 0.8634
- `sum_amount via columnar @ 10000 linhas` latency_p95_ms: 0.9869, 0.9426, 0.8886
- `sum_amount via columnar @ 10000 linhas` latency_p99_ms: 0.9884, 0.9485, 0.8908
- `sum_amount via columnar @ 10000 linhas` throughput_per_second: 1,090, 1,186, 1,168
- `filtered_sum via columnar @ 10000 linhas` latency_p50_ms: 2.142, 2.26, 2.102
- `filtered_sum via columnar @ 10000 linhas` latency_p95_ms: 2.22, 2.325, 2.339
- `filtered_sum via columnar @ 10000 linhas` latency_p99_ms: 2.227, 2.331, 2.36
- `filtered_sum via columnar @ 10000 linhas` throughput_per_second: 467.8, 467.4, 475.8
- `total_rows via parquet @ 10000 linhas` latency_p50_ms: 31.12, 28.05, 27.81
- `total_rows via parquet @ 10000 linhas` latency_p95_ms: 36.94, 28.7, 27.84
- `total_rows via parquet @ 10000 linhas` latency_p99_ms: 37.46, 28.75, 27.84
- `total_rows via parquet @ 10000 linhas` throughput_per_second: 36.09, 36.12, 36.23
- `sum_amount via parquet @ 10000 linhas` latency_p50_ms: 31.47, 31.61, 30.08
- `sum_amount via parquet @ 10000 linhas` latency_p95_ms: 31.83, 34.1, 30.73
- `sum_amount via parquet @ 10000 linhas` latency_p99_ms: 31.86, 34.33, 30.79
- `sum_amount via parquet @ 10000 linhas` throughput_per_second: 32.18, 32.31, 33.75
- `group_by_category via parquet @ 10000 linhas` latency_p50_ms: 33.05, 32.37, 32.92
- `group_by_category via parquet @ 10000 linhas` latency_p95_ms: 33.67, 32.41, 32.94
- `group_by_category via parquet @ 10000 linhas` latency_p99_ms: 33.73, 32.41, 32.94
- `group_by_category via parquet @ 10000 linhas` throughput_per_second: 30.92, 31.19, 30.96
- `filtered_sum via parquet @ 10000 linhas` latency_p50_ms: 29.93, 29.83, 29.89
- `filtered_sum via parquet @ 10000 linhas` latency_p95_ms: 30.19, 30.9, 30.07
- `filtered_sum via parquet @ 10000 linhas` latency_p99_ms: 30.21, 30.99, 30.08
- `filtered_sum via parquet @ 10000 linhas` throughput_per_second: 33.84, 33.57, 34.03
- `total_rows via row @ 50000 linhas` latency_p50_ms: 2.482, 2.602, 2.472
- `total_rows via row @ 50000 linhas` latency_p95_ms: 2.603, 2.671, 2.748
- `total_rows via row @ 50000 linhas` latency_p99_ms: 2.613, 2.677, 2.773
- `total_rows via row @ 50000 linhas` throughput_per_second: 422.8, 419.9, 408.6
- `sum_amount via row @ 50000 linhas` latency_p50_ms: 2.987, 3.046, 2.931
- `sum_amount via row @ 50000 linhas` latency_p95_ms: 3.077, 3.079, 2.997
- `sum_amount via row @ 50000 linhas` latency_p99_ms: 3.085, 3.082, 3.003
- `sum_amount via row @ 50000 linhas` throughput_per_second: 346.5, 331.1, 356.6
- `group_by_category via row @ 50000 linhas` latency_p50_ms: 7.997, 8.373, 7.723
- `group_by_category via row @ 50000 linhas` latency_p95_ms: 8.224, 8.424, 7.726
- `group_by_category via row @ 50000 linhas` latency_p99_ms: 8.244, 8.428, 7.727
- `group_by_category via row @ 50000 linhas` throughput_per_second: 128.1, 123.6, 131
- `filtered_sum via row @ 50000 linhas` latency_p50_ms: 3.882, 3.856, 3.934
- `filtered_sum via row @ 50000 linhas` latency_p95_ms: 4.03, 3.947, 3.946
- `filtered_sum via row @ 50000 linhas` latency_p99_ms: 4.043, 3.955, 3.947
- `filtered_sum via row @ 50000 linhas` throughput_per_second: 266.6, 262.2, 261.1
- `total_rows via columnar @ 50000 linhas` latency_p50_ms: 1.003, 1.039, 1.201
- `total_rows via columnar @ 50000 linhas` latency_p95_ms: 1.192, 1.164, 1.468
- `total_rows via columnar @ 50000 linhas` latency_p99_ms: 1.209, 1.176, 1.491
- `total_rows via columnar @ 50000 linhas` throughput_per_second: 1,034, 973.3, 980.2
- `sum_amount via columnar @ 50000 linhas` latency_p50_ms: 1.707, 1.753, 1.587
- `sum_amount via columnar @ 50000 linhas` latency_p95_ms: 1.741, 1.754, 1.637
- `sum_amount via columnar @ 50000 linhas` latency_p99_ms: 1.744, 1.754, 1.641
- `sum_amount via columnar @ 50000 linhas` throughput_per_second: 597.6, 589.6, 654.2
- `filtered_sum via columnar @ 50000 linhas` latency_p50_ms: 6.907, 6.961, 6.915
- `filtered_sum via columnar @ 50000 linhas` latency_p95_ms: 6.924, 6.984, 7.026
- `filtered_sum via columnar @ 50000 linhas` latency_p99_ms: 6.926, 6.986, 7.036
- `filtered_sum via columnar @ 50000 linhas` throughput_per_second: 145.3, 149.3, 144.6
- `total_rows via parquet @ 50000 linhas` latency_p50_ms: 153.9, 153.3, 154
- `total_rows via parquet @ 50000 linhas` latency_p95_ms: 155.2, 153.3, 155
- `total_rows via parquet @ 50000 linhas` latency_p99_ms: 155.3, 153.3, 155.1
- `total_rows via parquet @ 50000 linhas` throughput_per_second: 6.522, 6.555, 6.534
- `sum_amount via parquet @ 50000 linhas` latency_p50_ms: 166.2, 167.4, 168.7
- `sum_amount via parquet @ 50000 linhas` latency_p95_ms: 167.9, 168.7, 170.1
- `sum_amount via parquet @ 50000 linhas` latency_p99_ms: 168, 168.8, 170.2
- `sum_amount via parquet @ 50000 linhas` throughput_per_second: 6.044, 6.043, 5.938
- `group_by_category via parquet @ 50000 linhas` latency_p50_ms: 176.5, 177, 175.3
- `group_by_category via parquet @ 50000 linhas` latency_p95_ms: 177.9, 177.9, 177.7
- `group_by_category via parquet @ 50000 linhas` latency_p99_ms: 178, 178, 177.9
- `group_by_category via parquet @ 50000 linhas` throughput_per_second: 5.679, 5.682, 5.73
- `filtered_sum via parquet @ 50000 linhas` latency_p50_ms: 162.1, 161.9, 161.9
- `filtered_sum via parquet @ 50000 linhas` latency_p95_ms: 163.3, 164, 163.4
- `filtered_sum via parquet @ 50000 linhas` latency_p99_ms: 163.4, 164.1, 163.5
- `filtered_sum via parquet @ 50000 linhas` throughput_per_second: 6.202, 6.189, 6.204
- `total_rows via row @ 100000 linhas` latency_p50_ms: 4.779, 5.321, 4.786
- `total_rows via row @ 100000 linhas` latency_p95_ms: 4.99, 5.47, 5.019
- `total_rows via row @ 100000 linhas` latency_p99_ms: 5.008, 5.483, 5.04
- `total_rows via row @ 100000 linhas` throughput_per_second: 221.4, 192.8, 219.1
- `sum_amount via row @ 100000 linhas` latency_p50_ms: 6.126, 6.452, 5.847
- `sum_amount via row @ 100000 linhas` latency_p95_ms: 6.131, 6.585, 6.002
- `sum_amount via row @ 100000 linhas` latency_p99_ms: 6.131, 6.597, 6.016
- `sum_amount via row @ 100000 linhas` throughput_per_second: 167.7, 158.1, 171.3
- `group_by_category via row @ 100000 linhas` latency_p50_ms: 15.66, 15.76, 15.81
- `group_by_category via row @ 100000 linhas` latency_p95_ms: 15.73, 15.95, 15.82
- `group_by_category via row @ 100000 linhas` latency_p99_ms: 15.73, 15.96, 15.82
- `group_by_category via row @ 100000 linhas` throughput_per_second: 67.11, 64.2, 65.27
- `filtered_sum via row @ 100000 linhas` latency_p50_ms: 8.003, 7.61, 8.343
- `filtered_sum via row @ 100000 linhas` latency_p95_ms: 8.037, 7.693, 8.902
- `filtered_sum via row @ 100000 linhas` latency_p99_ms: 8.04, 7.701, 8.952
- `filtered_sum via row @ 100000 linhas` throughput_per_second: 128.4, 131.8, 125.6
- `total_rows via columnar @ 100000 linhas` latency_p50_ms: 1.337, 1.283, 1.313
- `total_rows via columnar @ 100000 linhas` latency_p95_ms: 1.395, 1.405, 1.483
- `total_rows via columnar @ 100000 linhas` latency_p99_ms: 1.4, 1.415, 1.498
- `total_rows via columnar @ 100000 linhas` throughput_per_second: 846.1, 824.5, 765
- `sum_amount via columnar @ 100000 linhas` latency_p50_ms: 2.374, 2.307, 2.416
- `sum_amount via columnar @ 100000 linhas` latency_p95_ms: 2.503, 2.311, 2.453
- `sum_amount via columnar @ 100000 linhas` latency_p99_ms: 2.515, 2.312, 2.456
- `sum_amount via columnar @ 100000 linhas` throughput_per_second: 423.9, 440.9, 417.5
- `filtered_sum via columnar @ 100000 linhas` latency_p50_ms: 12.33, 12.63, 12.99
- `filtered_sum via columnar @ 100000 linhas` latency_p95_ms: 12.79, 12.99, 13.39
- `filtered_sum via columnar @ 100000 linhas` latency_p99_ms: 12.83, 13.03, 13.42
- `filtered_sum via columnar @ 100000 linhas` throughput_per_second: 81.31, 79.36, 77.24
- `total_rows via parquet @ 100000 linhas` latency_p50_ms: 306.3, 308.1, 306.5
- `total_rows via parquet @ 100000 linhas` latency_p95_ms: 312.5, 308.2, 312.2
- `total_rows via parquet @ 100000 linhas` latency_p99_ms: 313, 308.2, 312.7
- `total_rows via parquet @ 100000 linhas` throughput_per_second: 3.273, 3.294, 3.301
- `sum_amount via parquet @ 100000 linhas` latency_p50_ms: 335.6, 336, 336.4
- `sum_amount via parquet @ 100000 linhas` latency_p95_ms: 337.4, 336.1, 338.5
- `sum_amount via parquet @ 100000 linhas` latency_p99_ms: 337.6, 336.1, 338.7
- `sum_amount via parquet @ 100000 linhas` throughput_per_second: 3, 2.987, 3.017
- `group_by_category via parquet @ 100000 linhas` latency_p50_ms: 353.3, 357, 352.3
- `group_by_category via parquet @ 100000 linhas` latency_p95_ms: 355.7, 358.5, 352.9
- `group_by_category via parquet @ 100000 linhas` latency_p99_ms: 356, 358.6, 352.9
- `group_by_category via parquet @ 100000 linhas` throughput_per_second: 2.842, 2.833, 2.878
- `filtered_sum via parquet @ 100000 linhas` latency_p50_ms: 324.6, 331.1, 330.6
- `filtered_sum via parquet @ 100000 linhas` latency_p95_ms: 325.6, 331.4, 334.7
- `filtered_sum via parquet @ 100000 linhas` latency_p99_ms: 325.7, 331.4, 335.1
- `filtered_sum via parquet @ 100000 linhas` throughput_per_second: 3.105, 3.038, 3.083
- `total_rows via row @ 500000 linhas` latency_p50_ms: 24.13, 24.56, 17.72
- `total_rows via row @ 500000 linhas` latency_p95_ms: 24.48, 25.25, 17.95
- `total_rows via row @ 500000 linhas` latency_p99_ms: 24.51, 25.32, 17.97
- `total_rows via row @ 500000 linhas` throughput_per_second: 48.26, 47.98, 59.46
- `sum_amount via row @ 500000 linhas` latency_p50_ms: 22.96, 22.52, 20.88
- `sum_amount via row @ 500000 linhas` latency_p95_ms: 27.47, 22.67, 20.97
- `sum_amount via row @ 500000 linhas` latency_p99_ms: 27.87, 22.69, 20.97
- `sum_amount via row @ 500000 linhas` throughput_per_second: 44.83, 45.38, 48.1
- `group_by_category via row @ 500000 linhas` latency_p50_ms: 40.61, 39.63, 37.88
- `group_by_category via row @ 500000 linhas` latency_p95_ms: 51.97, 41.09, 38.57
- `group_by_category via row @ 500000 linhas` latency_p99_ms: 52.98, 41.22, 38.63
- `group_by_category via row @ 500000 linhas` throughput_per_second: 25.45, 26.03, 27.46
- `filtered_sum via row @ 500000 linhas` latency_p50_ms: 26.22, 26.26, 22.88
- `filtered_sum via row @ 500000 linhas` latency_p95_ms: 26.36, 26.32, 23.16
- `filtered_sum via row @ 500000 linhas` latency_p99_ms: 26.37, 26.32, 23.19
- `filtered_sum via row @ 500000 linhas` throughput_per_second: 38.73, 38.52, 44.47
- `total_rows via columnar @ 500000 linhas` latency_p50_ms: 3.812, 4.062, 3.751
- `total_rows via columnar @ 500000 linhas` latency_p95_ms: 3.972, 4.074, 3.858
- `total_rows via columnar @ 500000 linhas` latency_p99_ms: 3.986, 4.075, 3.868
- `total_rows via columnar @ 500000 linhas` throughput_per_second: 263.9, 254.8, 273.7
- `sum_amount via columnar @ 500000 linhas` latency_p50_ms: 8.995, 8.802, 8.625
- `sum_amount via columnar @ 500000 linhas` latency_p95_ms: 9.052, 9.563, 8.672
- `sum_amount via columnar @ 500000 linhas` latency_p99_ms: 9.057, 9.63, 8.676
- `sum_amount via columnar @ 500000 linhas` throughput_per_second: 111.6, 113.7, 118.3
- `filtered_sum via columnar @ 500000 linhas` latency_p50_ms: 58.88, 61.31, 59.58
- `filtered_sum via columnar @ 500000 linhas` latency_p95_ms: 59.08, 86.45, 59.68
- `filtered_sum via columnar @ 500000 linhas` latency_p99_ms: 59.1, 88.68, 59.69
- `filtered_sum via columnar @ 500000 linhas` throughput_per_second: 17.01, 16.87, 16.82
- `total_rows via parquet @ 500000 linhas` latency_p50_ms: 1,577, 1,560, 1,563
- `total_rows via parquet @ 500000 linhas` latency_p95_ms: 1,577, 1,569, 1,566
- `total_rows via parquet @ 500000 linhas` latency_p99_ms: 1,577, 1,570, 1,566
- `total_rows via parquet @ 500000 linhas` throughput_per_second: 0.6377, 0.6421, 0.6397
- `sum_amount via parquet @ 500000 linhas` latency_p50_ms: 1,707, 1,705, 1,703
- `sum_amount via parquet @ 500000 linhas` latency_p95_ms: 1,712, 1,711, 1,732
- `sum_amount via parquet @ 500000 linhas` latency_p99_ms: 1,712, 1,711, 1,735
- `sum_amount via parquet @ 500000 linhas` throughput_per_second: 0.5916, 0.5867, 0.5932
- `group_by_category via parquet @ 500000 linhas` latency_p50_ms: 1,803, 1,794, 1,802
- `group_by_category via parquet @ 500000 linhas` latency_p95_ms: 1,806, 1,810, 1,812
- `group_by_category via parquet @ 500000 linhas` latency_p99_ms: 1,806, 1,811, 1,813
- `group_by_category via parquet @ 500000 linhas` throughput_per_second: 0.5582, 0.5687, 0.557
- `filtered_sum via parquet @ 500000 linhas` latency_p50_ms: 1,648, 1,669, 1,665
- `filtered_sum via parquet @ 500000 linhas` latency_p95_ms: 1,670, 1,683, 1,679
- `filtered_sum via parquet @ 500000 linhas` latency_p99_ms: 1,672, 1,684, 1,680
- `filtered_sum via parquet @ 500000 linhas` throughput_per_second: 0.6072, 0.6013, 0.605
- `total_rows via row @ 1000000 linhas` latency_p50_ms: 31.51, 24.06, 26.01
- `total_rows via row @ 1000000 linhas` latency_p95_ms: 31.69, 24.11, 29.87
- `total_rows via row @ 1000000 linhas` latency_p99_ms: 31.71, 24.12, 30.21
- `total_rows via row @ 1000000 linhas` throughput_per_second: 32.13, 43.62, 39.22
- `sum_amount via row @ 1000000 linhas` latency_p50_ms: 31.86, 31.36, 33.14
- `sum_amount via row @ 1000000 linhas` latency_p95_ms: 32.52, 31.62, 33.62
- `sum_amount via row @ 1000000 linhas` latency_p99_ms: 32.58, 31.64, 33.67
- `sum_amount via row @ 1000000 linhas` throughput_per_second: 32.12, 32.54, 30.3
- `group_by_category via row @ 1000000 linhas` latency_p50_ms: 63.24, 62.94, 66.2
- `group_by_category via row @ 1000000 linhas` latency_p95_ms: 64.07, 62.95, 81.67
- `group_by_category via row @ 1000000 linhas` latency_p99_ms: 64.14, 62.95, 83.04
- `group_by_category via row @ 1000000 linhas` throughput_per_second: 15.95, 15.89, 15.61
- `filtered_sum via row @ 1000000 linhas` latency_p50_ms: 37.11, 37.23, 38.71
- `filtered_sum via row @ 1000000 linhas` latency_p95_ms: 38.1, 37.77, 38.78
- `filtered_sum via row @ 1000000 linhas` latency_p99_ms: 38.19, 37.81, 38.79
- `filtered_sum via row @ 1000000 linhas` throughput_per_second: 27.25, 28.15, 26.17
- `total_rows via columnar @ 1000000 linhas` latency_p50_ms: 7.015, 6.833, 6.721
- `total_rows via columnar @ 1000000 linhas` latency_p95_ms: 7.119, 7.119, 6.838
- `total_rows via columnar @ 1000000 linhas` latency_p99_ms: 7.128, 7.144, 6.849
- `total_rows via columnar @ 1000000 linhas` throughput_per_second: 151.2, 147.4, 149
- `sum_amount via columnar @ 1000000 linhas` latency_p50_ms: 15.88, 16.88, 16.87
- `sum_amount via columnar @ 1000000 linhas` latency_p95_ms: 16.12, 17.14, 16.92
- `sum_amount via columnar @ 1000000 linhas` latency_p99_ms: 16.14, 17.16, 16.92
- `sum_amount via columnar @ 1000000 linhas` throughput_per_second: 63.27, 59.88, 60.46
- `filtered_sum via columnar @ 1000000 linhas` latency_p50_ms: 117.1, 115.6, 114.2
- `filtered_sum via columnar @ 1000000 linhas` latency_p95_ms: 118.1, 118, 115.9
- `filtered_sum via columnar @ 1000000 linhas` latency_p99_ms: 118.2, 118.2, 116.1
- `filtered_sum via columnar @ 1000000 linhas` throughput_per_second: 8.606, 8.66, 8.78
- `total_rows via parquet @ 1000000 linhas` latency_p50_ms: 3,060, 3,050, 3,088
- `total_rows via parquet @ 1000000 linhas` latency_p95_ms: 3,062, 3,115, 3,107
- `total_rows via parquet @ 1000000 linhas` latency_p99_ms: 3,062, 3,121, 3,109
- `total_rows via parquet @ 1000000 linhas` throughput_per_second: 0.3269, 0.3287, 0.3285
- `sum_amount via parquet @ 1000000 linhas` latency_p50_ms: 3,331, 3,395, 3,337
- `sum_amount via parquet @ 1000000 linhas` latency_p95_ms: 3,348, 3,417, 3,352
- `sum_amount via parquet @ 1000000 linhas` latency_p99_ms: 3,350, 3,419, 3,353
- `sum_amount via parquet @ 1000000 linhas` throughput_per_second: 0.3002, 0.2956, 0.2997
- `group_by_category via parquet @ 1000000 linhas` latency_p50_ms: 3,501, 3,576, 3,542
- `group_by_category via parquet @ 1000000 linhas` latency_p95_ms: 3,558, 3,602, 3,547
- `group_by_category via parquet @ 1000000 linhas` latency_p99_ms: 3,563, 3,604, 3,548
- `group_by_category via parquet @ 1000000 linhas` throughput_per_second: 0.2878, 0.2826, 0.2838
- `filtered_sum via parquet @ 1000000 linhas` latency_p50_ms: 3,241, 3,214, 3,219
- `filtered_sum via parquet @ 1000000 linhas` latency_p95_ms: 3,246, 3,244, 3,231
- `filtered_sum via parquet @ 1000000 linhas` latency_p99_ms: 3,246, 3,247, 3,232
- `filtered_sum via parquet @ 1000000 linhas` throughput_per_second: 0.3116, 0.3113, 0.311
- `total_rows via row @ 2000000 linhas` latency_p50_ms: 50.17, 38.02, 49.6
- `total_rows via row @ 2000000 linhas` latency_p95_ms: 51.04, 38.7, 50.11
- `total_rows via row @ 2000000 linhas` latency_p99_ms: 51.11, 38.77, 50.15
- `total_rows via row @ 2000000 linhas` throughput_per_second: 20.02, 26.39, 23.7
- `sum_amount via row @ 2000000 linhas` latency_p50_ms: 59.8, 51.51, 51.55
- `sum_amount via row @ 2000000 linhas` latency_p95_ms: 74, 53.05, 52.15
- `sum_amount via row @ 2000000 linhas` latency_p99_ms: 75.27, 53.19, 52.21
- `sum_amount via row @ 2000000 linhas` throughput_per_second: 17.15, 19.74, 20.31
- `group_by_category via row @ 2000000 linhas` latency_p50_ms: 125.1, 115.4, 114.9
- `group_by_category via row @ 2000000 linhas` latency_p95_ms: 169.7, 138.1, 114.9
- `group_by_category via row @ 2000000 linhas` latency_p99_ms: 173.6, 140.1, 114.9
- `group_by_category via row @ 2000000 linhas` throughput_per_second: 8.032, 8.872, 8.811
- `filtered_sum via row @ 2000000 linhas` latency_p50_ms: 70.26, 62.98, 64.26
- `filtered_sum via row @ 2000000 linhas` latency_p95_ms: 71.46, 63.87, 65.24
- `filtered_sum via row @ 2000000 linhas` latency_p99_ms: 71.56, 63.94, 65.33
- `filtered_sum via row @ 2000000 linhas` throughput_per_second: 14.44, 15.96, 16.01
- `total_rows via columnar @ 2000000 linhas` latency_p50_ms: 12.51, 13.44, 12.47
- `total_rows via columnar @ 2000000 linhas` latency_p95_ms: 12.73, 56.17, 12.9
- `total_rows via columnar @ 2000000 linhas` latency_p99_ms: 12.75, 59.97, 12.94
- `total_rows via columnar @ 2000000 linhas` throughput_per_second: 79.94, 75.47, 81.59
- `sum_amount via columnar @ 2000000 linhas` latency_p50_ms: 31.83, 32.74, 32.07
- `sum_amount via columnar @ 2000000 linhas` latency_p95_ms: 31.9, 32.99, 32.22
- `sum_amount via columnar @ 2000000 linhas` latency_p99_ms: 31.9, 33.01, 32.23
- `sum_amount via columnar @ 2000000 linhas` throughput_per_second: 32.29, 30.99, 32.34
- `filtered_sum via columnar @ 2000000 linhas` latency_p50_ms: 230.8, 232.1, 228.1
- `filtered_sum via columnar @ 2000000 linhas` latency_p95_ms: 231.2, 233.6, 228.2
- `filtered_sum via columnar @ 2000000 linhas` latency_p99_ms: 231.2, 233.7, 228.2
- `filtered_sum via columnar @ 2000000 linhas` throughput_per_second: 4.37, 4.325, 4.403
- `total_rows via parquet @ 2000000 linhas` latency_p50_ms: 5,600, 5,613, 5,607
- `total_rows via parquet @ 2000000 linhas` latency_p95_ms: 5,636, 5,632, 5,622
- `total_rows via parquet @ 2000000 linhas` latency_p99_ms: 5,639, 5,634, 5,624
- `total_rows via parquet @ 2000000 linhas` throughput_per_second: 0.1789, 0.1801, 0.1787
- `sum_amount via parquet @ 2000000 linhas` latency_p50_ms: 6,204, 6,189, 6,236
- `sum_amount via parquet @ 2000000 linhas` latency_p95_ms: 6,217, 6,375, 6,268
- `sum_amount via parquet @ 2000000 linhas` latency_p99_ms: 6,218, 6,392, 6,271
- `sum_amount via parquet @ 2000000 linhas` throughput_per_second: 0.1619, 0.1623, 0.1606
- `group_by_category via parquet @ 2000000 linhas` latency_p50_ms: 6,545, 6,596, 6,592
- `group_by_category via parquet @ 2000000 linhas` latency_p95_ms: 6,567, 6,848, 6,609
- `group_by_category via parquet @ 2000000 linhas` latency_p99_ms: 6,569, 6,870, 6,610
- `group_by_category via parquet @ 2000000 linhas` throughput_per_second: 0.1531, 0.1533, 0.1524
- `filtered_sum via parquet @ 2000000 linhas` latency_p50_ms: 6,017, 6,032, 6,035
- `filtered_sum via parquet @ 2000000 linhas` latency_p95_ms: 6,059, 6,053, 6,053
- `filtered_sum via parquet @ 2000000 linhas` latency_p99_ms: 6,063, 6,054, 6,055
- `filtered_sum via parquet @ 2000000 linhas` throughput_per_second: 0.1667, 0.1659, 0.1663

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
| process_containment | PASS | no |  |
| cpu_limit | UNAVAILABLE | no | unavailable: no CPU set declared |
| memory_limit | UNAVAILABLE | no | unavailable: no memory bound declared |
| no_oom | PASS | yes |  |
| telemetry_complete | PASS | no |  |
| quality_reported | PASS | yes |  |
| clean_source_tree | UNAVAILABLE | no | unavailable: git status failed |

## Environment

- Host: theo-b097-20260821
- CPU: Intel(R) Xeon(R) Platinum 8280 CPU @ 2.70GHz (16 logical, 16 physical)
- SMT: False · Governor: _unavailable_
- Memory: 67424509952 bytes
- Kernel: 6.8.0-124-generic · Runner: theodb-bench 0.6.0
- Benchmark commit: _none_ (dirty: _none_)

Fields shown in italics were not available on this host and are recorded as absent rather than as zero.

