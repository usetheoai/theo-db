---
type: Failure Mode
title: EXPLAIN ANALYZE taxa por linha e fabrica speedup entre braços de tamanhos diferentes
description: A instrumentação por-tupla infla o braço que produz mais linhas; medir com \timing na query nua deu 1,60× onde o EXPLAIN ANALYZE lia 1,64×.
resource: docs/benchmarks/m158-late-mat-verdict.md
tags: [medicao, instrumentacao, postgres]
timestamp: 2026-07-30T00:00:00Z
---

# `EXPLAIN ANALYZE` taxa **por linha** e fabrica speedup entre braços de tamanhos diferentes

## O mecanismo

A instrumentação de `EXPLAIN (ANALYZE)` custa **por tupla processada**. Num A/B em que os dois braços processam
quantidades **muito diferentes** de linhas, ela taxa muito mais um lado que o outro — e o resultado é um speedup
que existe na medição e não no sistema.

Caso M158 (late-materialization top-k): o braço OFF processa **2M** tuplas, o ON processa **10**. O `EXPLAIN
ANALYZE` lia **1,64×**; a mesma comparação com `\timing` na **query nua** lê **1,60×**.

## Distinto do instrumento cego

| Conceito | O instrumento… |
|---|---|
| [instrumento-cego-a-arquitetura](instrumento-cego-a-arquitetura.md) | **não vê** o fenômeno |
| **este** | **vê, e distorce** — proporcionalmente ao braço |

O segundo é mais perigoso porque produz um número plausível e na direção esperada.

## A regra

Para comparar latência entre braços de **cardinalidade desigual**, meça com `\timing` na query nua. Reserve o
`EXPLAIN ANALYZE` para **entender o plano**, nunca para cronometrar.

## Ressalva que o mesmo artefato registra

O 1,60× é **limite superior**: o dado é sintético e o viés favorece o ganho. E a byte-identidade vale porque a
chave de ordenação (`wid`) é **única** — com chave não-única, o empate de fronteira não é invariante.

## Relacionados

- [failure-mode/instrumento-cego-a-arquitetura](instrumento-cego-a-arquitetura.md)
- [technique/ablacao-mesmo-indice](../techniques/ablacao-mesmo-indice.md)
