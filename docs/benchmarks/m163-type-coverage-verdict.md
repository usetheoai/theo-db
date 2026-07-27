# M163 — type-coverage A/B run

**Date:** 2026-07-27 · **Build:** `theodb_rs` @ develop v0.153.0 (PG18, DataFusion 54 / Arrow 58), fresh droplet
theo-m163 (8 GB), `CREATE EXTENSION theodb_rs`. **Harness:** `benchmarks/columnar_type_ab.py` (bespoke pytest
differential — ADR-1; reuses the M156-M162 symmetric-EXCEPT oracle, no new dep). **Tests:**
`pytest benchmarks/test_columnar_type_ab.py` → **10 passed** (5 pure-logic + 5 live).

## Why this milestone exists

The ClickBench A/B runs over benchmark data, which does not exercise the TYPE space, so type-class bugs (M151/M154/M157/
M161) survive the A/B and are only caught by council review after a 14-min rebuild. This harness runs the routed classes
against a synthetic `theodb_columnar` table across a **per-type edge catalog** (int2 `32767`, int4/int8 max, float
`-0.0`/`NaN`/`inf`, timestamp/date/timestamptz, non-C text, NULL) and asserts the M161 fail-closed contract:
**byte-identical if it routes, OR correct-decline to native** — over the edges the benchmark misses.

## The M161 regression it catches (the raison d'être)

The M161 BLOCKER: int±k used the column type as `out_typoid` (not `opresulttype`) → `int2col+5` @ 32767 → PG int4
`32772`, but the buggy code emitted int2 → `i16::try_from(32772)` errors. The `intpk_i2` case (`SELECT c2+5 … GROUP BY
c2+5`) with the `c2=32767` edge row runs exactly that shape: on the **fixed** v0.153.0 it is `diverged=0` (below); on the
**buggy** M161 code the ON arm would have errored / diverged → the harness FAILS before review. The **positive control**
(a deliberately-divergent pair, `diverged=4` below) proves the oracle detects a wrong result — the ClickBench A/B
(ClientIP int4−int4) never seeds a 32767 int2, so it never triggered it.

**Rows loaded:** 2000 (equal in `hits` columnar + `hits_heap`).  
**Positive control:** seeded divergence detected (diverged=4) — the oracle catches a wrong result.  
**Result:** 16/16 cases as-expected.

| case | expect | got | diverged | ok |
|---|---|---|---|---|
| agg_count | route | ok | 0 | ✅ |
| agg_sum_i4 | route | ok | 0 | ✅ |
| inlist_i4 | route | ok | 0 | ✅ |
| inlist_i2 | route | ok | 0 | ✅ |
| inlist_null | decline | declined | None | ✅ |
| intpk_i2 | route | ok | 0 | ✅ |
| intpk_i4 | route | ok | 0 | ✅ |
| intpk_i8_result | decline | declined | None | ✅ |
| intpk_i4_wide | decline | declined | None | ✅ |
| date_plus | decline | declined | None | ✅ |
| ts_inlist | decline | declined | None | ✅ |
| extract_minute | route | ok | 0 | ✅ |
| extract_day | decline | declined | None | ✅ |
| group_f8 | route | ok | 0 | ✅ |
| group_i2 | route | ok | 0 | ✅ |
| group_bool | route | ok | 0 | ✅ |

Each `route` case is EXPLAIN=Custom Scan + symmetric-EXCEPT diverged=0; each `decline` case is native (no Custom Scan), the M161 fail-closed contract, over the type-edge catalog the ClickBench A/B misses.
