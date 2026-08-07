---
type: Measurement
title: m152 — mapa de roteamento: por que cada query não vetoriza
description: Instrumenta 19 pontos de decisão para capturar a razão REAL de cada recusa, e prova que a instrumentação é neutra em comportamento antes de confiar nela.
resource: git:f7c7b93:docs/benchmarks/m152-routing-map.md
tags: [benchmark, instrumentacao, roteamento, neutralidade, spike, m152]
milestone: M152
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m152
    resource: git:f7c7b93:docs/benchmarks/m152-routing-map.md
    title: M152 — Routing-map
    last_modified: 2026-07-25
---

**O spike corrigiu a hipótese** — assim como [m148](/benchmarks/m148-flamegraph-scan.md) fizera antes.

# O método

Instrumentação de **19 pontos de decisão**, ativável por variável de ambiente, capturando a **razão real
de declínio** de cada uma das 43 queries.

Antes disso, a resposta para "por que essa query não vetoriza" era **inferência a partir da forma da
query**. Com o trace, é **o motivo que o código de fato registrou**.

É a mesma diferença que o [m133](/benchmarks/archive/m133-ci-signal-blocked.md) apontou entre inferência
plausível e evidência primária, e que o [m131](/benchmarks/m131-columnar-agg-accelerated.md) obteve com
um depurador.

# A prova de neutralidade, antes de confiar

**Com o trace desligado, a contagem de queries roteadas é idêntica e a divergência é zero.**

Isso é essencial: **instrumentação que altera o comportamento mede a si mesma**. Provar a neutralidade
**antes** de usar os dados é o que torna o mapa confiável.

# O valor do artefato

Um **mapa de razões**, e não um número. Ele transforma "16 queries não vetorizam" em uma lista de
**causas específicas**, cada uma podendo virar milestone próprio — e foi exatamente isso que aconteceu:
[texto agrupado](/benchmarks/m153-groupby-text.md),
[contagem distinta](/benchmarks/m154-count-distinct.md),
[filtro em texto](/benchmarks/m156-text-where.md) e
[expressões agrupadas](/benchmarks/m157-expr-group.md) saíram desta lista.

**Medir para priorizar rende mais que otimizar o que estiver à mão.**
