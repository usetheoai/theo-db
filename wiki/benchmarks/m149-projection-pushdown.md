---
type: Measurement
title: m149 — projection pushdown no scan colunar
description: Deixa de decodificar colunas que a query não pede, com A/B sobre as 43 queries do benchmark e ganho reportado por query.
resource: git:f7c7b93:docs/benchmarks/m149-projection-pushdown.md
tags: [benchmark, columnar, projection-pushdown, clickbench, m149]
milestone: M149
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m149
    resource: git:f7c7b93:docs/benchmarks/m149-projection-pushdown.md
    title: M149 — Projection pushdown no scan colunar
    last_modified: 2026-07-24
---

Fecha uma das lacunas que o [substrato](/benchmarks/m99-columnar-tam.md) declarara em aberto: **o scan
decodificava todas as colunas de todos os blocos**, o que era, por desenho, paridade ou pior que o heap.

# O mecanismo

Um nó que substitui o scan e **empurra a projeção**, de modo que colunas não pedidas **nunca são
descomprimidas**.

Num formato colunar isso é a otimização de maior alavanca, e é a razão de existir do formato: com 105
colunas e uma query que usa três, decodificar tudo desperdiça 97% do trabalho.

O ganho isolado desse mecanismo já fora quantificado em outro contexto — **77,4% do tempo de decode**,
medido por controle isolado em [m103](/benchmarks/m103-vector-columnar.md).

# A evidência

**A/B sobre as 43 queries** do benchmark padrão, com **ganho reportado por query** — e não só em
agregado.

Reportar por query é o que revela **onde** a otimização paga e onde não paga; uma média esconderia
queries em que ela é neutra ou negativa.

# Contexto

Faz parte da série destravada pelo [profile](/benchmarks/m148-flamegraph-scan.md), junto com
[m150](/benchmarks/m150-chunk-group-filtering.md) e
[m151](/benchmarks/m151-datafusion-coverage.md). A feature é
[analítico colunar](/features/14-analitico-colunar.md).
