---
type: Invariant
title: O TableAmRoutine tem de ser alocado em TopMemoryContext
description: Alocar a routine handler num contexto de menor duração produz ponteiro pendente e segfault quando o contexto é resetado.
tags: [postgres, tableam, memoria, unsafe]
timestamp: 2026-07-30T00:00:00Z
---

# O `TableAmRoutine` tem de ser alocado em `TopMemoryContext`

## O invariante

O handler de um Table Access Method devolve um `TableAmRoutine*` que o PostgreSQL guarda em `rd_tableam` da
`Relation`. Esse ponteiro precisa sobreviver a todos os contextos de memória de consulta. Alocá-lo num contexto
de menor duração produz **dangling pointer** e segfault no acesso seguinte.

## Onde custou

Fase A do M99 (`theodb_columnar` TableAM). O sintoma é um crash distante do ponto de erro, o que torna o
diagnóstico caro.

## Invariante irmão — `with_active_snapshot` no SPI

Ainda no M99: chamadas SPI a partir do TableAM precisam de snapshot ativo garantido. O helper
`with_active_snapshot` existe por isso; sem ele, um flush point (`finish_bulk_insert`) sem snapshot falha de
forma obscura.

## Relacionados

- [invariant/panic-atraves-da-fronteira-c](panic-atraves-da-fronteira-c.md)
