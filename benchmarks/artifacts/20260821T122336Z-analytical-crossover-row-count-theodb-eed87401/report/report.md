# analytical/crossover/row-count on theodb

**Status:** EXPLORATORY · **Profile:** research · **Run:** `20260821T122336Z-analytical-crossover-row-count-theodb-eed87401`

> This run is **EXPLORATORY**. Research runs may use non-frozen parameters, so the numbers below cannot back a published claim.

## Results

| Configuration | Throughput/s | Recall | p50 ms | p95 ms | p99 ms | Stable |
|---|---|---|---|---|---|---|
| total_rows via row @ 10000 linhas | 1,648 | _not measured_ | 1.079 | 1.287 | 1.306 | **no** |
| sum_amount via row @ 10000 linhas | 1,325 | _not measured_ | 0.8029 | 1.85 | 1.952 | **no** |
| group_by_category via row @ 10000 linhas | 500.8 | _not measured_ | 2.895 | 3.432 | 3.48 | **no** |
| filtered_sum via row @ 10000 linhas | 1,127 | _not measured_ | 1.518 | 1.737 | 1.793 | **no** |
| total_rows via columnar @ 10000 linhas | 1,109 | _not measured_ | 1.017 | 1.108 | 1.11 | **no** |
| sum_amount via columnar @ 10000 linhas | 928 | _not measured_ | 1.085 | 1.285 | 1.303 | **no** |
| group_by_category via columnar @ 10000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 10000 linhas | 367.9 | _not measured_ | 3.584 | 3.866 | 3.89 | **no** |
| total_rows via parquet @ 10000 linhas | 25.73 | _not measured_ | 42.01 | 43.51 | 43.64 | **no** |
| sum_amount via parquet @ 10000 linhas | 26.3 | _not measured_ | 42.18 | 50.44 | 51.27 | **no** |
| group_by_category via parquet @ 10000 linhas | 24.92 | _not measured_ | 43.18 | 45.2 | 45.26 | **no** |
| filtered_sum via parquet @ 10000 linhas | 29.92 | _not measured_ | 37.39 | 47.94 | 48.88 | **no** |
| total_rows via row @ 50000 linhas | 375.3 | _not measured_ | 2.765 | 3.47 | 3.536 | **no** |
| sum_amount via row @ 50000 linhas | 310.4 | _not measured_ | 3.865 | 4.462 | 4.515 | **no** |
| group_by_category via row @ 50000 linhas | 96.49 | _not measured_ | 13.58 | 15.87 | 16.02 | **no** |
| filtered_sum via row @ 50000 linhas | 222.7 | _not measured_ | 5.535 | 6.054 | 6.1 | **no** |
| total_rows via columnar @ 50000 linhas | 695.9 | _not measured_ | 1.501 | 1.523 | 1.525 | **no** |
| sum_amount via columnar @ 50000 linhas | 488 | _not measured_ | 2.176 | 2.222 | 2.226 | **no** |
| group_by_category via columnar @ 50000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 50000 linhas | 118.1 | _not measured_ | 9.119 | 9.505 | 9.612 | **no** |
| total_rows via parquet @ 50000 linhas | 5.127 | _not measured_ | 216.4 | 229.3 | 230.1 | **no** |
| sum_amount via parquet @ 50000 linhas | 4.416 | _not measured_ | 232.1 | 244.9 | 246 | yes |
| group_by_category via parquet @ 50000 linhas | 4.473 | _not measured_ | 247.4 | 259.3 | 260.2 | **no** |
| filtered_sum via parquet @ 50000 linhas | 4.736 | _not measured_ | 222.4 | 245.2 | 248.2 | **no** |
| total_rows via row @ 100000 linhas | 173.2 | _not measured_ | 7.527 | 8.634 | 8.719 | **no** |
| sum_amount via row @ 100000 linhas | 115.3 | _not measured_ | 9.446 | 11.02 | 11.23 | **no** |
| group_by_category via row @ 100000 linhas | 43.85 | _not measured_ | 25.94 | 29.42 | 29.77 | **no** |
| filtered_sum via row @ 100000 linhas | 108.5 | _not measured_ | 11.37 | 12.29 | 12.53 | **no** |
| total_rows via columnar @ 100000 linhas | 578.9 | _not measured_ | 1.942 | 1.992 | 1.997 | **no** |
| sum_amount via columnar @ 100000 linhas | 296.8 | _not measured_ | 3.427 | 3.566 | 3.578 | **no** |
| group_by_category via columnar @ 100000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 100000 linhas | 60.48 | _not measured_ | 18.02 | 18.95 | 19.04 | **no** |
| total_rows via parquet @ 100000 linhas | 2.732 | _not measured_ | 384.4 | 403.3 | 405 | **no** |
| sum_amount via parquet @ 100000 linhas | 2.547 | _not measured_ | 401.8 | 426.7 | 429.8 | **no** |
| group_by_category via parquet @ 100000 linhas | 2.402 | _not measured_ | 451.2 | 461.1 | 461.9 | **no** |
| filtered_sum via parquet @ 100000 linhas | 2.63 | _not measured_ | 407.5 | 425.2 | 425.2 | **no** |
| total_rows via row @ 500000 linhas | 44.91 | _not measured_ | 22.85 | 24.62 | 24.78 | **no** |
| sum_amount via row @ 500000 linhas | 37.18 | _not measured_ | 29.32 | 30.77 | 30.9 | **no** |
| group_by_category via row @ 500000 linhas | 19.96 | _not measured_ | 53.09 | 61.51 | 62.26 | **no** |
| filtered_sum via row @ 500000 linhas | 30.36 | _not measured_ | 33.57 | 35.41 | 35.57 | **no** |
| total_rows via columnar @ 500000 linhas | 176.2 | _not measured_ | 5.715 | 6.01 | 6.012 | **no** |
| sum_amount via columnar @ 500000 linhas | 80.38 | _not measured_ | 13.19 | 13.28 | 13.29 | yes |
| group_by_category via columnar @ 500000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 500000 linhas | 14.84 | _not measured_ | 78.52 | 87 | 87.75 | **no** |
| total_rows via parquet @ 500000 linhas | 0.4611 | _not measured_ | 2,174 | 2,225 | 2,229 | yes |
| sum_amount via parquet @ 500000 linhas | 0.4271 | _not measured_ | 2,417 | 2,462 | 2,469 | yes |
| group_by_category via parquet @ 500000 linhas | 0.409 | _not measured_ | 2,486 | 2,536 | 2,548 | **no** |
| filtered_sum via parquet @ 500000 linhas | 0.4638 | _not measured_ | 2,378 | 2,397 | 2,398 | yes |
| total_rows via row @ 1000000 linhas | 30.87 | _not measured_ | 33.63 | 35.59 | 35.76 | **no** |
| sum_amount via row @ 1000000 linhas | 24.9 | _not measured_ | 45.35 | 50.6 | 50.63 | **no** |
| group_by_category via row @ 1000000 linhas | 11.33 | _not measured_ | 93.54 | 95.58 | 95.76 | **no** |
| filtered_sum via row @ 1000000 linhas | 20.33 | _not measured_ | 52.82 | 54.04 | 54.15 | **no** |
| total_rows via columnar @ 1000000 linhas | 107.2 | _not measured_ | 9.359 | 9.419 | 9.425 | **no** |
| sum_amount via columnar @ 1000000 linhas | 44.21 | _not measured_ | 23.33 | 23.55 | 23.57 | **no** |
| group_by_category via columnar @ 1000000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 1000000 linhas | 6.28 | _not measured_ | 165.2 | 179.5 | 180.8 | **no** |
| total_rows via parquet @ 1000000 linhas | 0.228 | _not measured_ | 4,512 | 4,571 | 4,576 | yes |
| sum_amount via parquet @ 1000000 linhas | 0.2149 | _not measured_ | 4,795 | 4,977 | 5,004 | **no** |
| group_by_category via parquet @ 1000000 linhas | 0.202 | _not measured_ | 5,093 | 5,179 | 5,187 | **no** |
| filtered_sum via parquet @ 1000000 linhas | 0.2276 | _not measured_ | 4,551 | 4,652 | 4,671 | **no** |
| total_rows via row @ 2000000 linhas | 18.41 | _not measured_ | 54.51 | 57.39 | 57.65 | **no** |
| sum_amount via row @ 2000000 linhas | 14.06 | _not measured_ | 76.69 | 82.5 | 82.94 | **no** |
| group_by_category via row @ 2000000 linhas | 5.873 | _not measured_ | 170.8 | 188.6 | 189.5 | **no** |
| filtered_sum via row @ 2000000 linhas | 11.75 | _not measured_ | 90.06 | 95.37 | 95.52 | **no** |
| total_rows via columnar @ 2000000 linhas | 58.5 | _not measured_ | 18.92 | 120.4 | 129.6 | **no** |
| sum_amount via columnar @ 2000000 linhas | 24.18 | _not measured_ | 42.19 | 46.05 | 46.06 | **no** |
| group_by_category via columnar @ 2000000 linhas | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar @ 2000000 linhas | 3.047 | _not measured_ | 340 | 370.6 | 370.9 | **no** |
| total_rows via parquet @ 2000000 linhas | 0.125 | _not measured_ | 8,393 | 8,539 | 8,589 | yes |
| sum_amount via parquet @ 2000000 linhas | 0.1137 | _not measured_ | 9,184 | 9,600 | 9,624 | yes |
| group_by_category via parquet @ 2000000 linhas | 0.1057 | _not measured_ | 9,684 | 1.027e+04 | 1.033e+04 | **no** |
| filtered_sum via parquet @ 2000000 linhas | 0.1149 | _not measured_ | 9,035 | 9,228 | 9,239 | yes |

