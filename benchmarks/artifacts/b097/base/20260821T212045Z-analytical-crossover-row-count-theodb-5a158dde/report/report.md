# analytical/crossover/row-count on theodb

**Status:** EXPLORATORY · **Profile:** research · **Run:** `20260821T212045Z-analytical-crossover-row-count-theodb-5a158dde`

> This run is **EXPLORATORY**. Research runs may use non-frozen parameters, so the numbers below cannot back a published claim.

## Results

| Configuration | Throughput/s | Recall | p50 ms | p95 ms | p99 ms | Stable |
|---|---|---|---|---|---|---|
| total_rows via row @ 10000 linhas | 1,918 | _not measured_ | 0.55 | 0.6323 | 0.6396 | **no** |
| sum_amount via row @ 10000 linhas | 1,498 | _not measured_ | 0.6987 | 0.7886 | 0.7979 | **no** |
| group_by_category via row @ 10000 linhas | 599.8 | _not measured_ | 1.761 | 1.83 | 1.835 | yes |
| filtered_sum via row @ 10000 linhas | 1,197 | _not measured_ | 0.8594 | 0.9253 | 0.9324 | **no** |
| total_rows via columnar @ 10000 linhas | 1,395 | _not measured_ | 0.7478 | 0.8921 | 0.9049 | **no** |
| sum_amount via columnar @ 10000 linhas | 1,227 | _not measured_ | 0.8425 | 0.8879 | 0.8892 | **no** |
| group_by_category via columnar @ 10000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 10000 linhas | 470.9 | _not measured_ | 2.132 | 2.16 | 2.164 | **no** |
| total_rows via parquet @ 10000 linhas | 37.16 | _not measured_ | 27.07 | 27.38 | 27.43 | yes |
| sum_amount via parquet @ 10000 linhas | 33.6 | _not measured_ | 30.06 | 30.59 | 30.63 | yes |
| group_by_category via parquet @ 10000 linhas | 31.49 | _not measured_ | 32.24 | 32.38 | 32.39 | **no** |
| filtered_sum via parquet @ 10000 linhas | 34.26 | _not measured_ | 29.48 | 29.74 | 29.75 | **no** |
| total_rows via row @ 50000 linhas | 403.8 | _not measured_ | 2.553 | 2.643 | 2.651 | yes |
| sum_amount via row @ 50000 linhas | 321.1 | _not measured_ | 3.156 | 3.157 | 3.157 | **no** |
| group_by_category via row @ 50000 linhas | 128.7 | _not measured_ | 7.804 | 7.832 | 7.835 | **no** |
| filtered_sum via row @ 50000 linhas | 277.9 | _not measured_ | 3.628 | 3.758 | 3.771 | yes |
| total_rows via columnar @ 50000 linhas | 1,032 | _not measured_ | 1.016 | 1.131 | 1.14 | yes |
| sum_amount via columnar @ 50000 linhas | 672.8 | _not measured_ | 1.543 | 1.581 | 1.584 | yes |
| group_by_category via columnar @ 50000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 50000 linhas | 152.9 | _not measured_ | 6.597 | 6.672 | 6.684 | yes |
| total_rows via parquet @ 50000 linhas | 6.683 | _not measured_ | 150.5 | 151.2 | 151.3 | yes |
| sum_amount via parquet @ 50000 linhas | 6.151 | _not measured_ | 165.5 | 166.7 | 166.7 | yes |
| group_by_category via parquet @ 50000 linhas | 5.751 | _not measured_ | 174 | 174.7 | 174.8 | yes |
| filtered_sum via parquet @ 50000 linhas | 6.25 | _not measured_ | 160.7 | 161.4 | 161.5 | yes |
| total_rows via row @ 100000 linhas | 217 | _not measured_ | 4.767 | 5.165 | 5.203 | **no** |
| sum_amount via row @ 100000 linhas | 163.6 | _not measured_ | 6.17 | 6.176 | 6.177 | yes |
| group_by_category via row @ 100000 linhas | 66.86 | _not measured_ | 14.99 | 15.15 | 15.17 | yes |
| filtered_sum via row @ 100000 linhas | 137.2 | _not measured_ | 7.429 | 7.698 | 7.703 | yes |
| total_rows via columnar @ 100000 linhas | 846.7 | _not measured_ | 1.251 | 1.484 | 1.512 | **no** |
| sum_amount via columnar @ 100000 linhas | 446.1 | _not measured_ | 2.341 | 2.411 | 2.419 | yes |
| group_by_category via columnar @ 100000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 100000 linhas | 81.56 | _not measured_ | 12.28 | 12.53 | 12.53 | yes |
| total_rows via parquet @ 100000 linhas | 3.719 | _not measured_ | 273.3 | 278.6 | 279 | **no** |
| sum_amount via parquet @ 100000 linhas | 3.38 | _not measured_ | 297.2 | 303.3 | 303.7 | yes |
| group_by_category via parquet @ 100000 linhas | 3.188 | _not measured_ | 316.4 | 318.8 | 319 | yes |
| filtered_sum via parquet @ 100000 linhas | 3.484 | _not measured_ | 289 | 290.1 | 290.2 | yes |
| total_rows via row @ 500000 linhas | 49.75 | _not measured_ | 20.14 | 20.33 | 20.35 | **no** |
| sum_amount via row @ 500000 linhas | 48.69 | _not measured_ | 20.61 | 26.61 | 27.14 | **no** |
| group_by_category via row @ 500000 linhas | 26.15 | _not measured_ | 39.95 | 52.43 | 53.53 | **no** |
| filtered_sum via row @ 500000 linhas | 41.52 | _not measured_ | 26.68 | 27.61 | 27.69 | **no** |
| total_rows via columnar @ 500000 linhas | 280.7 | _not measured_ | 3.741 | 3.993 | 4.001 | **no** |
| sum_amount via columnar @ 500000 linhas | 115.9 | _not measured_ | 8.658 | 8.734 | 8.741 | yes |
| group_by_category via columnar @ 500000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 500000 linhas | 17.15 | _not measured_ | 58.47 | 60.02 | 60.09 | yes |
| total_rows via parquet @ 500000 linhas | 0.6651 | _not measured_ | 1,509 | 1,513 | 1,513 | yes |
| sum_amount via parquet @ 500000 linhas | 0.6075 | _not measured_ | 1,651 | 1,659 | 1,659 | yes |
| group_by_category via parquet @ 500000 linhas | 0.573 | _not measured_ | 1,750 | 1,758 | 1,758 | yes |
| filtered_sum via parquet @ 500000 linhas | 0.6226 | _not measured_ | 1,608 | 1,620 | 1,620 | yes |
| total_rows via row @ 1000000 linhas | 36 | _not measured_ | 30.6 | 31.17 | 31.22 | **no** |
| sum_amount via row @ 1000000 linhas | 31.1 | _not measured_ | 33.04 | 34.28 | 34.36 | **no** |
| group_by_category via row @ 1000000 linhas | 15.62 | _not measured_ | 65.6 | 66.07 | 66.12 | yes |
| filtered_sum via row @ 1000000 linhas | 26.48 | _not measured_ | 38.42 | 38.96 | 39.01 | yes |
| total_rows via columnar @ 1000000 linhas | 162.8 | _not measured_ | 6.249 | 6.687 | 6.706 | yes |
| sum_amount via columnar @ 1000000 linhas | 63.96 | _not measured_ | 15.74 | 15.98 | 16 | **no** |
| group_by_category via columnar @ 1000000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 1000000 linhas | 8.745 | _not measured_ | 115 | 116 | 116.2 | yes |
| total_rows via parquet @ 1000000 linhas | 0.3283 | _not measured_ | 3,073 | 3,084 | 3,085 | yes |
| sum_amount via parquet @ 1000000 linhas | 0.3001 | _not measured_ | 3,359 | 3,381 | 3,383 | yes |
| group_by_category via parquet @ 1000000 linhas | 0.2841 | _not measured_ | 3,535 | 3,546 | 3,548 | yes |
| filtered_sum via parquet @ 1000000 linhas | 0.307 | _not measured_ | 3,267 | 3,277 | 3,278 | yes |
| total_rows via row @ 2000000 linhas | 25.64 | _not measured_ | 39.22 | 48.92 | 49.78 | **no** |
| sum_amount via row @ 2000000 linhas | 19.92 | _not measured_ | 51.56 | 51.78 | 51.8 | **no** |
| group_by_category via row @ 2000000 linhas | 8.642 | _not measured_ | 116.4 | 123.7 | 123.8 | **no** |
| filtered_sum via row @ 2000000 linhas | 15.77 | _not measured_ | 63.78 | 64.49 | 64.55 | **no** |
| total_rows via columnar @ 2000000 linhas | 79.76 | _not measured_ | 12.8 | 101.8 | 109.7 | **no** |
| sum_amount via columnar @ 2000000 linhas | 31.69 | _not measured_ | 32 | 32.29 | 32.34 | yes |
| group_by_category via columnar @ 2000000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 2000000 linhas | 4.479 | _not measured_ | 228.3 | 230.4 | 230.6 | yes |
| total_rows via parquet @ 2000000 linhas | 0.179 | _not measured_ | 5,594 | 5,636 | 5,641 | yes |
| sum_amount via parquet @ 2000000 linhas | 0.1621 | _not measured_ | 6,182 | 6,257 | 6,263 | yes |
| group_by_category via parquet @ 2000000 linhas | 0.1535 | _not measured_ | 6,594 | 6,737 | 6,749 | yes |
| filtered_sum via parquet @ 2000000 linhas | 0.1672 | _not measured_ | 6,010 | 6,108 | 6,118 | yes |

