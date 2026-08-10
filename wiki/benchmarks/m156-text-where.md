---
type: Measurement
title: m156 — pushdown de predicados de texto no WHERE
description: O maior salto de cobertura da série — de 21 para 31 queries — mantendo identidade byte a byte nos dois regimes medidos.
resource: git:f7c7b93:docs/benchmarks/m156-text-where.md
tags: [benchmark, columnar, predicado, texto, cobertura, m156]
milestone: M156
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m156
    resource: git:f7c7b93:docs/benchmarks/m156-text-where.md
    title: M156 — Text WHERE predicate pushdown
    last_modified: 2026-07-25
---

**Critério cumprido: cobertura de 21 para 31 queries — o maior salto da série —, com A/B byte-idêntico e
divergência zero nos dois regimes.**

# O que passou a ser roteado

Predicados de texto no `WHERE` — igualdade, desigualdade e correspondência por padrão, inclusive
negada — que antes eram **recusados por completo**, derrubando a query inteira para o plano nativo.

Um único predicado de texto numa query invalidava toda a aceleração. É por isso que fechar esta lacuna
rende dez queries de uma vez: **o custo da recusa não era proporcional ao predicado, era total**.

# Os dois regimes

A verificação roda em **dois regimes de amostragem** do dataset, e a identidade byte a byte vale nos
dois.

Verificar em mais de um regime protege contra o caso em que uma amostra específica esconde uma
divergência — por exemplo, se um valor problemático simplesmente não aparecer naquele recorte.

# Contexto

Sai da lista de causas do [mapa de roteamento](/benchmarks/m152-routing-map.md), junto com
[m153](/benchmarks/m153-groupby-text.md), [m154](/benchmarks/m154-count-distinct.md) e
[m157](/benchmarks/m157-expr-group.md).
