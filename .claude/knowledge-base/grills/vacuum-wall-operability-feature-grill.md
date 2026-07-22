---
slug: vacuum-wall-operability
milestone_id: M116
generated_by: roadmap-feature
mode: batch-evidence-derived
status: completed
date: 2026-07-20
---

# Feature grill — M116 Operabilidade em escala — muro do VACUUM

> Batch/evidence-derived (user chose "DoD derived from evidence"). Answers grounded in
> the deep-view SOTA audit (`.claude/knowledge-base/audits/deep-view-sota-ai-native-2026-07-07.md`),
> `.claude/knowledge-base/backlog.md`, and the North-Star reposition ADRs (0033/0035/0036) —
> the same rigorous grounding the interactive grill seeks. User reviews the ROADMAP block.

## Q1 — What is this feature and why now?
Implementar a fase 1 do ADR-0017 (tombstone-in-place + compaction incremental) para remover o fold O(N) whole-index sob EXCLUSIVE (~86s@100k, ~14min@1M). Now: é o gate honesto de v1.0 (deep-view §P3) e pré-requisito da narrativa billion-scale do reposicionamento (ADR-0033).

## Q2 — Dependencies (must be [x] before start)
M48 (fold crash-safe/meta-pivot), M55 (decisão ADR-0017) — ambos [x].

## Q3 — Definition of done
(1) tombstone in-place; (2) compaction bounded c/ trigger; (3) crash-safe por harness check-crash; (4) MEDIDO stall@1M dentro do limite, sem regressão de recall.

## Q4 — Top 2 new risks
(1) tombstones acumulados derivam recall até compaction (precisa trigger medido); (2) crash-safety do fold incremental sob concorrência (meta-pivot+advisory lock).

## Out-of-scope cross-check
ROADMAP.md has no `### Explicitly out of scope` section (this roadmap version = M71–M115,
predates that template field) → cross-check vacuously satisfied, no overlap possible.