Unstable points are reported, not removed. Their repetitions disagree by more than the declared threshold, so the median below is a weaker claim than it looks:

- `total_rows via row @ 10000 linhas`: latency_p50_ms cv=0.080; throughput_per_second cv=0.067
- `sum_amount via row @ 10000 linhas`: latency_p99_ms cv=0.050; throughput_per_second cv=0.085
- `filtered_sum via row @ 10000 linhas`: latency_p95_ms cv=0.051; latency_p99_ms cv=0.053
- `total_rows via columnar @ 10000 linhas`: latency_p50_ms cv=0.056; latency_p95_ms cv=0.058; latency_p99_ms cv=0.058
- `sum_amount via columnar @ 10000 linhas`: latency_p95_ms cv=0.098; latency_p99_ms cv=0.106
- `filtered_sum via columnar @ 10000 linhas`: throughput_per_second cv=0.069
- `group_by_category via parquet @ 10000 linhas`: latency_p95_ms cv=0.078; latency_p99_ms cv=0.083
- `filtered_sum via parquet @ 10000 linhas`: latency_p99_ms cv=0.050
- `sum_amount via row @ 50000 linhas`: latency_p50_ms cv=0.415; latency_p95_ms cv=0.462; latency_p99_ms cv=0.466; throughput_per_second cv=0.339
- `group_by_category via row @ 50000 linhas`: latency_p50_ms cv=0.087; latency_p95_ms cv=0.256; latency_p99_ms cv=0.270
- `total_rows via row @ 100000 linhas`: throughput_per_second cv=0.055
- `total_rows via columnar @ 100000 linhas`: latency_p50_ms cv=0.091; throughput_per_second cv=0.056
- `total_rows via parquet @ 100000 linhas`: latency_p50_ms cv=0.073; latency_p95_ms cv=0.064; latency_p99_ms cv=0.063; throughput_per_second cv=0.071
- `total_rows via row @ 500000 linhas`: latency_p50_ms cv=0.164; latency_p95_ms cv=0.168; latency_p99_ms cv=0.168; throughput_per_second cv=0.115
- `sum_amount via row @ 500000 linhas`: latency_p50_ms cv=0.218; latency_p95_ms cv=0.190; latency_p99_ms cv=0.193; throughput_per_second cv=0.127
- `group_by_category via row @ 500000 linhas`: latency_p95_ms cv=0.180; latency_p99_ms cv=0.191
- `filtered_sum via row @ 500000 linhas`: latency_p50_ms cv=0.138; latency_p95_ms cv=0.140; latency_p99_ms cv=0.140
- `total_rows via columnar @ 500000 linhas`: latency_p50_ms cv=0.054; throughput_per_second cv=0.070
- `total_rows via row @ 1000000 linhas`: latency_p50_ms cv=0.118; latency_p95_ms cv=0.136; latency_p99_ms cv=0.138; throughput_per_second cv=0.103
- `sum_amount via row @ 1000000 linhas`: latency_p95_ms cv=0.101; latency_p99_ms cv=0.110
- `sum_amount via columnar @ 1000000 linhas`: latency_p95_ms cv=0.055; latency_p99_ms cv=0.056
- `total_rows via row @ 2000000 linhas`: latency_p50_ms cv=0.132; latency_p95_ms cv=0.131; latency_p99_ms cv=0.135; throughput_per_second cv=0.123
- `sum_amount via row @ 2000000 linhas`: latency_p50_ms cv=0.092; latency_p95_ms cv=0.098; latency_p99_ms cv=0.099; throughput_per_second cv=0.092
- `group_by_category via row @ 2000000 linhas`: latency_p99_ms cv=0.053
- `filtered_sum via row @ 2000000 linhas`: latency_p50_ms cv=0.062; latency_p95_ms cv=0.066; latency_p99_ms cv=0.067; throughput_per_second cv=0.065
- `total_rows via columnar @ 2000000 linhas`: latency_p95_ms cv=0.722; latency_p99_ms cv=0.732

