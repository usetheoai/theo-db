---
type: Measurement
title: m53 — híbrida em BEIR real: a fusão iguala, não supera
description: A primeira medição decision-grade da híbrida, com dataset e embedder reais; e a que desmonta um gap aparente de 9,8× explicando que ele conflaciona qualidade de ranker com tamanho do conjunto de candidatos.
resource: git:f7c7b93:docs/benchmarks/m53-hybrid-beir.md
tags: [benchmark, beir, busca-hibrida, rrf, bm25, decision-grade, m53]
dataset: BEIR scifact
milestone: M53
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m53
    resource: git:f7c7b93:docs/benchmarks/m53-hybrid-beir.md
    title: M53 — Híbrida de verdade, BEIR real
    last_modified: 2026-07-07
---

**A medição decision-grade** que substituiu o fixture sintético do [m7](/benchmarks/m7-hybrid-recall.md):
dataset [BEIR](/technologies/beir.md) real com 5.183 documentos e 300 queries, embeddings de API real, 3
runs.

# Resultado

| Recuperador | nDCG@10 | Recall@100 |
|---|---|---|
| **híbrido (RRF) — o caminho do produto** | **0,7337** | **0,9733** |
| vetorial | 0,7296 | 0,9733 |
| BM25 | 0,6881 | 0,9182 |
| full-text nativo (**a perna embarcada**) | **0,0703** | 0,0694 |

# O veredito, formulado com cuidado

**A fusão IGUALA o vetorial** — paridade em Recall@100, com uma vantagem marginal de +0,004 em nDCG@10
que é **explicitamente marcada como não testada para significância** entre queries.

O critério — "a híbrida não regride contra vetorial e o claim tem artefato" — é cumprido. **A fusão NÃO
é declarada superior.** Um delta de 0,004 sem teste de significância não é ganho, e o documento se recusa
a tratá-lo como tal.

# O desmonte do número mais chamativo

O BM25 mede 0,6881 contra 0,0703 do full-text nativo — um gap aparente de **~9,8×**. E o próprio
documento **desmonta a leitura fácil**:

> ⚠️ esse gap conflaciona **qualidade de ranker** com **tamanho do conjunto de candidatos** — o operador
> de match do full-text **descarta ~93% dos relevantes** antes de qualquer ranqueamento.

Ou seja: **a maior parte daquele fator não é o ranker ser pior, é ele nem ver os documentos.** Publicar
9,8× sem essa explicação seria tecnicamente verdadeiro e substancialmente enganoso.

# Papel

Esta é a medição que **executa o gate de adoção de BM25** que o
[ADR 0013](/decisions/0013-v1-legacy-columnar-bm25-scope.md) deixara pendente, e fecha um claim de recall
que estava aberto **sem artefato decision-grade**.

A continuação — e o honest-negative de que trocar a perna lexical **na fusão** não ganha — está em
[m138](/benchmarks/m138-bm25-fusion.md).
