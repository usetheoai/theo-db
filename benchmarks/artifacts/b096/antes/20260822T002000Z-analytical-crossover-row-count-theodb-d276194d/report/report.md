# analytical/crossover/row-count on theodb

**Status:** VALID · **Profile:** nightly · **Run:** `20260822T002000Z-analytical-crossover-row-count-theodb-d276194d`

> This result is **not publishable evidence**: the profile it ran under does not freeze methodology or datasets.

## Results

| Configuration | Throughput/s | Recall | p50 ms | p95 ms | p99 ms | Stable |
|---|---|---|---|---|---|---|
| total_rows via row @ 10000 linhas | 1,739 | _not measured_ | 0.595 | 0.6885 | 0.6969 | **no** |
| sum_amount via row @ 10000 linhas | 1,485 | _not measured_ | 0.7327 | 0.7821 | 0.7837 | **no** |
| group_by_category via row @ 10000 linhas | 574.4 | _not measured_ | 1.818 | 1.869 | 1.874 | yes |
| filtered_sum via row @ 10000 linhas | 1,181 | _not measured_ | 0.8597 | 0.9594 | 0.9661 | **no** |
| total_rows via columnar @ 10000 linhas | 1,325 | _not measured_ | 0.7803 | 0.9944 | 1.014 | yes |
| sum_amount via columnar @ 10000 linhas | 1,117 | _not measured_ | 0.9253 | 0.9689 | 0.9728 | **no** |
| group_by_category via columnar @ 10000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 10000 linhas | 454.1 | _not measured_ | 2.264 | 2.407 | 2.425 | **no** |
| total_rows via parquet @ 10000 linhas | 33.33 | _not measured_ | 30.89 | 32.44 | 32.58 | **no** |
| sum_amount via parquet @ 10000 linhas | 27.25 | _not measured_ | 36.88 | 37.72 | 37.8 | **no** |
| group_by_category via parquet @ 10000 linhas | 27.44 | _not measured_ | 37.76 | 38.29 | 38.3 | yes |
| filtered_sum via parquet @ 10000 linhas | 30.31 | _not measured_ | 35.29 | 35.92 | 35.98 | yes |
| total_rows via row @ 50000 linhas | 383.6 | _not measured_ | 2.767 | 2.894 | 2.905 | **no** |
| sum_amount via row @ 50000 linhas | 320.8 | _not measured_ | 3.281 | 3.362 | 3.364 | yes |
| group_by_category via row @ 50000 linhas | 123.6 | _not measured_ | 8.207 | 8.352 | 8.368 | yes |
| filtered_sum via row @ 50000 linhas | 247.6 | _not measured_ | 4.07 | 4.073 | 4.073 | yes |
| total_rows via columnar @ 50000 linhas | 989.7 | _not measured_ | 1.056 | 1.17 | 1.186 | **no** |
| sum_amount via columnar @ 50000 linhas | 622.4 | _not measured_ | 1.618 | 1.697 | 1.705 | **no** |
| group_by_category via columnar @ 50000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 50000 linhas | 132.7 | _not measured_ | 7.563 | 7.713 | 7.726 | yes |
| total_rows via parquet @ 50000 linhas | 6.333 | _not measured_ | 160 | 160.4 | 160.4 | yes |
| sum_amount via parquet @ 50000 linhas | 5.735 | _not measured_ | 175.2 | 175.2 | 175.2 | yes |
| group_by_category via parquet @ 50000 linhas | 5.458 | _not measured_ | 184.7 | 185.1 | 185.1 | yes |
| filtered_sum via parquet @ 50000 linhas | 5.897 | _not measured_ | 170 | 170.8 | 171 | yes |
| total_rows via row @ 100000 linhas | 186.3 | _not measured_ | 5.51 | 5.607 | 5.619 | yes |
| sum_amount via row @ 100000 linhas | 154.6 | _not measured_ | 6.489 | 6.585 | 6.593 | **no** |
| group_by_category via row @ 100000 linhas | 60.05 | _not measured_ | 16.91 | 17.36 | 17.38 | yes |
| filtered_sum via row @ 100000 linhas | 120.8 | _not measured_ | 8.397 | 8.505 | 8.508 | yes |
| total_rows via columnar @ 100000 linhas | 747.7 | _not measured_ | 1.373 | 1.616 | 1.638 | yes |
| sum_amount via columnar @ 100000 linhas | 399.5 | _not measured_ | 2.608 | 2.616 | 2.616 | yes |
| group_by_category via columnar @ 100000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 100000 linhas | 69.62 | _not measured_ | 14.37 | 14.6 | 14.6 | yes |
| total_rows via parquet @ 100000 linhas | 3.499 | _not measured_ | 287.5 | 289.4 | 289.7 | yes |
| sum_amount via parquet @ 100000 linhas | 3.186 | _not measured_ | 317.3 | 318.6 | 318.7 | yes |
| group_by_category via parquet @ 100000 linhas | 3.008 | _not measured_ | 336.5 | 338.4 | 338.6 | yes |
| filtered_sum via parquet @ 100000 linhas | 3.268 | _not measured_ | 309.7 | 311 | 311.2 | yes |
| total_rows via row @ 500000 linhas | 49.26 | _not measured_ | 20.41 | 20.77 | 20.8 | **no** |
| sum_amount via row @ 500000 linhas | 42.71 | _not measured_ | 24.09 | 24.38 | 24.4 | **no** |
| group_by_category via row @ 500000 linhas | 24.19 | _not measured_ | 42.38 | 42.91 | 42.95 | **no** |
| filtered_sum via row @ 500000 linhas | 38.67 | _not measured_ | 25.99 | 26.13 | 26.14 | **no** |
| total_rows via columnar @ 500000 linhas | 247.9 | _not measured_ | 4.048 | 4.275 | 4.296 | **no** |
| sum_amount via columnar @ 500000 linhas | 107.2 | _not measured_ | 9.479 | 9.551 | 9.557 | yes |
| group_by_category via columnar @ 500000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 500000 linhas | 15.71 | _not measured_ | 64.07 | 65.02 | 65.11 | yes |
| total_rows via parquet @ 500000 linhas | 0.6273 | _not measured_ | 1,601 | 1,602 | 1,602 | yes |
| sum_amount via parquet @ 500000 linhas | 0.5764 | _not measured_ | 1,740 | 1,748 | 1,749 | yes |
| group_by_category via parquet @ 500000 linhas | 0.5498 | _not measured_ | 1,825 | 1,840 | 1,841 | yes |
| filtered_sum via parquet @ 500000 linhas | 0.5939 | _not measured_ | 1,686 | 1,696 | 1,697 | yes |
| total_rows via row @ 1000000 linhas | 36.24 | _not measured_ | 27.75 | 32.83 | 33.29 | **no** |
| sum_amount via row @ 1000000 linhas | 29.38 | _not measured_ | 34.33 | 34.64 | 34.66 | yes |
| group_by_category via row @ 1000000 linhas | 15.08 | _not measured_ | 67.01 | 67.86 | 68.01 | yes |
| filtered_sum via row @ 1000000 linhas | 25.36 | _not measured_ | 40.13 | 40.71 | 40.76 | **no** |
| total_rows via columnar @ 1000000 linhas | 134.6 | _not measured_ | 7.61 | 7.631 | 7.636 | yes |
| sum_amount via columnar @ 1000000 linhas | 58.53 | _not measured_ | 17.39 | 17.59 | 17.61 | yes |
| group_by_category via columnar @ 1000000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 1000000 linhas | 8.161 | _not measured_ | 123.4 | 124.3 | 124.4 | yes |
| total_rows via parquet @ 1000000 linhas | 0.3142 | _not measured_ | 3,188 | 3,209 | 3,212 | yes |
| sum_amount via parquet @ 1000000 linhas | 0.2871 | _not measured_ | 3,502 | 3,515 | 3,518 | yes |
| group_by_category via parquet @ 1000000 linhas | 0.2717 | _not measured_ | 3,706 | 3,715 | 3,716 | yes |
| filtered_sum via parquet @ 1000000 linhas | 0.294 | _not measured_ | 3,414 | 3,423 | 3,424 | yes |
| total_rows via row @ 2000000 linhas | 23.22 | _not measured_ | 43.18 | 44.59 | 44.63 | **no** |
| sum_amount via row @ 2000000 linhas | 18.16 | _not measured_ | 55.16 | 56.77 | 56.91 | **no** |
| group_by_category via row @ 2000000 linhas | 8.28 | _not measured_ | 121.9 | 122.2 | 122.3 | **no** |
| filtered_sum via row @ 2000000 linhas | 15.09 | _not measured_ | 68.25 | 69.57 | 69.63 | **no** |
| total_rows via columnar @ 2000000 linhas | 72.07 | _not measured_ | 14.24 | 49.12 | 52.22 | **no** |
| sum_amount via columnar @ 2000000 linhas | 29.82 | _not measured_ | 34.1 | 34.51 | 34.54 | yes |
| group_by_category via columnar @ 2000000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 2000000 linhas | 4.055 | _not measured_ | 247.6 | 249.5 | 249.7 | yes |
| total_rows via parquet @ 2000000 linhas | 0.1687 | _not measured_ | 5,963 | 6,123 | 6,135 | yes |
| sum_amount via parquet @ 2000000 linhas | 0.1534 | _not measured_ | 6,616 | 6,733 | 6,754 | yes |
| group_by_category via parquet @ 2000000 linhas | 0.1457 | _not measured_ | 6,903 | 7,062 | 7,075 | yes |
| filtered_sum via parquet @ 2000000 linhas | 0.1589 | _not measured_ | 6,309 | 6,553 | 6,568 | yes |

