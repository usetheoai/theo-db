---
slug: official-db-benchmark-harness
version: 1.0
owner: paulohenriquevn
created_at: 2026-07-20
cycle: discover
---

# Discovery Plan: Official DB benchmark harness (replace the bespoke harness, all four pillars)

## Context

TheoDB has ~40 bespoke benchmark scripts (`benchmarks/run_m*.py`, `bench_*.py`, `*_ab.py`) + `theodb_bench/` and
190 artifacts under `docs/benchmarks/`. They are **self-authored**: heavy on the vector pillar (798 pgvector / 357
ScaNN references, 135 BEIR) but with our OWN protocol, and near-zero standard coverage on columnar/OLAP (ClickBench
mentioned 3×, no TPC-H/DS), OLTP (pgbench 2×, no TPC-C), and HTAP (no CH-benCHmark). The `/analysis`
(`knowledge-base/audits/2026-07-20-analysis.md`) rated the trajectory ON_TRACK_WITH_RISKS ~80/100 with the dominant
risk being external credibility of performance claims. `rules/public-copy.md § 4` requires a reproducible +
third-party-reproducible artifact for any comparative claim — which a **self-authored** harness cannot satisfy.

The owner decided (2026-07-20): **replace** the bespoke harness with the benchmark(s) **officially used by
database-engineering teams**, across **all four pillars**, using **real up-to-date datasets**. This discovery
selects the canonical benchmark per pillar and the exact protocol to run each against a PostgreSQL-wire-compatible
engine, so the resulting blueprint can seed a new roadmap program (M127 vector, M128 columnar, M129 OLTP, M130
HTAP). This plan asks; `/discover-execute` answers.

## Objective

Produce a blueprint that, for each of the four pillars, **names the canonical official benchmark**, its exact run
protocol against a PG-wire-compatible engine, its current public leaderboard/SOTA numbers (source URL + date), its
dataset license vs the D1 permissive gate, and the honest expected outcome for TheoDB's positioning — AND answers
the critical replacement question (do the official tools cover paired significance + byte-identical regression;
what does "replacing" drop). Success = every one of the 11 research questions answered with ≥2 primary web sources
(REGRA MÁXIMA), all four coverage corners populated, and the replacement-risk question resolved with a named
verdict.

## In-Scope / Out-of-Scope

### Reference projects (already cloned — read directly)

- `knowledge-base/references/pgvector/` — the permissive vector-pillar peer; **in scope:** how it is benchmarked
  by the field (its README benchmark section + any `test/`/`bench` harness). **Out of scope:** its C index
  internals (covered by prior discovery).
- `knowledge-base/references/pgvectorscale/` — the Rust pgrx AM peer; **in scope:** its published
  StreamingDiskANN benchmark methodology + dataset choices. **Out of scope:** its DiskANN algorithm internals.

### Web sources (WebFetch at execute-time, `rules/discover-web-allowlist.txt` + REGRA MÁXIMA)

- **In scope:** the official benchmark repos + leaderboards — ann-benchmarks (github.com/erikbern +
  `*.github.io`), VectorDBBench (github.com/zilliztech), big-ann-benchmarks (github.io), ClickBench
  (github.com/ClickHouse/ClickBench + benchmark.clickhouse.com), TPC specs (tpc.org), HammerDB
  (github.com/TPC-Council/HammerDB), BenchBase/CH-benCHmark (github.com/cmu-db/benchbase), and the primary papers
  (ClickBench/CH-benCHmark on vldb.org/cidrdb.org; BigANN on proceedings.neurips.cc).
- **Out of scope:** vendor marketing blogs, non-primary "top N vector DB" listicles, any source outside the
  allowlist (R5). Commercial-only benchmarks requiring a paid TPC license to obtain results.

### Explicitly deferred (ADR-D1)

- **Actually running** any benchmark — this is discovery (a document), not execution. No numbers are produced
  here; only the protocol + published numbers are captured.

## ADRs (how to investigate)

### D1 — discovery only, no benchmark execution

**Decision:** capture protocols + published numbers + licenses; do NOT run any benchmark in this cycle.
**Rationale:** running is implementation (the M127–M130 milestones). Discovery de-risks the bets (which tool,
which dataset, which license) before any harness code. Running now would violate `cycle-discover` (output is a
document, never code).

### D2 — web-evidenced, ≥2 primary sources per canonical choice (discover-phd-rigor R0/R1/R2/R3)

**Decision:** every canonical-benchmark selection cites ≥2 primary web sources (the official repo + the paper or
the leaderboard); every perf number carries methodology + source URL + date, or the literal marker
`UNBENCHMARKED`. **Rationale:** `rules/discover-phd-rigor.md` R0 (mandatory web search) + R2 (≥2 sources) + R3
(benchmark evidence). Internal knowledge is insufficient for a "which tool is canonical NOW" question.

