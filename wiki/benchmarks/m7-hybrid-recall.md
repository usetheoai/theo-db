---
type: Measurement
title: m7 — recall da busca híbrida (full-text + vetorial + RRF)
description: A primeira medição da fusão, com metodologia BEIR sobre um fixture determinístico; a híbrida empata com a perna vetorial, o que é resultado honesto e não vitória.
resource: git:f7c7b93:docs/benchmarks/m7-hybrid-recall.md
tags: [benchmark, busca-hibrida, rrf, ndcg, beir, m7]
milestone: M7-S1
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m7hyb
    resource: git:f7c7b93:docs/benchmarks/m7-hybrid-recall.md
    title: M7-S1 — Hybrid search recall, measured
    last_modified: 2026-06-28
---

A primeira medição da [busca híbrida](/features/06-busca-hibrida.md), com três recuperadores avaliados
pela metodologia [BEIR](/technologies/beir.md) — nDCG@10 primária, Recall@100 secundária.

# Resultado

| Recuperador | nDCG@10 | Recall@100 |
|---|---|---|
| vetorial | 0,8311 | 1,0000 |
| full-text nativo | 0,5143 | 0,3125 |
| **híbrido ([RRF](/technologies/rrf.md), k=60)** | **0,8311** | **1,0000** |

**A híbrida empata com a perna vetorial** neste fixture — ela não perde, mas também não ganha. Reportar
esse empate como empate, em vez de enquadrá-lo como sucesso da fusão, é a postura correta.

# O fixture, e por que ele é declaradamente limitado

Corpus sintético rotulado à mão — 12 documentos, 4 queries, relevância graduada sobre dois tópicos —, com
embeddings de um embedder determinístico por hashing de features, de dimensão 16, **sem dependência de
endpoint**.

**Ele existe para tornar a avaliação reproduzível em CI, e NÃO é benchmark decision-grade** de qualidade
híbrida no mundo real. Doze documentos não distinguem recuperadores com significância.

# O que a linha de medição da fusão mostrou depois

Esta primeira medição é o começo de uma sequência que ficou progressivamente mais rigorosa, e cujo
resultado agregado é sóbrio:

- [m123](/benchmarks/m123-hybrid-significance.md) trouxe **teste de significância pareado**, com a lição
  de que coeficiente de variação não é significância;
- [m125](/benchmarks/m125-hybrid-lexical.md) mediu o eixo lexical;
- [m138](/benchmarks/m138-bm25-fusion.md) deu **honest-negative** para trocar a perna lexical na fusão.

Ou seja: **a fusão é útil, mas os ganhos que se atribuem a ela precisam de medição pareada para
sobreviver** — e vários não sobreviveram.