Unstable points are reported, not removed. Their repetitions disagree by more than the declared threshold, so the median below is a weaker claim than it looks:

- `total_rows via row @ 10000 linhas`: latency_p50_ms cv=0.060; latency_p95_ms cv=0.079; latency_p99_ms cv=0.082; throughput_per_second cv=0.086
- `sum_amount via row @ 10000 linhas`: latency_p95_ms cv=0.155; latency_p99_ms cv=0.166
- `filtered_sum via row @ 10000 linhas`: latency_p95_ms cv=0.090; latency_p99_ms cv=0.098
- `sum_amount via columnar @ 10000 linhas`: latency_p99_ms cv=0.052
- `filtered_sum via columnar @ 10000 linhas`: latency_p50_ms cv=0.164; latency_p95_ms cv=0.143; latency_p99_ms cv=0.142; throughput_per_second cv=0.159
- `total_rows via parquet @ 10000 linhas`: latency_p50_ms cv=0.052
- `sum_amount via parquet @ 10000 linhas`: throughput_per_second cv=0.073
- `total_rows via row @ 50000 linhas`: latency_p95_ms cv=0.052; latency_p99_ms cv=0.052; throughput_per_second cv=0.053
- `total_rows via columnar @ 50000 linhas`: latency_p95_ms cv=0.067; latency_p99_ms cv=0.069
- `sum_amount via columnar @ 50000 linhas`: latency_p99_ms cv=0.054
- `sum_amount via row @ 100000 linhas`: latency_p50_ms cv=0.428; latency_p95_ms cv=0.418; latency_p99_ms cv=0.417; throughput_per_second cv=0.087
- `total_rows via row @ 500000 linhas`: latency_p50_ms cv=0.190; latency_p95_ms cv=0.215; latency_p99_ms cv=0.217; throughput_per_second cv=0.085
- `sum_amount via row @ 500000 linhas`: latency_p95_ms cv=0.055; latency_p99_ms cv=0.064; throughput_per_second cv=0.055
- `group_by_category via row @ 500000 linhas`: latency_p50_ms cv=0.057; latency_p95_ms cv=0.051; latency_p99_ms cv=0.050
- `filtered_sum via row @ 500000 linhas`: latency_p50_ms cv=0.052; latency_p95_ms cv=0.160; latency_p99_ms cv=0.169; throughput_per_second cv=0.060
- `total_rows via columnar @ 500000 linhas`: latency_p50_ms cv=0.066; throughput_per_second cv=0.082
- `total_rows via row @ 1000000 linhas`: latency_p50_ms cv=0.123; latency_p95_ms cv=0.118; latency_p99_ms cv=0.121; throughput_per_second cv=0.115
- `filtered_sum via row @ 1000000 linhas`: latency_p95_ms cv=0.090; latency_p99_ms cv=0.097
- `total_rows via row @ 2000000 linhas`: latency_p95_ms cv=0.108; latency_p99_ms cv=0.117
- `sum_amount via row @ 2000000 linhas`: latency_p95_ms cv=0.145; latency_p99_ms cv=0.157
- `group_by_category via row @ 2000000 linhas`: latency_p95_ms cv=0.175; latency_p99_ms cv=0.186
- `filtered_sum via row @ 2000000 linhas`: latency_p95_ms cv=0.144; latency_p99_ms cv=0.155
- `total_rows via columnar @ 2000000 linhas`: latency_p95_ms cv=0.552; latency_p99_ms cv=0.568

