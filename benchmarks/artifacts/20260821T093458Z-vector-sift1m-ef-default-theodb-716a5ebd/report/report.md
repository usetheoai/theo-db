# vector/sift1m/ef-default on theodb

**Status:** EXPLORATORY · **Profile:** research · **Run:** `20260821T093458Z-vector-sift1m-ef-default-theodb-716a5ebd`

> This run is **EXPLORATORY**. Research runs may use non-frozen parameters, so the numbers below cannot back a published claim.

## Results

| Configuration | Throughput/s | Recall | p50 ms | p95 ms | p99 ms | Stable |
|---|---|---|---|---|---|---|
| hnsw m=16 ef_search=40 | 895.7 | 0.8316 | 1.035 | 1.311 | 1.44 | yes |
| hnsw m=16 ef_search=64 | 654.2 | 0.9018 | 1.464 | 1.797 | 1.968 | **no** |

Unstable points are reported, not removed. Their repetitions disagree by more than the declared threshold, so the median below is a weaker claim than it looks:

- `hnsw m=16 ef_search=64`: latency_p99_ms cv=0.057

### Repetitions

Every repetition is retained:

- `hnsw m=16 ef_search=40` build_seconds: 166.9, 166.9, 166.9
- `hnsw m=16 ef_search=40` index_size_bytes: 7.593e+08, 7.593e+08, 7.593e+08
- `hnsw m=16 ef_search=40` latency_p50_ms: 1.009, 1.035, 1.039
- `hnsw m=16 ef_search=40` latency_p95_ms: 1.311, 1.296, 1.397
- `hnsw m=16 ef_search=40` latency_p99_ms: 1.436, 1.44, 1.549
- `hnsw m=16 ef_search=40` recall: 0.8316, 0.8316, 0.8316
- `hnsw m=16 ef_search=40` throughput_per_second: 917.7, 895.7, 890.1
- `hnsw m=16 ef_search=64` build_seconds: 163.6, 163.6, 163.6
- `hnsw m=16 ef_search=64` index_size_bytes: 7.593e+08, 7.593e+08, 7.593e+08
- `hnsw m=16 ef_search=64` latency_p50_ms: 1.464, 1.453, 1.467
- `hnsw m=16 ef_search=64` latency_p95_ms: 1.768, 1.797, 1.835
- `hnsw m=16 ef_search=64` latency_p99_ms: 1.909, 1.968, 2.129
- `hnsw m=16 ef_search=64` recall: 0.9018, 0.9018, 0.9018
- `hnsw m=16 ef_search=64` throughput_per_second: 660.1, 654.2, 648.8

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

- Host: theo-b018-ef-default-20260821
- CPU: Intel(R) Xeon(R) Platinum 8168 CPU @ 2.70GHz (16 logical, 16 physical)
- SMT: False · Governor: _unavailable_
- Memory: 67424518144 bytes
- Kernel: 6.8.0-124-generic · Runner: theodb-bench 0.4.0
- Benchmark commit: _none_ (dirty: _none_)

Fields shown in italics were not available on this host and are recorded as absent rather than as zero.

