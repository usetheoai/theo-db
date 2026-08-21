# vector/sift1m/frontier on theodb

**Status:** EXPLORATORY · **Profile:** research · **Run:** `20260821T095912Z-vector-sift1m-frontier-theodb-03a84625`

> This run is **EXPLORATORY**. Research runs may use non-frozen parameters, so the numbers below cannot back a published claim.

## Results

| Configuration | Throughput/s | Recall | p50 ms | p95 ms | p99 ms | Stable |
|---|---|---|---|---|---|---|
| hnsw m=16 ef_search=40 | 858.8 | 0.832 | 1.094 | 1.354 | 1.541 | yes |
| hnsw m=16 ef_search=64 | 677.6 | 0.8994 | 1.42 | 1.681 | 1.877 | **no** |
| hnsw m=16 ef_search=128 | 413 | 0.9574 | 2.411 | 2.788 | 3.06 | yes |
| hnsw m=16 ef_search=256 | 261.7 | 0.989 | 3.919 | 4.479 | 4.666 | **no** |

Unstable points are reported, not removed. Their repetitions disagree by more than the declared threshold, so the median below is a weaker claim than it looks:

- `hnsw m=16 ef_search=64`: latency_p95_ms cv=0.051; latency_p99_ms cv=0.083
- `hnsw m=16 ef_search=256`: latency_p50_ms cv=0.064; latency_p95_ms cv=0.067; latency_p99_ms cv=0.101; throughput_per_second cv=0.062

### Repetitions

Every repetition is retained:

- `hnsw m=16 ef_search=40` build_seconds: 142, 142, 142
- `hnsw m=16 ef_search=40` index_size_bytes: 7.593e+08, 7.593e+08, 7.593e+08
- `hnsw m=16 ef_search=40` latency_p50_ms: 1.094, 1.14, 1.087
- `hnsw m=16 ef_search=40` latency_p95_ms: 1.348, 1.425, 1.354
- `hnsw m=16 ef_search=40` latency_p99_ms: 1.489, 1.605, 1.541
- `hnsw m=16 ef_search=40` recall: 0.832, 0.832, 0.832
- `hnsw m=16 ef_search=40` throughput_per_second: 865.8, 827.2, 858.8
- `hnsw m=16 ef_search=64` build_seconds: 135.9, 135.9, 135.9
- `hnsw m=16 ef_search=64` index_size_bytes: 7.593e+08, 7.593e+08, 7.593e+08
- `hnsw m=16 ef_search=64` latency_p50_ms: 1.386, 1.494, 1.42
- `hnsw m=16 ef_search=64` latency_p95_ms: 1.611, 1.781, 1.681
- `hnsw m=16 ef_search=64` latency_p99_ms: 1.73, 2.042, 1.877
- `hnsw m=16 ef_search=64` recall: 0.8994, 0.8994, 0.8994
- `hnsw m=16 ef_search=64` throughput_per_second: 696.5, 644, 677.6
- `hnsw m=16 ef_search=128` build_seconds: 139.6, 139.6, 139.6
- `hnsw m=16 ef_search=128` index_size_bytes: 7.593e+08, 7.593e+08, 7.593e+08
- `hnsw m=16 ef_search=128` latency_p50_ms: 2.459, 2.411, 2.348
- `hnsw m=16 ef_search=128` latency_p95_ms: 2.839, 2.788, 2.733
- `hnsw m=16 ef_search=128` latency_p99_ms: 3.071, 2.952, 3.06
- `hnsw m=16 ef_search=128` recall: 0.9574, 0.9574, 0.9574
- `hnsw m=16 ef_search=128` throughput_per_second: 401.3, 413, 419.4
- `hnsw m=16 ef_search=256` build_seconds: 136, 136, 136
- `hnsw m=16 ef_search=256` index_size_bytes: 7.593e+08, 7.593e+08, 7.593e+08
- `hnsw m=16 ef_search=256` latency_p50_ms: 3.629, 3.919, 4.121
- `hnsw m=16 ef_search=256` latency_p95_ms: 4.219, 4.479, 4.818
- `hnsw m=16 ef_search=256` latency_p99_ms: 4.384, 4.666, 5.326
- `hnsw m=16 ef_search=256` recall: 0.989, 0.989, 0.989
- `hnsw m=16 ef_search=256` throughput_per_second: 277.8, 261.7, 245.3

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

- Host: theo-b046-frontier-20260821
- CPU: Intel(R) Xeon(R) Platinum 8358 CPU @ 2.60GHz (16 logical, 16 physical)
- SMT: False · Governor: _unavailable_
- Memory: 67424526336 bytes
- Kernel: 6.8.0-124-generic · Runner: theodb-bench 0.4.0
- Benchmark commit: _none_ (dirty: _none_)

Fields shown in italics were not available on this host and are recorded as absent rather than as zero.

