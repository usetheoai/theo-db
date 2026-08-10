---
type: Measurement
title: m7 — BM25 contra ts_rank_cd em qualidade lexical
description: Mede nDCG@10 de 0,95 para BM25 contra 0,51 do full-text nativo, num fixture sintético declaradamente não decision-grade.
resource: git:f7c7b93:docs/benchmarks/m7-bm25-vs-tsrank.md
tags: [benchmark, bm25, lexical, ndcg, beir, m7]
milestone: M7-S2
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m7bm25
    resource: git:f7c7b93:docs/benchmarks/m7-bm25-vs-tsrank.md
    title: M7-S2 — BM25 vs ts_rank_cd, measured
    last_modified: 2026-06-28
---

O gate measurement-first que informaria se a peça [BM25](/technologies/bm25.md) permissiva identificada
no [ADR 0003](/decisions/0003-permissive-bm25-pg-textsearch.md) deveria ser adotada. **Nenhuma peça
embarca pela força de uma especificação; embarca pela força desta medição.**

# Resultado

Quatro recuperadores, com metodologia [BEIR](/technologies/beir.md) — nDCG@10 como métrica primária e
Recall@100 como secundária:

| Recuperador | nDCG@10 | Recall@100 |
|---|---|---|
| vetorial (cosseno) | 0,8311 | 1,0000 |
| **full-text nativo (`ts_rank_cd`)** | **0,5143** | **0,3125** |
| **BM25** | **0,9546** | **1,0000** |
| híbrido (RRF de vetorial + nativo) | 0,8311 | 1,0000 |

O BM25 domina em qualidade lexical, e o `ts_rank_cd` — que era a perna embarcada — fica bem atrás, com
recall de apenas 0,31.

# A ressalva que limita a força do resultado

O corpus é um **fixture sintético rotulado à mão**, com 12 documentos e 4 queries, e embeddings gerados
por um embedder determinístico **sem endpoint** — para que a corrida seja reproduzível em CI, offline.

**Ele existe para tornar a avaliação reproduzível, e é declaradamente NÃO um benchmark decision-grade de
qualidade do mundo real.**

Registrar isso é o que impede o número de 0,95 de virar claim de marketing.

# O que aconteceu com esta linha

O [ADR 0013](/decisions/0013-v1-legacy-columnar-bm25-scope.md) usou este resultado para **manter** o
pilar BM25 — "descartá-lo jogaria fora um ganho medido". A peça externa acabou substituída por
[motor próprio](/features/18-motor-lexical-bm25.md)
([ADR 0054](/decisions/0054-m140-3-bm25-supersede-textsearch.md)).

E a conclusão final é mais sutil que este número isolado sugere: em corpora reais, o motor próprio ganha
por **margem modesta** em lexical puro, e **na fusão RRF não há ganho** —
[m138](/benchmarks/m138-bm25-fusion.md). **A vantagem lexical isolada não sobrevive à fusão**, que é o
motivo de a perna embarcada continuar sendo a nativa.