Unstable points are reported, not removed. Their repetitions disagree by more than the declared threshold, so the median below is a weaker claim than it looks:

- `total_rows via row @ 10000 linhas`: latency_p50_ms cv=0.419; latency_p95_ms cv=0.430; latency_p99_ms cv=0.431; throughput_per_second cv=0.128
- `sum_amount via row @ 10000 linhas`: latency_p50_ms cv=0.558; latency_p95_ms cv=0.459; latency_p99_ms cv=0.463; throughput_per_second cv=0.327
- `group_by_category via row @ 10000 linhas`: latency_p50_ms cv=0.214; latency_p95_ms cv=0.311; latency_p99_ms cv=0.318; throughput_per_second cv=0.117
- `filtered_sum via row @ 10000 linhas`: latency_p50_ms cv=0.172; latency_p95_ms cv=0.084; latency_p99_ms cv=0.087; throughput_per_second cv=0.081
- `total_rows via columnar @ 10000 linhas`: latency_p50_ms cv=0.088; throughput_per_second cv=0.067
- `sum_amount via columnar @ 10000 linhas`: latency_p50_ms cv=0.119; latency_p95_ms cv=0.095; latency_p99_ms cv=0.095; throughput_per_second cv=0.095
- `filtered_sum via columnar @ 10000 linhas`: latency_p50_ms cv=0.177; latency_p95_ms cv=0.179; latency_p99_ms cv=0.179; throughput_per_second cv=0.118
- `total_rows via parquet @ 10000 linhas`: latency_p50_ms cv=0.083; latency_p95_ms cv=0.097; latency_p99_ms cv=0.098
- `sum_amount via parquet @ 10000 linhas`: throughput_per_second cv=0.073
- `group_by_category via parquet @ 10000 linhas`: latency_p99_ms cv=0.055; throughput_per_second cv=0.054
- `filtered_sum via parquet @ 10000 linhas`: latency_p50_ms cv=0.161; latency_p95_ms cv=0.142; latency_p99_ms cv=0.145; throughput_per_second cv=0.098
- `total_rows via row @ 50000 linhas`: latency_p50_ms cv=0.318; latency_p95_ms cv=0.306; latency_p99_ms cv=0.305; throughput_per_second cv=0.255
- `sum_amount via row @ 50000 linhas`: latency_p50_ms cv=0.142; latency_p95_ms cv=0.187; latency_p99_ms cv=0.191
- `group_by_category via row @ 50000 linhas`: latency_p50_ms cv=0.154; latency_p95_ms cv=0.075; latency_p99_ms cv=0.070; throughput_per_second cv=0.115
- `filtered_sum via row @ 50000 linhas`: latency_p50_ms cv=0.164; latency_p95_ms cv=0.228; latency_p99_ms cv=0.234; throughput_per_second cv=0.150
- `total_rows via columnar @ 50000 linhas`: latency_p50_ms cv=0.213; latency_p95_ms cv=0.207; latency_p99_ms cv=0.207; throughput_per_second cv=0.215
- `sum_amount via columnar @ 50000 linhas`: latency_p50_ms cv=0.121; latency_p95_ms cv=0.142; latency_p99_ms cv=0.144; throughput_per_second cv=0.130
- `filtered_sum via columnar @ 50000 linhas`: latency_p50_ms cv=0.117; latency_p95_ms cv=0.144; latency_p99_ms cv=0.147; throughput_per_second cv=0.087
- `total_rows via parquet @ 50000 linhas`: latency_p50_ms cv=0.060; throughput_per_second cv=0.061
- `group_by_category via parquet @ 50000 linhas`: latency_p50_ms cv=0.062; throughput_per_second cv=0.053
- `filtered_sum via parquet @ 50000 linhas`: latency_p95_ms cv=0.111; latency_p99_ms cv=0.118; throughput_per_second cv=0.072
- `total_rows via row @ 100000 linhas`: latency_p50_ms cv=0.144; latency_p95_ms cv=0.090; latency_p99_ms cv=0.087; throughput_per_second cv=0.061
- `sum_amount via row @ 100000 linhas`: latency_p50_ms cv=0.084; latency_p95_ms cv=0.199; latency_p99_ms cv=0.207; throughput_per_second cv=0.173
- `group_by_category via row @ 100000 linhas`: latency_p95_ms cv=0.074; latency_p99_ms cv=0.078; throughput_per_second cv=0.060
- `filtered_sum via row @ 100000 linhas`: latency_p50_ms cv=0.113; latency_p95_ms cv=0.135; latency_p99_ms cv=0.139; throughput_per_second cv=0.151
- `total_rows via columnar @ 100000 linhas`: latency_p95_ms cv=0.091; latency_p99_ms cv=0.096; throughput_per_second cv=0.078
- `sum_amount via columnar @ 100000 linhas`: latency_p50_ms cv=0.144; latency_p95_ms cv=0.132; latency_p99_ms cv=0.131; throughput_per_second cv=0.146
- `filtered_sum via columnar @ 100000 linhas`: latency_p50_ms cv=0.076; latency_p95_ms cv=0.070; latency_p99_ms cv=0.070; throughput_per_second cv=0.103
- `total_rows via parquet @ 100000 linhas`: latency_p50_ms cv=0.072; latency_p95_ms cv=0.061; latency_p99_ms cv=0.061; throughput_per_second cv=0.058
- `sum_amount via parquet @ 100000 linhas`: latency_p50_ms cv=0.072; throughput_per_second cv=0.062
- `group_by_category via parquet @ 100000 linhas`: latency_p50_ms cv=0.074; latency_p95_ms cv=0.092; latency_p99_ms cv=0.094
- `filtered_sum via parquet @ 100000 linhas`: latency_p95_ms cv=0.064; latency_p99_ms cv=0.067; throughput_per_second cv=0.060
- `total_rows via row @ 500000 linhas`: latency_p50_ms cv=0.100; latency_p95_ms cv=0.128; latency_p99_ms cv=0.130; throughput_per_second cv=0.079
- `sum_amount via row @ 500000 linhas`: latency_p95_ms cv=0.124; latency_p99_ms cv=0.131; throughput_per_second cv=0.138
- `group_by_category via row @ 500000 linhas`: latency_p50_ms cv=0.118; latency_p95_ms cv=0.093; latency_p99_ms cv=0.092; throughput_per_second cv=0.073
- `filtered_sum via row @ 500000 linhas`: latency_p50_ms cv=0.129; latency_p95_ms cv=0.135; latency_p99_ms cv=0.135; throughput_per_second cv=0.135
- `total_rows via columnar @ 500000 linhas`: latency_p95_ms cv=0.052; latency_p99_ms cv=0.053
- `filtered_sum via columnar @ 500000 linhas`: latency_p50_ms cv=0.117; latency_p95_ms cv=0.083; latency_p99_ms cv=0.080; throughput_per_second cv=0.160
- `group_by_category via parquet @ 500000 linhas`: latency_p50_ms cv=0.057
- `total_rows via row @ 1000000 linhas`: latency_p50_ms cv=0.183; latency_p95_ms cv=0.163; latency_p99_ms cv=0.161; throughput_per_second cv=0.150
- `sum_amount via row @ 1000000 linhas`: latency_p50_ms cv=0.093; throughput_per_second cv=0.123
- `group_by_category via row @ 1000000 linhas`: throughput_per_second cv=0.069
- `filtered_sum via row @ 1000000 linhas`: latency_p50_ms cv=0.109; latency_p95_ms cv=0.086; latency_p99_ms cv=0.086; throughput_per_second cv=0.068
- `total_rows via columnar @ 1000000 linhas`: latency_p50_ms cv=0.082; latency_p95_ms cv=0.071; latency_p99_ms cv=0.070; throughput_per_second cv=0.082
- `sum_amount via columnar @ 1000000 linhas`: throughput_per_second cv=0.052
- `filtered_sum via columnar @ 1000000 linhas`: latency_p50_ms cv=0.056; latency_p95_ms cv=0.055; latency_p99_ms cv=0.055; throughput_per_second cv=0.075
- `sum_amount via parquet @ 1000000 linhas`: throughput_per_second cv=0.077
- `group_by_category via parquet @ 1000000 linhas`: latency_p95_ms cv=0.065; latency_p99_ms cv=0.068; throughput_per_second cv=0.066
- `filtered_sum via parquet @ 1000000 linhas`: latency_p95_ms cv=0.059; latency_p99_ms cv=0.060
- `total_rows via row @ 2000000 linhas`: latency_p50_ms cv=0.132; latency_p95_ms cv=0.127; latency_p99_ms cv=0.127
- `sum_amount via row @ 2000000 linhas`: latency_p95_ms cv=0.058; latency_p99_ms cv=0.060; throughput_per_second cv=0.068
- `group_by_category via row @ 2000000 linhas`: latency_p50_ms cv=0.057; throughput_per_second cv=0.053
- `filtered_sum via row @ 2000000 linhas`: latency_p50_ms cv=0.076; throughput_per_second cv=0.077
- `total_rows via columnar @ 2000000 linhas`: latency_p50_ms cv=0.066; latency_p95_ms cv=0.676; latency_p99_ms cv=0.689; throughput_per_second cv=0.073
- `sum_amount via columnar @ 2000000 linhas`: latency_p50_ms cv=0.050; latency_p95_ms cv=0.061; latency_p99_ms cv=0.065; throughput_per_second cv=0.056
- `filtered_sum via columnar @ 2000000 linhas`: latency_p95_ms cv=0.055; latency_p99_ms cv=0.058
- `group_by_category via parquet @ 2000000 linhas`: throughput_per_second cv=0.063

