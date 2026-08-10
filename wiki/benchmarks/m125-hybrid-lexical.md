---
type: Measurement
title: m125 — híbrida num corpus que favorece o lexical: significativa
description: Resolve o risco que a paridade anterior deixara aberto — existe um regime em que a fusão ganha com significância —, e diz que o ganho é pequeno e dependente de regime.
resource: git:f7c7b93:docs/benchmarks/m125-hybrid-lexical.md
tags: [benchmark, significancia, busca-hibrida, regime, nfcorpus, m125]
dataset: BEIR NFCorpus
milestone: M125
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m125
    resource: git:f7c7b93:docs/benchmarks/m125-hybrid-lexical.md
    title: M125 — Hybrid vs vector on a lexical-favoring set
    last_modified: 2026-07-20
---

**Veredito: num conjunto que favorece o lexical, e onde a perna lexical embarcada está viva, a híbrida
SUPERA o vetorial com significância em qualidade de ranking.**

| Recuperador | nDCG@10 | Recall@100 |
|---|---|---|
| vetorial | 0,3845 | 0,3619 |
| lexical | **0,2076** | 0,1019 |
| híbrido | — significativamente melhor que vetorial | — |

# O que este artefato resolve

A [paridade anterior](/benchmarks/m123-hybrid-significance.md) deixara um risco em aberto: **a fusão
nunca ganha, ou não ganha naquele corpus?**

A diferença entre as duas leituras é enorme para o produto — a primeira diria para remover a fusão, a
segunda diz para documentar quando usá-la.

**Escolher deliberadamente um corpus onde a perna lexical está viva** é o que responde a pergunta. No
corpus anterior a perna lexical media 0,07 — praticamente morta —, e uma fusão com uma perna morta é
apenas a outra perna.

Aqui ela mede 0,21: **contribui de fato**, e a fusão ganha.

# A honestidade que acompanha

**O ganho é pequeno e dependente de regime.** Isso é dito no próprio veredito.

O par de artefatos junto — paridade num regime, ganho significativo noutro — é mais útil que qualquer um
isolado: ele descreve **quando** a capacidade paga, que é o que um operador precisa saber.

E é coerente com o achado posterior de que **trocar a perna lexical na fusão não ganha**
([m138](/benchmarks/m138-bm25-fusion.md)): a fusão importa, mas o ranker de dentro dela importa menos do
que se supunha.
