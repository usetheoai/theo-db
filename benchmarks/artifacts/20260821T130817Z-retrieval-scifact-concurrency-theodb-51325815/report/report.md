# retrieval/scifact/concurrency on theodb

**Status:** EXPLORATORY · **Profile:** research · **Run:** `20260821T130817Z-retrieval-scifact-concurrency-theodb-51325815`

> This run is **EXPLORATORY**. Research runs may use non-frozen parameters, so the numbers below cannot back a published claim.

## Results

| Configuration | Throughput/s | Recall | p50 ms | p95 ms | p99 ms | Stable |
|---|---|---|---|---|---|---|
| pipeline=lexical @ 1 clientes | 338.5 | _not measured_ | 2.763 | 3.328 | 3.588 | **no** |
| pipeline=lexical @ 5 clientes | 1,151 | _not measured_ | 3.025 | 5.06 | 38.23 | **no** |
| pipeline=lexical @ 10 clientes | 1,618 | _not measured_ | 4.157 | 10.9 | 42.15 | **no** |
| pipeline=lexical @ 20 clientes | 965.7 | _not measured_ | 12.74 | 46.99 | 71.67 | **no** |
| pipeline=lexical @ 40 clientes | 792.5 | _not measured_ | 22.1 | 71.22 | 93.51 | **no** |
| pipeline=lexical @ 80 clientes | 623.4 | _not measured_ | 29.44 | 89.41 | 126.1 | **no** |

Unstable points are reported, not removed. Their repetitions disagree by more than the declared threshold, so the median below is a weaker claim than it looks:

- `pipeline=lexical @ 1 clientes`: latency_p99_ms cv=0.081
- `pipeline=lexical @ 5 clientes`: latency_p50_ms cv=0.084; latency_p95_ms cv=0.066; latency_p99_ms cv=0.202; stage_response_p50_ms_seconds cv=0.084; stage_service_p50_ms_seconds cv=0.084; throughput_per_second cv=0.077
- `pipeline=lexical @ 10 clientes`: latency_p50_ms cv=0.105; latency_p95_ms cv=0.289; latency_p99_ms cv=0.116; stage_response_p50_ms_seconds cv=0.105; stage_service_p50_ms_seconds cv=0.105; throughput_per_second cv=0.139
- `pipeline=lexical @ 20 clientes`: latency_p95_ms cv=0.055
- `pipeline=lexical @ 40 clientes`: latency_p95_ms cv=0.052; latency_p99_ms cv=0.122
- `pipeline=lexical @ 80 clientes`: latency_p50_ms cv=0.060; latency_p95_ms cv=0.134; latency_p99_ms cv=0.090; stage_response_p50_ms_seconds cv=0.060; stage_service_p50_ms_seconds cv=0.060

### Repetitions

Every repetition is retained:

- `pipeline=lexical @ 1 clientes` latency_p50_ms: 2.74, 2.763, 2.775
- `pipeline=lexical @ 1 clientes` latency_p95_ms: 3.478, 3.283, 3.328
- `pipeline=lexical @ 1 clientes` latency_p99_ms: 4.021, 3.446, 3.588
- `pipeline=lexical @ 1 clientes` mrr: 0.6493, 0.6493, 0.6493
- `pipeline=lexical @ 1 clientes` ndcg_at_10: 0.6864, 0.6864, 0.6864
- `pipeline=lexical @ 1 clientes` recall_at_k: 0.8227, 0.8227, 0.8227
- `pipeline=lexical @ 1 clientes` throughput_per_second: 338.3, 341.7, 338.5
- `pipeline=lexical @ 5 clientes` latency_p50_ms: 3.46, 2.982, 3.025
- `pipeline=lexical @ 5 clientes` latency_p95_ms: 5.077, 5.06, 4.507
- `pipeline=lexical @ 5 clientes` latency_p99_ms: 38.23, 40.23, 27.04
- `pipeline=lexical @ 5 clientes` stage_response_p50_ms_seconds: 3.462, 2.988, 3.025
- `pipeline=lexical @ 5 clientes` stage_service_p50_ms_seconds: 3.462, 2.988, 3.025
- `pipeline=lexical @ 5 clientes` throughput_per_second: 1,099, 1,151, 1,275
- `pipeline=lexical @ 10 clientes` latency_p50_ms: 4.157, 4.701, 3.821
- `pipeline=lexical @ 10 clientes` latency_p95_ms: 10.9, 14.63, 8.163
- `pipeline=lexical @ 10 clientes` latency_p99_ms: 38.93, 48.74, 42.15
- `pipeline=lexical @ 10 clientes` stage_response_p50_ms_seconds: 4.157, 4.703, 3.828
- `pipeline=lexical @ 10 clientes` stage_service_p50_ms_seconds: 4.157, 4.703, 3.828
- `pipeline=lexical @ 10 clientes` throughput_per_second: 1,653, 1,272, 1,618
- `pipeline=lexical @ 20 clientes` latency_p50_ms: 12.74, 13.01, 12.15
- `pipeline=lexical @ 20 clientes` latency_p95_ms: 46.99, 42.8, 47.25
- `pipeline=lexical @ 20 clientes` latency_p99_ms: 71.67, 75.98, 68.91
- `pipeline=lexical @ 20 clientes` stage_response_p50_ms_seconds: 12.75, 13.02, 12.17
- `pipeline=lexical @ 20 clientes` stage_service_p50_ms_seconds: 12.75, 13.02, 12.17
- `pipeline=lexical @ 20 clientes` throughput_per_second: 896.4, 972.8, 965.7
- `pipeline=lexical @ 40 clientes` latency_p50_ms: 22.1, 21.76, 22.93
- `pipeline=lexical @ 40 clientes` latency_p95_ms: 71.22, 69.65, 76.86
- `pipeline=lexical @ 40 clientes` latency_p99_ms: 93.51, 89.26, 111.8
- `pipeline=lexical @ 40 clientes` stage_response_p50_ms_seconds: 22.19, 21.87, 22.94
- `pipeline=lexical @ 40 clientes` stage_service_p50_ms_seconds: 22.19, 21.87, 22.94
- `pipeline=lexical @ 40 clientes` throughput_per_second: 803.6, 735.3, 792.5
- `pipeline=lexical @ 80 clientes` latency_p50_ms: 28.22, 29.44, 31.76
- `pipeline=lexical @ 80 clientes` latency_p95_ms: 77.26, 89.41, 101.2
- `pipeline=lexical @ 80 clientes` latency_p99_ms: 109.9, 131.1, 126.1
- `pipeline=lexical @ 80 clientes` stage_response_p50_ms_seconds: 28.28, 29.47, 31.8
- `pipeline=lexical @ 80 clientes` stage_service_p50_ms_seconds: 28.28, 29.47, 31.8
- `pipeline=lexical @ 80 clientes` throughput_per_second: 640.3, 620.9, 623.4

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

- Host: theo-b043-concorrencia-20260821
- CPU: Intel(R) Xeon(R) Platinum 8168 CPU @ 2.70GHz (16 logical, 16 physical)
- SMT: False · Governor: _unavailable_
- Memory: 67424518144 bytes
- Kernel: 6.8.0-124-generic · Runner: theodb-bench 0.5.0
- Benchmark commit: _none_ (dirty: _none_)

Fields shown in italics were not available on this host and are recorded as absent rather than as zero.