### D3 — allowlist domains added at execute-time, CHANGELOG-tracked

**Decision:** `clickhouse.com` / `benchmark.clickhouse.com`, `tpc.org`, `zilliz.com`, `big-ann-benchmarks.com`
are not yet in `rules/discover-web-allowlist.txt`; `/discover-execute` adds them (narrow, primary leaderboard
sources) with a CHANGELOG entry, per the allowlist policy. **Rationale:** these host the primary leaderboards; the
allowlist is deliberately curated and additions are a per-project tracked change.

## Research Questions

| # | Question | Corner | Sources | Fase A (broad — WebSearch/grep map) | Fase B (deep — WebFetch/Read) | Expected answer shape |
|---|---|---|---|---|---|---|
| Q1 | Which benchmark does the field treat as canonical for recall×QPS ANN on a PG-compatible engine — `ann-benchmarks` vs `VectorDBBench` vs `big-ann-benchmarks` — and its EXACT protocol (datasets, recall@k, "QPS at fixed recall", build-vs-query split, single vs concurrent)? Which does pgvector/pgvectorscale/AlloyDB report on? | techniques | `knowledge-base/references/pgvector/`, ann-benchmarks + VectorDBBench + big-ann repos | WebSearch which ANN benchmark the field cites as canonical (2024-2026); map the candidate repos + leaderboards | WebFetch github.com/erikbern/ann-benchmarks + github.com/zilliztech/VectorDBBench + big-ann-benchmarks.github.io; Read `knowledge-base/references/pgvector` README bench section | A named canonical choice + its protocol table + who reports on it |
| Q2 | What is ClickBench's exact protocol (43 queries over `hits`, load-then-3-runs-report-min, cold vs hot, single-node), what does a PG-compatible engine submit to enter its public leaderboard, and where do TPC-H/TPC-DS fit as the heavier complement? | techniques | github.com/ClickHouse/ClickBench + benchmark.clickhouse.com + ClickBench paper | WebSearch ClickBench protocol + leaderboard + the ClickBench VLDB paper | WebFetch github.com/ClickHouse/ClickBench + benchmark.clickhouse.com + the paper (vldb.org) | ClickBench protocol steps + PG-entry contract + the TPC-H/DS role |
| Q3 | What is the canonical PostgreSQL OLTP protocol — native `pgbench` (TPC-B-like) vs HammerDB TPC-C — the transaction mix, warehouse/scale factor, metric (TPS/tpmC/NOPM), and the standard rampup + steady-state duration? | techniques | www.postgresql.org pgbench + github.com/TPC-Council/HammerDB + tpc.org | WebSearch pgbench-vs-HammerDB canonical OLTP practice for PG | WebFetch www.postgresql.org/docs pgbench + HammerDB docs + the TPC-C spec (tpc.org) | OLTP protocol: mix, scale, metric, duration |
| Q4 | What is CH-benCHmark's protocol (TPC-C + TPC-H fused on one schema, concurrent OLTP + OLAP streams), what metric(s) it reports, and which runner (BenchBase/OLTP-Bench) is the current standard? | techniques | CH-benCHmark paper (cidrdb/vldb) + github.com/cmu-db/benchbase | WebSearch CH-benCHmark + the current HTAP-benchmark runner | WebFetch the CH-benCHmark paper (www.cidrdb.org/vldb.org) + the BenchBase repo | HTAP protocol + metric + runner |
| Q5 | On a current apples-to-apples ann-benchmarks/VectorDBBench run, what are the PUBLISHED numbers for pgvector/pgvectorscale vs ScaNN/AlloyDB, and does TheoDB's recall-parity + billion-scale positioning survive (QPS gap non-achievable per `docs/adr/0033`/`0035`/`0036` — confirm a standard run REPRODUCES it)? | techniques | `knowledge-base/references/pgvectorscale/`, ann-benchmarks leaderboard (*.github.io) + cloud.google.com AlloyDB | WebSearch current pgvector/pgvectorscale-vs-ScaNN published recall×QPS | WebFetch the ann-benchmarks leaderboard + pgvectorscale bench numbers + AlloyDB/ScaNN pages; Read `knowledge-base/references/pgvectorscale` bench docs | Published recall×QPS numbers + a verdict: parity survives, QPS gap reproduces |
| Q6 | What must a new database implement to be a first-class entrant in ann-benchmarks (module/Docker contract + `results/*.json`) AND ClickBench (the per-db dir, `benchmark.sh`, `create.sql`/`queries.sql`, the leaderboard PR flow)? | tools | github.com/erikbern/ann-benchmarks + github.com/ClickHouse/ClickBench | WebSearch "how to add a database to ann-benchmarks / ClickBench" | WebFetch the ann-benchmarks add-an-algorithm docs + a ClickBench per-db directory | The exact files/contract WE implement to enter each |
| Q7 | What are the concrete current run commands + reproducibility knobs for `pgbench` against PG17, HammerDB TPC-C, and BenchBase CH-benCHmark (Docker images, config files, seed control)? | tools | www.postgresql.org pgbench + HammerDB + github.com/cmu-db/benchbase | WebSearch current run recipes for each runner against PG17 | WebFetch the run docs of pgbench + HammerDB + BenchBase | Concrete run commands + reproducibility knobs |
| Q8 | For each canonical benchmark, what is the dataset license vs the D1 permissive gate (ClickBench `hits`; ann-benchmarks SIFT/GIST/GloVe/deep-image; VectorDBBench Cohere/OpenAI sets; TPC-H/DS `dbgen`/`dsdgen` generated data) — redistributable, CI-internal-only, or generated? | deps | each dataset license page/README; `docs/adr/0006` (D1) | WebSearch each dataset's license terms | WebFetch each dataset license page; Read `docs/adr/0006` (the D1 permissive gate) | Per-dataset: redistributable / CI-internal / generated |
| Q9 | What toolchain does each harness require (ann-benchmarks Python stack + versions; ClickBench per-db shell/SQL; HammerDB Tcl; BenchBase Java/Maven), and is any a maintenance liability? | deps | each harness repo's deps manifest | WebSearch each harness's runtime/deps + release cadence | WebFetch each repo's deps manifest (requirements.txt / pom.xml / scripts) | Toolchain + versions + a maintenance-liability note |
| Q10 | How does each official benchmark guarantee reproducibility + result CORRECTNESS (ann-benchmarks recall vs ground-truth; ClickBench result checking; TPC-C consistency checks) — what prevents a wrong-but-fast result from scoring? | tests | each benchmark's result-verification docs/code | WebSearch each benchmark's correctness/verification mechanism | WebFetch each benchmark's result-checking docs/code | How correctness/reproducibility is enforced per benchmark |
| Q11 | Do these official tools provide (a) paired significance testing (permutation/bootstrap, à la `benchmarks/theodb_bench/significance.py`, M123/M125) and (b) byte-identical regression A/B (à la M114/M126 same-index A/B)? If NOT, what does replacing the bespoke harness DROP, and is a thin retained methodology layer warranted? | tests | each repo (search "significance"/"regression"/"variance"); `benchmarks/theodb_bench/significance.py` | WebSearch each tool for significance/variance/regression support | WebFetch each repo's methodology docs; Read our `benchmarks/theodb_bench/significance.py` | Yes/No per tool + a named RETAIN/DROP verdict on what "replace" drops |

