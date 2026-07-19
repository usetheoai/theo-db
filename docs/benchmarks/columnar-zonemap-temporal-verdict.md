# theodb_columnar zone-map skip-pruning — temporal (timestamptz + date) verdict

**Date:** 2026-07-19 · **Module:** `theodb_rs/src/am/columnar.rs::minmax_kind_of` (temporal OID→domain),
`am/df_executor.rs::build_arrow` (Timestamp/Date Arrow arrays) + `build_filter_expr` (Arrow-typed temporal literal),
`am/zonemap.rs` (temporal-domain regression test). Own-code; Apache-permissive (D1).

**What is measured** (DigitalOcean **c-8** dedicated, PG 17.10 + pgrx 0.19.0, `shared_buffers=2GB`): a clustered
**1M-row** `theodb_columnar` table with a **monotonic `ts timestamptz`** (1-minute steps from 2020-01-01 → tight
per-chunk-group ranges, 100 chunk groups of 10 000 rows) and its `::date`. A ~**10%-selective** time-range filtered
aggregate `SELECT sum(x) FROM czt WHERE ts BETWEEN <lo> AND <hi>` (and the same on `d date`) is run with
`theodb.columnar_zonemap_skip` **ON vs OFF** on the SAME table. Reproduce: `benchmarks/columnar_zonemap_ts_ab.py`.
Raw: `columnar-zonemap-temporal-verdict.json`.

**Goal:** byte-identical result AND the skip path decodes **≤ 25%** of the chunk groups the skip-off baseline
decodes, for BOTH the `timestamptz` and the `date` column. **Result: MET.**

---

## The gap this closes

The columnar zone-map skip-pruning consumer shipped (2026-07-18) covered `col <op> const` on the native int/float/bool
min/max types (I2/I4/I8/F4/F8/Bool) — but `minmax_kind_of` returned `None` for temporal types, so a **time-range
filter (`WHERE ts BETWEEN …`) — the most common analytical filter on time-series data — did not prune**. This slice
extends the consumer to `timestamp` / `timestamptz` / `date`.

## How it works (parsimony: reuse the proven skip path)

Temporal types share an **integer** min/max domain — the stored bytes ARE the internal int: `timestamp`/`timestamptz`
are int64 microseconds (→ the proven **I8** path), `date` is int32 days (→ **I4**). So:

1. `minmax_kind_of` maps `1114`/`1184`→I8, `1082`→I4. The **skip** (`chunk_can_match`), `compute_minmax` (min/max
   write), `extract_zone_predicate` (btree strategy + D5 same-type gate), and `encode_const_bits` all reuse the I8/I4
   path **unchanged** — a numeric i64/i32 compare is exactly correct for temporal ordering.
2. `build_arrow` maps the OIDs to a naive (tz=None) Arrow `Timestamp(µs)` / `Date32` array.
3. `build_filter_expr` emits a matching Arrow-typed literal (`ScalarValue::TimestampMicrosecond` / `Date32`) — a bare
   Int64 lit would type-mismatch the Timestamp column. The DataFusion Filter remains the **final authority** (D3).

The tz is display-only; the comparison is on the raw int domain, so a naive Timestamp literal compares correctly.

## Result — byte-identical + effective, on BOTH temporal columns

`EXPLAIN` shows `Custom Scan (theodb_columnar_agg)` on the filtered aggregate (the temporal WHERE is admitted — not a
trivial native-plan identity).

| column | byte-identical | CustomScan | chunk groups skipped | decoded | latency ON | latency OFF | speedup |
|---|:---:|:---:|---:|---:|---:|---:|---:|
| `timestamptz` | ✅ (300002.0) | YES | **89**/100 | 11% ≤ 25% | 18.1 ms | 157.6 ms | **8.69×** |
| `date` | ✅ (302400.0) | YES | **88**/100 | 12% ≤ 25% | 17.1 ms | 139.8 ms | **8.19×** |

---

## Verdict (honest)

- **GOAL MET** for both temporal columns: byte-identical AND skips ~88–89/100 chunk groups (decodes ~11–12% ≤ 25%
  target) for a measured **8.2–8.7× lower latency**. A real measured win, extending the columnar predicate-pushdown
  advantage to the canonical time-series filter.
- Reuses the already-proven skip path (I8/I4) — the temporal knowledge is localized to the two Arrow-facing functions
  (`build_arrow` / `build_filter_expr`) plus the OID→domain map. `chunk_can_match` / `compute_minmax` untouched.

## Caveats (honest)

- Same as the integer slice: the skip ratio tracks **selectivity × clustering**. The 8.2–8.7× is on a **monotonic**
  `ts` (the natural time-series case — insert order correlated with time). An unsorted timestamp column prunes little
  (`public-copy.md` rule 5). Real time-series workloads (append-mostly, time-correlated) are exactly the clustered case.
- Scope: `timestamptz` / `timestamp` / `date`. Deferred (separate slices): `time` / `interval`, text zone-maps, the
  seqscan path, GROUP BY pushdown, out-of-RAM. `arrow_cache` (M101 heap path) still declines temporal at `encode_cell`
  — untouched (a different consumer; no regression).
- Warm (in-`shared_buffers`) regime; out-of-RAM would show a larger win but was out of scope.
