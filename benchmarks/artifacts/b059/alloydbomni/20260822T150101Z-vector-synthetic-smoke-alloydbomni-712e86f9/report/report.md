# vector/synthetic/smoke on alloydbomni

**Status:** EXPLORATORY · **Profile:** research · **Run:** `20260822T150101Z-vector-synthetic-smoke-alloydbomni-712e86f9`

> This run is **EXPLORATORY**. Research runs may use non-frozen parameters, so the numbers below cannot back a published claim.

## Results

| Configuration | Throughput/s | Recall | p50 ms | p95 ms | p99 ms | Stable |
|---|---|---|---|---|---|---|
| none | 1,344 | 1 | 0.6702 | 0.7318 | 0.8468 | **no** |

Unstable points are reported, not removed. Their repetitions disagree by more than the declared threshold, so the median below is a weaker claim than it looks:

- `none`: no spread available for build_seconds, latency_p50_ms, latency_p95_ms, latency_p99_ms, recall, throughput_per_second

### Repetitions

Every repetition is retained:

- `none` build_seconds: 0
- `none` latency_p50_ms: 0.6702
- `none` latency_p95_ms: 0.7318
- `none` latency_p99_ms: 0.8468
- `none` recall: 1
- `none` throughput_per_second: 1,344

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

- Host: theo-bench-20260822T145236Z
- CPU: Intel(R) Xeon(R) Platinum 8280 CPU @ 2.70GHz (16 logical, 16 physical)
- SMT: False · Governor: _unavailable_
- Memory: 67424522240 bytes
- Kernel: 6.8.0-124-generic · Runner: theodb-bench 0.6.0
- Benchmark commit: 623faed9e52a910c4b6e82e5ccc3d089630bb858 (dirty: False)

Fields shown in italics were not available on this host and are recorded as absent rather than as zero.

