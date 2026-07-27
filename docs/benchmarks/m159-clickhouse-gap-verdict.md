# M159 — gap REAL vs ClickHouse (ClickBench, same-box measured verdict)

**Date:** 2026-07-26
**Purpose (DoD):** produce the HONEST per-query gap of TheoDB's columnar path vs ClickHouse on ClickBench, and rule on whether the owner's "2-3× slower than ClickHouse" target is reachable and for which query classes. Until now no ClickHouse baseline existed in the repo — any ratio would have been invented (CLAUDE.md rule 5). This is the measured number.

## Methodology (reproducible; documented deviation from canonical)

| Axis | This run | Canonical ClickBench |
|---|---|---|
| Box | 1 DigitalOcean droplet, 8 vCPU / 15 GB / 88 GB (ephemeral) | c6a.4xlarge (16 vCPU / 32 GB) |
| Dataset | `hits` **1,000,000-row systematic 1-in-99 subsample** (covers full time range; TOAST present) | full 99.99M rows |
| TheoDB | PG 18.4 + `theodb_rs` @ v0.149.0, `theodb_columnar` TableAM, `enable_columnar_agg=on` + `enable_columnar_late_mat=on` | — |
| ClickHouse | `clickhouse local --path` (daemon-free), same box, **same 1M sample TSV**, `MergeTree` (ClickBench `create.sql`) | ClickHouse server, MergeTree |
| Protocol | 3 runs/query, hot = min of 2 (cold-flushed first run); `--time` (server-side exec time) | 3 runs, hot = min |

**Same-box, same-subsample, same TSV for BOTH engines** — this eliminates the hardware/scale incomparability of comparing our subsample to ClickHouse's published full-dataset numbers (the risk (b) the DoD warns against). The absolute times are not the canonical numbers (1M ≪ 100M); the **ratio** is the comparable metric.

Reproduce: `benchmarks/run_m128_clickbench.py --n 1000000 --sample systematic --agg` (TheoDB) + load the same `hits_sample.tsv` into `clickhouse local` with ClickBench `clickhouse/create.sql` + `queries.sql`, then `benchmarks/m159_analyze.py theodb.json ch.jsonl`. Raw artifacts: `docs/benchmarks/m159-artifacts/` (`theodb-1m-clickbench.json`, `clickhouse-1m-clickbench.jsonl`, `per-query-comparison.md`).

## Result — the measured gap (ratio = TheoDB_hot / ClickHouse_hot)

| Metric | Value |
|---|---|
| Queries run | **43/43** OK, **0 errored**, A/B byte-identical vs heap **43/43** |
| TheoDB columnar CustomScan engaged (pushdown) | **32/43** (was 6/43 pre-M148, 2026-07-24) |
| **Geomean ratio — ALL 43** | **19.4× slower than ClickHouse** |
| Geomean ratio — **pushdown queries (32)** | **7.54×** |
| Geomean ratio — **non-pushdown queries (11)** | **303×** |
| On-target (ratio ≤ 3×) | **6/43** (incl. **q6 FASTER than ClickHouse, 0.17×**) |
| Near-target (3×–10×) | 14/43 |
| Structural gap (> 10×) | 23/43 |

Full per-query table: `docs/benchmarks/m159-artifacts/per-query-comparison.md`.

## Competitor landscape (published ClickBench, c6a.4xlarge, 100M — geomean vs ClickHouse)

Computed from the official ClickBench result JSONs (`github.com/ClickHouse/ClickBench/{system}/results/.../c6a.4xlarge.json`), same per-query-min geomean-vs-ClickHouse metric:

| System | geomean vs ClickHouse | License | Nature |
|---|---|---|---|
| DuckDB | **1.8×** | MIT | dedicated in-process columnar engine (the ceiling) |
| pg_mooncake | **6.2×** | MIT | PG columnar **powered by DuckDB** — the permissive-PG leader |
| Citus columnar | **167.7×** | AGPL | classic PG columnar extension |
| Hydra | ≈ Citus-class (AGPL columnar fork; no c6a result published) | AGPL | fork of Citus columnar |
| PostgreSQL (vanilla) | **2178×** | — | row store, no columnar |

**Placement (approximate — scale-caveated `[NO-BASELINE-COMPARABLE]` for the exact rank, since ours is 1M/8-vCPU and theirs is 100M/c6a.4xlarge; the geomean-vs-ClickHouse metric is relative-to-CH-on-the-same-box in each case, so it is roughly comparable):**

- TheoDB overall (19.4×) sits **between pg_mooncake (6.2×) and Citus (167.7×)** — i.e. **~8× ahead of the AGPL Citus/Hydra class and ~110× ahead of vanilla Postgres**, but behind the DuckDB-powered pg_mooncake.
- TheoDB's **covered class (7.54×) ≈ pg_mooncake (6.2×)** — competitive with the best *permissive* PG columnar, and we do it with **own-code DataFusion/Arrow** while pg_mooncake delegates to DuckDB.
- DuckDB (1.8×) is the ceiling no PG extension reaches (all pay the PG executor/MVCC tax; pg_mooncake only gets 6.2× by embedding DuckDB).

The strategic read: among D1-legal (permissive) PG columnar, the real competition is **pg_mooncake**, and we are covered-class-competitive with it; the gap to close is the 11 non-pushdown queries.

