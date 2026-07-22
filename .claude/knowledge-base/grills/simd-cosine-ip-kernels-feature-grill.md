---
slug: simd-cosine-ip-kernels
milestone_id: M117
generated_by: roadmap-feature
mode: batch-evidence-derived
status: completed
date: 2026-07-20
---

# Feature grill — M117 SIMD cosine/IP

> Batch/evidence-derived (user chose "DoD derived from evidence"). Answers grounded in
> the deep-view SOTA audit (`.claude/knowledge-base/audits/deep-view-sota-ai-native-2026-07-07.md`),
> `.claude/knowledge-base/backlog.md`, and the North-Star reposition ADRs (0033/0035/0036) —
> the same rigorous grounding the interactive grill seeks. User reviews the ROADMAP block.

## Q1 — What is this feature and why now?
AVX2+FMA para cosine/IP (hoje escalar; só L2 tem SIMD). Now: quick-win no hot path dos embeddings reais (cosine/IP), eixo que o M50 aponta como teto (deep-view §P2 + backlog).

## Q2 — Dependencies (must be [x] before start)
M31b (SIMD L2) — [x].

## Q3 — Definition of done
(1) cosine/IP com AVX2+FMA + dispatch runtime + fallback escalar; (2) recall-neutro por ablação mesmo-índice; (3) MEDIDO microbench same-graph (nunca cross-box).

## Q4 — Top 2 new risks
(1) dispatch runtime precisa fallback escalar testado; (2) medir kernel exige ablação mesmo-índice (cross-box confunde).

## Out-of-scope cross-check
ROADMAP.md has no `### Explicitly out of scope` section (this roadmap version = M71–M115,
predates that template field) → cross-check vacuously satisfied, no overlap possible.
