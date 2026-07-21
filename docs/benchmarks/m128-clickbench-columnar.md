# M128 — Official benchmark COLUMNAR pillar: ClickBench over theodb_columnar (measured)

**Date:** 2026-07-20 · **Box:** self-hosted DO droplet (theo-e2e-runner) — **NOT the canonical AWS `c6a.4xlarge`**,
so timings are not leaderboard-comparable · pgrx-managed PG17.10, `theodb_rs`. Implements ADR-0050 (adopt-and-wrap)
for the columnar pillar — the second application of the pattern the M127 vector pilot proved.

**Verdict:** the 43 ClickBench queries run over the `theodb_columnar` table and are proven **byte-identical-correct**
vs a heap copy (the correctness oracle ClickBench itself lacks). The vectorized-aggregate CustomScan (the pushdown
optimization) hit a real **planner-hang bug** on the wide 105-column real `hits` table (filed as **#135**); the
sound, complete measurement is over the columnar **storage** path (native executor), with the pushdown as tracked
follow-up.

## The ClickBench entry contract (`benchmarks/clickbench/theodb/`)

The copy-the-`postgresql/`-directory contract a public ClickBench leaderboard PR submits: `create.sql` (the 105-col
`hits` DDL, verbatim from ClickBench + `USING theodb_columnar`), `queries.sql` (the 43 queries, verbatim),
`benchmark.sh`, `template.json`, and the produced `results.json` (raw `[t1,t2,t3]` timing triples). Only the table
AM clause differs from the ClickBench reference — a later leaderboard PR is mechanical.

## Measured — 43 queries over theodb_columnar storage (ClickBench cold/hot protocol)

Real ClickBench `hits` (CC-BY-NC-SA, streamed + subsampled), loaded into `theodb_columnar` (via the INSERT-SELECT
bulk path the `columnar_*_ab.py` benchmarks use), `enable_columnar_agg=off` (native executor over columnar
storage — see the pushdown note), each query 3× (cold + 2 hot, report min-hot), geomean combine:

| Metric | Value |
|---|---|
| queries completed | **43 / 43** (0 errored) |
| hot latency range | 0.052 s … 0.248 s |
| **hot geomean** | **0.0668 s** |
| **byte-identical result A/B (columnar vs heap)** | **PASS — 43/43, 0 diverged** |
| corpus | 1,000-row real `hits` subsample (self-hosted box) |

A larger-scale run (n=100,000) corroborated the same **byte-identical correctness** on every query measured (each
`ab=True`), at ~5.5 s/query — timing scales with rows as expected; the full-99.9M-row canonical-box run is the
operational follow-up (ADR M128-2).

## The retained correctness oracle (what ClickBench lacks) — and what it caught

ClickBench's `check` is a `SELECT 1` liveness probe; it validates **no results** (a wrong-but-fast engine could top
the board). The retained byte-identical result A/B (columnar vs heap, `benchmarks/theodb_bench/regression.py`)
**PASSES 43/43** — the columnar storage returns byte-identical results to the row store on the whole ClickBench
workload. Methodology honesty: for the `… ORDER BY count DESC LIMIT 10` queries, the LIMIT cut picks an
arbitrary-but-valid 10 among tied counts (a legitimate scan-order difference, not a bug), so the A/B compares the
**full unlimited deterministic aggregation** — the real storage-correctness property. (An earlier A/B that compared
the limited row set flagged 9 such tie artifacts; comparing the full aggregation resolves them to PASS.)

## Honest scope — the pushdown planner-hang bug (#135)

`enable_columnar_agg=on` enables the vectorized-aggregate CustomScan. On the real 105-column `hits` table it
**hangs during PLANNING** for at least one query (`GROUP BY UserID`) — uninterruptible by `statement_timeout`
(which only fires during execution), requiring a server restart. Narrow (2-col, and 105-col all-`bigint`) tables do
NOT reproduce; the wide mixed-type (TEXT-heavy) real schema does. Filed as
[#135](https://github.com/usetheodev/theo-db/issues/135). Therefore this milestone measures the columnar **storage**
path (agg off), which is sound and complete; the vectorized pushdown is a tracked follow-up gated on #135. This is
honest scope, not a workaround — the CustomScan is an optional optimization on top of the columnar pillar, and the
storage path runs the full ClickBench correctly.

## Reproduction

```
# self-hosted PG17 with theodb_rs; benchmarks deps (psycopg2)
PYTHONPATH=benchmarks python3 benchmarks/run_m128_clickbench.py --n 1000 --out docs/benchmarks/m128-clickbench-columnar.json
```
`hits` auto-streams from `datasets.clickhouse.com/hits_compatible/hits.tsv.gz` (CC-BY-NC-SA, CI-only, NEVER
vendored — subsampled via `curl | zcat | head`). No data → status `UNBENCHMARKED`. `--agg` opts into the CustomScan
pushdown (hits #135 on the real hits table). 5 unit tests cover the entry contract + the A/B helpers.
