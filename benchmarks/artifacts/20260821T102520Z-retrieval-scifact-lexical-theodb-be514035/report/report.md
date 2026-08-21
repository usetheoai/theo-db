# retrieval/scifact/lexical on theodb

**Status:** EXPLORATORY · **Profile:** research · **Run:** `20260821T102520Z-retrieval-scifact-lexical-theodb-be514035`

> This run is **EXPLORATORY**. Research runs may use non-frozen parameters, so the numbers below cannot back a published claim.

## Results

| Configuration | Throughput/s | Recall | p50 ms | p95 ms | p99 ms | Stable |
|---|---|---|---|---|---|---|
| pipeline=lexical | 214.8 | _not measured_ | 4.316 | 6.231 | 7.269 | **no** |

Unstable points are reported, not removed. Their repetitions disagree by more than the declared threshold, so the median below is a weaker claim than it looks:

- `pipeline=lexical`: latency_p99_ms cv=0.188

### Repetitions

Every repetition is retained:

- `pipeline=lexical` latency_p50_ms: 4.332, 4.316, 4.19
- `pipeline=lexical` latency_p95_ms: 6.407, 6.007, 6.231
- `pipeline=lexical` latency_p99_ms: 9.191, 6.393, 7.269
- `pipeline=lexical` mrr: 0.6493, 0.6493, 0.6493
- `pipeline=lexical` ndcg_at_10: 0.6864, 0.6864, 0.6864
- `pipeline=lexical` recall_at_k: 0.8227, 0.8227, 0.8227
- `pipeline=lexical` throughput_per_second: 208.6, 214.8, 217.2

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
| clean_source_tree | FAIL | no | uncommitted changes in the benchmark tree |

## Environment

- Host: paulohenriquevn
- CPU: 13th Gen Intel(R) Core(TM) i7-1355U (12 logical, 10 physical)
- SMT: True · Governor: powersave
- Memory: 16439533568 bytes
- Kernel: 6.8.0-136-generic · Runner: theodb-bench 0.4.0
- Benchmark commit: 86ff52dcdaeb34e172944f1c562548f890e37294 (dirty: True)

Fields shown in italics were not available on this host and are recorded as absent rather than as zero.

