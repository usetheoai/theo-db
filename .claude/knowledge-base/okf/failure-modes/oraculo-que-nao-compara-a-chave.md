---
type: Failure Mode
title: Um A/B que compara só agregados é cego a uma chave de GROUP BY errada
description: count e sum sobrevivem ao colapso da chave; o oráculo tem de comparar a COLUNA-CHAVE por symmetric-EXCEPT, senão um erro de epoch passa como diverged=0.
resource: docs/benchmarks/m157-expr-group.md
tags: [oraculo, colunar, falso-verde]
timestamp: 2026-07-30T00:00:00Z
---

# Um A/B que compara só agregados é cego a uma chave de `GROUP BY` errada

## O caso — M157, um CRITICAL que o A/B deu como verde

O pushdown de `date_trunc` no `GROUP BY` passou com `diverged = 0`. O review achou um **CRITICAL de epoch de
calendário**: o storage guarda µs desde **2000-01-01** (interno do PG) e o DataFusion lê como µs desde
**1970-01-01** — offset de **10.957 dias**.

O que salva as granularidades pequenas e condena as grandes:

| Granularidade | Sobrevive? | Por quê |
|---|---|---|
| second, minute, hour, day | **sim** | 10.957 dias é múltiplo inteiro dessas unidades → a truncagem **comuta** com o offset |
| month, quarter, year | **não** | não é múltiplo inteiro, e a contagem de anos bissextos difere entre os dois epochs |

## Por que o oráculo não viu

O A/B comparava `count` e `sum`. **Os dois sobrevivem ao colapso da chave** — se duas chaves erradas caem no
mesmo balde, a soma dos dois baldes continua igual. O agregado é invariante à permutação da chave; a chave não é.

## A regra

> **O oráculo tem de comparar a COLUNA-CHAVE, não só os agregados** — `symmetric-EXCEPT` KEY-exact sobre o
> conjunto agrupado completo.

A whitelist do M157 acabou restrita a `{second, minute, hour, day}` — as granularidades provadamente
epoch-invariantes —, e não à classe que o benchmark exercitava.

## Relacionados

- [failure-mode/ab-prova-o-espaco-de-dados-nao-o-de-tipos](ab-prova-o-espaco-de-dados-nao-o-de-tipos.md)
- [technique/controle-positivo](../techniques/controle-positivo.md)
