---
slug: pushdown-coverage-slow-shapes
generated_by: roadmap-feature
date: 2026-07-27
status: completed
milestones_added: [M165, M166, M167]
---

# Feature grill — pushdown coverage for the slow/non-pushdown ClickBench shapes

## Decomposition decision (AskUserQuestion)

User chose **3 focused milestones** (Option A) after the shape classification, grounded in the fresh
`docs/benchmarks/clickbench-fresh-vs-clickhouse-2026-07-27.md` measurement. Each milestone matches the grain of the
existing M156/M157/M161 coverage milestones (one cohesive shape, one DoD, one release cut). Explicitly **excluded**:
q28/q39 (regex/CASE computed GROUP BY key — niche, high complexity) and q19 (`WHERE UserID = const` — a sparse-index
problem, a *different capability* from columnar pushdown; alone it barely moves the geomean).

| Milestone | Shape | Queries (measured ratio) |
|---|---|---|
| M165 | multi-key GROUP BY | q17 (115×), q34 (152×) |
| M166 | string aggregates (MIN/MAX text) + wide SUM(expr) | q21 (300×), q22 (260×), q27 (817×), q29 (567×) |
| M167 | projection top-k (SELECT cols WHERE … ORDER BY … LIMIT) | q23 (110×), q24 (73×), q25 (113×), q26 (132×) |

## Grill answers (grounded in the measured benchmark — not speculative)

**Q1 — What is this and why NOW?** The fresh 2026-07-27 same-box ClickBench measurement (post-M160/M161, v0.155.0)
showed the gap vs ClickHouse halved (19.4×→9.95× overall; 7.54×→4.53× on the covered class), but 8 non-pushdown
row-executor queries at ~25–35 s each (312× geomean) plus a few routed-but-slow shapes now DOMINATE the geomean drag.
The data names exactly which shapes and their ratios — this is measured leverage, not a guess.

**Q2 — Dependencies?** All prerequisite coverage/rigor milestones are `[x]`: M158 (late-mat top-k), M160 (zero-copy
decode), M161 (expr routing), M163 (type-coverage harness), M164 (hardened harness), M154 (count-distinct), M156
(text WHERE / collation). M165 → M166 (M166 builds on the multi-key GROUP BY coverage).

**Q3 — DoD (per milestone, in ROADMAP.md):** each names the exact queries that must flip non-pushdown→pushdown with
`Custom Scan` in EXPLAIN + A/B byte-identical (`diverged=0`) measured by the **M164-hardened harness** (so a declined
arm cannot green-pass as a trivial `diverged=0`), plus the collation-determinism / overflow guards the earlier
milestones established, plus a type-coverage A/B case (M163) and a CHANGELOG entry.

**Q4 — Top NEW risks (per milestone):** (M165) text component of a multi-key GROUP BY must decline non-deterministic
collation; constant key may be folded by the planner. (M166) MIN/MAX(text) ordering vs PG collation must decline
non-deterministic; SUM over float/numeric must honor IEEE/overflow (M154/M163). (M167) ORDER BY text collation must
decline non-C/non-deterministic (M158 HIGH); the WHERE filter must route too (M156) or the top-k isn't comparable.

## SOTA delta

None. These milestones optimize the existing **own-code** `theodb_columnar` scan/planner — no new reference peers
needed. References are internal: the fresh benchmark artifact + the M153/M154/M156/M158/M161 memories + the real code
(`columnar_agg.rs`, `df_executor.rs`, `columnar_project.rs`).

## Out-of-scope cross-check

No conflict with `## Fora de escopo do v2` (which bars rewriting the PG engine / generic HTTP/serde/crypto/parser).
These milestones continue optimizing the shipped own-code columnar scan — the same class of work M148–M161 already did.
