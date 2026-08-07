---
type: Measurement
title: m87 — ANN filtrado sobre o layout separado, e a lacuna que o causava
description: O mecanismo que recupera recall sob filtro seletivo existia apenas para um dos access methods; estendê-lo aos demais era o conserto.
resource: git:f7c7b93:docs/benchmarks/archive/m87-filtered-ann.md
tags: [benchmark, filtered-ann, iterative-scan, planner, arquivo, m87]
dataset: SIFT1M
milestone: M87
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m87
    resource: git:f7c7b93:docs/benchmarks/archive/m87-filtered-ann.md
    title: M87 — pg_scann filtered ANN + planner
    last_modified: 2026-07-12
---

**Veredito: GO.**

# A lacuna

O mecanismo que recupera recall sob um `WHERE` seletivo — crescendo a busca até satisfazer o `LIMIT` —
existia **apenas para o índice de grafo**. Os índices por listas invertidas **não participavam**.

A consequência é uma cadeia causal que vale entender: os candidatos das primeiras listas sondadas são
**filtrados fora**, o access method reporta que acabou, e **o `LIMIT` nunca é satisfeito** — o recall
colapsa.

**Um mecanismo correto aplicado a metade dos caminhos é uma lacuna silenciosa**: a query funciona, só
devolve menos resultados do que deveria.

# O que foi verificado

Além do conserto, o artefato **verifica que o planner escolhe o access method corretamente** — porque um
mecanismo de recall só importa se o índice for de fato usado, que é o primeiro passo do
[runbook de diagnóstico](/runbooks/vector-scan-diagnostics.md).

# O que superou este trabalho

O post-filter consertado aqui foi **superado pelo filtro inline** medido em
[m90](/benchmarks/m90-inline-filter.md), que a ~1% de seletividade dá **recall 1,00 contra 0,52** — ou
seja, o post-filter passa fome exatamente no regime seletivo, e crescer a busca é remediar em vez de
evitar. A decisão está no [ADR 0040](/decisions/0040-m90-inline-label-filter-verdict.md).
