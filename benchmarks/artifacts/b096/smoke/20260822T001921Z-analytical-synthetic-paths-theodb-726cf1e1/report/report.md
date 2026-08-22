# analytical/synthetic/paths on theodb

**Status:** VALID · **Profile:** nightly · **Run:** `20260822T001921Z-analytical-synthetic-paths-theodb-726cf1e1`

> This result is **not publishable evidence**: the profile it ran under does not freeze methodology or datasets.

## Results

| Configuration | Throughput/s | Recall | p50 ms | p95 ms | p99 ms | Stable |
|---|---|---|---|---|---|---|
| total_rows via row | 97.17 | _not measured_ | 10.44 | 10.54 | 10.55 | yes |
| sum_amount via row | 77.57 | _not measured_ | 13.09 | 13.36 | 13.38 | yes |
| group_by_category via row | 38.37 | _not measured_ | 26.58 | 26.69 | 26.7 | yes |
| filtered_sum via row | 58.47 | _not measured_ | 17.56 | 17.58 | 17.59 | yes |
| total_rows via columnar | 469 | _not measured_ | 2.214 | 2.37 | 2.383 | **no** |
| sum_amount via columnar | 233.5 | _not measured_ | 4.363 | 4.448 | 4.456 | **no** |
| group_by_category via columnar | _not measured_ | _not measured_ | _not measured_ | _not measured_ | _not measured_ | yes |
| filtered_sum via columnar | 38.44 | _not measured_ | 26.07 | 26.1 | 26.11 | **no** |
| total_rows via parquet | 1.569 | _not measured_ | 638.2 | 642.6 | 643 | yes |
| sum_amount via parquet | 1.449 | _not measured_ | 695.8 | 703.2 | 703.9 | yes |
| group_by_category via parquet | 1.365 | _not measured_ | 734.8 | 737.4 | 737.7 | yes |
| filtered_sum via parquet | 1.485 | _not measured_ | 680.2 | 683.2 | 684.1 | yes |

Unstable points are reported, not removed. Their repetitions disagree by more than the declared threshold, so the median below is a weaker claim than it looks:

- `total_rows via columnar`: latency_p95_ms cv=0.059; latency_p99_ms cv=0.060; throughput_per_second cv=0.078
- `sum_amount via columnar`: latency_p95_ms cv=0.074; latency_p99_ms cv=0.076; throughput_per_second cv=0.056
- `filtered_sum via columnar`: latency_p50_ms cv=0.074; latency_p95_ms cv=0.073; latency_p99_ms cv=0.073; throughput_per_second cv=0.085

### Repetitions

Every repetition is retained:

- `total_rows via row` latency_p50_ms: 10.44, 10.49, 10.29
- `total_rows via row` latency_p95_ms: 10.54, 10.58, 10.41
- `total_rows via row` latency_p99_ms: 10.55, 10.59, 10.42
- `total_rows via row` throughput_per_second: 99.35, 95.69, 97.17
- `sum_amount via row` latency_p50_ms: 13.09, 13.09, 13.23
- `sum_amount via row` latency_p95_ms: 13.12, 13.36, 13.53
- `sum_amount via row` latency_p99_ms: 13.12, 13.38, 13.56
- `sum_amount via row` throughput_per_second: 77.87, 77.57, 76.94
- `group_by_category via row` latency_p50_ms: 26.53, 26.58, 26.86
- `group_by_category via row` latency_p95_ms: 26.69, 26.69, 27.1
- `group_by_category via row` latency_p99_ms: 26.7, 26.7, 27.12
- `group_by_category via row` throughput_per_second: 38.03, 38.77, 38.37
- `filtered_sum via row` latency_p50_ms: 17.56, 17.58, 17.42
- `filtered_sum via row` latency_p95_ms: 17.58, 17.82, 17.46
- `filtered_sum via row` latency_p99_ms: 17.59, 17.84, 17.46
- `filtered_sum via row` throughput_per_second: 58.25, 58.75, 58.47
- `total_rows via columnar` latency_p50_ms: 2.214, 2.274, 2.104
- `total_rows via columnar` latency_p95_ms: 2.37, 2.478, 2.205
- `total_rows via columnar` latency_p99_ms: 2.383, 2.496, 2.214
- `total_rows via columnar` throughput_per_second: 453.9, 469, 524.9
- `sum_amount via columnar` latency_p50_ms: 4.115, 4.534, 4.363
- `sum_amount via columnar` latency_p95_ms: 4.124, 4.781, 4.448
- `sum_amount via columnar` latency_p99_ms: 4.124, 4.803, 4.456
- `sum_amount via columnar` throughput_per_second: 248.5, 222.2, 233.5
- `filtered_sum via columnar` latency_p50_ms: 23.22, 26.07, 26.76
- `filtered_sum via columnar` latency_p95_ms: 23.31, 26.1, 26.83
- `filtered_sum via columnar` latency_p99_ms: 23.31, 26.11, 26.84
- `filtered_sum via columnar` throughput_per_second: 43.82, 38.44, 37.59
- `total_rows via parquet` latency_p50_ms: 638, 638.2, 643.6
- `total_rows via parquet` latency_p95_ms: 642.4, 642.6, 650.3
- `total_rows via parquet` latency_p99_ms: 642.8, 643, 650.9
- `total_rows via parquet` throughput_per_second: 1.574, 1.569, 1.555
- `sum_amount via parquet` latency_p50_ms: 695.8, 703.1, 695.3
- `sum_amount via parquet` latency_p95_ms: 703.2, 713.8, 696
- `sum_amount via parquet` latency_p99_ms: 703.9, 714.7, 696.1
- `sum_amount via parquet` throughput_per_second: 1.45, 1.423, 1.449
- `group_by_category via parquet` latency_p50_ms: 726.6, 734.8, 737.1
- `group_by_category via parquet` latency_p95_ms: 738.1, 737.4, 737.3
- `group_by_category via parquet` latency_p99_ms: 739.2, 737.7, 737.3
- `group_by_category via parquet` throughput_per_second: 1.38, 1.365, 1.36
- `filtered_sum via parquet` latency_p50_ms: 681.8, 680.2, 673.6
- `filtered_sum via parquet` latency_p95_ms: 682, 687.1, 683.2
- `filtered_sum via parquet` latency_p99_ms: 682, 687.7, 684.1
- `filtered_sum via parquet` throughput_per_second: 1.472, 1.485, 1.488

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

