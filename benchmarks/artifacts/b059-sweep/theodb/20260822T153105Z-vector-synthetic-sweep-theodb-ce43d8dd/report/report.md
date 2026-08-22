# vector/synthetic/sweep on theodb

**Status:** EXPLORATORY · **Profile:** research · **Run:** `20260822T153105Z-vector-synthetic-sweep-theodb-ce43d8dd`

> This run is **EXPLORATORY**. Research runs may use non-frozen parameters, so the numbers below cannot back a published claim.

## Results

| Configuration | Throughput/s | Recall | p50 ms | p95 ms | p99 ms | Stable |
|---|---|---|---|---|---|---|
| none | 169.3 | 1 | 5.787 | 6.019 | 6.167 | **no** |
| hnsw m=16 ef_search=16 | 1,963 | 0.5892 | 0.424 | 0.5135 | 0.5652 | **no** |
| hnsw m=16 ef_search=64 | 1,491 | 0.7794 | 0.5523 | 0.7478 | 0.7762 | **no** |
| hnsw m=16 ef_search=256 | 710.6 | 0.9662 | 1.325 | 1.488 | 1.547 | yes |

Unstable points are reported, not removed. Their repetitions disagree by more than the declared threshold, so the median below is a weaker claim than it looks:

- `none`: latency_p99_ms cv=0.259
- `hnsw m=16 ef_search=16`: latency_p50_ms cv=0.090; latency_p95_ms cv=0.080; latency_p99_ms cv=0.097; throughput_per_second cv=0.124
- `hnsw m=16 ef_search=64`: latency_p50_ms cv=0.113; latency_p95_ms cv=0.158; latency_p99_ms cv=0.181; throughput_per_second cv=0.085

### Repetitions

Every repetition is retained:

- `none` build_seconds: 0, 0, 0
- `none` latency_p50_ms: 5.746, 5.787, 5.8
- `none` latency_p95_ms: 5.964, 6.019, 6.095
- `none` latency_p99_ms: 6.107, 6.167, 9.373
- `none` recall: 1, 1, 1
- `none` throughput_per_second: 170.8, 169.3, 167
- `hnsw m=16 ef_search=16` build_seconds: 0.7424, 0.7424, 0.7424
- `hnsw m=16 ef_search=16` index_size_bytes: 5.079e+06, 5.079e+06, 5.079e+06
- `hnsw m=16 ef_search=16` latency_p50_ms: 0.4089, 0.424, 0.4834
- `hnsw m=16 ef_search=16` latency_p95_ms: 0.4956, 0.5135, 0.5762
- `hnsw m=16 ef_search=16` latency_p99_ms: 0.5267, 0.5652, 0.6364
- `hnsw m=16 ef_search=16` recall: 0.5892, 0.5892, 0.5892
- `hnsw m=16 ef_search=16` throughput_per_second: 2,051, 1,963, 1,611
- `hnsw m=16 ef_search=64` build_seconds: 0.7317, 0.7317, 0.7317
- `hnsw m=16 ef_search=64` index_size_bytes: 5.079e+06, 5.079e+06, 5.079e+06
- `hnsw m=16 ef_search=64` latency_p50_ms: 0.5523, 0.5333, 0.6555
- `hnsw m=16 ef_search=64` latency_p95_ms: 0.8701, 0.6333, 0.7478
- `hnsw m=16 ef_search=64` latency_p99_ms: 0.9605, 0.6728, 0.7762
- `hnsw m=16 ef_search=64` recall: 0.7794, 0.7794, 0.7794
- `hnsw m=16 ef_search=64` throughput_per_second: 1,491, 1,603, 1,351
- `hnsw m=16 ef_search=256` build_seconds: 0.6622, 0.6622, 0.6622
- `hnsw m=16 ef_search=256` index_size_bytes: 5.079e+06, 5.079e+06, 5.079e+06
- `hnsw m=16 ef_search=256` latency_p50_ms: 1.325, 1.323, 1.348
- `hnsw m=16 ef_search=256` latency_p95_ms: 1.489, 1.488, 1.475
- `hnsw m=16 ef_search=256` latency_p99_ms: 1.553, 1.547, 1.546
- `hnsw m=16 ef_search=256` recall: 0.9662, 0.9662, 0.9662
- `hnsw m=16 ef_search=256` throughput_per_second: 710.6, 711.5, 706.9

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
| clean_source_tree | PASS | no |  |

## Environment

- Host: theo-bench-20260822T152358Z
- CPU: Intel(R) Xeon(R) Platinum 8280 CPU @ 2.70GHz (16 logical, 16 physical)
- SMT: False · Governor: _unavailable_
- Memory: 67424522240 bytes
- Kernel: 6.8.0-124-generic · Runner: theodb-bench 0.6.0
- Benchmark commit: 623faed9e52a910c4b6e82e5ccc3d089630bb858 (dirty: False)

Fields shown in italics were not available on this host and are recorded as absent rather than as zero.

