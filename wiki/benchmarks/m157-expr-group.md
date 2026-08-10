---
type: Measurement
title: m157 — agrupamento por expressão, e não só por coluna
description: Aceita uma chave de agrupamento que é expressão sobre timestamp, ganhando uma query — e o ganho pequeno é reportado como pequeno.
resource: git:f7c7b93:docs/benchmarks/m157-expr-group.md
tags: [benchmark, columnar, expressao, group-by, cobertura, m157]
milestone: M157
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m157
    resource: git:f7c7b93:docs/benchmarks/m157-expr-group.md
    title: M157 — GROUP BY date_trunc expression pushdown
    last_modified: 2026-07-25
---

**Critério cumprido: cobertura de 31 para 32 — uma query —, com A/B byte-idêntico nos dois regimes.**

# O que muda

O caminho acelerado aceitava **apenas colunas nuas** como chave de agrupamento. Passa a aceitar uma
**expressão** — truncamento de timestamp por unidade —, que é o idioma padrão de agregação temporal.

# Sobre o tamanho do ganho

**Uma query.** E o artefato reporta exatamente isso, sem inflar.

A série de cobertura tem saltos muito diferentes — dez queries em
[m156](/benchmarks/m156-text-where.md), quatro em [m154](/benchmarks/m154-count-distinct.md), três em
[m153](/benchmarks/m153-groupby-text.md), uma aqui. **Reportar cada um pelo tamanho real** é o que
permite ver os rendimentos decrescentes da série e decidir quando parar.

Um resumo agregado — "a cobertura subiu de 14 para 32" — esconderia essa informação, que é justamente a
que importa para priorizar.

# O gate constante

**Divergência zero** contra o heap, em todas as 43 queries, a cada ampliação. O gate não afrouxa
conforme os ganhos diminuem.

# Contexto

Último item da lista de causas do [mapa de roteamento](/benchmarks/m152-routing-map.md).
