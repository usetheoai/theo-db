# M163 — type-coverage A/B verdict

**Milestone:** M163 — per-type differential harness over the columnar routing admit-paths.
**Date:** 2026-07-27.
**Box:** DigitalOcean droplet (15 GB), PostgreSQL 18 + `theodb_rs` release build (DataFusion 54 / Arrow 58), own `theodb_columnar` TableAM.
**Harness:** `benchmarks/columnar_type_ab.py` (Tier-1 pure-logic via `pytest benchmarks/test_columnar_type_ab.py`; Tier-2 live matrix here).

## What this proves

For each routed admit-path × each per-type **edge value** (int2 `32767`, int4 max, int8 `INT_MIN`/max, float
`-0.0`/`NaN`/`Infinity`, timestamp, date, ≥2-distinct timestamptz, text, bool, NULL — the `EDGE_CATALOG`), the harness
asserts the M161 **fail-closed contract**: either the query **routes AND is byte-identical** to the row executor, **or**
it **correctly declines** (no `Custom Scan`). This is the type space the ClickBench A/B never exercises — the blind spot
that let the recurring M151/M154/M157/M161 type-class bugs survive to council review after a 14-min rebuild.

**Routing evidence comes from the SAME execution that produces the compared data** (council-benchmark HIGH fix). For a
`route` case, `EXPLAIN (ANALYZE) CREATE TEMP TABLE _ab_on AS <sql>` runs the routed query at statement top level,
materializes its result, **and** returns the real executed plan — so the "it routed" claim and the "diverged=0" claim
are one execution, not a bare-query EXPLAIN whose plan a CTE/set-op comparison wrapper could silently diverge from. A
`decline` case is routing-checked with a plain `EXPLAIN` (no execution), so a decline query that would raise on execution
(e.g. `c8+5` overflowing int8 at the INT_MAX edge) is reported `declined`, not `error`.

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

## Council-benchmark review — findings addressed

- **HIGH (routing measured on a different query than the divergence)** — closed by taking routing evidence from the real
  execution (`EXPLAIN ANALYZE CREATE TEMP TABLE _ab_on AS <sql>`), re-validated live: every `route` case still routes AND
  is `diverged=0` under the materialized-arm comparison, empirically confirming the pushdown fires in the executed query.
- **MEDIUM (rot-guard was substring-based, trivially true for 1-char columns)** — `test_every_routed_type_appears_in_a_case`
  now matches `\bcol\b` word-boundaries, so deleting the only `date`/`text`/`bool` case fails the guard.
- **LOW (tz single value)** — `EDGE_CATALOG["tz"]` now has ≥2 distinct instants, so `tz_group` proves distinct-instant
  discrimination, not a single-group round-trip.
- **LOW (INT_MIN omitted)** — added int8 `INT_MIN` (`'-9223372036854775808'::int8`). int4 `INT_MIN` is intentionally
  **not** added: the `c4-1` route case would underflow it, and error-parity (not value-parity) is a different assertion
  than this oracle makes — an honest, documented gap rather than a broken case.
- **LOW (`plan_routes` presence-anywhere) / INFO (min/max-float aggregate NaN)** — accepted with a code comment; the
  routed cases are single-node plans, and min/max-float-as-*aggregate* is a separate surface (M105), not this GROUP-BY guard.

## Honest limitations

- **Text collation is the box default**, not pinned. `group_text` exercises the text group path but the "non-C collation
  declines" behavior (M156) is not asserted here — it needs an ICU non-deterministic collation that may not exist on the
  box. Stated so the claim is not overread.
- **Two expectation corrections** made during hardening: `tz_group` expects **route** (timestamptz by exact equality is a
  bijection under the epoch offset — the M157 divergence was `date_trunc` calendar bucketing, not bare equality; `route`
  so a future output-epoch regression surfaces as `diverged>0`). Scope excludes the late-materialization *projection* path
  (M158) — query-shape, not type-class, cost-gated on table size + `enable_sort`; it belongs to M158's own A/B.

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
