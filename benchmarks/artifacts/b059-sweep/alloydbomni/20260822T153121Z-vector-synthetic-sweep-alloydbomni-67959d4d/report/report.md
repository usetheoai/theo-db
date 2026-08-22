# vector/synthetic/sweep on alloydbomni

**Status:** INVALID · **Profile:** research · **Run:** `20260822T153121Z-vector-synthetic-sweep-alloydbomni-67959d4d`

> This run is **INVALID**. It failed protocol validation, which says nothing about whether the numbers were favourable -- invalidation is never based on the measured outcome.

## Results

No configuration produced a measurement.

## Validation

| Check | Outcome | Required | Detail |
|---|---|---|---|
| sut_alive | PASS | yes |  |
| run_not_refused | FAIL | yes | the harness refused to measure: a precondition it checks was not met, so no number was taken. This is the harness working, not a fault of the system under test |
| within_time_budget | PASS | yes |  |
| client_alive | PASS | yes |  |
| operation_count | UNAVAILABLE | no | benchmark declared no expected operation count |
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
| quality_reported | FAIL | yes | approximate retrieval reported without a quality axis |
| clean_source_tree | PASS | no |  |

Invalidated by: `run_not_refused`, `quality_reported`

Invalidation is based on protocol criteria, never on whether the numbers looked good.

## Environment

- Host: theo-bench-20260822T152358Z
- CPU: Intel(R) Xeon(R) Platinum 8280 CPU @ 2.70GHz (16 logical, 16 physical)
- SMT: False · Governor: _unavailable_
- Memory: 67424522240 bytes
- Kernel: 6.8.0-124-generic · Runner: theodb-bench 0.6.0
- Benchmark commit: 623faed9e52a910c4b6e82e5ccc3d089630bb858 (dirty: False)

Fields shown in italics were not available on this host and are recorded as absent rather than as zero.

