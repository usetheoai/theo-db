# M114 Blueprint — columnar aggregate completeness (byte-identical to PostgreSQL)

Deep-research (Staff DB engineer) blueprint for M114. Primary-source evidence gathered live from PostgreSQL official
docs + DataFusion 54 / Arrow 58 source. Versions pinned to `theodb_rs/Cargo.lock` (datafusion 54.0.0, arrow 58.3.0).

## Coverage Corner 1 — Integration Tests
Every admission change needs a heap-vs-columnar A/B `#[pg_test]` mirroring `m100_columnar_agg_customscan_matches_heap`
(columnar_agg.rs) + an in-PG 1M A/B (`benchmarks/columnar_aggregate_ab.py`) proving byte-identity per shipped shape,
AND an EXPLAIN assertion that each DECLINED shape is NOT a CustomScan (native plan).

## Coverage Corner 2 — Dependencies
No new dependency. Reuses DataFusion `functions_aggregate::expr_fn::{sum,avg}` (already pulled), the existing
`extract_all_predicates` / `build_filter_expr` zone-map machinery, and `Int64Array`/`Float64Array` from arrow.
`AnyNumeric` (pgrx 0.19) exists but is NOT used (the numeric-output shapes are declined).

## Coverage Corner 3 — Tools
pgrx 0.19 / DataFusion 54 / Arrow 58, droplet c-8 for the in-PG A/B (local box has no pgrx).

## Coverage Corner 4 — Techniques (the load-bearing research)

### PG aggregate output types (E1 — postgresql.org/docs/current/functions-aggregate.html, Table 9.62)
`sum(int2)→int8`, `sum(int4)→int8`, `sum(int8)→numeric`, `sum(float4)→float4`, `sum(float8)→float8`;
`avg(int2/4/8)→numeric`, `avg(float4)→float8`, `avg(float8)→float8`; `count(*)→int8`.
Promote to numeric: `sum(int8)`, all `avg` of exact types.

### DataFusion 54 output types (E2 sum.rs, E3 average.rs)
`sum(Int16/32/64)`→Int64; `sum(Float32/64)`→Float64. `avg(*)`→Float64 (catch-all `_ => Float64`; only Decimal
inputs keep Decimal).

### THE MISMATCH → admission decisions
| Shape | Decision | Arrow | PG | Strategy |
|---|---|---|---|---|
| GROUP BY + WHERE | **SHIP** | (existing aggs) | (existing) | `df.filter(f)?.aggregate(g,a)` — filter before aggregate (E4) |
| `avg(float8)` | **SHIP** | Float64 | float8 | Float64Array → float8 datum |
| `sum(int2)`,`sum(int4)` | **SHIP** | Int64 | int8 | Int64Array → int8 datum (no overflow — why PG chose bigint) |
| `avg(int2/4/8)` | **DECLINE** | Float64 | numeric | lossy f64 ≠ exact numeric; native plan |
| `sum(int8)` | **DECLINE** | Int64 | numeric | int8 sum overflows int64; native plan |
| `sum(float4)`,`avg(float4)` | **DECLINE** | Float64 | real/float8 | f4-vs-f64 accumulation differs at ULP |

### filter+group in one plan (E4 — datafusion dataframe/mod.rs)
`df.filter(pred)?.aggregate(group_exprs, aggr_exprs)?` builds Filter then Aggregate on top → filters before grouping
= SQL `WHERE … GROUP BY`. The zone-map decode already projects predicate + group columns.

### Mature-extension pattern (E5 — Citus multi_logical_optimizer.c)
Citus never hardcodes aggregate result types: resolves via `get_func_rettype` and inserts explicit casts; decomposes
`avg` into `sum/count` to keep exact numeric. **Lesson:** drive the output-Datum cast from the aggregate's declared PG
output type per-AggSpec (not a fixed count→int8/sum→float8 map); decline where the exact type is numeric and only a
lossy value is cheaply available.

## ADRs
- **ADR-M114-1:** Ship GROUP BY+WHERE, avg(float8), sum(int2/4); DECLINE avg(int*), sum(int8), sum(float4/avg(float4))
  → native plan. Alternative (deferred): Decimal128/`AnyNumeric` accumulator + numeric datum, OR Citus sum/count
  decomposition — disproportionate for M114 (accidental complexity). Byte-fidelity, not a defect (Rule 3).
- **ADR-M114-2:** `agg_datum` emits the PG output type per-AggSpec variant (count/sumInt→int8, sum/avgFloat8→float8),
  mirroring Citus's rettype-driven cast — the minimal structural change the new shapes require.

## Evidence citations
E1 postgresql.org/docs/current/functions-aggregate.html · E2 github.com/apache/datafusion/blob/54.0.0/datafusion/functions-aggregate/src/sum.rs ·
E3 .../functions-aggregate/src/average.rs · E4 .../core/src/dataframe/mod.rs · E5 github.com/citusdata/citus multi_logical_optimizer.c
