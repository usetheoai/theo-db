---
type: Technology
title: Reciprocal Rank Fusion (RRF)
description: A técnica que funde rankings de recuperadores diferentes usando posições em vez de scores — o que a torna robusta a escalas incomparáveis, e explica por que melhorar uma perna nem sempre melhora a fusão.
resource: https://dl.acm.org/doi/10.1145/1571941.1572114
tags: [tecnologia, algoritmo, fusao, busca-hibrida, ranqueamento]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: recalled
    resource: conhecimento do produtor em 2026-08-07, não lido de fonte
    title: Conhecimento do produtor
---

A RRF funde rankings de recuperadores diferentes somando o **inverso da posição** de cada documento em
cada lista:

$$ \mathrm{score}(d) = \sum_{i} \frac{w_i}{k + \mathrm{rank}_i(d)} $$

com $k$ tipicamente em 60.[^recalled]

# A propriedade que a torna adequada aqui

**Ela usa POSIÇÕES, não scores.** Isso importa porque as pernas de uma busca híbrida produzem valores em
escalas **incomparáveis** — uma distância de cosseno e um score lexical não vivem na mesma unidade, e
normalizá-los exigiria calibração frágil.

A RRF dispensa isso: só a **ordem** de cada perna entra na conta.

# Papel neste acervo

**É o mecanismo da [busca híbrida](/features/06-busca-hibrida.md)**, na forma **ponderável** — pesos por
perna, com o default equivalendo à RRF pura ([m106](/benchmarks/m106-weighted-rrf.md)). Documentos
presentes numa só perna entram por junção externa, sem penalização, e empates são desempatados por
identificador, o que torna o resultado determinístico.

# O que a medição ensinou sobre ela

A linhagem de medição da fusão é sóbria e vale ser lida junto:

- num corpus onde a perna lexical está **quase morta**, a fusão **empata** com o vetorial, e o delta
  **não sobrevive a teste pareado** ([m123](/benchmarks/m123-hybrid-significance.md));
- num corpus que **favorece o lexical**, a fusão **ganha com significância**, embora pouco
  ([m125](/benchmarks/m125-hybrid-lexical.md));
- e **trocar a perna lexical por um ranker melhor não melhora a fusão**
  ([m138](/benchmarks/m138-bm25-fusion.md)).

O terceiro ponto é consequência direta da propriedade de cima: **como a fusão consome posições, ganhos
internos de ordenação numa perna se diluem** — sobretudo quando as duas pernas já concordam sobre quais
documentos importam.

**A fusão paga quando as pernas são complementares, não quando uma delas é boa.**

[^recalled]: Conhecimento do produtor, não verificado contra fonte nesta redação
