---
slug: coverage-perf-bench-abc
generated_by: roadmap-feature
date: 2026-07-25
status: completed
milestones_added: [M157, M158, M159]
requirements_source: evidence-based analysis (M148/M152/M155 measured) accepted by owner ("Crie os milestones para A - B - C")
---

# Grill log — batch amendment M157/M158/M159 (candidates A/B/C)

The 4-question grill was NOT run interactively: the requirements (what/why/deps/DoD/risks) were already resolved
from measured evidence and presented to the owner, who accepted the three candidates and directed their creation.
This log records the derived answers for audit (95%-confidence: requirements clear, no interrogation theater).

## M157 (A) — coverage: expr-group / HAVING
- **What/why now:** the largest remaining ClickBench coverage gap after M156 (text-WHERE). GROUP BY expression keys
  (`date_trunc`/`EXTRACT`/`CASE`) + HAVING predicates — the CustomScan today only accepts `Var` group keys and has no
  HAVING. Incremental, low-risk, same mechanic as M156.
- **Deps:** M156 [x]. **DoD:** coverage > 31, diverged=0 both regimes, fail-closed guards, date_trunc timezone matches
  PG or declines, CHANGELOG. **New risks:** date_trunc timezone divergence; CASE type unification; scope creep.

## M158 (B) — late materialization (perf)
- **What/why now:** the highest-value performance lever. M148 measured the dominant scan cost as row-by-row
  materialization (~80%); no coverage milestone touches it. This is the only path that changes the *time* of the
  already-columnar path — the structural ceiling for the owner's "2-3× vs ClickHouse" goal.
- **Deps:** M156 [x]. **DoD:** MEASURED flamegraph before/after (materialization share drops), diverged=0, MVCC
  preserved, honest-negative accepted if no measured win, CHANGELOG. **New risks:** deferred-materialization
  correctness under MVCC/NULL; measured win may not materialize; essential vs accidental complexity.

## M159 (C) — measure gap vs ClickHouse (Passo 0)
- **What/why now:** measurement-first anchor for the owner's "2-3×" target. No ClickHouse baseline exists in the repo;
  any ratio would be fabricated (rule 5). Produces the honest per-query gap vs published ClickHouse numbers.
- **Deps:** M157 [ ], M158 [ ] (measure after coverage + perf maxed; a current-state baseline can be measured anytime).
  **DoD:** canonical ClickBench (or documented deviation) + per-query comparison to published ClickHouse numbers,
  honest verdict on 2-3× feasibility per query-class, NO fabricated ratio, artifact + CHANGELOG. **New risks:**
  temptation to invent a ratio without a reproducible baseline (FORBIDDEN); incomparable baselines; reading a
  structural honest-negative as failure.

## Ordering
Owner directed order A→B→C. Each depends on M156 (A,B) / M157+M158 (C), so cycle-roadmap selects M157→M158→M159 by
lowest eligible ID = A→B→C. NOTE: B (M158) is the highest-value lever for the 2-3× goal; the owner may run
`/auto-plan M158` directly to prioritize it ahead of A.
