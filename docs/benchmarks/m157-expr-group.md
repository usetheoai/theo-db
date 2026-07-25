# M157 — GROUP BY date_trunc expression pushdown: measured coverage + byte-identity

**Date:** 2026-07-25
**Milestone:** M157 (`.claude/knowledge-base/plans/m157-expr-group-plan.md`)
**Verdict:** DoD met — coverage rose **31 → 32** (q42), **A/B byte-identical (diverged = 0)** in both regimes.

## What was measured

Routing of `GROUP BY date_trunc('unit', ts)` (an EXPRESSION group key over a `timestamp` column) to the vectorized
columnar aggregate CustomScan, which previously accepted only bare `Var` group keys. The expression key is serialized
in a 3rd `custom_private` channel and reconstructed as `date_trunc(lit(unit), col(base))` in DataFusion's `.aggregate`.

The metric is **coverage** (`columnar_customscan_count`) and **correctness** (`result_ab.diverged` — every routed
query's result compared byte-for-byte, order-canonicalized, against a heap twin). Not a speed claim (M148 materialization
cost unchanged); M157 widens *what routes* (a time-bucketing capability), proven *correct*.

## Honest scope (measurement-first, from the blueprint)

The discovery blueprint measured, by primary source, that of the 7 expr-group/HAVING ClickBench queries **only q42
(`date_trunc('minute', EventTime)`) is genuinely achievable**. The 2 HAVING queries (q27, q28) die on *independent*
blockers (`AVG(length(URL))` agg-over-expression; `REGEXP_REPLACE`) — **implementing HAVING alone routes ZERO queries**
(the M155 lesson). CASE/EXTRACT decline (numeric group-key / cross-branch type unification). So M157 delivers the
**date_trunc time-bucketing capability** (+1 measured query, q42) and declines the rest as documented honest-negatives.
EventTime is `TIMESTAMP` (not `timestamptz`), which makes q42 byte-safe (see the timezone guard below).

## Methodology

- Harness: `benchmarks/run_m128_clickbench.py --agg` (builds a columnar `hits` + heap `hits_heap`, EXPLAIN-detects the
  columnar Custom Scan per query, strips `LIMIT` + canonicalizes order-insensitively for the A/B).
- Two regimes, both run: **head 100k** and **systematic 300k** (higher cardinality). `work_mem=256MB`, parallelism off.
- Box: **self-hosted ephemeral droplet (c-8) — NOT the canonical ClickBench c6a.4xlarge**, subsampled data. Coverage +
  byte-identity are box-independent (structural); the geomean is not leaderboard-comparable and no ClickHouse/AlloyDB
  baseline is claimed. PostgreSQL 18.4, `theodb_rs` release build (pgrx 0.19), extension 1.2.0.

## Results

| Regime | queries ok | `columnar_customscan_count` | `result_ab.diverged` |
|---|---|---|---|
| head 100k | 43/43 | **32** | **0** (byte-identical) |
| systematic 300k | 43/43 | **32** | **0** (byte-identical) |

Artifacts: `m157-artifacts/m157_head_100k.json`, `m157-artifacts/m157_systematic_300k.json`,
`m157-artifacts/m157_measure_full.log`.

### Coverage delta (honest)

Baseline (M156): **31** routed. M157: **32** routed. **Newly routed (+1): q42** — the `date_trunc('minute', EventTime)`
time-bucketing query. All 31 M156 queries still route (no regression, diverged=0).

## Correctness guards (proven on-box by `benchmarks/m157_ec_harness.sql`)

Routing only happens for a `date_trunc('unit', ts::timestamp)` key with `unit ∈ {second,minute,hour,day,month,quarter,
year}`; everything else declines to the native plan (fail-closed). Each verified on-box:

| Case | Behavior | Evidence |
|---|---|---|
| `GROUP BY date_trunc('day'/'month'/'minute', ts)` | routes | Custom Scan; grouped A/B `mismatches=0` (col_groups=40=heap, col_total=3000=heap) |
| bare-Var `GROUP BY v` | routes (backward-compat) | Custom Scan |
| `date_trunc('week', ts)` (ISO week ≠ Arrow) | **declines** | Seq Scan |
| `date_trunc(..., timestamptz)` under `TimeZone='America/Sao_Paulo'` | **declines** | Seq Scan |
| `EXTRACT(hour FROM ts)` (numeric group-key) | **declines** | Seq Scan |
| `CASE WHEN … END` group-key | **declines** | Seq Scan |
| arithmetic `v+1` group-key | **declines** | Seq Scan |

### Why the timezone guard (ADR-2), proven by primary source

- `timestamp` (tz-independent): PG `timestamp_trunc` truncates field-by-field (`timestamp.c`), matching DataFusion's
  `parsed_tz=None` naive truncation — CASES → admits.
- `timestamptz`: PG `timestamptz_trunc` uses `session_timezone`; DataFusion truncates in UTC → DIVERGES under
  `TimeZone≠UTC` → declines unconditionally (the same class the M151 temporal cross-type review caught; a UTC-only
  ClickBench A/B never exercises it).
- DataFusion `date_trunc` preserves the input Arrow unit (`return_field_from_args` returns the input type), so a
  `Timestamp(Microsecond)` input yields a `Timestamp(Microsecond)` output — no ns→µs off-by-1000 (verified by `mismatches=0`).

## Reproduction

```bash
python3 benchmarks/run_m128_clickbench.py --agg --n 100000 --sample head       --out m157_head.json
python3 benchmarks/run_m128_clickbench.py --agg --n 300000 --sample systematic --out m157_systematic.json
psql -f benchmarks/m157_ec_harness.sql   # date_trunc routing + timezone/granularity/EXTRACT/CASE decline guards
```
