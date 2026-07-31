---
type: Measurement
title: count(*) sobre 100M colunar — 11,4 s com o pushdown agregado, >948 s sem ele
description: Medido 2026-07-31 na box de bench (16 vCPU / 31 GB, corpus em page cache). O caminho sem pushdown fica a 99,9% de CPU com zero wait events — é materialização linha a linha, não I/O. Diferença ≥80×.
resource: benchmarks/m169_rebuild_heap.sh
tags: [colunar, pushdown, escala, timeout, guard, 100m]
timestamp: 2026-07-31T00:00:00Z
---

# `count(*)` sobre 100M colunar: **11,4 s** com pushdown, **>948 s** sem

## Os dois números

| caminho | tempo | estado do backend |
|---|---|---|
| `theodb.enable_columnar_agg = on` | **11,4 s** | — |
| default (**off**) | **>948 s** e ainda rodando quando medi | 99,9% CPU, `Rs`, **zero wait events** |

Mesmas 99.997.497 linhas, mesma box (16 vCPU / 31 GB, corpus de 16 GB em page cache), mesmo binário. O número
rápido foi obtido **sob contenção** — o backend lento ocupava um núcleo inteiro ao mesmo tempo —, então 11,4 s é
um limite superior, não um melhor caso.

## O que os wait events dizem

`wait_event_type` e `wait_event` **nulos** com 99,9% de CPU eliminam I/O como explicação. O custo é
materialização linha a linha no executor do PostgreSQL: sem o pushdown, cada uma das 100M linhas vira tupla. É a
mesma conclusão do flamegraph do M148 (~80% do scan colunar em materialização), agora com um segundo número
independente.

## Por que isto vale registrar

`theodb.enable_columnar_agg` tem default **off**. Qualquer script auxiliar — guard de integridade, sanity check,
oráculo — que faça `count(*)` sem ligá-lo paga 80× sem perceber, porque o comando parece trivial. Foi exatamente
o que aconteceu com o guard de `m169_rebuild_heap.sh`: 35 minutos antes de a recarga sequer começar.

**A consequência de segunda ordem é pior que o tempo:** um guard de 35 minutos é um guard que alguém vai pular.
Um de 11 s roda sempre. Custo de execução alto não é neutro — ele compra o incentivo errado.

## O trade-off, dito

A versão rápida valida o colunar **através** do caminho de pushdown, então um defeito nesse caminho poderia se
mascarar. Aceitável aqui porque `columnar_type_ab.py` prova byte-identidade do pushdown em 35 casos com controle
positivo. Num contexto sem esse oráculo, a escolha se inverteria.

## Relacionados

- [measurement/limite-de-escala-100m-nao-conclusao](limite-de-escala-100m-nao-conclusao.md)
- [failure-mode/config-do-operador-que-inviabiliza-a-medicao](../failure-modes/config-do-operador-que-inviabiliza-a-medicao.md)
- [invariant/chunk-group-e-a-unidade-de-tudo](../invariants/chunk-group-e-a-unidade-de-tudo.md)