## Coverage Matrix

| Corner | Questions | Covered |
|---|---|---|
| Techniques | Q1, Q2, Q3, Q4, Q5 | Covered (≥2, R4) |
| Tools | Q6, Q7 | Covered |
| Dependencies | Q8, Q9 | Covered |
| Integration tests | Q10, Q11 | Covered |

11 questions — within the discover-phd-rigor frontier budget (6–14, max 5/corner). No corner empty; no
ADR-deferred corner. Q11 is the critical replacement-risk question (RETAIN/DROP verdict on significance +
regression).

## Halt-loop Checkpoints (for /discover-execute)

A research question may be marked `done` only when: (a) it has ≥2 primary web citations (or a Read of a cloned
ref) resolving on fetch; (b) every perf number carries methodology + source URL + date OR the literal
`UNBENCHMARKED`; (c) for Q10, an explicit `RETAIN`/`DROP` verdict on the significance + regression layers is
written. A question with a paywalled/unreachable primary source is marked `blocked` with the reason (never padded
with a fabricated answer — Rule 3).

## Acceptance Criteria

- All 11 questions answered or explicitly `blocked` with reason.
- Every canonical-benchmark selection cites ≥2 primary sources (ADR-D2 / R2).
- All four coverage corners populated in the resulting blueprint.
- Q10 resolved with a named verdict (does "replace" drop significance/regression; retain a thin layer or not).
- Q11 states, with sources, the honest expected outcome (parity survives; QPS gap reproduces).
- No fabricated citation (`knowledge-base/references/` paths resolve; web URLs are on the allowlist).

## Global Definition of Done

Feeds `/discover-edge-cases` → `/discover-plan-confidence` (this plan) → `/discover-execute` (the blueprint) →
`/discover-confidence`. Thresholds + golden rule: `rules/discover-plan-golden-rule.md`,
`rules/discover-blueprint-golden-rule.md`, `rules/discover-phd-rigor.md`. The blueprint then seeds the
roadmap-program milestones M127–M130 (one pillar each) via `/roadmap-feature`.
