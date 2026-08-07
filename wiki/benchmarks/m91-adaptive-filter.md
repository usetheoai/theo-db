---
type: Measurement
title: m91 — busca filtrada adaptativa: o eixo real é probes, não estratégia
description: O milestone saiu para construir um seletor de estratégias e a medição o re-escopou — em dados reais não há cruzamento entre estratégias, então não há o que selecionar.
resource: git:f7c7b93:docs/benchmarks/m91-adaptive-filter.md
tags: [benchmark, filtered-ann, adaptativo, reescopo, dados-reais, m91]
dataset: SIFT1M
milestone: M91
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m91
    resource: git:f7c7b93:docs/benchmarks/m91-adaptive-filter.md
    title: M91 — adaptive filtered vector search
    last_modified: 2026-07-13
---

O milestone **saiu para construir um seletor adaptativo de estratégias** — que escolheria entre inline,
post e pre conforme a seletividade — e **a medição o re-escopou**.

# O achado que dispensou o seletor

**Em dados reais não há cruzamento entre estratégias.** A estratégia inline **domina** a post **em todas
as seletividades**.

E isso mata a premissa inteira: **um seletor adaptativo só faz sentido se estratégias diferentes vencerem
em regimes diferentes.** Se uma domina em toda parte, o seletor é maquinaria para escolher sempre a mesma
coisa.

**A alavanca adaptativa real é o número de probes, dirigido pela seletividade do filtro** — que é uma
variável contínua, não uma escolha discreta entre implementações.

# Por que dados reais importaram aqui

O achado depende de rodar sobre dataset e queries **reais**. Um corpus sintético poderia produzir um
cruzamento artificial e justificar a construção do seletor — exatamente o tipo de erro que
[m40](/benchmarks/m40-carrier.md) sinalizou ao declarar que gaussiano é o pior caso para índice de grafo
e que o veredito **não generaliza**.

# O padrão

É a terceira vez na linhagem em que **medir antes de construir mudou o que seria construído**: a
[sonda de teto](/benchmarks/m40-ceiling-probe.md) mostrou que o quantizador era o alvo errado; a
investigação do filtro inline mostrou que o mecanismo pesado era desnecessário; e aqui a medição mostrou
que o seletor não tem função.

**Todos os três economizaram implementação, não a otimizaram.**
