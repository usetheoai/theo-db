---
slug: ai-native-depth-rerank-chunking
milestone_id: M119
generated_by: roadmap-feature
mode: batch-evidence-derived
status: completed
date: 2026-07-20
---

# Feature grill — M119 AI-native depth

> Batch/evidence-derived (user chose "DoD derived from evidence"). Answers grounded in
> the deep-view SOTA audit (`.claude/knowledge-base/audits/deep-view-sota-ai-native-2026-07-07.md`),
> `.claude/knowledge-base/backlog.md`, and the North-Star reposition ADRs (0033/0035/0036) —
> the same rigorous grounding the interactive grill seeks. User reviews the ROADMAP block.

## Q1 — What is this feature and why now?
Cross-encoder re-rank opcional no hybrid + chunking recursivo separator-aware. Now: pós-reposição (ADR-0033) AI-native é eixo diferenciador; hoje igualamos pgai/Supabase mas não superamos (deep-view §P6 + backlog M54).

## Q2 — Dependencies (must be [x] before start)
M53 (hybrid/RRF), M54 (vectorizer/chunking) — ambos [x].

## Q3 — Definition of done
(1) re-rank cross-encoder opt-in via HTTP existente, bounded; (2) chunk_text recursivo separator-aware; (3) MEDIDO lift nDCG@10 BEIR c/ significância.

## Q4 — Top 2 new risks
(1) cross-encoder adiciona latência/I/O → opt-in bounded, não regride default; (2) chunking recursivo muda embeddings → migração/reindex documentado.

## Out-of-scope cross-check
ROADMAP.md has no `### Explicitly out of scope` section (this roadmap version = M71–M115,
predates that template field) → cross-check vacuously satisfied, no overlap possible.