### Repetitions

Every repetition is retained:

- `total_rows via row @ 10000 linhas` latency_p50_ms: 0.6495, 0.595, 0.5799
- `total_rows via row @ 10000 linhas` latency_p95_ms: 0.7091, 0.6885, 0.6087
- `total_rows via row @ 10000 linhas` latency_p99_ms: 0.7144, 0.6969, 0.6112
- `total_rows via row @ 10000 linhas` throughput_per_second: 1,562, 1,739, 1,855
- `sum_amount via row @ 10000 linhas` latency_p50_ms: 0.764, 0.7327, 0.6923
- `sum_amount via row @ 10000 linhas` latency_p95_ms: 0.7821, 0.9421, 0.6958
- `sum_amount via row @ 10000 linhas` latency_p99_ms: 0.7837, 0.9607, 0.6961
- `sum_amount via row @ 10000 linhas` throughput_per_second: 1,485, 1,437, 1,499
- `group_by_category via row @ 10000 linhas` latency_p50_ms: 1.832, 1.799, 1.818
- `group_by_category via row @ 10000 linhas` latency_p95_ms: 1.844, 1.922, 1.869
- `group_by_category via row @ 10000 linhas` latency_p99_ms: 1.845, 1.933, 1.874
- `group_by_category via row @ 10000 linhas` throughput_per_second: 574.4, 564.9, 584.5
- `filtered_sum via row @ 10000 linhas` latency_p50_ms: 0.8851, 0.8526, 0.8597
- `filtered_sum via row @ 10000 linhas` latency_p95_ms: 0.9594, 1.046, 0.8729
- `filtered_sum via row @ 10000 linhas` latency_p99_ms: 0.9661, 1.063, 0.8741
- `filtered_sum via row @ 10000 linhas` throughput_per_second: 1,172, 1,201, 1,181
- `total_rows via columnar @ 10000 linhas` latency_p50_ms: 0.7803, 0.829, 0.7702
- `total_rows via columnar @ 10000 linhas` latency_p95_ms: 0.9508, 1.023, 0.9944
- `total_rows via columnar @ 10000 linhas` latency_p99_ms: 0.966, 1.04, 1.014
- `total_rows via columnar @ 10000 linhas` throughput_per_second: 1,325, 1,269, 1,329
- `sum_amount via columnar @ 10000 linhas` latency_p50_ms: 0.9238, 0.9257, 0.9253
- `sum_amount via columnar @ 10000 linhas` latency_p95_ms: 0.9295, 1.023, 0.9689
- `sum_amount via columnar @ 10000 linhas` latency_p99_ms: 0.93, 1.032, 0.9728
- `sum_amount via columnar @ 10000 linhas` throughput_per_second: 1,094, 1,117, 1,143
- `filtered_sum via columnar @ 10000 linhas` latency_p50_ms: 2.208, 2.938, 2.264
- `filtered_sum via columnar @ 10000 linhas` latency_p95_ms: 2.407, 2.997, 2.318
- `filtered_sum via columnar @ 10000 linhas` latency_p99_ms: 2.425, 3.002, 2.323
- `filtered_sum via columnar @ 10000 linhas` throughput_per_second: 454.1, 341.7, 458.8
- `total_rows via parquet @ 10000 linhas` latency_p50_ms: 30.03, 33.18, 30.89
- `total_rows via parquet @ 10000 linhas` latency_p95_ms: 31.7, 33.43, 32.44
- `total_rows via parquet @ 10000 linhas` latency_p99_ms: 31.85, 33.45, 32.58
- `total_rows via parquet @ 10000 linhas` throughput_per_second: 34, 32.34, 33.33
- `sum_amount via parquet @ 10000 linhas` latency_p50_ms: 35.93, 36.88, 36.97
- `sum_amount via parquet @ 10000 linhas` latency_p95_ms: 37.92, 37.72, 37
- `sum_amount via parquet @ 10000 linhas` latency_p99_ms: 38.1, 37.8, 37
- `sum_amount via parquet @ 10000 linhas` throughput_per_second: 30.72, 27.25, 27.09
- `group_by_category via parquet @ 10000 linhas` latency_p50_ms: 37.76, 35, 38.11
- `group_by_category via parquet @ 10000 linhas` latency_p95_ms: 38.81, 37.15, 38.29
- `group_by_category via parquet @ 10000 linhas` latency_p99_ms: 38.9, 37.35, 38.3
- `group_by_category via parquet @ 10000 linhas` throughput_per_second: 27.44, 28.71, 26.29
- `filtered_sum via parquet @ 10000 linhas` latency_p50_ms: 33.31, 35.31, 35.29
- `filtered_sum via parquet @ 10000 linhas` latency_p95_ms: 36.4, 35.92, 35.57
- `filtered_sum via parquet @ 10000 linhas` latency_p99_ms: 36.67, 35.98, 35.6
- `filtered_sum via parquet @ 10000 linhas` throughput_per_second: 30.76, 30.31, 30.12
- `total_rows via row @ 50000 linhas` latency_p50_ms: 2.619, 2.767, 2.873
- `total_rows via row @ 50000 linhas` latency_p95_ms: 2.72, 2.894, 3.015
- `total_rows via row @ 50000 linhas` latency_p99_ms: 2.729, 2.905, 3.028
- `total_rows via row @ 50000 linhas` throughput_per_second: 383.6, 394.4, 355.7
- `sum_amount via row @ 50000 linhas` latency_p50_ms: 3.281, 3.264, 3.34
- `sum_amount via row @ 50000 linhas` latency_p95_ms: 3.367, 3.272, 3.362
- `sum_amount via row @ 50000 linhas` latency_p99_ms: 3.375, 3.273, 3.364
- `sum_amount via row @ 50000 linhas` throughput_per_second: 323.6, 320.8, 302.1
- `group_by_category via row @ 50000 linhas` latency_p50_ms: 8.176, 8.207, 8.575
- `group_by_category via row @ 50000 linhas` latency_p95_ms: 8.352, 8.248, 8.614
- `group_by_category via row @ 50000 linhas` latency_p99_ms: 8.368, 8.251, 8.618
- `group_by_category via row @ 50000 linhas` throughput_per_second: 123.6, 124.1, 118
- `filtered_sum via row @ 50000 linhas` latency_p50_ms: 3.976, 4.07, 4.159
- `filtered_sum via row @ 50000 linhas` latency_p95_ms: 3.992, 4.073, 4.259
- `filtered_sum via row @ 50000 linhas` latency_p99_ms: 3.994, 4.073, 4.268
- `filtered_sum via row @ 50000 linhas` throughput_per_second: 255.6, 247.6, 241.7
- `total_rows via columnar @ 50000 linhas` latency_p50_ms: 1.056, 0.994, 1.09
- `total_rows via columnar @ 50000 linhas` latency_p95_ms: 1.152, 1.17, 1.3
- `total_rows via columnar @ 50000 linhas` latency_p99_ms: 1.161, 1.186, 1.319
- `total_rows via columnar @ 50000 linhas` throughput_per_second: 989.7, 1,027, 977.8
- `sum_amount via columnar @ 50000 linhas` latency_p50_ms: 1.633, 1.618, 1.617
- `sum_amount via columnar @ 50000 linhas` latency_p95_ms: 1.823, 1.697, 1.658
- `sum_amount via columnar @ 50000 linhas` latency_p99_ms: 1.84, 1.705, 1.662
- `sum_amount via columnar @ 50000 linhas` throughput_per_second: 622.4, 623.6, 619.6
- `filtered_sum via columnar @ 50000 linhas` latency_p50_ms: 7.861, 7.499, 7.563
- `filtered_sum via columnar @ 50000 linhas` latency_p95_ms: 8.138, 7.573, 7.713
- `filtered_sum via columnar @ 50000 linhas` latency_p99_ms: 8.163, 7.58, 7.726
- `filtered_sum via columnar @ 50000 linhas` throughput_per_second: 128.8, 134.7, 132.7
- `total_rows via parquet @ 50000 linhas` latency_p50_ms: 162.3, 159.3, 160
- `total_rows via parquet @ 50000 linhas` latency_p95_ms: 162.5, 160.1, 160.4
- `total_rows via parquet @ 50000 linhas` latency_p99_ms: 162.5, 160.2, 160.4
- `total_rows via parquet @ 50000 linhas` throughput_per_second: 6.275, 6.371, 6.333
- `sum_amount via parquet @ 50000 linhas` latency_p50_ms: 175.4, 173.1, 175.2
- `sum_amount via parquet @ 50000 linhas` latency_p95_ms: 175.8, 173.5, 175.2
- `sum_amount via parquet @ 50000 linhas` latency_p99_ms: 175.8, 173.6, 175.2
- `sum_amount via parquet @ 50000 linhas` throughput_per_second: 5.711, 5.814, 5.735
- `group_by_category via parquet @ 50000 linhas` latency_p50_ms: 184.7, 184.7, 183.4
- `group_by_category via parquet @ 50000 linhas` latency_p95_ms: 185.1, 185.5, 184.6
- `group_by_category via parquet @ 50000 linhas` latency_p99_ms: 185.1, 185.5, 184.7
- `group_by_category via parquet @ 50000 linhas` throughput_per_second: 5.506, 5.458, 5.453
- `filtered_sum via parquet @ 50000 linhas` latency_p50_ms: 170.7, 170, 168.2
- `filtered_sum via parquet @ 50000 linhas` latency_p95_ms: 171.2, 170.2, 170.8
- `filtered_sum via parquet @ 50000 linhas` latency_p99_ms: 171.3, 170.2, 171
- `filtered_sum via parquet @ 50000 linhas` throughput_per_second: 5.887, 5.897, 5.952
- `total_rows via row @ 100000 linhas` latency_p50_ms: 5.51, 5.534, 5.481
- `total_rows via row @ 100000 linhas` latency_p95_ms: 5.55, 5.64, 5.607
- `total_rows via row @ 100000 linhas` latency_p99_ms: 5.554, 5.649, 5.619
- `total_rows via row @ 100000 linhas` throughput_per_second: 186, 186.8, 186.3
- `sum_amount via row @ 100000 linhas` latency_p50_ms: 6.407, 6.489, 12.79
- `sum_amount via row @ 100000 linhas` latency_p95_ms: 6.51, 6.585, 12.8
- `sum_amount via row @ 100000 linhas` latency_p99_ms: 6.519, 6.593, 12.8
- `sum_amount via row @ 100000 linhas` throughput_per_second: 156.3, 154.6, 133.2
- `group_by_category via row @ 100000 linhas` latency_p50_ms: 16.66, 16.91, 17.06
- `group_by_category via row @ 100000 linhas` latency_p95_ms: 16.8, 17.38, 17.36
- `group_by_category via row @ 100000 linhas` latency_p99_ms: 16.82, 17.42, 17.38
- `group_by_category via row @ 100000 linhas` throughput_per_second: 60.11, 60.05, 59.03
- `filtered_sum via row @ 100000 linhas` latency_p50_ms: 8.468, 8.285, 8.397
- `filtered_sum via row @ 100000 linhas` latency_p95_ms: 8.505, 8.328, 8.52
- `filtered_sum via row @ 100000 linhas` latency_p99_ms: 8.508, 8.332, 8.531
- `filtered_sum via row @ 100000 linhas` throughput_per_second: 120.1, 120.8, 121.5
- `total_rows via columnar @ 100000 linhas` latency_p50_ms: 1.332, 1.373, 1.455
- `total_rows via columnar @ 100000 linhas` latency_p95_ms: 1.674, 1.616, 1.56
- `total_rows via columnar @ 100000 linhas` latency_p99_ms: 1.705, 1.638, 1.569
- `total_rows via columnar @ 100000 linhas` throughput_per_second: 756.2, 730.8, 747.7
- `sum_amount via columnar @ 100000 linhas` latency_p50_ms: 2.608, 2.633, 2.456
- `sum_amount via columnar @ 100000 linhas` latency_p95_ms: 2.616, 2.655, 2.468
- `sum_amount via columnar @ 100000 linhas` latency_p99_ms: 2.616, 2.657, 2.469
- `sum_amount via columnar @ 100000 linhas` throughput_per_second: 399.5, 390.4, 419.5
- `filtered_sum via columnar @ 100000 linhas` latency_p50_ms: 13.74, 14.37, 14.56
- `filtered_sum via columnar @ 100000 linhas` latency_p95_ms: 13.75, 14.65, 14.6
- `filtered_sum via columnar @ 100000 linhas` latency_p99_ms: 13.75, 14.67, 14.6
- `filtered_sum via columnar @ 100000 linhas` throughput_per_second: 72.83, 69.62, 69.21
- `total_rows via parquet @ 100000 linhas` latency_p50_ms: 287.5, 292.7, 286.8
- `total_rows via parquet @ 100000 linhas` latency_p95_ms: 288, 293.6, 289.4
- `total_rows via parquet @ 100000 linhas` latency_p99_ms: 288.1, 293.7, 289.7
- `total_rows via parquet @ 100000 linhas` throughput_per_second: 3.486, 3.512, 3.499
- `sum_amount via parquet @ 100000 linhas` latency_p50_ms: 317.3, 322.5, 316.8
- `sum_amount via parquet @ 100000 linhas` latency_p95_ms: 318.6, 322.6, 317.9
- `sum_amount via parquet @ 100000 linhas` latency_p99_ms: 318.7, 322.7, 318
- `sum_amount via parquet @ 100000 linhas` throughput_per_second: 3.186, 3.182, 3.212
- `group_by_category via parquet @ 100000 linhas` latency_p50_ms: 336, 336.5, 339.4
- `group_by_category via parquet @ 100000 linhas` latency_p95_ms: 338.4, 336.8, 341.2
- `group_by_category via parquet @ 100000 linhas` latency_p99_ms: 338.6, 336.8, 341.3
- `group_by_category via parquet @ 100000 linhas` throughput_per_second: 3.008, 3.035, 2.965
- `filtered_sum via parquet @ 100000 linhas` latency_p50_ms: 309.7, 304, 310.5
- `filtered_sum via parquet @ 100000 linhas` latency_p95_ms: 311, 304.6, 318.1
- `filtered_sum via parquet @ 100000 linhas` latency_p99_ms: 311.2, 304.6, 318.8
- `filtered_sum via parquet @ 100000 linhas` throughput_per_second: 3.236, 3.294, 3.268
- `total_rows via row @ 500000 linhas` latency_p50_ms: 26.61, 20.41, 18.69
- `total_rows via row @ 500000 linhas` latency_p95_ms: 27.92, 20.77, 18.74
- `total_rows via row @ 500000 linhas` latency_p99_ms: 28.04, 20.8, 18.74
- `total_rows via row @ 500000 linhas` throughput_per_second: 45.71, 49.26, 54.08
- `sum_amount via row @ 500000 linhas` latency_p50_ms: 24.09, 24.2, 22.18
- `sum_amount via row @ 500000 linhas` latency_p95_ms: 24.38, 24.29, 26.74
- `sum_amount via row @ 500000 linhas` latency_p99_ms: 24.4, 24.3, 27.15
- `sum_amount via row @ 500000 linhas` throughput_per_second: 42.71, 41.57, 46.16
- `group_by_category via row @ 500000 linhas` latency_p50_ms: 42.38, 43.53, 38.98
- `group_by_category via row @ 500000 linhas` latency_p95_ms: 42.91, 44.45, 40.18
- `group_by_category via row @ 500000 linhas` latency_p99_ms: 42.95, 44.53, 40.28
- `group_by_category via row @ 500000 linhas` throughput_per_second: 23.92, 24.19, 26.11
- `filtered_sum via row @ 500000 linhas` latency_p50_ms: 27.43, 25.99, 24.72
- `filtered_sum via row @ 500000 linhas` latency_p95_ms: 33.58, 26.13, 25.36
- `filtered_sum via row @ 500000 linhas` latency_p99_ms: 34.12, 26.14, 25.41
- `filtered_sum via row @ 500000 linhas` throughput_per_second: 36.48, 38.67, 41.16
- `total_rows via columnar @ 500000 linhas` latency_p50_ms: 3.793, 4.048, 4.325
- `total_rows via columnar @ 500000 linhas` latency_p95_ms: 4.087, 4.275, 4.4
- `total_rows via columnar @ 500000 linhas` latency_p99_ms: 4.113, 4.296, 4.407
- `total_rows via columnar @ 500000 linhas` throughput_per_second: 278.9, 247.9, 239.2
- `sum_amount via columnar @ 500000 linhas` latency_p50_ms: 9.066, 9.725, 9.479
- `sum_amount via columnar @ 500000 linhas` latency_p95_ms: 9.178, 9.788, 9.551
- `sum_amount via columnar @ 500000 linhas` latency_p99_ms: 9.188, 9.793, 9.557
- `sum_amount via columnar @ 500000 linhas` throughput_per_second: 111.1, 107.2, 105.7
- `filtered_sum via columnar @ 500000 linhas` latency_p50_ms: 63.99, 64.62, 64.07
- `filtered_sum via columnar @ 500000 linhas` latency_p95_ms: 65.02, 65.4, 64.37
- `filtered_sum via columnar @ 500000 linhas` latency_p99_ms: 65.11, 65.47, 64.4
- `filtered_sum via columnar @ 500000 linhas` throughput_per_second: 16.12, 15.64, 15.71
- `total_rows via parquet @ 500000 linhas` latency_p50_ms: 1,584, 1,601, 1,610
- `total_rows via parquet @ 500000 linhas` latency_p95_ms: 1,585, 1,602, 1,616
- `total_rows via parquet @ 500000 linhas` latency_p99_ms: 1,585, 1,602, 1,616
- `total_rows via parquet @ 500000 linhas` throughput_per_second: 0.636, 0.6256, 0.6273
- `sum_amount via parquet @ 500000 linhas` latency_p50_ms: 1,712, 1,744, 1,740
- `sum_amount via parquet @ 500000 linhas` latency_p95_ms: 1,719, 1,748, 1,753
- `sum_amount via parquet @ 500000 linhas` latency_p99_ms: 1,719, 1,749, 1,754
- `sum_amount via parquet @ 500000 linhas` throughput_per_second: 0.5843, 0.5764, 0.5748
- `group_by_category via parquet @ 500000 linhas` latency_p50_ms: 1,830, 1,823, 1,825
- `group_by_category via parquet @ 500000 linhas` latency_p95_ms: 1,840, 1,841, 1,826
- `group_by_category via parquet @ 500000 linhas` latency_p99_ms: 1,841, 1,843, 1,826
- `group_by_category via parquet @ 500000 linhas` throughput_per_second: 0.5463, 0.5498, 0.5511
- `filtered_sum via parquet @ 500000 linhas` latency_p50_ms: 1,676, 1,705, 1,686
- `filtered_sum via parquet @ 500000 linhas` latency_p95_ms: 1,687, 1,712, 1,696
- `filtered_sum via parquet @ 500000 linhas` latency_p99_ms: 1,688, 1,713, 1,697
- `filtered_sum via parquet @ 500000 linhas` throughput_per_second: 0.597, 0.5889, 0.5939
- `total_rows via row @ 1000000 linhas` latency_p50_ms: 27.75, 33.47, 26.82
- `total_rows via row @ 1000000 linhas` latency_p95_ms: 32.83, 33.53, 26.86
- `total_rows via row @ 1000000 linhas` latency_p99_ms: 33.29, 33.53, 26.86
- `total_rows via row @ 1000000 linhas` throughput_per_second: 36.24, 29.97, 37.37
- `sum_amount via row @ 1000000 linhas` latency_p50_ms: 33.42, 34.33, 34.46
- `sum_amount via row @ 1000000 linhas` latency_p95_ms: 33.59, 34.91, 34.64
- `sum_amount via row @ 1000000 linhas` latency_p99_ms: 33.6, 34.96, 34.66
- `sum_amount via row @ 1000000 linhas` throughput_per_second: 31.62, 29.38, 29.2
- `group_by_category via row @ 1000000 linhas` latency_p50_ms: 66.23, 67.01, 67.96
- `group_by_category via row @ 1000000 linhas` latency_p95_ms: 67.86, 67.48, 68.54
- `group_by_category via row @ 1000000 linhas` latency_p99_ms: 68.01, 67.53, 68.59
- `group_by_category via row @ 1000000 linhas` throughput_per_second: 15.11, 15.08, 14.82
- `filtered_sum via row @ 1000000 linhas` latency_p50_ms: 40.13, 40.17, 39.67
- `filtered_sum via row @ 1000000 linhas` latency_p95_ms: 46.89, 40.71, 39.9
- `filtered_sum via row @ 1000000 linhas` latency_p99_ms: 47.49, 40.76, 39.92
- `filtered_sum via row @ 1000000 linhas` throughput_per_second: 25.36, 25.4, 25.23
- `total_rows via columnar @ 1000000 linhas` latency_p50_ms: 7.61, 7.436, 7.64
- `total_rows via columnar @ 1000000 linhas` latency_p95_ms: 7.631, 7.62, 7.711
- `total_rows via columnar @ 1000000 linhas` latency_p99_ms: 7.633, 7.636, 7.717
- `total_rows via columnar @ 1000000 linhas` throughput_per_second: 131.9, 134.6, 136.4
- `sum_amount via columnar @ 1000000 linhas` latency_p50_ms: 17.39, 17.26, 17.68
- `sum_amount via columnar @ 1000000 linhas` latency_p95_ms: 17.59, 17.37, 17.69
- `sum_amount via columnar @ 1000000 linhas` latency_p99_ms: 17.61, 17.38, 17.69
- `sum_amount via columnar @ 1000000 linhas` throughput_per_second: 59.51, 58.53, 56.78
- `filtered_sum via columnar @ 1000000 linhas` latency_p50_ms: 122.7, 123.4, 123.6
- `filtered_sum via columnar @ 1000000 linhas` latency_p95_ms: 123, 124.8, 124.3
- `filtered_sum via columnar @ 1000000 linhas` latency_p99_ms: 123, 124.9, 124.4
- `filtered_sum via columnar @ 1000000 linhas` throughput_per_second: 8.164, 8.115, 8.161
- `total_rows via parquet @ 1000000 linhas` latency_p50_ms: 3,177, 3,188, 3,225
- `total_rows via parquet @ 1000000 linhas` latency_p95_ms: 3,209, 3,203, 3,226
- `total_rows via parquet @ 1000000 linhas` latency_p99_ms: 3,212, 3,204, 3,226
- `total_rows via parquet @ 1000000 linhas` throughput_per_second: 0.315, 0.3142, 0.3133
- `sum_amount via parquet @ 1000000 linhas` latency_p50_ms: 3,487, 3,507, 3,502
- `sum_amount via parquet @ 1000000 linhas` latency_p95_ms: 3,515, 3,517, 3,504
- `sum_amount via parquet @ 1000000 linhas` latency_p99_ms: 3,518, 3,518, 3,504
- `sum_amount via parquet @ 1000000 linhas` throughput_per_second: 0.2871, 0.2863, 0.2881
- `group_by_category via parquet @ 1000000 linhas` latency_p50_ms: 3,670, 3,710, 3,706
- `group_by_category via parquet @ 1000000 linhas` latency_p95_ms: 3,693, 3,716, 3,715
- `group_by_category via parquet @ 1000000 linhas` latency_p99_ms: 3,696, 3,716, 3,716
- `group_by_category via parquet @ 1000000 linhas` throughput_per_second: 0.2727, 0.2717, 0.2712
- `filtered_sum via parquet @ 1000000 linhas` latency_p50_ms: 3,385, 3,414, 3,452
- `filtered_sum via parquet @ 1000000 linhas` latency_p95_ms: 3,390, 3,423, 3,453
- `filtered_sum via parquet @ 1000000 linhas` latency_p99_ms: 3,390, 3,424, 3,453
- `filtered_sum via parquet @ 1000000 linhas` throughput_per_second: 0.2969, 0.294, 0.2907
- `total_rows via row @ 2000000 linhas` latency_p50_ms: 43.18, 44.13, 42.59
- `total_rows via row @ 2000000 linhas` latency_p95_ms: 52.4, 44.59, 42.97
- `total_rows via row @ 2000000 linhas` latency_p99_ms: 53.22, 44.63, 43
- `total_rows via row @ 2000000 linhas` throughput_per_second: 23.22, 23.08, 23.56
- `sum_amount via row @ 2000000 linhas` latency_p50_ms: 54.6, 55.22, 55.16
- `sum_amount via row @ 2000000 linhas` latency_p95_ms: 54.7, 70.93, 56.77
- `sum_amount via row @ 2000000 linhas` latency_p99_ms: 54.71, 72.32, 56.91
- `sum_amount via row @ 2000000 linhas` throughput_per_second: 18.54, 18.15, 18.16
- `group_by_category via row @ 2000000 linhas` latency_p50_ms: 131.7, 120.4, 121.9
- `group_by_category via row @ 2000000 linhas` latency_p95_ms: 163.4, 122, 122.2
- `group_by_category via row @ 2000000 linhas` latency_p99_ms: 166.2, 122.2, 122.3
- `group_by_category via row @ 2000000 linhas` throughput_per_second: 8.28, 8.317, 8.23
- `filtered_sum via row @ 2000000 linhas` latency_p50_ms: 65.35, 68.25, 68.9
- `filtered_sum via row @ 2000000 linhas` latency_p95_ms: 67.01, 86.79, 69.57
- `filtered_sum via row @ 2000000 linhas` latency_p99_ms: 67.16, 88.44, 69.63
- `filtered_sum via row @ 2000000 linhas` throughput_per_second: 15.43, 15.09, 14.74
- `total_rows via columnar @ 2000000 linhas` latency_p50_ms: 13.53, 14.25, 14.24
- `total_rows via columnar @ 2000000 linhas` latency_p95_ms: 13.66, 49.12, 49.99
- `total_rows via columnar @ 2000000 linhas` latency_p99_ms: 13.67, 52.22, 53.17
- `total_rows via columnar @ 2000000 linhas` throughput_per_second: 74.32, 72.07, 71.87
- `sum_amount via columnar @ 2000000 linhas` latency_p50_ms: 34.1, 34.64, 32.98
- `sum_amount via columnar @ 2000000 linhas` latency_p95_ms: 34.51, 34.69, 33.15
- `sum_amount via columnar @ 2000000 linhas` latency_p99_ms: 34.54, 34.7, 33.17
- `sum_amount via columnar @ 2000000 linhas` throughput_per_second: 29.55, 29.82, 30.96
- `filtered_sum via columnar @ 2000000 linhas` latency_p50_ms: 247.2, 247.6, 253
- `filtered_sum via columnar @ 2000000 linhas` latency_p95_ms: 247.7, 249.5, 253.1
- `filtered_sum via columnar @ 2000000 linhas` latency_p99_ms: 247.7, 249.7, 253.1
- `filtered_sum via columnar @ 2000000 linhas` throughput_per_second: 4.055, 4.087, 4.017
- `total_rows via parquet @ 2000000 linhas` latency_p50_ms: 5,956, 5,963, 5,989
- `total_rows via parquet @ 2000000 linhas` latency_p95_ms: 5,975, 6,123, 6,123
- `total_rows via parquet @ 2000000 linhas` latency_p99_ms: 5,976, 6,137, 6,135
- `total_rows via parquet @ 2000000 linhas` throughput_per_second: 0.1687, 0.1691, 0.1685
- `sum_amount via parquet @ 2000000 linhas` latency_p50_ms: 6,616, 6,692, 6,497
- `sum_amount via parquet @ 2000000 linhas` latency_p95_ms: 6,696, 6,785, 6,733
- `sum_amount via parquet @ 2000000 linhas` latency_p99_ms: 6,703, 6,793, 6,754
- `sum_amount via parquet @ 2000000 linhas` throughput_per_second: 0.1534, 0.1525, 0.154
- `group_by_category via parquet @ 2000000 linhas` latency_p50_ms: 6,903, 6,912, 6,877
- `group_by_category via parquet @ 2000000 linhas` latency_p95_ms: 7,137, 7,062, 6,878
- `group_by_category via parquet @ 2000000 linhas` latency_p99_ms: 7,158, 7,075, 6,878
- `group_by_category via parquet @ 2000000 linhas` throughput_per_second: 0.1462, 0.1455, 0.1457
- `filtered_sum via parquet @ 2000000 linhas` latency_p50_ms: 6,377, 6,309, 6,301
- `filtered_sum via parquet @ 2000000 linhas` latency_p95_ms: 6,553, 6,560, 6,316
- `filtered_sum via parquet @ 2000000 linhas` latency_p99_ms: 6,568, 6,583, 6,318
- `filtered_sum via parquet @ 2000000 linhas` throughput_per_second: 0.1584, 0.1596, 0.1589

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

