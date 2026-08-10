---
type: Measurement
title: m123 — significância pareada da híbrida: paridade
description: O ganho da fusão sobre o vetorial NÃO é estatisticamente significativo neste dataset — o teste que faltava a todas as medições anteriores da híbrida.
resource: git:f7c7b93:docs/benchmarks/m123-hybrid-significance.md
tags: [benchmark, significancia, busca-hibrida, beir, honest-negative, m123]
dataset: BEIR SciFact
milestone: M123
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m123
    resource: git:f7c7b93:docs/benchmarks/m123-hybrid-significance.md
    title: M123 — Paired significance of hybrid vs vector
    last_modified: 2026-07-20
---

**Veredito: o ganho da híbrida sobre o vetorial NÃO é estatisticamente significativo neste dataset —
paridade. Honest-negative medido, sem overclaim.**

| Recuperador | nDCG@10 | Recall@100 |
|---|---|---|
| vetorial | 0,7296 | 0,9733 |
| lexical | 0,0703 | 0,0694 |
| **híbrido** | **0,7337** | 0,9733 |

# O que este artefato acrescenta

O [m53](/benchmarks/m53-hybrid-beir.md) já reportara o mesmo delta de +0,004 e já dissera "não testado
para significância". **Este milestone faz o teste.**

E o resultado é o esperado: **um delta de 0,004 não sobrevive a um teste pareado entre queries.**

**Isso é o oposto de descobrir um problema — é fechar uma pendência declarada.** O artefato anterior
recusou-se a afirmar o que não podia; este mediu e confirmou a recusa.

# Por que pareado

Comparar médias de dois recuperadores sobre o mesmo conjunto de queries **sem parear** ignora que a
dificuldade varia enormemente por query. O teste pareado compara **a mesma query nos dois braços**, que é
a única forma de detectar um efeito pequeno em meio a variância grande de dificuldade.

**Coeficiente de variação não é significância pareada** — a lição que o repositório passou a citar a
partir daqui.

# O que veio depois

A pergunta aberta — se existe um regime em que a fusão **ganha** — foi respondida por
[m125](/benchmarks/m125-hybrid-lexical.md): num corpus que favorece o lexical, **sim, e com
significância**, embora o ganho seja pequeno e dependente de regime.
