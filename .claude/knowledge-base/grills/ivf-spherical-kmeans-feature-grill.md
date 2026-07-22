---
slug: ivf-spherical-kmeans
milestone_id: M121
generated_by: roadmap-feature
mode: batch-evidence-derived
status: completed
date: 2026-07-20
---

# Feature grill — M121 IVF cosine/ip spherical k-means

> Batch/evidence-derived. Answers grounded in `.claude/knowledge-base/backlog.md` + shipped code (verified
> against the current tree post-M118, not a stale audit). Out-of-scope cross-check: ROADMAP.md has no
> `### Explicitly out of scope` section → vacuously satisfied.

## Q1 — What / why now
M49 (IVF cosine/ip) [x]

## Q2 — Dependencies (must be [x])
Recall quality no eixo de correção (não QPS): IVF cosine tem o maior gap de recall conhecido (0.83-0.89 vs HNSW 1.0, backlog M49 HIGH-2); centroides arithmetic-mean derivam da esfera. Now: own-code permissivo, não esbarra no teto estrutural de QPS do M118.

## Q3 — Definition of done
(1) k-means cosine/ip normaliza centroide no update (spherical), L2 byte-idêntico; (2) MEDIDO recall sobe a QPS casado; (3) gate honesto: lift insuficiente -> reverter + honest-negative.

## Q4 — Top 2 new risks
(1) convergência mais lenta (capar iterações, reusar cap M88); (2) lift marginal -> gate por benchmark, aceitar honest-negative.
