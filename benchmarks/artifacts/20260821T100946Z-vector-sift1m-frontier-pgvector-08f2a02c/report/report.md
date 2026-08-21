# vector/sift1m/frontier on pgvector

**Status:** EXPLORATORY · **Profile:** research · **Run:** `20260821T100946Z-vector-sift1m-frontier-pgvector-08f2a02c`

> This run is **EXPLORATORY**. Research runs may use non-frozen parameters, so the numbers below cannot back a published claim.

## Results

| Configuration | Throughput/s | Recall | p50 ms | p95 ms | p99 ms | Stable |
|---|---|---|---|---|---|---|
| hnsw m=16 ef_search=40 | 626.7 | 0.9188 | 1.543 | 1.815 | 1.978 | yes |
| hnsw m=16 ef_search=64 | 491.2 | 0.958 | 2.021 | 2.344 | 2.475 | yes |
| hnsw m=16 ef_search=128 | 273.9 | 0.9884 | 3.712 | 4.274 | 4.513 | yes |
| hnsw m=16 ef_search=256 | 167.6 | 0.9968 | 6.098 | 7.224 | 7.48 | yes |

### Repetitions

Every repetition is retained:

- `hnsw m=16 ef_search=40` build_seconds: 78.01, 78.01, 78.01
- `hnsw m=16 ef_search=40` index_size_bytes: 8.205e+08, 8.205e+08, 8.205e+08
- `hnsw m=16 ef_search=40` latency_p50_ms: 1.574, 1.543, 1.509
- `hnsw m=16 ef_search=40` latency_p95_ms: 1.884, 1.815, 1.781
- `hnsw m=16 ef_search=40` latency_p99_ms: 2.101, 1.978, 1.928
- `hnsw m=16 ef_search=40` recall: 0.9188, 0.9188, 0.9188
- `hnsw m=16 ef_search=40` throughput_per_second: 616.4, 626.7, 641.7
- `hnsw m=16 ef_search=64` build_seconds: 72.73, 72.73, 72.73
- `hnsw m=16 ef_search=64` index_size_bytes: 8.201e+08, 8.201e+08, 8.201e+08
- `hnsw m=16 ef_search=64` latency_p50_ms: 2.021, 2.005, 2.045
- `hnsw m=16 ef_search=64` latency_p95_ms: 2.347, 2.317, 2.344
- `hnsw m=16 ef_search=64` latency_p99_ms: 2.475, 2.468, 2.529
- `hnsw m=16 ef_search=64` recall: 0.958, 0.958, 0.958
- `hnsw m=16 ef_search=64` throughput_per_second: 491.2, 495.7, 488.3
- `hnsw m=16 ef_search=128` build_seconds: 71.58, 71.58, 71.58
- `hnsw m=16 ef_search=128` index_size_bytes: 8.201e+08, 8.201e+08, 8.201e+08
- `hnsw m=16 ef_search=128` latency_p50_ms: 3.792, 3.71, 3.712
- `hnsw m=16 ef_search=128` latency_p95_ms: 4.433, 4.217, 4.274
- `hnsw m=16 ef_search=128` latency_p99_ms: 4.638, 4.452, 4.513
- `hnsw m=16 ef_search=128` recall: 0.9884, 0.9884, 0.9884
- `hnsw m=16 ef_search=128` throughput_per_second: 263, 274.3, 273.9
- `hnsw m=16 ef_search=256` build_seconds: 70.45, 70.45, 70.45
- `hnsw m=16 ef_search=256` index_size_bytes: 8.201e+08, 8.201e+08, 8.201e+08
- `hnsw m=16 ef_search=256` latency_p50_ms: 5.947, 6.098, 6.19
- `hnsw m=16 ef_search=256` latency_p95_ms: 7.062, 7.224, 7.284
- `hnsw m=16 ef_search=256` latency_p99_ms: 7.374, 7.48, 7.672
- `hnsw m=16 ef_search=256` recall: 0.9968, 0.9968, 0.9968
- `hnsw m=16 ef_search=256` throughput_per_second: 173, 167.6, 165.9

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