### Repetitions

Every repetition is retained:

- `total_rows via row @ 10000 linhas` latency_p50_ms: 1.465, 0.5913, 1.079
- `total_rows via row @ 10000 linhas` latency_p95_ms: 1.721, 0.671, 1.287
- `total_rows via row @ 10000 linhas` latency_p99_ms: 1.744, 0.6781, 1.306
- `total_rows via row @ 10000 linhas` throughput_per_second: 1,362, 1,756, 1,648
- `sum_amount via row @ 10000 linhas` latency_p50_ms: 1.817, 0.8029, 0.6997
- `sum_amount via row @ 10000 linhas` latency_p95_ms: 2.404, 0.8588, 1.85
- `sum_amount via row @ 10000 linhas` latency_p99_ms: 2.456, 0.8638, 1.952
- `sum_amount via row @ 10000 linhas` throughput_per_second: 735.6, 1,325, 1,454
- `group_by_category via row @ 10000 linhas` latency_p50_ms: 2.956, 1.962, 2.895
- `group_by_category via row @ 10000 linhas` latency_p95_ms: 3.964, 2.065, 3.432
- `group_by_category via row @ 10000 linhas` latency_p99_ms: 4.054, 2.074, 3.48
- `group_by_category via row @ 10000 linhas` throughput_per_second: 451.4, 569.3, 500.8
- `filtered_sum via row @ 10000 linhas` latency_p50_ms: 1.518, 1.107, 1.521
- `filtered_sum via row @ 10000 linhas` latency_p95_ms: 1.679, 1.737, 1.965
- `filtered_sum via row @ 10000 linhas` latency_p99_ms: 1.694, 1.793, 2.004
- `filtered_sum via row @ 10000 linhas` throughput_per_second: 976.7, 1,127, 1,128
- `total_rows via columnar @ 10000 linhas` latency_p50_ms: 1.084, 1.017, 0.9093
- `total_rows via columnar @ 10000 linhas` latency_p95_ms: 1.108, 1.119, 1.027
- `total_rows via columnar @ 10000 linhas` latency_p99_ms: 1.11, 1.128, 1.038
- `total_rows via columnar @ 10000 linhas` throughput_per_second: 987, 1,109, 1,113
- `sum_amount via columnar @ 10000 linhas` latency_p50_ms: 1.318, 1.075, 1.085
- `sum_amount via columnar @ 10000 linhas` latency_p95_ms: 1.411, 1.166, 1.285
- `sum_amount via columnar @ 10000 linhas` latency_p99_ms: 1.42, 1.174, 1.303
- `sum_amount via columnar @ 10000 linhas` throughput_per_second: 804.3, 968.5, 928
- `filtered_sum via columnar @ 10000 linhas` latency_p50_ms: 3.598, 2.593, 3.584
- `filtered_sum via columnar @ 10000 linhas` latency_p95_ms: 3.866, 2.839, 4.011
- `filtered_sum via columnar @ 10000 linhas` latency_p99_ms: 3.89, 2.861, 4.049
- `filtered_sum via columnar @ 10000 linhas` throughput_per_second: 306.7, 385.9, 367.9
- `total_rows via parquet @ 10000 linhas` latency_p50_ms: 45.8, 42.01, 38.83
- `total_rows via parquet @ 10000 linhas` latency_p95_ms: 49.04, 43.51, 40.6
- `total_rows via parquet @ 10000 linhas` latency_p99_ms: 49.33, 43.64, 40.76
- `total_rows via parquet @ 10000 linhas` throughput_per_second: 25.14, 25.73, 26.46
- `sum_amount via parquet @ 10000 linhas` latency_p50_ms: 42.18, 41.15, 44.5
- `sum_amount via parquet @ 10000 linhas` latency_p95_ms: 47.69, 50.44, 52.45
- `sum_amount via parquet @ 10000 linhas` latency_p99_ms: 48.18, 51.27, 53.15
- `sum_amount via parquet @ 10000 linhas` throughput_per_second: 24.04, 27.79, 26.3
- `group_by_category via parquet @ 10000 linhas` latency_p50_ms: 43.18, 40.81, 44.57
- `group_by_category via parquet @ 10000 linhas` latency_p95_ms: 43.59, 47.94, 45.2
- `group_by_category via parquet @ 10000 linhas` latency_p99_ms: 43.63, 48.58, 45.26
- `group_by_category via parquet @ 10000 linhas` throughput_per_second: 23.34, 24.92, 26.01
- `filtered_sum via parquet @ 10000 linhas` latency_p50_ms: 37.14, 37.39, 48.71
- `filtered_sum via parquet @ 10000 linhas` latency_p95_ms: 38.28, 47.94, 50.53
- `filtered_sum via parquet @ 10000 linhas` latency_p99_ms: 38.38, 48.88, 50.69
- `filtered_sum via parquet @ 10000 linhas` throughput_per_second: 30.81, 29.92, 25.55
- `total_rows via row @ 50000 linhas` latency_p50_ms: 4.594, 2.765, 2.723
- `total_rows via row @ 50000 linhas` latency_p95_ms: 5.365, 3.099, 3.47
- `total_rows via row @ 50000 linhas` latency_p99_ms: 5.433, 3.128, 3.536
- `total_rows via row @ 50000 linhas` throughput_per_second: 233.8, 384, 375.3
- `sum_amount via row @ 50000 linhas` latency_p50_ms: 3.865, 3.24, 4.317
- `sum_amount via row @ 50000 linhas` latency_p95_ms: 4.462, 3.354, 4.893
- `sum_amount via row @ 50000 linhas` latency_p99_ms: 4.515, 3.365, 4.944
- `sum_amount via row @ 50000 linhas` throughput_per_second: 311.5, 310.4, 290.5
- `group_by_category via row @ 50000 linhas` latency_p50_ms: 13.58, 10.56, 14.25
- `group_by_category via row @ 50000 linhas` latency_p95_ms: 15.91, 13.91, 15.87
- `group_by_category via row @ 50000 linhas` latency_p99_ms: 16.11, 14.2, 16.02
- `group_by_category via row @ 50000 linhas` throughput_per_second: 81.08, 96.49, 101.6
- `filtered_sum via row @ 50000 linhas` latency_p50_ms: 4.185, 5.746, 5.535
- `filtered_sum via row @ 50000 linhas` latency_p95_ms: 4.233, 6.741, 6.054
- `filtered_sum via row @ 50000 linhas` latency_p99_ms: 4.238, 6.829, 6.1
- `filtered_sum via row @ 50000 linhas` throughput_per_second: 248.4, 183.4, 222.7
- `total_rows via columnar @ 50000 linhas` latency_p50_ms: 1.2, 1.845, 1.501
- `total_rows via columnar @ 50000 linhas` latency_p95_ms: 1.34, 1.99, 1.523
- `total_rows via columnar @ 50000 linhas` latency_p99_ms: 1.353, 2.002, 1.525
- `total_rows via columnar @ 50000 linhas` throughput_per_second: 857.4, 555.4, 695.9
- `sum_amount via columnar @ 50000 linhas` latency_p50_ms: 2.063, 2.584, 2.176
- `sum_amount via columnar @ 50000 linhas` latency_p95_ms: 2.096, 2.726, 2.222
- `sum_amount via columnar @ 50000 linhas` latency_p99_ms: 2.099, 2.739, 2.226
- `sum_amount via columnar @ 50000 linhas` throughput_per_second: 529.2, 407.9, 488
- `filtered_sum via columnar @ 50000 linhas` latency_p50_ms: 8.307, 10.45, 9.119
- `filtered_sum via columnar @ 50000 linhas` latency_p95_ms: 9.505, 12.04, 9.431
- `filtered_sum via columnar @ 50000 linhas` latency_p99_ms: 9.612, 12.18, 9.459
- `filtered_sum via columnar @ 50000 linhas` throughput_per_second: 121.8, 103, 118.1
- `total_rows via parquet @ 50000 linhas` latency_p50_ms: 220.3, 196.5, 216.4
- `total_rows via parquet @ 50000 linhas` latency_p95_ms: 229.3, 224.9, 229.6
- `total_rows via parquet @ 50000 linhas` latency_p99_ms: 230.1, 227.4, 230.8
- `total_rows via parquet @ 50000 linhas` throughput_per_second: 5.127, 5.236, 4.657
- `sum_amount via parquet @ 50000 linhas` latency_p50_ms: 242.5, 232.1, 228.8
- `sum_amount via parquet @ 50000 linhas` latency_p95_ms: 248.7, 244.9, 230.4
- `sum_amount via parquet @ 50000 linhas` latency_p99_ms: 249.2, 246, 230.5
- `sum_amount via parquet @ 50000 linhas` throughput_per_second: 4.439, 4.416, 4.373
- `group_by_category via parquet @ 50000 linhas` latency_p50_ms: 224, 247.4, 252
- `group_by_category via parquet @ 50000 linhas` latency_p95_ms: 257.3, 262.1, 259.3
- `group_by_category via parquet @ 50000 linhas` latency_p99_ms: 260.2, 263.4, 259.9
- `group_by_category via parquet @ 50000 linhas` throughput_per_second: 4.473, 4.125, 4.571
- `filtered_sum via parquet @ 50000 linhas` latency_p50_ms: 230.3, 211.8, 222.4
- `filtered_sum via parquet @ 50000 linhas` latency_p95_ms: 281.5, 245.2, 226.6
- `filtered_sum via parquet @ 50000 linhas` latency_p99_ms: 286.1, 248.2, 226.9
- `filtered_sum via parquet @ 50000 linhas` throughput_per_second: 4.374, 4.736, 5.054
- `total_rows via row @ 100000 linhas` latency_p50_ms: 7.677, 5.856, 7.527
- `total_rows via row @ 100000 linhas` latency_p95_ms: 8.634, 7.424, 8.791
- `total_rows via row @ 100000 linhas` latency_p99_ms: 8.719, 7.563, 8.904
- `total_rows via row @ 100000 linhas` throughput_per_second: 173.2, 182.9, 161.9
- `sum_amount via row @ 100000 linhas` latency_p50_ms: 10.22, 8.642, 9.446
- `sum_amount via row @ 100000 linhas` latency_p95_ms: 15.18, 11.02, 10.84
- `sum_amount via row @ 100000 linhas` latency_p99_ms: 15.62, 11.23, 10.97
- `sum_amount via row @ 100000 linhas` throughput_per_second: 101, 141.6, 115.3
- `group_by_category via row @ 100000 linhas` latency_p50_ms: 25.48, 25.94, 27.62
- `group_by_category via row @ 100000 linhas` latency_p95_ms: 29.42, 26.81, 31.09
- `group_by_category via row @ 100000 linhas` latency_p99_ms: 29.77, 26.88, 31.39
- `group_by_category via row @ 100000 linhas` throughput_per_second: 46.78, 43.85, 41.51
- `filtered_sum via row @ 100000 linhas` latency_p50_ms: 11.94, 9.568, 11.37
- `filtered_sum via row @ 100000 linhas` latency_p95_ms: 15.23, 12.29, 12.05
- `filtered_sum via row @ 100000 linhas` latency_p99_ms: 15.52, 12.53, 12.11
- `filtered_sum via row @ 100000 linhas` throughput_per_second: 83.84, 112, 108.5
- `total_rows via columnar @ 100000 linhas` latency_p50_ms: 1.794, 1.942, 1.965
- `total_rows via columnar @ 100000 linhas` latency_p95_ms: 1.98, 1.992, 2.318
- `total_rows via columnar @ 100000 linhas` latency_p99_ms: 1.996, 1.997, 2.349
- `total_rows via columnar @ 100000 linhas` throughput_per_second: 589.4, 578.9, 509.4
- `sum_amount via columnar @ 100000 linhas` latency_p50_ms: 2.948, 3.427, 3.935
- `sum_amount via columnar @ 100000 linhas` latency_p95_ms: 3.121, 3.566, 4.069
- `sum_amount via columnar @ 100000 linhas` latency_p99_ms: 3.136, 3.578, 4.081
- `sum_amount via columnar @ 100000 linhas` throughput_per_second: 347.9, 296.8, 260.5
- `filtered_sum via columnar @ 100000 linhas` latency_p50_ms: 18.02, 19.07, 16.37
- `filtered_sum via columnar @ 100000 linhas` latency_p95_ms: 18.95, 20.46, 17.78
- `filtered_sum via columnar @ 100000 linhas` latency_p99_ms: 19.04, 20.58, 17.91
- `filtered_sum via columnar @ 100000 linhas` throughput_per_second: 60.48, 53.89, 66.32
- `total_rows via parquet @ 100000 linhas` latency_p50_ms: 423.4, 368.2, 384.4
- `total_rows via parquet @ 100000 linhas` latency_p95_ms: 426.1, 376.7, 403.3
- `total_rows via parquet @ 100000 linhas` latency_p99_ms: 426.3, 377.4, 405
- `total_rows via parquet @ 100000 linhas` throughput_per_second: 2.531, 2.839, 2.732
- `sum_amount via parquet @ 100000 linhas` latency_p50_ms: 447.5, 401.8, 391.9
- `sum_amount via parquet @ 100000 linhas` latency_p95_ms: 450, 415.2, 426.7
- `sum_amount via parquet @ 100000 linhas` latency_p99_ms: 450.2, 416.4, 429.8
- `sum_amount via parquet @ 100000 linhas` throughput_per_second: 2.333, 2.547, 2.636
- `group_by_category via parquet @ 100000 linhas` latency_p50_ms: 451.2, 412.2, 477.8
- `group_by_category via parquet @ 100000 linhas` latency_p95_ms: 461.1, 414.2, 498.6
- `group_by_category via parquet @ 100000 linhas` latency_p99_ms: 461.9, 414.4, 500.4
- `group_by_category via parquet @ 100000 linhas` throughput_per_second: 2.389, 2.527, 2.402
- `filtered_sum via parquet @ 100000 linhas` latency_p50_ms: 384.9, 424.5, 407.5
- `filtered_sum via parquet @ 100000 linhas` latency_p95_ms: 400.1, 425.2, 454.8
- `filtered_sum via parquet @ 100000 linhas` latency_p99_ms: 401.4, 425.2, 459
- `filtered_sum via parquet @ 100000 linhas` throughput_per_second: 2.642, 2.371, 2.63
- `total_rows via row @ 500000 linhas` latency_p50_ms: 25.42, 22.85, 20.84
- `total_rows via row @ 500000 linhas` latency_p95_ms: 29.52, 24.62, 23.24
- `total_rows via row @ 500000 linhas` latency_p99_ms: 29.88, 24.78, 23.46
- `total_rows via row @ 500000 linhas` throughput_per_second: 41.31, 44.91, 48.38
- `sum_amount via row @ 500000 linhas` latency_p50_ms: 30.87, 29.16, 29.32
- `sum_amount via row @ 500000 linhas` latency_p95_ms: 36.78, 29.24, 30.77
- `sum_amount via row @ 500000 linhas` latency_p99_ms: 37.31, 29.25, 30.9
- `sum_amount via row @ 500000 linhas` throughput_per_second: 34.18, 44.55, 37.18
- `group_by_category via row @ 500000 linhas` latency_p50_ms: 63.74, 51.54, 53.09
- `group_by_category via row @ 500000 linhas` latency_p95_ms: 66.09, 54.88, 61.51
- `group_by_category via row @ 500000 linhas` latency_p99_ms: 66.3, 55.18, 62.26
- `group_by_category via row @ 500000 linhas` throughput_per_second: 17.98, 20.74, 19.96
- `filtered_sum via row @ 500000 linhas` latency_p50_ms: 39.33, 30.56, 33.57
- `filtered_sum via row @ 500000 linhas` latency_p95_ms: 41.25, 31.6, 35.41
- `filtered_sum via row @ 500000 linhas` latency_p99_ms: 41.42, 31.69, 35.57
- `filtered_sum via row @ 500000 linhas` throughput_per_second: 26.69, 34.97, 30.36
- `total_rows via columnar @ 500000 linhas` latency_p50_ms: 5.987, 5.715, 5.47
- `total_rows via columnar @ 500000 linhas` latency_p95_ms: 6.01, 6.012, 5.489
- `total_rows via columnar @ 500000 linhas` latency_p99_ms: 6.012, 6.039, 5.49
- `total_rows via columnar @ 500000 linhas` throughput_per_second: 173.6, 176.2, 187.5
- `sum_amount via columnar @ 500000 linhas` latency_p50_ms: 13.06, 13.19, 13.31
- `sum_amount via columnar @ 500000 linhas` latency_p95_ms: 13.14, 13.28, 13.49
- `sum_amount via columnar @ 500000 linhas` latency_p99_ms: 13.15, 13.29, 13.51
- `sum_amount via columnar @ 500000 linhas` throughput_per_second: 80.38, 82.63, 76.2
- `filtered_sum via columnar @ 500000 linhas` latency_p50_ms: 78.52, 78.51, 95.63
- `filtered_sum via columnar @ 500000 linhas` latency_p95_ms: 87, 85.12, 98.91
- `filtered_sum via columnar @ 500000 linhas` latency_p99_ms: 87.75, 85.71, 99.2
- `filtered_sum via columnar @ 500000 linhas` throughput_per_second: 15.22, 14.84, 11.23
- `total_rows via parquet @ 500000 linhas` latency_p50_ms: 2,066, 2,174, 2,183
- `total_rows via parquet @ 500000 linhas` latency_p95_ms: 2,144, 2,225, 2,249
- `total_rows via parquet @ 500000 linhas` latency_p99_ms: 2,151, 2,229, 2,254
- `total_rows via parquet @ 500000 linhas` throughput_per_second: 0.4932, 0.461, 0.4611
- `sum_amount via parquet @ 500000 linhas` latency_p50_ms: 2,417, 2,418, 2,387
- `sum_amount via parquet @ 500000 linhas` latency_p95_ms: 2,453, 2,498, 2,462
- `sum_amount via parquet @ 500000 linhas` latency_p99_ms: 2,457, 2,505, 2,469
- `sum_amount via parquet @ 500000 linhas` throughput_per_second: 0.4251, 0.4271, 0.4485
- `group_by_category via parquet @ 500000 linhas` latency_p50_ms: 2,486, 2,677, 2,395
- `group_by_category via parquet @ 500000 linhas` latency_p95_ms: 2,518, 2,729, 2,536
- `group_by_category via parquet @ 500000 linhas` latency_p99_ms: 2,520, 2,734, 2,548
- `group_by_category via parquet @ 500000 linhas` throughput_per_second: 0.409, 0.3957, 0.4362
- `filtered_sum via parquet @ 500000 linhas` latency_p50_ms: 2,349, 2,389, 2,378
- `filtered_sum via parquet @ 500000 linhas` latency_p95_ms: 2,366, 2,397, 2,551
- `filtered_sum via parquet @ 500000 linhas` latency_p99_ms: 2,367, 2,398, 2,566
- `filtered_sum via parquet @ 500000 linhas` throughput_per_second: 0.4638, 0.4657, 0.4313
- `total_rows via row @ 1000000 linhas` latency_p50_ms: 44.5, 33.63, 32.16
- `total_rows via row @ 1000000 linhas` latency_p95_ms: 44.96, 35.59, 33.31
- `total_rows via row @ 1000000 linhas` latency_p99_ms: 45, 35.76, 33.41
- `total_rows via row @ 1000000 linhas` throughput_per_second: 24.47, 30.87, 32.95
- `sum_amount via row @ 1000000 linhas` latency_p50_ms: 50.29, 45.35, 41.77
- `sum_amount via row @ 1000000 linhas` latency_p95_ms: 50.6, 53.33, 49.7
- `sum_amount via row @ 1000000 linhas` latency_p99_ms: 50.63, 54.04, 50.4
- `sum_amount via row @ 1000000 linhas` throughput_per_second: 20.02, 25.12, 24.9
- `group_by_category via row @ 1000000 linhas` latency_p50_ms: 96.24, 89.1, 93.54
- `group_by_category via row @ 1000000 linhas` latency_p95_ms: 101.6, 94.96, 95.58
- `group_by_category via row @ 1000000 linhas` latency_p99_ms: 102, 95.48, 95.76
- `group_by_category via row @ 1000000 linhas` throughput_per_second: 10.48, 11.33, 12.04
- `filtered_sum via row @ 1000000 linhas` latency_p50_ms: 55.86, 52.82, 45.06
- `filtered_sum via row @ 1000000 linhas` latency_p95_ms: 60.87, 54.04, 51.65
- `filtered_sum via row @ 1000000 linhas` latency_p99_ms: 61.32, 54.15, 52.23
- `filtered_sum via row @ 1000000 linhas` throughput_per_second: 19.55, 20.33, 22.27
- `total_rows via columnar @ 1000000 linhas` latency_p50_ms: 9.359, 10.21, 8.675
- `total_rows via columnar @ 1000000 linhas` latency_p95_ms: 9.419, 10.32, 8.986
- `total_rows via columnar @ 1000000 linhas` latency_p99_ms: 9.425, 10.33, 9.014
- `total_rows via columnar @ 1000000 linhas` throughput_per_second: 107.2, 98.3, 115.8
- `sum_amount via columnar @ 1000000 linhas` latency_p50_ms: 23.33, 23.49, 21.79
- `sum_amount via columnar @ 1000000 linhas` latency_p95_ms: 23.55, 23.94, 22.21
- `sum_amount via columnar @ 1000000 linhas` latency_p99_ms: 23.57, 23.98, 22.25
- `sum_amount via columnar @ 1000000 linhas` throughput_per_second: 44.21, 43.17, 47.63
- `filtered_sum via columnar @ 1000000 linhas` latency_p50_ms: 177.4, 165.2, 159
- `filtered_sum via columnar @ 1000000 linhas` latency_p95_ms: 186.6, 179.5, 167.4
- `filtered_sum via columnar @ 1000000 linhas` latency_p99_ms: 187.4, 180.8, 168.1
- `filtered_sum via columnar @ 1000000 linhas` throughput_per_second: 6.28, 6.195, 7.078
- `total_rows via parquet @ 1000000 linhas` latency_p50_ms: 4,547, 4,512, 4,506
- `total_rows via parquet @ 1000000 linhas` latency_p95_ms: 4,670, 4,569, 4,571
- `total_rows via parquet @ 1000000 linhas` latency_p99_ms: 4,681, 4,574, 4,576
- `total_rows via parquet @ 1000000 linhas` throughput_per_second: 0.2266, 0.228, 0.2312
- `sum_amount via parquet @ 1000000 linhas` latency_p50_ms: 4,950, 4,678, 4,795
- `sum_amount via parquet @ 1000000 linhas` latency_p95_ms: 5,111, 4,977, 4,938
- `sum_amount via parquet @ 1000000 linhas` latency_p99_ms: 5,125, 5,004, 4,951
- `sum_amount via parquet @ 1000000 linhas` throughput_per_second: 0.2037, 0.2149, 0.2366
- `group_by_category via parquet @ 1000000 linhas` latency_p50_ms: 5,121, 4,809, 5,093
- `group_by_category via parquet @ 1000000 linhas` latency_p95_ms: 5,494, 4,818, 5,179
- `group_by_category via parquet @ 1000000 linhas` latency_p99_ms: 5,527, 4,819, 5,187
- `group_by_category via parquet @ 1000000 linhas` throughput_per_second: 0.202, 0.2255, 0.2009
- `filtered_sum via parquet @ 1000000 linhas` latency_p50_ms: 4,833, 4,551, 4,441
- `filtered_sum via parquet @ 1000000 linhas` latency_p95_ms: 5,086, 4,562, 4,652
- `filtered_sum via parquet @ 1000000 linhas` latency_p99_ms: 5,108, 4,563, 4,671
- `filtered_sum via parquet @ 1000000 linhas` throughput_per_second: 0.2128, 0.2276, 0.2285
- `total_rows via row @ 2000000 linhas` latency_p50_ms: 52.93, 66.9, 54.51
- `total_rows via row @ 2000000 linhas` latency_p95_ms: 57.09, 70.84, 57.39
- `total_rows via row @ 2000000 linhas` latency_p99_ms: 57.46, 71.19, 57.65
- `total_rows via row @ 2000000 linhas` throughput_per_second: 19.05, 17.37, 18.41
- `sum_amount via row @ 2000000 linhas` latency_p50_ms: 77.52, 71.76, 76.69
- `sum_amount via row @ 2000000 linhas` latency_p95_ms: 82.5, 75.76, 84.9
- `sum_amount via row @ 2000000 linhas` latency_p99_ms: 82.94, 76.12, 85.63
- `sum_amount via row @ 2000000 linhas` throughput_per_second: 13.19, 15.11, 14.06
- `group_by_category via row @ 2000000 linhas` latency_p50_ms: 178.5, 159.2, 170.8
- `group_by_category via row @ 2000000 linhas` latency_p95_ms: 188.6, 183.3, 195.3
- `group_by_category via row @ 2000000 linhas` latency_p99_ms: 189.5, 185.5, 197.5
- `group_by_category via row @ 2000000 linhas` throughput_per_second: 5.661, 6.285, 5.873
- `filtered_sum via row @ 2000000 linhas` latency_p50_ms: 90.06, 93.71, 80.78
- `filtered_sum via row @ 2000000 linhas` latency_p95_ms: 98.03, 95.37, 91.34
- `filtered_sum via row @ 2000000 linhas` latency_p99_ms: 98.74, 95.52, 92.28
- `filtered_sum via row @ 2000000 linhas` throughput_per_second: 11.49, 11.75, 13.22
- `total_rows via columnar @ 2000000 linhas` latency_p50_ms: 19.09, 16.93, 18.92
- `total_rows via columnar @ 2000000 linhas` latency_p95_ms: 19.28, 120.4, 124
- `total_rows via columnar @ 2000000 linhas` latency_p99_ms: 19.29, 129.6, 133.4
- `total_rows via columnar @ 2000000 linhas` throughput_per_second: 52.86, 61.05, 58.5
- `sum_amount via columnar @ 2000000 linhas` latency_p50_ms: 42.05, 42.19, 45.9
- `sum_amount via columnar @ 2000000 linhas` latency_p95_ms: 42.37, 47.81, 46.05
- `sum_amount via columnar @ 2000000 linhas` latency_p99_ms: 42.4, 48.31, 46.06
- `sum_amount via columnar @ 2000000 linhas` throughput_per_second: 24.18, 24.29, 21.95
- `filtered_sum via columnar @ 2000000 linhas` latency_p50_ms: 367.1, 337.5, 340
- `filtered_sum via columnar @ 2000000 linhas` latency_p95_ms: 370.6, 341.1, 379.3
- `filtered_sum via columnar @ 2000000 linhas` latency_p99_ms: 370.9, 341.4, 382.8
- `filtered_sum via columnar @ 2000000 linhas` throughput_per_second: 3.197, 3.032, 3.047
- `total_rows via parquet @ 2000000 linhas` latency_p50_ms: 7,974, 8,483, 8,393
- `total_rows via parquet @ 2000000 linhas` latency_p95_ms: 8,539, 8,614, 8,492
- `total_rows via parquet @ 2000000 linhas` latency_p99_ms: 8,589, 8,626, 8,501
- `total_rows via parquet @ 2000000 linhas` throughput_per_second: 0.1256, 0.1181, 0.125
- `sum_amount via parquet @ 2000000 linhas` latency_p50_ms: 9,184, 9,336, 8,869
- `sum_amount via parquet @ 2000000 linhas` latency_p95_ms: 9,650, 9,600, 9,120
- `sum_amount via parquet @ 2000000 linhas` latency_p99_ms: 9,691, 9,624, 9,142
- `sum_amount via parquet @ 2000000 linhas` throughput_per_second: 0.1137, 0.1073, 0.1139
- `group_by_category via parquet @ 2000000 linhas` latency_p50_ms: 1.028e+04, 9,684, 9,514
- `group_by_category via parquet @ 2000000 linhas` latency_p95_ms: 1.036e+04, 1.027e+04, 9,844
- `group_by_category via parquet @ 2000000 linhas` latency_p99_ms: 1.037e+04, 1.033e+04, 9,873
- `group_by_category via parquet @ 2000000 linhas` throughput_per_second: 0.09877, 0.1057, 0.112
- `filtered_sum via parquet @ 2000000 linhas` latency_p50_ms: 9,105, 9,035, 8,916
- `filtered_sum via parquet @ 2000000 linhas` latency_p95_ms: 9,228, 9,307, 9,088
- `filtered_sum via parquet @ 2000000 linhas` latency_p99_ms: 9,239, 9,331, 9,103
- `filtered_sum via parquet @ 2000000 linhas` throughput_per_second: 0.1123, 0.116, 0.1149

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

- Host: theo-b058-crossover-20260821
- CPU: Intel(R) Xeon(R) Platinum 8168 CPU @ 2.70GHz (16 logical, 16 physical)
- SMT: False · Governor: _unavailable_
- Memory: 67424514048 bytes
- Kernel: 6.8.0-124-generic · Runner: theodb-bench 0.4.0
- Benchmark commit: _none_ (dirty: _none_)

Fields shown in italics were not available on this host and are recorded as absent rather than as zero.

