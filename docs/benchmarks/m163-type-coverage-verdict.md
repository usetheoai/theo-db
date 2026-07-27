# M163 — type-coverage A/B verdict

**Milestone:** M163 — per-type differential harness over the columnar routing admit-paths.
**Date:** 2026-07-27.
**Box:** DigitalOcean droplet (15 GB), PostgreSQL 18 + `theodb_rs` release build (DataFusion 54 / Arrow 58), own `theodb_columnar` TableAM.
**Harness:** `benchmarks/columnar_type_ab.py` (Tier-1 pure-logic via `pytest benchmarks/test_columnar_type_ab.py`; Tier-2 live matrix here).

## What this proves

For each routed admit-path × each per-type **edge value** (int2 `32767`, int4/int8 max, float `-0.0`/`NaN`/`Infinity`,
timestamp/date/timestamptz, non-C text, NULL — the `EDGE_CATALOG`), the harness asserts the M161 **fail-closed contract**:
either the query **routes AND is byte-identical** to the row executor (EXPLAIN shows the specific `Custom Scan` node
**and** symmetric-EXCEPT `diverged=0`), **or** it **correctly declines** (no `Custom Scan` in EXPLAIN). This is the type
space the ClickBench A/B never exercises — the blind spot that let the recurring M151/M154/M157/M161 type-class bugs
survive to council review after a 14-min rebuild.

**Positive control (oracle self-test):** a deliberately-divergent twin (`hits_bad` = `hits_heap` with one `c4` bumped),
run **through `ab_check`** (the exact path the real cases use), reported `diverged=2`. A green control genuinely proves the
oracle catches a wrong result — the M161 `out_typoid` BLOCKER shape.

## The bug this harness found on its first live run

`SELECT f, count(*) FROM t GROUP BY f` where `f` is `float4`/`float8` **diverged** columnar-vs-heap (symmetric-EXCEPT
`diverged=2` on the `-0.0`/`+0.0` edge). The vectorized DataFusion path groups by IEEE-754 byte value — `−0.0` and `+0.0`
land in *distinct* groups, and distinct `NaN` bit-patterns each get their own group — whereas PostgreSQL's
`float8eq`/`float4eq` group `−0.0` **with** `+0.0` and collapse all `NaN`s into one. Fix: the group-key classifier now
declines float types (`admit_trace("group_key_float_ieee_semantics")` in `columnar_agg.rs`), routing them to the row
executor — the same fail-closed remedy as the M154 float `COUNT(DISTINCT)` decline. Post-fix, `group_f8`/`group_f4`
DECLINE and the full matrix is green.

## Result — 20/20 cases as-expected

**Rows:** 2000 (equal in `hits` columnar + `hits_heap`). **Positive control:** diverged=2. **Exit:** 0.

| case | expect | got | diverged | ok |
|---|---|---|---|---|
| agg_count | route | ok | 0 | ✅ |
| agg_sum_i4 | route | ok | 0 | ✅ |
| agg_sum_i8 | route | ok | 0 | ✅ |
| inlist_i4 | route | ok | 0 | ✅ |
| inlist_i2 | route | ok | 0 | ✅ |
| inlist_null | decline | declined | None | ✅ |
| intpk_i2 | route | ok | 0 | ✅ |
| intpk_i4 | route | ok | 0 | ✅ |
| intpk_i8_result | decline | declined | None | ✅ |
| intpk_i4_wide | decline | declined | None | ✅ |
| date_plus | decline | declined | None | ✅ |
| ts_inlist | decline | declined | None | ✅ |
| tz_group | route | ok | 0 | ✅ |
| extract_minute | route | ok | 0 | ✅ |
| extract_day | decline | declined | None | ✅ |
| group_f8 | decline | declined | None | ✅ |
| group_f4 | decline | declined | None | ✅ |
| group_i2 | route | ok | 0 | ✅ |
| group_bool | route | ok | 0 | ✅ |
| group_text | route | ok | 0 | ✅ |

Two expectation corrections made during hardening (council-benchmark review):

- **`tz_group` routes byte-identical.** timestamptz grouped by *exact equality* is safe — the storage-vs-Arrow epoch
  offset is a bijection under equality (the M157 divergence was `date_trunc` *calendar bucketing*, not bare equality).
  `expect=route` so a future output-epoch regression surfaces as `diverged>0` instead of being masked by a pessimistic
  decline.
- **Scope.** The harness covers the AGG / group / IN-list / zone-pred / expr admit-paths — where every historical
  type-class bug lived. The late-materialization *projection* path (M158) is query-shape, not type-class, and is
  cost-gated on table size + `enable_sort`; it belongs to M158's own A/B, not here.

## Reproduce

```bash
# Tier-1 (no DB): pure-logic guards — catalog completeness, EXPLAIN-route detection, case-matrix rot-guard
pytest benchmarks/test_columnar_type_ab.py -q

# Tier-2 (live TheoDB): full matrix + positive control
PGHOST=127.0.0.1 PGPORT=5432 PGDATABASE=<db-with-theodb_rs> PGUSER=postgres PGPASSWORD=x \
  python3 benchmarks/columnar_type_ab.py --out docs/benchmarks/m163-type-coverage-verdict.md
# exit 0 = all cases as-expected; positive control MUST report diverged>0 or the run aborts
```

Gate wiring: `rules/testing.md § 5.1` — any change to the columnar routing admit-paths MUST pass this harness before `/review`.
