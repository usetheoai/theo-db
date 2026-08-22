# analytical/synthetic/paths on theodb

**Status:** EXPLORATORY · **Profile:** research · **Run:** `20260821T212008Z-analytical-synthetic-paths-theodb-865ec86a`

> This run is **EXPLORATORY**. Research runs may use non-frozen parameters, so the numbers below cannot back a published claim.

## Results

| Configuration | Throughput/s | Recall | p50 ms | p95 ms | p99 ms | Stable |
|---|---|---|---|---|---|---|
| total_rows via row | 102.8 | _not measured_ | 9.764 | 9.866 | 9.873 | **no** |
| sum_amount via row | 86.06 | _not measured_ | 11.65 | 11.87 | 11.89 | yes |
| group_by_category via row | 43.75 | _not measured_ | 23.5 | 23.8 | 23.82 | yes |
| filtered_sum via row | 68.05 | _not measured_ | 15.07 | 16.84 | 16.98 | **no** |
| total_rows via columnar | 545.1 | _not measured_ | 1.914 | 1.981 | 1.987 | **no** |
| sum_amount via columnar | 274.2 | _not measured_ | 3.771 | 4.177 | 4.184 | **no** |
| group_by_category via columnar | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar | 43.46 | _not measured_ | 23.25 | 23.56 | 23.58 | **no** |
| total_rows via parquet | 1.663 | _not measured_ | 606.4 | 607 | 607 | yes |
| sum_amount via parquet | 1.516 | _not measured_ | 663.9 | 665.6 | 665.7 | yes |
| group_by_category via parquet | 1.444 | _not measured_ | 694.2 | 699.9 | 700.4 | yes |
| filtered_sum via parquet | 1.555 | _not measured_ | 643.4 | 656.8 | 657 | yes |

Unstable points are reported, not removed. Their repetitions disagree by more than the declared threshold, so the median below is a weaker claim than it looks:

- `total_rows via row`: throughput_per_second cv=0.053
- `filtered_sum via row`: latency_p95_ms cv=0.202; latency_p99_ms cv=0.216
- `total_rows via columnar`: latency_p95_ms cv=0.073; latency_p99_ms cv=0.077; throughput_per_second cv=0.051
- `sum_amount via columnar`: latency_p50_ms cv=0.058; latency_p95_ms cv=0.152; latency_p99_ms cv=0.163; throughput_per_second cv=0.062
- `filtered_sum via columnar`: latency_p95_ms cv=0.055; latency_p99_ms cv=0.056; throughput_per_second cv=0.054

### Repetitions

Every repetition is retained:

- `total_rows via row` latency_p50_ms: 9.782, 9.127, 9.764
- `total_rows via row` latency_p95_ms: 9.866, 9.569, 10.19
- `total_rows via row` latency_p99_ms: 9.873, 9.608, 10.22
- `total_rows via row` throughput_per_second: 102.3, 112.4, 102.8
- `sum_amount via row` latency_p50_ms: 12.06, 11.54, 11.65
- `sum_amount via row` latency_p95_ms: 12.23, 11.67, 11.87
- `sum_amount via row` latency_p99_ms: 12.24, 11.68, 11.89
- `sum_amount via row` throughput_per_second: 83.58, 87.21, 86.06
- `group_by_category via row` latency_p50_ms: 23.5, 22.56, 23.64
- `group_by_category via row` latency_p95_ms: 24.45, 23.16, 23.8
- `group_by_category via row` latency_p99_ms: 24.53, 23.22, 23.82
- `group_by_category via row` throughput_per_second: 43.75, 44.52, 43.35
- `filtered_sum via row` latency_p50_ms: 14.75, 15.2, 15.07
- `filtered_sum via row` latency_p95_ms: 14.94, 16.84, 21.92
- `filtered_sum via row` latency_p99_ms: 14.95, 16.98, 22.53
- `filtered_sum via row` throughput_per_second: 68.26, 68.05, 68.02
- `total_rows via columnar` latency_p50_ms: 1.811, 1.914, 1.92
- `total_rows via columnar` latency_p95_ms: 1.851, 1.981, 2.141
- `total_rows via columnar` latency_p99_ms: 1.855, 1.987, 2.161
- `total_rows via columnar` throughput_per_second: 592.3, 545.1, 540.5
- `sum_amount via columnar` latency_p50_ms: 3.671, 3.771, 4.098
- `sum_amount via columnar` latency_p95_ms: 3.691, 4.976, 4.177
- `sum_amount via columnar` latency_p99_ms: 3.693, 5.083, 4.184
- `sum_amount via columnar` throughput_per_second: 279.4, 274.2, 248.3
- `filtered_sum via columnar` latency_p50_ms: 21.73, 23.25, 23.93
- `filtered_sum via columnar` latency_p95_ms: 22.11, 23.56, 24.7
- `filtered_sum via columnar` latency_p99_ms: 22.15, 23.58, 24.77
- `filtered_sum via columnar` throughput_per_second: 46.79, 43.46, 42.22
- `total_rows via parquet` latency_p50_ms: 605.5, 606.9, 606.4
- `total_rows via parquet` latency_p95_ms: 605.8, 607, 609.8
- `total_rows via parquet` latency_p99_ms: 605.8, 607, 610.1
- `total_rows via parquet` throughput_per_second: 1.652, 1.663, 1.664
- `sum_amount via parquet` latency_p50_ms: 665, 663.9, 661.6
- `sum_amount via parquet` latency_p95_ms: 665.6, 665.6, 664.6
- `sum_amount via parquet` latency_p99_ms: 665.7, 665.7, 664.9
- `sum_amount via parquet` throughput_per_second: 1.514, 1.526, 1.516
- `group_by_category via parquet` latency_p50_ms: 697.5, 694.2, 693.7
- `group_by_category via parquet` latency_p95_ms: 698.2, 700.3, 699.9
- `group_by_category via parquet` latency_p99_ms: 698.2, 700.8, 700.4
- `group_by_category via parquet` throughput_per_second: 1.436, 1.447, 1.444
- `filtered_sum via parquet` latency_p50_ms: 638.5, 655, 643.4
- `filtered_sum via parquet` latency_p95_ms: 639.1, 656.8, 658.1
- `filtered_sum via parquet` latency_p99_ms: 639.2, 657, 659.4
- `filtered_sum via parquet` throughput_per_second: 1.575, 1.552, 1.555

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

