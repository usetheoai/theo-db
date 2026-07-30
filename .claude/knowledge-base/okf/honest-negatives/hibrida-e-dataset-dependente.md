---
type: Honest Negative
title: A superioridade da busca híbrida sobre vector-only é dataset-dependente — e os dois resultados se explicam
description: SciFact: paridade sem poder (p=0,253, 296/300 empates). NFCorpus: significativa (p=0,0099). A diferença é a força da perna lexical, não do método.
resource: docs/benchmarks/m125-hybrid-lexical.md
tags: [retrieval, hibrida, rrf, veredito]
timestamp: 2026-07-30T00:00:00Z
---

# A superioridade da híbrida sobre vector-only é **dataset-dependente** — e os dois resultados se explicam

## Os dois resultados

| Corpus | Δ̄ nDCG@10 | p | W/L/T | leitura |
|---|---|---|---|---|
| **SciFact** (M123) | +0,0041 | **0,253** | 3 / 1 / **296** | **paridade** — e sem poder |
| **NFCorpus** (M125) | +0,0105 | **0,0099** | 55 / 49 / 219 | **significativa**, CI exclui 0 |

## Por que não se contradizem

A **perna lexical** tem forças muito diferentes nos dois corpora:

| | nDCG@10 da perna `fts` isolada |
|---|---|
| SciFact | **0,0703** — praticamente morta |
| NFCorpus | **0,2076** — viva |

No SciFact a fusão não tinha o que fundir: **296 de 300 pares empatados**, e `Recall@100` idêntico (0,9733) nos
dois braços — concordância real, não artefato. Com 4 pares informativos, o p exato combinatório é 4/16 = 0,25.

> Um teste "não significativo" com 4 pares informativos é um fato sobre o **experimento**, não sobre o efeito.

## Duas disciplinas que o par registra

1. **Endpoint pré-declarado.** O M123 deliberadamente **não** rodou o NFCorpus no mesmo milestone, *"to avoid
   dataset-shopping for a significant result"* (ADR M123-2). O follow-up virou milestone próprio.
2. **Magnitude honesta.** O +0,0105 é *"significativo mas PEQUENO (219/323 empates) — um lift medido, não uma
   transformação"*. E o artefato veta o claim universal: *"Never claim hybrid beats dense unqualified — BEIR
   shows dense wins on FiQA/ArguAna"*.

## Relacionados

- [honest-negative/bm25-na-fusao-rrf](bm25-na-fusao-rrf.md) — por que a perna mais forte não vence a fusão
- [failure-mode/conflacao-ranker-com-candidate-set](../failure-modes/conflacao-ranker-com-candidate-set.md)
- [failure-mode/estatistica-que-nao-sustenta-a-alegacao](../failure-modes/estatistica-que-nao-sustenta-a-alegacao.md)
