---
type: Technology
title: BEIR
description: O conjunto de benchmarks de recuperação zero-shot que o projeto usa como metodologia; sua lição central é que resultado de recuperação é dependente de corpus.
resource: https://github.com/beir-cellar/beir
tags: [tecnologia, benchmark, recuperacao, metodologia, avaliacao]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: beir-repo
    resource: https://github.com/beir-cellar/beir
    title: BEIR, repositório oficial
  - id: recalled
    resource: conhecimento do produtor em 2026-08-07, não lido de fonte
    title: Conhecimento do produtor
---

O BEIR é uma coleção heterogênea de datasets de recuperação com julgamentos de relevância, projetada para
avaliação **zero-shot** — o mesmo recuperador medido em domínios muito diferentes. A métrica primária
convencional é o nDCG@10.[^recalled]

# Papel neste acervo

**É a metodologia de avaliação de recuperação do projeto** — as medições de
[busca híbrida](/benchmarks/m53-hybrid-beir.md), de
[rerank](/benchmarks/archive/m65-rerank.md), de [chunking](/benchmarks/archive/m66-chunking.md) e de
[significância](/benchmarks/m123-hybrid-significance.md) o usam.

# A lição que a heterogeneidade dele ensinou

**Resultado de recuperação é dependente de corpus** — e o projeto aprendeu isso repetidamente, sempre da
mesma forma:

- num corpus especializado, o rerank por cross-encoder **degradou** a qualidade — exatamente o previsto
  pela literatura para domínios fora da distribuição de treino;
- naquele mesmo corpus a fusão **empatou** com o vetorial, enquanto num corpus que favorece o lexical ela
  **ganhou com significância**;
- e a diferença entre estratégias de chunking **não generaliza** entre corpora.

**Um número de recuperação sem o corpus ao lado não significa nada** — e é por isso que os artefatos do
projeto sempre nomeiam o dataset e o regime, e que dois corpora com regimes opostos passaram a ser o
padrão para decisões de default.

# O que ele não dá

**Significância.** O BEIR fornece dados e julgamentos, não teste estatístico. A disciplina de
**significância pareada entre queries**, com contagem de vitórias, derrotas e empates, foi acrescentada
pelo projeto — e é a metade que o [ADR 0050](/decisions/0050-official-benchmark-adopt-and-wrap.md)
identifica como ausente nas ferramentas de benchmark em geral.

[^beir-repo]: BEIR, repositório oficial
[^recalled]: Conhecimento do produtor, não verificado contra fonte nesta redação
