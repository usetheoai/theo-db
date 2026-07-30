---
type: Honest Negative
title: ai.rerank PIOROU o nDCG@10 em 3,8 pt e custou 1,95 s por query
description: O reranker de 2ª ordem só reordena — Recall@50 idêntico nos dois braços — e num corpus onde a perna densa já é forte ele tem mais a perder que a ganhar.
resource: docs/benchmarks/m65-rerank.json
tags: [retrieval, rerank, veredito, custo]
timestamp: 2026-07-30T00:00:00Z
---

# Um reranker LLM de segunda ordem **piorou** o nDCG@10 em 3,8 pt — e custou 1,95 s por query

## O número (M65, SciFact, top_k=50, 100 queries, 3 runs)

| | sem rerank (A) | **com `ai.rerank`** (B) | Δ |
|---|---|---|---|
| **nDCG@10** | 0,7327 | **0,6947** | **−0,0380** |
| MRR@10 | 0,7007 | 0,6651 | −0,0356 |
| Recall@50 | 0,92 | **0,92** | **0** |
| latência p50 do rerank | — | **1953 ms** | +1,95 s/query |
| p95 | — | 2241 ms | |

Os três runs deram valores **idênticos** — o pipeline é determinístico, então isto não é ruído amostral: é o
efeito.

## O mecanismo, e ele é o mesmo de outra aposta refutada

O `Recall@50` é **idêntico** nos dois braços — obviamente, já que o reranker só **reordena** o candidate-set que
recebe. Logo todo o Δ vem da **ordenação**, e a ordenação piorou: o reranker moveu documentos relevantes para
baixo do top-10.

> O reranker não amplia o que foi recuperado. Ele só pode **estragar** uma ordem que já estava boa — e num corpus
> onde a perna densa já entrega nDCG 0,73, há muito mais a perder do que a ganhar.

É exatamente a distinção que a [conflacao-ranker-com-candidate-set](../failure-modes/conflacao-ranker-com-candidate-set.md)
descreve: quem espera que um ranker melhore recall está esperando da peça errada.

## O que fazer com isto

1. **`ai.rerank` foi shipado assim mesmo, com o veredito registrado** (release v0.55.0 rotulada
   *honest-negative*) — a função existe e é útil onde a perna base é fraca; o que **não** existe é a alegação de
   que ela melhora qualidade por padrão.
2. **Nunca ligue rerank por default.** O ganho depende do corpus e da força da perna base, e o custo é
   ~2 s/query — três ordens de grandeza acima da busca.
3. **Meça no seu corpus.** Este é um ponto (SciFact, `text-embedding-3-small`, k=50); um corpus com perna densa
   fraca pode inverter o sinal — a mesma dependência de dataset medida em
   [hibrida-e-dataset-dependente](hibrida-e-dataset-dependente.md).

## Relacionados

- [failure-mode/conflacao-ranker-com-candidate-set](../failure-modes/conflacao-ranker-com-candidate-set.md)
- [honest-negative/hibrida-e-dataset-dependente](hibrida-e-dataset-dependente.md)
- [honest-negative/bm25-na-fusao-rrf](bm25-na-fusao-rrf.md)
