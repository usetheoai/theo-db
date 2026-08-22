# vector/synthetic/sweep on pgvector

**Status:** EXPLORATORY · **Profile:** research · **Run:** `20260822T153129Z-vector-synthetic-sweep-pgvector-c7b63dbc`

> This run is **EXPLORATORY**. Research runs may use non-frozen parameters, so the numbers below cannot back a published claim.

## Results

| Configuration | Throughput/s | Recall | p50 ms | p95 ms | p99 ms | Stable |
|---|---|---|---|---|---|---|
| none | 385.7 | 1 | 2.5 | 2.642 | 2.689 | yes |
| hnsw m=16 ef_search=16 | 2,029 | 0.6214 | 0.4169 | 0.4966 | 0.5363 | **no** |
| hnsw m=16 ef_search=64 | 1,307 | 0.9012 | 0.6801 | 0.7751 | 0.8084 | yes |
| hnsw m=16 ef_search=256 | 544.2 | 0.9898 | 1.732 | 1.947 | 2.053 | **no** |

Unstable points are reported, not removed. Their repetitions disagree by more than the declared threshold, so the median below is a weaker claim than it looks:

- `hnsw m=16 ef_search=16`: latency_p50_ms cv=0.069; latency_p95_ms cv=0.064; latency_p99_ms cv=0.077; throughput_per_second cv=0.064
- `hnsw m=16 ef_search=256`: latency_p95_ms cv=0.110; latency_p99_ms cv=0.177

### Repetitions

Every repetition is retained:

- `none` build_seconds: 0, 0, 0
- `none` latency_p50_ms: 2.478, 2.556, 2.5
- `none` latency_p95_ms: 2.632, 2.685, 2.642
- `none` latency_p99_ms: 2.687, 2.721, 2.689
- `none` recall: 1, 1, 1
- `none` throughput_per_second: 389.3, 377.4, 385.7
- `hnsw m=16 ef_search=16` build_seconds: 2.216, 2.216, 2.216
- `hnsw m=16 ef_search=16` index_size_bytes: 5.718e+06, 5.718e+06, 5.718e+06
- `hnsw m=16 ef_search=16` latency_p50_ms: 0.4169, 0.4355, 0.3795
- `hnsw m=16 ef_search=16` latency_p95_ms: 0.4966, 0.5149, 0.4543
- `hnsw m=16 ef_search=16` latency_p99_ms: 0.5363, 0.5595, 0.4807
- `hnsw m=16 ef_search=16` recall: 0.6214, 0.6214, 0.6214
- `hnsw m=16 ef_search=16` throughput_per_second: 2,029, 1,903, 2,161
- `hnsw m=16 ef_search=64` build_seconds: 2.249, 2.249, 2.249
- `hnsw m=16 ef_search=64` index_size_bytes: 5.718e+06, 5.718e+06, 5.718e+06
- `hnsw m=16 ef_search=64` latency_p50_ms: 0.6949, 0.6801, 0.679
- `hnsw m=16 ef_search=64` latency_p95_ms: 0.8039, 0.7751, 0.7728
- `hnsw m=16 ef_search=64` latency_p99_ms: 0.8668, 0.8084, 0.8018
- `hnsw m=16 ef_search=64` recall: 0.9012, 0.9012, 0.9012
- `hnsw m=16 ef_search=64` throughput_per_second: 1,266, 1,307, 1,309
- `hnsw m=16 ef_search=256` build_seconds: 2.196, 2.196, 2.196
- `hnsw m=16 ef_search=256` index_size_bytes: 5.718e+06, 5.718e+06, 5.718e+06
- `hnsw m=16 ef_search=256` latency_p50_ms: 1.732, 1.727, 1.735
- `hnsw m=16 ef_search=256` latency_p95_ms: 1.947, 1.943, 2.342
- `hnsw m=16 ef_search=256` latency_p99_ms: 2.017, 2.053, 2.731
- `hnsw m=16 ef_search=256` recall: 0.9898, 0.9898, 0.9898
- `hnsw m=16 ef_search=256` throughput_per_second: 546.1, 544.2, 533.4

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

