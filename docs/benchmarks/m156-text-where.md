# M156 — Text WHERE predicate pushdown (columnar CustomScan): measured coverage + byte-identity

**Date:** 2026-07-25
**Milestone:** M156 (`.claude/knowledge-base/plans/m156-text-where-plan.md`)
**Verdict:** DoD met — coverage rose **21 → 31** (+10 queries), **A/B byte-identical (diverged = 0)** in both regimes.

## What was measured

Routing of text predicates in the WHERE clause (`col = 'x'`, `col <> ''`, `col LIKE '%p%'`, `col NOT LIKE 'a%'`)
to the vectorized columnar aggregate CustomScan (DataFusion), which previously declined any text WHERE
(`extract_all_predicates` returned `None` → `unpushable_where_qual`, the largest first-blocker M152 measured).

The metric is **coverage** (`columnar_customscan_count` — how many of the 43 ClickBench queries route to the
vectorized path) and **correctness** (`result_ab.diverged` — every routed query's result compared byte-for-byte,
order-canonicalized, against the same query on a heap twin). This is **not** a speed claim: per M148 the dominant
scan cost is row-by-row materialization, unchanged here; M156 widens *what routes*, proven *correct*, not *faster*.

## Methodology

- Harness: `benchmarks/run_m128_clickbench.py --agg` (builds a columnar `hits` + heap `hits_heap`, EXPLAIN-detects the
  columnar Custom Scan per query, strips `LIMIT` + canonicalizes order-insensitively for the A/B).
- Two regimes, both run: **head 100k** (`--n 100000 --sample head`) and **systematic 300k** (`--n 300000 --sample
  systematic`, higher cardinality). `work_mem = 256MB`, `max_parallel_workers_per_gather = 0`.
- Box: **self-hosted ephemeral droplet (c-8, 8 vCPU) — NOT the canonical ClickBench c6a.4xlarge**, subsampled data.
  Coverage + byte-identity are box-independent (structural); the geomean is **not leaderboard-comparable** and no
  ClickHouse/AlloyDB baseline is claimed (none exists in this repo).
- PostgreSQL 18.4, `theodb_rs` release build (pgrx 0.19), extension 1.2.0.

## Results

| Regime | queries ok | `columnar_customscan_count` | `result_ab.diverged` | hot geomean (s) |
|---|---|---|---|---|
| head 100k | 43/43 | **31** | **0** (byte-identical) | 0.06219 |
| systematic 300k | 43/43 | **31** | **0** (byte-identical) | 0.06216 |

Artifacts: `m156-artifacts/m156_head_100k.json`, `m156-artifacts/m156_systematic_300k.json`,
`m156-artifacts/m156_measure_full.log` (EC harness + both A/B runs).

### Coverage delta (honest)

Baseline (M153/M155): **21** routed — ids `[0,1,2,3,4,5,6,7,8,9,15,16,19,23,24,25,26,32,33,38,41]`.
M156: **31** routed. **Newly routed (+10): q10, q11, q12, q13, q14, q20, q30, q31, q36, q37** — the text-WHERE queries
M152 measured as `unpushable_where_qual` first-blockers, now pushed.

**Still declined (honest-negative), by design:** q27 (and any query whose WHERE contains ILIKE / regex / a bpchar
column / a non-deterministic collation / a non-UTF-8 literal / a dangling-escape LIKE pattern) — these fail-close to
the native plan because a byte-wise DataFusion filter would diverge from PostgreSQL there (see the guards below). We
do NOT promise 43/43: min/max-text-by-collation and regex/ILIKE are structural honest-negatives.

## Correctness guards (proven on-box by `benchmarks/m156_ec_harness.sql`)

Routing only happens under provably-safe conditions; everything else declines to the native plan (fail-closed). Each
was A/B-verified byte-identical on the box:

| Case | Behavior | A/B (col == heap) |
|---|---|---|
| `= 'p1'` / `<> ''` / `LIKE '%x%'` / `NOT LIKE 'http%'` | routes | 200=200 / 1960=1960 / 1681=1681 / 319=319 |
| `LIKE 'a\%b'` (backslash escape → literal `%`) | routes | 279=279 |
| mixed text + numeric (`phrase='p1' AND uid>5`) | routes | 160=160 |
| round-trip needles (`''`, `'a%b'`, `'%\_%'`) | routes | all equal |
| ILIKE (`~~*`) / regex (`~`) | **declines** (Seq Scan) | 1681=1681 |
| `bpchar` column predicate | **declines** | 1000=1000 |
| non-deterministic collation | **declines** | 667=667 |
| `col = NULL` (null const) | **declines** | 0=0 |
| LIKE pattern ending in `\` (dangling escape) | **declines** (Seq Scan, `Filter: url ~~ 'ab\'`) | 0=0 |
| non-UTF-8 literal in a LATIN1 database (`= chr(233)`) | **declines** (no planner panic) | 100=100 |
| ASCII literal in a LATIN1 database | routes | 200=200 |

## Review findings fixed (both invisible to the ClickBench A/B)

Two adversarial councils each found a real defect the UTF-8 / no-backslash ClickBench A/B never exercises — the
recurring lesson (M151/M154): the review proves the shape space the benchmark does not.

1. **council-rust-pgrx (HIGH)** — `String::from_datum` on the text literal PANICS in the upper-paths planner hook under
   a non-UTF-8 server encoding (LATIN1/WIN1252 → Ascii policy; SQL_ASCII → strict UTF-8), turning a valid query into a
   planner ERROR. **Fix:** read via `text_to_cstring` (no assertion) + decline fail-closed on invalid UTF-8. Proven by
   EC-6 (LATIN1, no panic).
2. **council-index-storage (MEDIUM)** — a LIKE/NOT LIKE pattern ending in an odd number of `\` diverges: PostgreSQL
   rejects a dangling escape with ERROR 22025 (`like_match.c:105-108`, fired when a row's text matches up to the escape
   char) while arrow's kernel treats the trailing `\` as a literal and returns rows. Either way the columnar path can
   never replicate PG. **Fix:** decline such patterns at plan time. **Proven by EC-7:** the plan is `Seq Scan …
   Filter: url ~~ 'ab\'::text` (declined, not routed). NOTE: PG's 22025 is *data-dependent* — it fires only when a row
   matches the pattern prefix up to the `\`; the EC-7 dataset has no `ab`-prefixed row, so native returns `count 0`
   (no error) in the artifact. What EC-7 proves is the decline (the mechanism that prevents arrow's divergent answer);
   the PG-error consequence is by `like_match.c` citation, not shown in this run's log.

All other guards (collation, bpchar exclusion, operator whitelist via `FirstNormalObjectId`, default `\` escape,
NULL 3-valued, min/max fast-path disable) were verified correct against PostgreSQL primary source.

## Reproduction

```bash
# on a PG18 box with the theodb_rs extension installed, work_mem=256MB, parallelism off:
python3 benchmarks/run_m128_clickbench.py --agg --n 100000 --sample head       --out m156_head.json
python3 benchmarks/run_m128_clickbench.py --agg --n 300000 --sample systematic --out m156_systematic.json
psql -f benchmarks/m156_ec_harness.sql        # guards + round-trip + council-fix probes
```
