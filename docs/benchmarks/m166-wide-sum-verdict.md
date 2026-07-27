# M166 — wide `SUM(int2_col ± const)` aggregate-argument pushdown (ClickBench q29)

**Date:** 2026-07-27 · **Milestone:** M166 · **Verdict:** q29 routes byte-identical; q21/q22 collation honest-negatives; q27 deferred.

## What is now possible

ClickBench **q29** — the wide-SUM query `SELECT SUM(ResolutionWidth), SUM(ResolutionWidth + 1), … SUM(ResolutionWidth + 89) FROM hits`
(90 aggregates) — now routes to the vectorized columnar-agg CustomScan. It previously declined because each aggregate
argument is a `T_OpExpr` (`col + const`), not a bare `Var`, and the agg-arg classifier admitted only bare-`Var` args.

## The guarantee added

`SUM(int2_col ± int_const)` is admitted **only in a provably-overflow-free class**, so the routed result is
byte-identical to PostgreSQL's native plan:

- **int2 base column** + **int4 operator result** only.
- The whole int2 domain ± delta must stay inside int4 (`i32::try_from(32767 + delta)` **and**
  `i32::try_from(-32768 + delta)` both succeed). Then PG raises no per-row `22003`, and the widened int8 sum is exact.
- An int4/int8 base, an int2/int8 result, a non-additive op, a non-integer const, or an out-of-range delta **decline**
  to the native plan (trace `agg_sum_expr_unsupported`). This is the fail-closed gate — a `SUM` over an int4 column would
  reach PG's per-row overflow that a widened sum silently swallows, so it is refused.

`ResolutionWidth` is `SMALLINT` (int2) and every delta is 0..89 → q29 is squarely inside the safe class.

## Evidence

| Oracle | Result |
|---|---|
| `EXPLAIN` q29 (columnar `hits`, `theodb.enable_columnar_agg=on`) | `Custom Scan (theodb_columnar_agg)` ✅ |
| A/B symmetric-EXCEPT (columnar `hits` vs heap `hits_heap`), all 90 output columns | **diverged = 0** ✅ (byte-identical) |
| `EXPLAIN (VERBOSE)` q29 | renders without error (M131 `OUTER_VAR` deparse hazard cleared) ✅ |
| Type-coverage A/B (`benchmarks/columnar_type_ab.py`, db `m163_type_ab`) | **31/31 as-expected, positive control diverged=2** ✅ |
| — `sum_i2_add` / `sum_i2_sub` | route, diverged=0 ✅ |
| — `sum_i4_add_decline` / `sum_i8_add_decline` / `sum_i2_wide_decline` | declined ✅ (fail-closed) |
| Pure-logic unit tier (`pytest benchmarks/test_columnar_type_ab.py`) | M166 contract test green ✅ |
| Full ClickBench `--agg` (1M rows, same-box) | q29 routes; **43/43 A/B byte-identical (0 regressions)** ✅ |

### Ratio + no-regression (full `--agg`, 1M ClickBench, same-box vs ClickHouse 26.8.1)

Artifacts: `docs/benchmarks/m166-clickbench-agg.json` (43-query run), `docs/benchmarks/m166-type-coverage.md`.

| Query | Before (M165) | After (M166) | vs ClickHouse |
|---|---|---|---|
| q29 (wide `SUM(ResolutionWidth + k)`, 90 aggregates) — TheoDB hot | **27.79 s** (storage path, non-pushdown) | **0.1027 s** (columnar-agg pushdown) | — |
| q29 ratio vs ClickHouse (baseline 0.049 s) | **567.13×** | **≈ 2.10×** | flips non-pushdown → pushdown |

- Same-engine: **27.79 s → 0.1027 s (≈ 270× faster)**; the gap vs ClickHouse collapses from 567× to ≈ 2.1× — inside the
  covered class (fresh-benchmark pushdown-class geomean 4.53×).
- **No regression:** all 43 queries report `result_ab_identical == True` (0 `ab=False`); `columnar_agg_routed == True`
  for q29 and the previously-routed queries, unchanged.
- q28 (the REGEXP + `AVG(length)` + `MIN(text)` query) stays `columnar_agg_routed == False` — declines, as the q27/q21/q22
  honest-negatives predict.

## Cost / trade-off

- The safe class is **narrower** than the GROUP BY `IntAddConst` gate (which materializes each per-row int4 and
  reproduces 22003 with a range check). A `SUM` never forms the per-row value, so admission is restricted to inputs where
  overflow is provably impossible over the whole domain — an honest, deliberate narrowing, not a gap.
- Wire format: the agg channel grew from 2 to 4 ints per aggregate (`kind, attno, delta_hi, delta_lo`), the delta split
  hi/lo like the M161 IN-list / M165 const-out channels.

## Honest-negatives (recorded, not shipped wrong)

- **q21 / q22 (`MIN(text)`)** — DataFusion computes byte-minimum (memcmp); PostgreSQL computes collation-minimum. A
  deterministic collation constrains *equality*, not *order*, so routing gives an A/B-visible wrong `MIN` under any
  non-C collation. Safe only under `C`/`POSIX`, which ClickBench's default-collation columns are not — a C-only admit
  gate would decline them in practice (YAGNI to implement). Same class as the M165 q17 honest-negative. The correct fix
  (a collation-aware executor min/max) is a separate deferred capability.
- **q27 (`AVG(length(URL))`)** — the agg arg is a `T_FuncExpr`; routable only under a UTF-8 encoding gate plus a new
  scalar-func-in-agg mechanism. Deferred as a separate capability (higher leverage, but new mechanism + encoding
  correctness — not shipped without proof).

## Reproduction

```bash
# type-coverage A/B (fail-closed proof, runs before review)
PGHOST=127.0.0.1 PGDATABASE=m163_type_ab PGUSER=postgres python3 benchmarks/columnar_type_ab.py
# q29 EXPLAIN + A/B on the real ClickBench hits
Q=$(sed -n 30p benchmarks/clickbench/theodb/queries.sql | sed 's/;$//')
psql -d postgres -c "SET theodb.enable_columnar_agg=on; EXPLAIN $Q"       # -> Custom Scan (theodb_columnar_agg)
# full ratio
PGHOST=127.0.0.1 PGDATABASE=postgres python3 benchmarks/run_m128_clickbench.py --agg --n 1000000
```
