# Review — m100-datafusion-customscan-executor

**Date:** 2026-07-16
**Verdict:** READY_TO_MERGE
**Milestone:** M100
**Plan:** knowledge-base/plans/m100-datafusion-customscan-executor-plan.md

## Scope reviewed

The DataFusion vectorized `CustomScan` executor over the M99 `theodb_columnar` TAM: the async-in-C `block_on`
executor over real columnar Arrow batches (A), projection pushdown (B), the `create_upper_paths_hook` planner
integration replacing a simple aggregate with a single-plan `CustomScan` (C), and the safety hardening (`work_mem`
MemoryPool + `target_partitions=1` Send-pinning + `HeldInterrupts`) + the measured OLAP benchmark (D). Files:
`theodb_rs/src/am/{df_executor.rs,columnar_agg.rs,columnar.rs}`, `theodb_rs/isolation/bench_m100.sh`,
`docs/benchmarks/m100-datafusion-executor.{md,json}`.

## Measured evidence (droplet pg17 / pgrx 0.19)

- **300 pg_tests GREEN**, zero regression (M99 + M98 + the 3 new M100 tests: agg-matches-heap, projection,
  CustomScan-matches-heap).
- `EXPLAIN` over a columnar `count(*), sum(measure)` shows the `Custom Scan` node; the aggregate is result-identical
  to a heap table.
- **Benchmark:** the vectorized CustomScan (531 ms) is **9.89× faster than the M99 row-at-a-time seqscan** (5251 ms)
  on the same columnar data (2M rows, 5 runs); result-equivalent to heap; honest ceiling (heap 147 ms is faster for
  this single narrow aggregate — no superiority claim vs heap or AlloyDB).

## Specialist sign-off

| Reviewer | Domain | Verdict | Blockers |
|---|---|---|---|
| council-rust-pgrx | FFI / planner hooks / panic-across-C / async seam | READY_TO_MERGE | none |
| council-benchmark | measurement rigor / honesty | READY | none |

**council-rust-pgrx:** the FFI surface is sound — the `create_upper_paths_hook` calls the previous hook first and
never errors (only returns → fail-safe); the admission guard is comprehensive (GROUP BY / HAVING / WHERE / DISTINCT
/ window / non-Aggref / non-float8-sum / other-rel Var); `scanrelid=0` + `custom_scan_tlist=copy(tlist)` is the
sanctioned idiom (setrefs rewrites the targetlist into INDEX_VAR Vars, tupdesc from the agg output types, no Aggref
evaluation); `relation_close` is unconditional before the error path; the async seam holds interrupts across the
small aggregate `collect()` (acceptable — the per-batch safe-point is for row scans), the MemoryPool errors not
panics, `target_partitions=1` pins Send. All Datums match the custom_scan_tlist types; the numeric/int-sum mismatch
cases are excluded from admission.

**council-benchmark:** the benchmark passes "did you measure or assume?" — reproducible methodology, correct
population stddev, apples-to-apples (VEC vs SEQ toggle the same `t_col` via the GUC), the 9.89× arithmetic checks
out, and the anti-spin point (heap 147 ms is faster than the vectorized 531 ms) is stated prominently, not masked.

## Corrections applied (from the review)

- **Admission guard rejects `aggsplit != AGGSPLIT_SIMPLE`** (council-rust-pgrx HIGH — prevents a partial/parallel
  aggregate's transtype from being emitted as an int8/float8 Datum). Re-tested GREEN.
- 3 benchmark-doc honesty qualifiers (9.89× reflects a 5-column table + scales with width; EXPLAIN evidence is a
  `Custom Scan` grep; the heap `VACUUM ANALYZE` asymmetry is intentional and does not affect the measured pair).

## Follow-up issues filed (accepted post-merge)

- #102 — `build_arrow` `try_into().unwrap()`/fixed-index decode should be a typed error on a truncated stored value
  (council-rust-pgrx MEDIUM; unreachable given the codec's attlen invariant, but a discipline gap).

## DoD coverage (ROADMAP M100)

| DoD | Status |
|---|---|
| (1) CustomScan DataFusion over the M99 TAM, single plan (EXPLAIN shows the node) | ✅ |
| (2) result-equivalence vs row-store | ✅ |
| (3) interrupt/MemoryPool/Send discipline implemented + tested | ✅ (HeldInterrupts + GreedyMemoryPool(work_mem) + target_partitions=1) |
| (4) measured OLAP benchmark, honest DuckDB/Photon ceiling | ✅ (9.89× vs M99 seqscan; heap-faster stated; pg_duckdb absence disclosed) |
| (5) sign-off council-rust-pgrx + council-benchmark | ✅ |
| honest boundary (gain on columnar-resident data; not AlloyDB-in-core superiority) | ✅ |
| projection pushdown + min/max stored-consumption | ✅ projection (B); min/max consumption is a later slice (WHERE pushdown) |

## Honest scope note

Slice-1 admits `count(*)` / `sum(float8)` without GROUP BY / WHERE (the type-matching cases). GROUP BY (vectorized
hash aggregate), WHERE + min/max skip-pruning pushdown, `avg`, and `sum(int/numeric)` are follow-up slices that widen
the measured gain — recorded honestly, not shipped as done.
