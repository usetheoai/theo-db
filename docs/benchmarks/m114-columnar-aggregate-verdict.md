# M114 — columnar aggregate completeness: verdict

**Date:** 2026-07-19 · **Module:** `theodb_rs/src/am/columnar_agg.rs` (admit avg(float8)/sum(int2/4) + GROUP BY+WHERE),
`am/df_executor.rs` (`AggSpec::{SumInt,AvgFloat8}`, per-variant `agg_datum`, grouped filter). Own-code over the adopted
DataFusion aggregate (Rule 9). Plan: `m114-columnar-aggregate-completeness`; blueprint:
`knowledge-base/discoveries/blueprints/m114-columnar-aggregate-completeness-blueprint.md`.

**What is measured** (DigitalOcean **c-8** dedicated, PG 17.10 + pgrx 0.19.0, `shared_buffers=2GB`, `work_mem=256MB`):
a **1M-row** `theodb_columnar` table vs an identical heap table. Each SHIPPED aggregate shape's result is compared to
the heap's native result (scalar value, or full grouped result set for GROUP BY+WHERE); each DECLINED shape is asserted
NOT a CustomScan (native plan) and still correct. Speedup = GUC-on (CustomScan) vs GUC-off (native, same table).
Reproduce: `benchmarks/columnar_aggregate_ab.py`. Raw: `m114-columnar-aggregate-verdict.json`.

**Goal:** admit GROUP BY+WHERE, avg(float8), sum(int2/int4) byte-identical; decline the numeric-output shapes.
**Result: MET.**

## Byte-identical strategy (blueprint E1/E2/E3/E5)

PostgreSQL aggregate output types (E1) vs DataFusion 54 (E2/E3): `sum(int2/int4)→bigint` = Arrow Int64 (no overflow —
why PG chose bigint); `avg(float8)→float8` = Arrow Float64. The numeric-output shapes (`avg(int*)`, `sum(int8)`,
`sum(float4)`, `avg(float4)`) would need a lossy Float64→numeric or an overflow-prone Int64 → they DECLINE to the
native plan (ADR-M114-1). `agg_datum` emits the PG output type per `AggSpec` variant (ADR-M114-2, the Citus
rettype-driven pattern E5). GROUP BY+WHERE composes the zone-map skip + a DataFusion Filter (filter-before-aggregate,
E4) with the hash aggregate in one plan.

## Result — SHIPPED shapes (byte-identical + CustomScan + measured speedup)

| shape | CustomScan | byte-identical | latency CustomScan | latency native | speedup |
|---|:---:|:---:|---:|---:|---:|
| `avg(float8)` | YES | YES (500000.5) | 85.7 ms | 815.8 ms | **9.52×** |
| `sum(int4)` | YES | YES (500000500000) | 70.2 ms | 824.3 ms | **11.74×** |
| `sum(int2)` | YES | YES (499500000) | 64.7 ms | 840.2 ms | **12.99×** |
| `GROUP BY k … WHERE k BETWEEN … ` (41 groups) | YES | YES (full result set) | 152.0 ms | 1000.0 ms | **6.58×** |

## Result — DECLINED shapes (native plan + still correct)

| shape | CustomScan (expect NO) | correct vs heap |
|---|:---:|:---:|
| `avg(int4)` → numeric | NO ✅ | YES (500000.500000000000 — exact numeric) |
| `sum(int8)` → numeric | NO ✅ | YES |
| `sum(real)` | NO ✅ | YES |

## Verdict (honest)

- **GOAL MET.** The M100 columnar aggregate surface now admits GROUP BY+WHERE, `avg(float8)`, `sum(int2/int4)` —
  byte-identical to the native plan, **6.58×–12.99×** faster. The numeric-output shapes correctly decline to the
  native plan (which returns the exact numeric), proven by EXPLAIN + a correctness spot-check — no silent wrong result.
- Scope is exactly the byte-identical set the blueprint's primary-source analysis (PG docs + DataFusion 54 source)
  proved achievable. Declining `avg(int*)`/`sum(int8)`/`sum(float4)` is a byte-fidelity call (Rule 3), not a defect —
  the deferred alternative (Decimal128/`AnyNumeric` accumulator, or the Citus sum/count decomposition) is named in
  ADR-M114-1.

## Caveats (honest)

- GROUP BY+WHERE requires a **pushable** predicate (`col <op> const` on a native-min/max type); an un-pushable qual
  declines to the native plan (correct, unpruned). Warm regime; speedup is GUC-on vs GUC-off on the same table.
- The pre-existing M100 composability limitation (consuming an agg output value in an enclosing expression) is
  unchanged — it is the subject of M115, not M114.
