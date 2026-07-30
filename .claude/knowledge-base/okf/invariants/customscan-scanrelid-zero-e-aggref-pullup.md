---
type: Invariant
title: CustomScan com scanrelid=0 e Aggref no targetlist quebra sob subquery pullup
description: O pullup inlina o Aggref num nó superior e o planner falha com 'cache lookup failed for attribute N of relation 0' — crasha até o EXPLAIN.
tags: [postgres, customscan, planner, colunar]
timestamp: 2026-07-30T00:00:00Z
---

# `CustomScan` com `scanrelid=0` e `Aggref` no targetlist quebra sob subquery pullup

## O invariante

Um `CustomScan` declarado com `scanrelid = 0` (não escaneia uma relação-base) carregando `Aggref` no targetlist
é frágil ao **subquery pullup**: o planner inlina o `Aggref` num nó superior e depois falha ao resolver o
atributo contra a "relação 0":

```
ERROR: cache lookup failed for attribute N of relation 0
```

E não é erro de execução — **crashava até o `EXPLAIN`**, o que torna a consulta impossível de inspecionar.

## O que NÃO funcionou (medido, não suposto)

Três abordagens direcionadas falharam no droplet antes da rearquitetura:

1. `scanrelid > 0` + `RTE_VALUES` → crash de Assert;
2. e outras duas variantes do mesmo tipo — remendo local do nó.

## O que funcionou — Agg-swap (M115)

Rearquitetar para **trocar o nó `Agg`** (o padrão que o TimescaleDB usa) em vez de emitir um CustomScan com
`Aggref` solto. Isso torna a saída composável em subquery / join / `ORDER BY` de agregado, que era o objetivo.

## A lição transferível

Quando três remendos direcionados falham em pontos diferentes, o problema costuma ser **onde o nó foi
posicionado**, não como ele foi escrito. Vale parar de remendar e reconsiderar a integração com o planner.

## Relacionados

- [invariant/tableam-routine-em-topmemorycontext](tableam-routine-em-topmemorycontext.md)
