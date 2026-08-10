---
type: Measurement
title: m95 — modelo de custo honesto para o filtro vetorial
description: Substitui uma heurística que forçava a escolha por um custo derivado de fato, e testa a pergunta certa: o planner escolhe o nó sozinho?
resource: git:f7c7b93:docs/benchmarks/m95-cost-model.md
tags: [benchmark, cost-model, planner, heuristica, m95]
dataset: SIFT1M
milestone: M95
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m95
    resource: git:f7c7b93:docs/benchmarks/m95-cost-model.md
    title: M95 — honest vecfilter cost model
    last_modified: 2026-07-13
---

# O que estava errado

O nó era escolhido por uma heurística que **multiplicava o custo mínimo por 0,1** — ou seja, **forçava a
seleção** em vez de estimar. Um custo que sempre vence não é um modelo de custo; é um override
disfarçado.

# O modelo honesto

O custo passa a ser a soma de duas parcelas derivadas:

- o custo do **sub-plano bitmap** — a produção da pertinência —, **sem dupla contagem do heap**;
- o custo do **scan vetorial**, re-derivado a partir da seletividade do bitmap, modelando o laço
  adaptativo de sondagem.

O cuidado de **não contar o heap duas vezes** é o tipo de detalhe que faz um modelo de custo ser útil ou
enganoso.

# A pergunta que a varredura faz

Não é "o nó é mais rápido?" — isso o [m92](/benchmarks/archive/m92-arbitrary-where.md) já respondera. É:

**a cada seletividade, o planner com o custo honesto e sem forçar, ESCOLHE o nó sozinho?**

Essa é a pergunta certa, porque um nó que só vence quando forçado **não serve em produção**: o usuário
não força nada. **A capacidade só é real se o planner a alcançar por conta própria.**

A comparação com o caminho nativo é reportada em paralelo, para que se veja **onde** a escolha automática
acerta e onde ela erra.
