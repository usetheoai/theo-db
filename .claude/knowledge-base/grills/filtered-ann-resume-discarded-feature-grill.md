---
slug: filtered-ann-resume-discarded
milestone_id: M118
generated_by: roadmap-feature
mode: batch-evidence-derived
status: completed
date: 2026-07-20
---

# Feature grill — M118 Filtered ANN resume-from-discarded

> Batch/evidence-derived (user chose "DoD derived from evidence"). Answers grounded in
> the deep-view SOTA audit (`.claude/knowledge-base/audits/deep-view-sota-ai-native-2026-07-07.md`),
> `.claude/knowledge-base/backlog.md`, and the North-Star reposition ADRs (0033/0035/0036) —
> the same rigorous grounding the interactive grill seeks. User reviews the ROADMAP block.

## Q1 — What is this feature and why now?
Iterative scan resumível do discarded set (não re-buscar o grafo) p/ fechar ~3× vs pgvector 0.8 no caso RAG WHERE tenant ORDER BY emb. Now: é o caso RAG real; recall já em paridade, pagamos QPS (deep-view §P4 + backlog M52).

## Q2 — Dependencies (must be [x] before start)
M52 (filtered ANN/iterative scan) — [x].

## Q3 — Definition of done
(1) amgettuple mantém estado resumível entre chamadas; (2) recall ≥ atual, terminação provável; (3) MEDIDO multi-seed 1%/10%/50% fecha gap a recall casado.

## Q4 — Top 2 new risks
(1) estado resumível + MVCC/rescan sem skip/dup (self-join); (2) discarded set em memória bounded.

## Out-of-scope cross-check
ROADMAP.md has no `### Explicitly out of scope` section (this roadmap version = M71–M115,
predates that template field) → cross-check vacuously satisfied, no overlap possible.