### Repetitions

Every repetition is retained:

- `total_rows via row @ 10000 linhas` latency_p50_ms: 0.55, 0.5054, 0.5933
- `total_rows via row @ 10000 linhas` latency_p95_ms: 0.6323, 0.596, 0.6406
- `total_rows via row @ 10000 linhas` latency_p99_ms: 0.6396, 0.604, 0.6448
- `total_rows via row @ 10000 linhas` throughput_per_second: 1,847, 2,102, 1,918
- `sum_amount via row @ 10000 linhas` latency_p50_ms: 0.6842, 0.6987, 0.7264
- `sum_amount via row @ 10000 linhas` latency_p95_ms: 0.7886, 0.801, 0.7356
- `sum_amount via row @ 10000 linhas` latency_p99_ms: 0.7979, 0.81, 0.7364
- `sum_amount via row @ 10000 linhas` throughput_per_second: 1,498, 1,670, 1,416
- `group_by_category via row @ 10000 linhas` latency_p50_ms: 1.78, 1.681, 1.761
- `group_by_category via row @ 10000 linhas` latency_p95_ms: 1.83, 1.81, 1.868
- `group_by_category via row @ 10000 linhas` latency_p99_ms: 1.835, 1.822, 1.877
- `group_by_category via row @ 10000 linhas` throughput_per_second: 599.8, 618.6, 570.2
- `filtered_sum via row @ 10000 linhas` latency_p50_ms: 0.8982, 0.845, 0.8594
- `filtered_sum via row @ 10000 linhas` latency_p95_ms: 0.9796, 0.9253, 0.8846
- `filtered_sum via row @ 10000 linhas` latency_p99_ms: 0.9868, 0.9324, 0.8869
- `filtered_sum via row @ 10000 linhas` throughput_per_second: 1,210, 1,197, 1,167
- `total_rows via columnar @ 10000 linhas` latency_p50_ms: 0.7008, 0.7839, 0.7478
- `total_rows via columnar @ 10000 linhas` latency_p95_ms: 0.8171, 0.9129, 0.8921
- `total_rows via columnar @ 10000 linhas` latency_p99_ms: 0.8275, 0.9244, 0.9049
- `total_rows via columnar @ 10000 linhas` throughput_per_second: 1,430, 1,395, 1,358
- `sum_amount via columnar @ 10000 linhas` latency_p50_ms: 0.8325, 0.8425, 0.8733
- `sum_amount via columnar @ 10000 linhas` latency_p95_ms: 0.835, 1.009, 0.8879
- `sum_amount via columnar @ 10000 linhas` latency_p99_ms: 0.8352, 1.024, 0.8892
- `sum_amount via columnar @ 10000 linhas` throughput_per_second: 1,234, 1,198, 1,227
- `filtered_sum via columnar @ 10000 linhas` latency_p50_ms: 2.11, 2.285, 2.132
- `filtered_sum via columnar @ 10000 linhas` latency_p95_ms: 2.16, 2.341, 2.158
- `filtered_sum via columnar @ 10000 linhas` latency_p99_ms: 2.164, 2.345, 2.16
- `filtered_sum via columnar @ 10000 linhas` throughput_per_second: 504.4, 438.9, 470.9
- `total_rows via parquet @ 10000 linhas` latency_p50_ms: 27.53, 26.79, 27.07
- `total_rows via parquet @ 10000 linhas` latency_p95_ms: 28.07, 27.38, 27.17
- `total_rows via parquet @ 10000 linhas` latency_p99_ms: 28.12, 27.43, 27.18
- `total_rows via parquet @ 10000 linhas` throughput_per_second: 37.03, 37.51, 37.16
- `sum_amount via parquet @ 10000 linhas` latency_p50_ms: 30.06, 29.4, 30.2
- `sum_amount via parquet @ 10000 linhas` latency_p95_ms: 30.71, 29.59, 30.59
- `sum_amount via parquet @ 10000 linhas` latency_p99_ms: 30.77, 29.6, 30.63
- `sum_amount via parquet @ 10000 linhas` throughput_per_second: 33.28, 34.09, 33.6
- `group_by_category via parquet @ 10000 linhas` latency_p50_ms: 32.82, 31.53, 32.24
- `group_by_category via parquet @ 10000 linhas` latency_p95_ms: 36.58, 31.73, 32.38
- `group_by_category via parquet @ 10000 linhas` latency_p99_ms: 36.91, 31.75, 32.39
- `group_by_category via parquet @ 10000 linhas` throughput_per_second: 30.74, 31.77, 31.49
- `filtered_sum via parquet @ 10000 linhas` latency_p50_ms: 29.48, 29.68, 29.29
- `filtered_sum via parquet @ 10000 linhas` latency_p95_ms: 29.57, 29.74, 32.06
- `filtered_sum via parquet @ 10000 linhas` latency_p99_ms: 29.58, 29.75, 32.31
- `filtered_sum via parquet @ 10000 linhas` throughput_per_second: 34.32, 34.26, 34.2
- `total_rows via row @ 50000 linhas` latency_p50_ms: 2.506, 2.553, 2.703
- `total_rows via row @ 50000 linhas` latency_p95_ms: 2.607, 2.643, 2.783
- `total_rows via row @ 50000 linhas` latency_p99_ms: 2.616, 2.651, 2.79
- `total_rows via row @ 50000 linhas` throughput_per_second: 405.7, 403.8, 388.8
- `sum_amount via row @ 50000 linhas` latency_p50_ms: 6.015, 3.031, 3.156
- `sum_amount via row @ 50000 linhas` latency_p95_ms: 6.575, 3.131, 3.157
- `sum_amount via row @ 50000 linhas` latency_p99_ms: 6.625, 3.14, 3.157
- `sum_amount via row @ 50000 linhas` throughput_per_second: 170.1, 343.4, 321.1
- `group_by_category via row @ 50000 linhas` latency_p50_ms: 8.814, 7.48, 7.804
- `group_by_category via row @ 50000 linhas` latency_p95_ms: 11.64, 7.491, 7.832
- `group_by_category via row @ 50000 linhas` latency_p99_ms: 11.89, 7.492, 7.835
- `group_by_category via row @ 50000 linhas` throughput_per_second: 123.9, 133.8, 128.7
- `filtered_sum via row @ 50000 linhas` latency_p50_ms: 3.751, 3.613, 3.628
- `filtered_sum via row @ 50000 linhas` latency_p95_ms: 3.82, 3.758, 3.635
- `filtered_sum via row @ 50000 linhas` latency_p99_ms: 3.826, 3.771, 3.636
- `filtered_sum via row @ 50000 linhas` throughput_per_second: 270.5, 277.9, 279.2
- `total_rows via columnar @ 50000 linhas` latency_p50_ms: 1.036, 0.9932, 1.016
- `total_rows via columnar @ 50000 linhas` latency_p95_ms: 1.131, 1.069, 1.169
- `total_rows via columnar @ 50000 linhas` latency_p99_ms: 1.14, 1.076, 1.183
- `total_rows via columnar @ 50000 linhas` throughput_per_second: 1,066, 1,009, 1,032
- `sum_amount via columnar @ 50000 linhas` latency_p50_ms: 1.571, 1.543, 1.516
- `sum_amount via columnar @ 50000 linhas` latency_p95_ms: 1.619, 1.581, 1.533
- `sum_amount via columnar @ 50000 linhas` latency_p99_ms: 1.623, 1.584, 1.535
- `sum_amount via columnar @ 50000 linhas` throughput_per_second: 657.2, 677.3, 672.8
- `filtered_sum via columnar @ 50000 linhas` latency_p50_ms: 6.544, 6.597, 6.655
- `filtered_sum via columnar @ 50000 linhas` latency_p95_ms: 6.672, 6.739, 6.659
- `filtered_sum via columnar @ 50000 linhas` latency_p99_ms: 6.684, 6.752, 6.659
- `filtered_sum via columnar @ 50000 linhas` throughput_per_second: 152.9, 153.4, 150.5
- `total_rows via parquet @ 50000 linhas` latency_p50_ms: 152.7, 150.5, 149.8
- `total_rows via parquet @ 50000 linhas` latency_p95_ms: 152.7, 150.6, 151.2
- `total_rows via parquet @ 50000 linhas` latency_p99_ms: 152.7, 150.6, 151.3
- `total_rows via parquet @ 50000 linhas` throughput_per_second: 6.66, 6.701, 6.683
- `sum_amount via parquet @ 50000 linhas` latency_p50_ms: 165.5, 162.8, 166.7
- `sum_amount via parquet @ 50000 linhas` latency_p95_ms: 170.1, 166.4, 166.7
- `sum_amount via parquet @ 50000 linhas` latency_p99_ms: 170.5, 166.7, 166.7
- `sum_amount via parquet @ 50000 linhas` throughput_per_second: 6.081, 6.151, 6.16
- `group_by_category via parquet @ 50000 linhas` latency_p50_ms: 177.6, 173.5, 174
- `group_by_category via parquet @ 50000 linhas` latency_p95_ms: 179.2, 174.7, 174.6
- `group_by_category via parquet @ 50000 linhas` latency_p99_ms: 179.3, 174.8, 174.7
- `group_by_category via parquet @ 50000 linhas` throughput_per_second: 5.726, 5.807, 5.751
- `filtered_sum via parquet @ 50000 linhas` latency_p50_ms: 161, 160.7, 160.7
- `filtered_sum via parquet @ 50000 linhas` latency_p95_ms: 161.4, 162.8, 161.1
- `filtered_sum via parquet @ 50000 linhas` latency_p99_ms: 161.5, 163, 161.1
- `filtered_sum via parquet @ 50000 linhas` throughput_per_second: 6.256, 6.24, 6.25
- `total_rows via row @ 100000 linhas` latency_p50_ms: 5.108, 4.679, 4.767
- `total_rows via row @ 100000 linhas` latency_p95_ms: 5.23, 5.16, 5.165
- `total_rows via row @ 100000 linhas` latency_p99_ms: 5.24, 5.203, 5.201
- `total_rows via row @ 100000 linhas` throughput_per_second: 200.3, 222.8, 217
- `sum_amount via row @ 100000 linhas` latency_p50_ms: 6.362, 5.979, 6.17
- `sum_amount via row @ 100000 linhas` latency_p95_ms: 6.412, 6.017, 6.176
- `sum_amount via row @ 100000 linhas` latency_p99_ms: 6.416, 6.021, 6.177
- `sum_amount via row @ 100000 linhas` throughput_per_second: 163.2, 167.9, 163.6
- `group_by_category via row @ 100000 linhas` latency_p50_ms: 15.53, 14.81, 14.99
- `group_by_category via row @ 100000 linhas` latency_p95_ms: 15.62, 14.99, 15.15
- `group_by_category via row @ 100000 linhas` latency_p99_ms: 15.62, 15.01, 15.17
- `group_by_category via row @ 100000 linhas` throughput_per_second: 64.94, 67.9, 66.86
- `filtered_sum via row @ 100000 linhas` latency_p50_ms: 7.645, 7.429, 7.408
- `filtered_sum via row @ 100000 linhas` latency_p95_ms: 7.698, 7.497, 7.794
- `filtered_sum via row @ 100000 linhas` latency_p99_ms: 7.703, 7.503, 7.828
- `filtered_sum via row @ 100000 linhas` throughput_per_second: 133, 137.2, 139.1
- `total_rows via columnar @ 100000 linhas` latency_p50_ms: 1.406, 1.18, 1.251
- `total_rows via columnar @ 100000 linhas` latency_p95_ms: 1.408, 1.484, 1.491
- `total_rows via columnar @ 100000 linhas` latency_p99_ms: 1.408, 1.512, 1.513
- `total_rows via columnar @ 100000 linhas` throughput_per_second: 771.1, 854.3, 846.7
- `sum_amount via columnar @ 100000 linhas` latency_p50_ms: 2.423, 2.327, 2.341
- `sum_amount via columnar @ 100000 linhas` latency_p95_ms: 2.478, 2.411, 2.365
- `sum_amount via columnar @ 100000 linhas` latency_p99_ms: 2.483, 2.419, 2.367
- `sum_amount via columnar @ 100000 linhas` throughput_per_second: 431.9, 465.4, 446.1
- `filtered_sum via columnar @ 100000 linhas` latency_p50_ms: 12.23, 12.49, 12.28
- `filtered_sum via columnar @ 100000 linhas` latency_p95_ms: 12.31, 12.53, 12.61
- `filtered_sum via columnar @ 100000 linhas` latency_p99_ms: 12.31, 12.53, 12.63
- `filtered_sum via columnar @ 100000 linhas` throughput_per_second: 82.64, 81.06, 81.56
- `total_rows via parquet @ 100000 linhas` latency_p50_ms: 306.3, 273.3, 268.4
- `total_rows via parquet @ 100000 linhas` latency_p95_ms: 306.7, 278.6, 272.8
- `total_rows via parquet @ 100000 linhas` latency_p99_ms: 306.8, 279, 273.2
- `total_rows via parquet @ 100000 linhas` throughput_per_second: 3.285, 3.719, 3.732
- `sum_amount via parquet @ 100000 linhas` latency_p50_ms: 296.2, 298.3, 297.2
- `sum_amount via parquet @ 100000 linhas` latency_p95_ms: 303.8, 303.3, 298
- `sum_amount via parquet @ 100000 linhas` latency_p99_ms: 304.5, 303.7, 298.1
- `sum_amount via parquet @ 100000 linhas` throughput_per_second: 3.399, 3.36, 3.38
- `group_by_category via parquet @ 100000 linhas` latency_p50_ms: 316.4, 314.1, 316.5
- `group_by_category via parquet @ 100000 linhas` latency_p95_ms: 318.8, 315.6, 318.9
- `group_by_category via parquet @ 100000 linhas` latency_p99_ms: 319, 315.8, 319.1
- `group_by_category via parquet @ 100000 linhas` throughput_per_second: 3.172, 3.188, 3.19
- `filtered_sum via parquet @ 100000 linhas` latency_p50_ms: 288.5, 291.3, 289
- `filtered_sum via parquet @ 100000 linhas` latency_p95_ms: 289.5, 294.7, 290.1
- `filtered_sum via parquet @ 100000 linhas` latency_p99_ms: 289.5, 295, 290.2
- `filtered_sum via parquet @ 100000 linhas` throughput_per_second: 3.492, 3.48, 3.484
- `total_rows via row @ 500000 linhas` latency_p50_ms: 23.84, 20.14, 17.17
- `total_rows via row @ 500000 linhas` latency_p95_ms: 24.07, 20.33, 17.19
- `total_rows via row @ 500000 linhas` latency_p99_ms: 24.09, 20.35, 17.19
- `total_rows via row @ 500000 linhas` throughput_per_second: 49.46, 49.75, 60.22
- `sum_amount via row @ 500000 linhas` latency_p50_ms: 20.61, 28.6, 19.44
- `sum_amount via row @ 500000 linhas` latency_p95_ms: 26.61, 28.73, 19.64
- `sum_amount via row @ 500000 linhas` latency_p99_ms: 27.14, 28.74, 19.66
- `sum_amount via row @ 500000 linhas` throughput_per_second: 48.69, 40.57, 52.23
- `group_by_category via row @ 500000 linhas` latency_p50_ms: 40.33, 39.95, 37.9
- `group_by_category via row @ 500000 linhas` latency_p95_ms: 53.83, 52.43, 38.16
- `group_by_category via row @ 500000 linhas` latency_p99_ms: 55.03, 53.53, 38.18
- `group_by_category via row @ 500000 linhas` throughput_per_second: 26.15, 25.59, 26.72
- `filtered_sum via row @ 500000 linhas` latency_p50_ms: 30.7, 26.68, 23.29
- `filtered_sum via row @ 500000 linhas` latency_p95_ms: 31.27, 27.61, 23.59
- `filtered_sum via row @ 500000 linhas` latency_p99_ms: 31.32, 27.69, 23.62
- `filtered_sum via row @ 500000 linhas` throughput_per_second: 41.52, 40.26, 43.13
- `total_rows via columnar @ 500000 linhas` latency_p50_ms: 3.899, 3.741, 3.502
- `total_rows via columnar @ 500000 linhas` latency_p95_ms: 3.993, 4.133, 3.812
- `total_rows via columnar @ 500000 linhas` latency_p99_ms: 4.001, 4.167, 3.839
- `total_rows via columnar @ 500000 linhas` throughput_per_second: 256.7, 280.7, 294.9
- `sum_amount via columnar @ 500000 linhas` latency_p50_ms: 8.658, 8.816, 8.175
- `sum_amount via columnar @ 500000 linhas` latency_p95_ms: 8.734, 8.819, 8.471
- `sum_amount via columnar @ 500000 linhas` latency_p99_ms: 8.741, 8.819, 8.497
- `sum_amount via columnar @ 500000 linhas` throughput_per_second: 115.9, 115.6, 125
- `filtered_sum via columnar @ 500000 linhas` latency_p50_ms: 59.18, 58.47, 57.9
- `filtered_sum via columnar @ 500000 linhas` latency_p95_ms: 60.02, 59.9, 60.52
- `filtered_sum via columnar @ 500000 linhas` latency_p99_ms: 60.09, 60.03, 60.75
- `filtered_sum via columnar @ 500000 linhas` throughput_per_second: 17.15, 17.12, 17.51
- `total_rows via parquet @ 500000 linhas` latency_p50_ms: 1,508, 1,509, 1,521
- `total_rows via parquet @ 500000 linhas` latency_p95_ms: 1,510, 1,513, 1,523
- `total_rows via parquet @ 500000 linhas` latency_p99_ms: 1,510, 1,513, 1,523
- `total_rows via parquet @ 500000 linhas` throughput_per_second: 0.6677, 0.6651, 0.6588
- `sum_amount via parquet @ 500000 linhas` latency_p50_ms: 1,644, 1,651, 1,668
- `sum_amount via parquet @ 500000 linhas` latency_p95_ms: 1,656, 1,659, 1,671
- `sum_amount via parquet @ 500000 linhas` latency_p99_ms: 1,657, 1,659, 1,671
- `sum_amount via parquet @ 500000 linhas` throughput_per_second: 0.6099, 0.6075, 0.6066
- `group_by_category via parquet @ 500000 linhas` latency_p50_ms: 1,741, 1,750, 1,757
- `group_by_category via parquet @ 500000 linhas` latency_p95_ms: 1,755, 1,762, 1,758
- `group_by_category via parquet @ 500000 linhas` latency_p99_ms: 1,756, 1,763, 1,758
- `group_by_category via parquet @ 500000 linhas` throughput_per_second: 0.576, 0.573, 0.5691
- `filtered_sum via parquet @ 500000 linhas` latency_p50_ms: 1,602, 1,618, 1,608
- `filtered_sum via parquet @ 500000 linhas` latency_p95_ms: 1,610, 1,620, 1,623
- `filtered_sum via parquet @ 500000 linhas` latency_p99_ms: 1,611, 1,620, 1,624
- `filtered_sum via parquet @ 500000 linhas` throughput_per_second: 0.6242, 0.6202, 0.6226
- `total_rows via row @ 1000000 linhas` latency_p50_ms: 30.6, 31.86, 25.35
- `total_rows via row @ 1000000 linhas` latency_p95_ms: 31.17, 34.79, 26.42
- `total_rows via row @ 1000000 linhas` latency_p99_ms: 31.22, 35.06, 26.52
- `total_rows via row @ 1000000 linhas` throughput_per_second: 33.9, 36, 41.32
- `sum_amount via row @ 1000000 linhas` latency_p50_ms: 33.46, 32.84, 33.04
- `sum_amount via row @ 1000000 linhas` latency_p95_ms: 34.28, 40.02, 33.31
- `sum_amount via row @ 1000000 linhas` latency_p99_ms: 34.36, 40.66, 33.33
- `sum_amount via row @ 1000000 linhas` throughput_per_second: 30.12, 31.42, 31.1
- `group_by_category via row @ 1000000 linhas` latency_p50_ms: 66.87, 65.6, 64.6
- `group_by_category via row @ 1000000 linhas` latency_p95_ms: 67.81, 66.07, 64.85
- `group_by_category via row @ 1000000 linhas` latency_p99_ms: 67.89, 66.12, 64.87
- `group_by_category via row @ 1000000 linhas` throughput_per_second: 15.3, 15.73, 15.62
- `filtered_sum via row @ 1000000 linhas` latency_p50_ms: 38.67, 38.42, 37.86
- `filtered_sum via row @ 1000000 linhas` latency_p95_ms: 39.57, 38.96, 38.53
- `filtered_sum via row @ 1000000 linhas` latency_p99_ms: 39.65, 39.01, 38.59
- `filtered_sum via row @ 1000000 linhas` throughput_per_second: 25.98, 26.48, 27.54
- `total_rows via columnar @ 1000000 linhas` latency_p50_ms: 6.159, 6.471, 6.249
- `total_rows via columnar @ 1000000 linhas` latency_p95_ms: 6.598, 6.687, 6.696
- `total_rows via columnar @ 1000000 linhas` latency_p99_ms: 6.637, 6.706, 6.735
- `total_rows via columnar @ 1000000 linhas` throughput_per_second: 168.2, 162.8, 161.6
- `sum_amount via columnar @ 1000000 linhas` latency_p50_ms: 15.65, 16.99, 15.74
- `sum_amount via columnar @ 1000000 linhas` latency_p95_ms: 15.77, 17.44, 15.98
- `sum_amount via columnar @ 1000000 linhas` latency_p99_ms: 15.79, 17.48, 16
- `sum_amount via columnar @ 1000000 linhas` throughput_per_second: 63.96, 59.78, 64.18
- `filtered_sum via columnar @ 1000000 linhas` latency_p50_ms: 115, 115.4, 114.7
- `filtered_sum via columnar @ 1000000 linhas` latency_p95_ms: 115.6, 117, 116
- `filtered_sum via columnar @ 1000000 linhas` latency_p99_ms: 115.6, 117.1, 116.2
- `filtered_sum via columnar @ 1000000 linhas` throughput_per_second: 8.745, 8.674, 8.77
- `total_rows via parquet @ 1000000 linhas` latency_p50_ms: 3,038, 3,073, 3,075
- `total_rows via parquet @ 1000000 linhas` latency_p95_ms: 3,048, 3,084, 3,095
- `total_rows via parquet @ 1000000 linhas` latency_p99_ms: 3,049, 3,085, 3,097
- `total_rows via parquet @ 1000000 linhas` throughput_per_second: 0.3297, 0.3283, 0.3278
- `sum_amount via parquet @ 1000000 linhas` latency_p50_ms: 3,359, 3,335, 3,363
- `sum_amount via parquet @ 1000000 linhas` latency_p95_ms: 3,381, 3,359, 3,403
- `sum_amount via parquet @ 1000000 linhas` latency_p99_ms: 3,383, 3,361, 3,407
- `sum_amount via parquet @ 1000000 linhas` throughput_per_second: 0.3004, 0.3, 0.3001
- `group_by_category via parquet @ 1000000 linhas` latency_p50_ms: 3,528, 3,535, 3,573
- `group_by_category via parquet @ 1000000 linhas` latency_p95_ms: 3,546, 3,539, 3,580
- `group_by_category via parquet @ 1000000 linhas` latency_p99_ms: 3,548, 3,540, 3,581
- `group_by_category via parquet @ 1000000 linhas` throughput_per_second: 0.2841, 0.2855, 0.2815
- `filtered_sum via parquet @ 1000000 linhas` latency_p50_ms: 3,267, 3,266, 3,275
- `filtered_sum via parquet @ 1000000 linhas` latency_p95_ms: 3,270, 3,277, 3,284
- `filtered_sum via parquet @ 1000000 linhas` latency_p99_ms: 3,270, 3,278, 3,285
- `filtered_sum via parquet @ 1000000 linhas` throughput_per_second: 0.307, 0.3079, 0.306
- `total_rows via row @ 2000000 linhas` latency_p50_ms: 48.77, 39.02, 39.22
- `total_rows via row @ 2000000 linhas` latency_p95_ms: 49.89, 39.03, 48.92
- `total_rows via row @ 2000000 linhas` latency_p99_ms: 49.99, 39.03, 49.78
- `total_rows via row @ 2000000 linhas` throughput_per_second: 20.62, 25.85, 25.64
- `sum_amount via row @ 2000000 linhas` latency_p50_ms: 59, 49.71, 51.56
- `sum_amount via row @ 2000000 linhas` latency_p95_ms: 60.05, 50.18, 51.78
- `sum_amount via row @ 2000000 linhas` latency_p99_ms: 60.15, 50.22, 51.8
- `sum_amount via row @ 2000000 linhas` throughput_per_second: 17.06, 20.26, 19.92
- `group_by_category via row @ 2000000 linhas` latency_p50_ms: 123, 116.4, 115.3
- `group_by_category via row @ 2000000 linhas` latency_p95_ms: 123.7, 116.9, 128.7
- `group_by_category via row @ 2000000 linhas` latency_p99_ms: 123.8, 116.9, 129.9
- `group_by_category via row @ 2000000 linhas` throughput_per_second: 8.155, 8.642, 8.741
- `filtered_sum via row @ 2000000 linhas` latency_p50_ms: 70.86, 63.74, 63.78
- `filtered_sum via row @ 2000000 linhas` latency_p95_ms: 71.82, 64.49, 63.83
- `filtered_sum via row @ 2000000 linhas` latency_p99_ms: 71.9, 64.55, 63.84
- `filtered_sum via row @ 2000000 linhas` throughput_per_second: 14.27, 16.17, 15.77
- `total_rows via columnar @ 2000000 linhas` latency_p50_ms: 12.86, 12.8, 12.66
- `total_rows via columnar @ 2000000 linhas` latency_p95_ms: 13.09, 101.8, 114
- `total_rows via columnar @ 2000000 linhas` latency_p99_ms: 13.11, 109.7, 123
- `total_rows via columnar @ 2000000 linhas` throughput_per_second: 78.49, 81.1, 79.76
- `sum_amount via columnar @ 2000000 linhas` latency_p50_ms: 31.75, 32.44, 32
- `sum_amount via columnar @ 2000000 linhas` latency_p95_ms: 32.29, 32.63, 32.24
- `sum_amount via columnar @ 2000000 linhas` latency_p99_ms: 32.34, 32.65, 32.27
- `sum_amount via columnar @ 2000000 linhas` throughput_per_second: 31.69, 31.08, 32.02
- `filtered_sum via columnar @ 2000000 linhas` latency_p50_ms: 228.3, 229, 225.2
- `filtered_sum via columnar @ 2000000 linhas` latency_p95_ms: 230.4, 231.2, 227.3
- `filtered_sum via columnar @ 2000000 linhas` latency_p99_ms: 230.6, 231.4, 227.5
- `filtered_sum via columnar @ 2000000 linhas` throughput_per_second: 4.48, 4.453, 4.479
- `total_rows via parquet @ 2000000 linhas` latency_p50_ms: 5,663, 5,572, 5,594
- `total_rows via parquet @ 2000000 linhas` latency_p95_ms: 5,670, 5,636, 5,614
- `total_rows via parquet @ 2000000 linhas` latency_p99_ms: 5,671, 5,641, 5,616
- `total_rows via parquet @ 2000000 linhas` throughput_per_second: 0.1775, 0.1795, 0.179
- `sum_amount via parquet @ 2000000 linhas` latency_p50_ms: 6,190, 6,170, 6,182
- `sum_amount via parquet @ 2000000 linhas` latency_p95_ms: 6,307, 6,195, 6,257
- `sum_amount via parquet @ 2000000 linhas` latency_p99_ms: 6,318, 6,197, 6,263
- `sum_amount via parquet @ 2000000 linhas` throughput_per_second: 0.1618, 0.1624, 0.1621
- `group_by_category via parquet @ 2000000 linhas` latency_p50_ms: 6,546, 6,594, 6,608
- `group_by_category via parquet @ 2000000 linhas` latency_p95_ms: 6,552, 6,737, 6,759
- `group_by_category via parquet @ 2000000 linhas` latency_p99_ms: 6,553, 6,749, 6,773
- `group_by_category via parquet @ 2000000 linhas` throughput_per_second: 0.1535, 0.1538, 0.1533
- `filtered_sum via parquet @ 2000000 linhas` latency_p50_ms: 5,994, 6,021, 6,010
- `filtered_sum via parquet @ 2000000 linhas` latency_p95_ms: 6,108, 6,048, 6,278
- `filtered_sum via parquet @ 2000000 linhas` latency_p99_ms: 6,118, 6,050, 6,302
- `filtered_sum via parquet @ 2000000 linhas` throughput_per_second: 0.1672, 0.1669, 0.1674

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