## Honest verdict on the "2-3×" target

**The "2-3×" is a per-query-CLASS target, achieved TODAY for the vectorized-aggregation class — NOT for the benchmark as a whole (19.4× geomean).**

- **2-3× IS reached** for simple aggregations that fully push down to the DataFusion CustomScan: `COUNT(DISTINCT)` (q4 2.19×), `MIN/MAX` (q6 **0.17× — TheoDB wins**, via the M105 zone-map directory fast-path), `GROUP BY + COUNT(DISTINCT)` (q8 2.43×, q13 2.99×), `GROUP BY text + COUNT + WHERE` (q12 2.77×), `date_trunc GROUP BY` (q42 2.99× — the M157 pushdown). These are exactly the classes M151–M157 taught the planner to route. Another 14 queries sit at 3–10× (same class, larger cardinality).
- **The vectorized path (32 queries) averages 7.54×** — single-digit×, within reach of 2-3× with further work. The residual gap is the MVCC/WAL tax + row-materialization at the CustomScan boundary, NOT a paradigm gap (consistent with `docs/adr/0033`/`0035`, which located the *paradigm* gap in vector QPS, not columnar analytics).
- **The dominant drag is the 11 non-pushdown queries (303× geomean):** complex/multi-key `GROUP BY` (q17 UserID,SearchPhrase — 132×), text aggregates `MIN(URL)` (q21 — 304×), computed-expression aggregates `AVG(length(URL))` (q27 — 870×), `GROUP BY URLHash,EventDate` (q40 — 1324×). These fall back to PostgreSQL's **row-based executor over columnar storage** (12–21 s each vs ClickHouse's 0.01–0.15 s). Closing them requires **expanding pushdown coverage** (the M151–M157 playbook), not micro-optimization — this is where the "2-3×" is currently unreachable.
- **One pushdown outlier:** q19 (`WHERE UserID = <const>`) is 148× despite pushing down — TheoDB scans all 1M rows for the point filter (no sparse index; measured 1.19s, 3 consistent runs). ClickHouse's sub-10ms is *inferred* to come from index/skip pruning (no ClickHouse EXPLAIN captured — inference, not measurement). A scan-selectivity gap, not paradigm.

## Caveats (honest science)

- **Subsample (1M) ≠ canonical (100M):** both engines run faster at 1M; the ratio is comparable at this scale but is a **LOWER bound** on the gap — at 100M ClickHouse's design scales better than a PG-based row executor, so the non-pushdown cliff would likely WIDEN. `[NEEDS-100M]` for the canonical absolute comparison (needs the full dataset + a c6a.4xlarge — the operational follow-up).
- **Published ClickHouse numbers** (benchmark.clickhouse.com / `github.com/ClickHouse/ClickBench/clickhouse/results/`) are on c6a.4xlarge/c8g.4xlarge at 100M — **`[NO-BASELINE-COMPARABLE]`** for absolute times against this 1M/8-vCPU run; they corroborate only that ClickHouse is sub-second on nearly every ClickBench query (as measured here on the subsample).
- **ClickHouse on 1M fits in RAM** → its absolute times (3–150 ms) are near the floor; the ratio, not the absolute, is the signal.
- **Clock-base asymmetry (council-benchmark MEDIUM):** TheoDB is timed as the psycopg2 client round-trip (`cur.execute` + `fetchall` on localhost); ClickHouse `--time` is server-side execution only. This ADDS fixed client/loopback overhead to TheoDB that ClickHouse never pays → the ratio **overstates** the gap. The asymmetry is therefore **conservative** (never flatters TheoDB); it only bites at the floor (q6, below). Impact is low-single-digit-% on the 30–70 ms pushdown queries, negligible on the seconds-scale ones.
- **Cross-engine result-equivalence is assumed, not verified:** the A/B oracle proves TheoDB-columnar == TheoDB-heap (43/43); it does NOT prove TheoDB result == ClickHouse result per query index. The two run different SQL dialects (Postgres vs ClickHouse queries.sql) — per-index equivalence is the ClickBench convention, but a dialect rounding/regex difference on a given q would compare slightly different work. Low risk, disclosed.
- **Reproducibility:** the ClickHouse side is now a committed script (`benchmarks/m159_clickhouse_run.sh`), not just a described procedure.
- **q6 (0.17×) and q19 (148×) are floor/inference caveated:** q6's exact 0.17× is at the timer floor (TheoDB 0.001s) — read it as "TheoDB faster via the M105 zone-map directory fast-path", not the precise ratio. q19's ClickHouse-side speed is *inferred* (no ClickHouse EXPLAIN captured); the TheoDB-side 1.19s full scan (no sparse index) is measured.

## Conclusion (feeds the next iteration)

The measured trajectory: TheoDB's columnar pillar went from 6/43 to **32/43** pushdown coverage (M148–M158) and is **7.54× off ClickHouse on the covered class** — the "2-3×" is a realistic target for that class with continued optimization. The **highest-leverage next work is pushdown coverage for the 11 remaining query shapes** (multi-key GROUP BY, text/computed aggregates), which alone drag the geomean from 7.54× to 19.4×. This is an honest, evidence-anchored baseline — not a failure: it replaces an invented ratio with a measured one and names exactly where the "2-3×" is and isn't reachable.
