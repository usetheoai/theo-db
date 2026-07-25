# M158 — coverage gate + baseline (measurement-first, pre-implementation)

**Date:** 2026-07-25
**Purpose:** the blueprint's mandatory gate (M155 echo) — measure whether late materialization has real ClickBench
coverage BEFORE building the CustomScan. Verdict: **PASSES → build** (a `SELECT *` ORDER-BY-LIMIT query is the prime
target).

## Coverage gate — pure `SELECT <cols> … ORDER BY key LIMIT k` (no GROUP BY)

From `benchmarks/clickbench/theodb/queries.sql`, the ORDER-BY-LIMIT scan queries (not aggregate/GROUP BY):

| Query | Shape | out-cols | late-mat fit |
|---|---|---|---|
| `SELECT * FROM hits WHERE URL LIKE '%google%' ORDER BY EventTime LIMIT 10` | **SELECT * (105 cols)** | 105 | **PRIME** (k=10≪N, wide) |
| `SELECT SearchPhrase … ORDER BY EventTime LIMIT 10` | 1 out-col, key≠output | 1 | marginal (1 col deferred) |
| `SELECT SearchPhrase … ORDER BY SearchPhrase LIMIT 10` | 1 out-col, key=output | 1 | none (key IS the output) |
| `SELECT SearchPhrase … ORDER BY EventTime, SearchPhrase LIMIT 10` | 1 out-col, 2 keys | 1 | marginal |

**≥1 query strongly benefits** (the `SELECT *`, 105 out-cols) → not honest-negative. The general capability (`SELECT *
… ORDER BY key LIMIT k`, the "latest N rows" pattern) benefits any wide columnar table.

## Baseline (the "before" number, EXPLAIN ANALYZE, TIMING ON, 100k subsample)

```
SELECT * FROM hits WHERE URL LIKE '%google%' ORDER BY EventTime LIMIT 10;
 Limit → Sort (key: eventtime) → Custom Scan (theodb_columnar_project) on hits
   Rows Removed by Filter: 100000   (no URL matched '%google%' in this subsample)
 Execution Time: 1287.766 ms
```

The `theodb_columnar_project` CustomScan (M149) materializes **all 105 projected columns × 100000 rows** before the
Sort+Limit picks 10 — this is the M148 bottleneck (~80% `form_row`/`palloc`). Late materialization would decode only
{filter col `url` ∪ sort key `eventtime`} for all rows, apply the filter + top-k, then materialize the full 105-column
projection for only the k survivors.

## Decision

**Build M158** — a late-materialization CustomScan that fuses `Limit(k) → Sort([key]) → columnar project/scan` and
defers full-row materialization to the k survivors. Trigger-gated (`k/N ≲ 0.1` AND wide projection), `enable_columnar_
late_mat` GUC default OFF until the A/B + flamegraph prove the measured win (else honest-negative, M155-style).
