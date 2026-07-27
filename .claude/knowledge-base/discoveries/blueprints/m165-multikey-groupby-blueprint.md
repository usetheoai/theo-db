# Discovery blueprint — M165 "GROUP BY multi-chave" (q17, q34)

**Date:** 2026-07-27 · **Cycle:** discover (for M165) · **Verdict:** the milestone premise is FALSE — multi-key GROUP BY
already works; q17/q34 decline for two *different*, non-multi-key reasons. Scope corrected below.

## Key finding — multi-key GROUP BY is already supported end-to-end

The columnar_agg admit/exec path already handles N group columns positionally (no `group_cols.len()` limit):
`admit` walk (`columnar_agg.rs:1063-1087`), `try_swap_agg` N-key match (`:1483`), executor
`run_columnar_grouped_aggs` (`df_executor.rs:648-708`). **Direct proof:** q16 (`SELECT UserID, SearchPhrase, COUNT(*)
… GROUP BY UserID, SearchPhrase ORDER BY c DESC LIMIT`) — the *identical* int+text 2-key grouping — already routes
byte-identical (`docs/benchmarks/m153-groupby-text.md:19`). q17 is q16 minus the `ORDER BY`.

## The two real decline reasons (different root causes)

### q17 — `GROUP BY UserID, SearchPhrase LIMIT 10` (no ORDER BY)
- Declines in `try_swap_agg`, `columnar_agg.rs:1502-1504`, trace `swap_sorted_text_group_not_resorted` (M153).
- PG plans an **AGG_SORTED** GroupAgg over the high-card text key; the M153 guard requires the Agg's parent to be a
  full `Sort` so the whole output is re-ordered in **collation** order. q17's parent is a `Limit`, not a `Sort`.
- Our executor emits text groups in **byte order** (`df_executor.rs:723` skips text keys in its sort), which ≠ PG's
  collation order. The guard declines to avoid shipping byte-order as collation-order.
- **Insight to test empirically (M152 — instrument, don't guess):** q17 has **no `ORDER BY`**, so its `LIMIT 10`
  returns an *arbitrary* 10 groups (SQL-legal non-determinism). The A/B oracle **strips the trailing LIMIT** and
  compares the full multiset (`run_m128_clickbench.py:_bench_query`), so byte-order-vs-collation-order does NOT affect
  the A/B. **Hypothesis:** the M153 guard is over-conservative for the *no-ORDER-BY* case; relaxing it there (parent is
  a bare `Limit` with no `Sort` anywhere / no ORDER BY in the query) may route q17 with a byte-identical A/B and a
  SQL-legal live result — WITHOUT the heavy collation-aware executor sort. Must be confirmed with THEODB_ADMIT_TRACE +
  a live A/B run before committing to a change.
- **Fallback (if order does matter):** implement collation-aware sort of the grouped rows via `pg_sys::varstr_cmp`
  with the key's `varcollid` (substantive, council-rust-pgrx FFI review needed), then relax the guard for deterministic
  collations only.

### q34 — `SELECT 1, URL, COUNT(*) … GROUP BY 1, URL ORDER BY c DESC LIMIT 10`
- Declines at admit, `classify_target_node` catch-all `else` (`columnar_agg.rs:947-949`), trace
  `target_grouping_expression_or_other`.
- The `SELECT 1` puts a bare **`T_Const`** node in the output target; `classify_target_node` has arms only for
  `T_Var`/`T_FuncExpr`/`T_OpExpr`/`T_Aggref`. Not multi-key: PG eliminates the constant key so `GROUP BY 1, URL`
  reduces to single-key `GROUP BY URL`. The sole blocker is the constant projection column.
- **Minimal fix:** add a `T_Const` arm to `classify_target_node` → a new `TargetSlot::ConstOut(datum, typoid)` + a new
  `layout` kind; the exec materialization emits the fixed Datum for that slot in every row. **Fail-closed:** admit only
  round-trippable integer Const types initially (the literal `1` is int4); decline float/text/numeric Consts to
  preserve byte-identity. Const slot counts toward `layout`/arity but NOT `group_cols`.

## SOTA (R0 web research)

- DataFusion (v28+, we use 54) uses two-phase parallel hash-partitioned grouping with composite keys for multi-column
  GROUP BY — same family as DuckDB's parallel grouped aggregates
  ([DataFusion blog](https://datafusion.apache.org/blog/2023/08/05/datafusion_fast_grouping/)).
- **Correctness gotcha:** DataFusion's `GroupByHash` historically mis-grouped NULLs
  ([apache/datafusion#790](https://github.com/apache/datafusion/issues/790)) — "produces incorrect answers when
  grouping on columns that contain NULL". Fixed in later versions; we're on 54. **Invariant to A/B-test:** a multi-key
  GROUP BY where one key is NULL must group identically to PG (NULLs together). The M163 type-coverage harness already
  carries NULL edges — extend it with a multi-key NULL case.

## Corrected M165 scope

The observable DoD (q17 + q34 route byte-identical via the M164-hardened harness) is unchanged, but the *work* is:
1. **q34:** add a fail-closed integer `T_Const` output-column arm (admit-side, small).
2. **q17:** resolve the text-group emission-order guard — EMPIRICALLY test the no-ORDER-BY relax first (simple); fall
   back to collation-aware sort only if the A/B proves order matters.

Both are planner/executor-side (CustomScan + `create_upper_paths_hook`) — no page-format/WAL/crash-safety surface. The
only correctness surface is byte-identical A/B, governed by the existing M153/M157/M163 per-key guards + the
type-coverage A/B (`benchmarks/columnar_type_ab.py`, `testing.md §5.1`).

## Invariants (Phase 2)

- Byte-identical A/B (columnar == heap) for q17, q34 (LIMIT-stripped multiset) — the correctness oracle.
- Per-key guards preserved: non-deterministic collation declines (M153), float declines (M163), temporal per M157.
- q34 const: only integer const types admitted (fail-closed); byte-identical to PG's output.
- NULL grouping identical to PG (DataFusion #790 — A/B with a NULL multi-key case).
- No page-format / WAL / VACUUM / upgrade surface (planner/executor only).

## References (resolve on disk)

- `theodb_rs/src/am/columnar_agg.rs` (`classify_target_node:661`, catch-all `:947`, `try_swap_agg:1492`, q17 guard `:1502`)
- `theodb_rs/src/am/df_executor.rs` (`run_columnar_grouped_aggs:610`, text-skip sort `:723`, `arrow_supported_group_type:258`)
- `docs/benchmarks/m153-groupby-text.md:19-27` (q16 routes; q17 honest-negative)
- `docs/benchmarks/m161-expr-routing-verdict.md:40-44` (q34 const honest-negative)
- `docs/benchmarks/clickbench-fresh-vs-clickhouse-2026-07-27.md` (q17=115×, q34=152× measured)
