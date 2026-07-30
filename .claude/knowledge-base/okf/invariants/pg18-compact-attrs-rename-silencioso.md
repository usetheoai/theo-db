---
type: Invariant
title: PG18 renomeou TupleDesc->attrs para compact_attrs, e o código antigo COMPILA lendo a struct errada
description: Os dois arrays coexistem e ambos são __IncompleteArrayField, então o compilador aceita — e passa a ler offsets de uma struct de 104 B sobre um array de 16 B.
resource: docs/benchmarks/m135-pg18-migration.md
tags: [postgres, pg18, ffi, corrupcao-silenciosa]
timestamp: 2026-07-30T00:00:00Z
---

# PG18 renomeou `TupleDesc->attrs` para `compact_attrs`, e o código antigo **compila** lendo a struct errada

## O invariante

No PostgreSQL 18 o `TupleDesc` ganhou `compact_attrs` (**16 B/coluna**) ao lado do `attrs` clássico
(`FormData_pg_attribute`, **104 B/coluna**). Os dois **coexistem**, e ambos são `__IncompleteArrayField` no
bindgen — então código que indexa o array antigo **continua compilando** e passa a ler `attname`, `atttypid` e
`atttypmod` em offsets de uma struct de 104 B sobre um array de 16 B.

**Nomes de coluna e OIDs viram lixo, sem diagnóstico algum.**

## Por que é a classe de defeito mais cara possível

| Classe | Onde aparece | Custo |
|---|---|---|
| erro de compilação | build | minutos |
| panic / crash | teste | horas |
| **rename que compila e lê errado** | **produção, em silêncio** | **corrupção** |

O M135 fechou **27 erros de compilação** no porte 17→18 — e essa foi a parte barata. A cara é a que **não**
apareceu como erro.

## O que o porte revelou, e contraria a intuição

A hipótese do autor era que o WAL e os Index AMs quebrariam. Medido: **zero erros** em `GenericXLog` (54
referências) e nos Index AMs. Os 27 erros concentraram-se em `compact_attrs` (9), `isset_offset` (10), rework do
bitmap (7), `vacuum_delay_point` (1) e `CompareType` (1).

> **Migração de major do PostgreSQL quebra onde o upstream mexeu de propósito, não onde a intuição aponta.**

## Regra derivada

Ao portar de major: `grep` por **cada** campo de struct do core que o código toca, e conferir contra o header da
versão nova — não confiar no compilador. Um rename com coexistência é invisível para ele.

## Relacionados

- [invariant/stub-extern-c-sem-guarda-derruba-o-servidor](stub-extern-c-sem-guarda-derruba-o-servidor.md) — do mesmo porte
- [invariant/panic-atraves-da-fronteira-c](panic-atraves-da-fronteira-c.md)
