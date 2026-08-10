---
type: Measurement
title: bloqueador-raiz do pilar vetorial: a navegabilidade do grafo
description: Consolida vários milestones numa tese única — todos os alvos de superioridade dependiam de um mesmo problema não resolvido, e nomeá-lo reorganizou o roadmap.
resource: git:f7c7b93:docs/benchmarks/p0-vector-superiority-root-blocker.md
tags: [benchmark, consolidacao, causa-raiz, navegabilidade, estrategia]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: p0rb
    resource: git:f7c7b93:docs/benchmarks/p0-vector-superiority-root-blocker.md
    title: P0 — o bloqueador-raiz
    last_modified: 2026-07-10
---

**Fonte de verdade estratégica** de vários milestones — uma consolidação medida, não um relatório de
execução.

# A tese

> **Todos os milestones de superioridade do pilar dependem de UM problema-raiz não resolvido:** a
> **navegabilidade do grafo** — quanto de pool de busca é preciso para atingir um dado recall.

Nomear um **bloqueador comum** é o que transforma uma lista de milestones que não fecharam num
**problema único** a atacar.

Antes disso, os sintomas pareciam independentes: recall abaixo do alvo, latência iso-recall pior,
throughput aquém. **Todos eram a mesma coisa vista de ângulos diferentes** — o índice precisa de mais
`ef` para o mesmo recall, e tudo o mais decorre.

# Por que a consolidação vale como artefato

Ela **muda o que se decide**. Enquanto os sintomas pareciam separados, cada milestone tentava a própria
alavanca — e sete delas caíram. Com a raiz nomeada, a pergunta vira "como melhorar a navegabilidade",
que foi respondida pela análise estrutural em
[gap1](/benchmarks/gap1-extend-candidates.md).

# O desfecho

O fix de navegabilidade **subiu o teto de recall** e resolveu a degradação por escala — mas **não igualou
a eficiência de recall por `ef`**, que é a fronteira.

E o veredito final do eixo inteiro, incluindo a parte que **não é fechável** por uma extensão permissiva,
está no [ADR 0035](/decisions/0035-m73-northstar-vector-verdict.md), consolidado em
[m73](/benchmarks/m73-headtohead-verdict.md) e formalizado no reposicionamento do
[ADR 0033](/decisions/0033-north-star-reposition-proposal.md).
