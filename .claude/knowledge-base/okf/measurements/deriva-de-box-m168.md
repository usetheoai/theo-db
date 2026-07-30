---
type: Measurement
title: O mesmo binário deu −0,6% e +2,3% em coletas diferentes da mesma box
description: Controle de deriva do M168: reconstruir o binário antigo e rodá-lo intercalado com o novo fechou a pergunta por experimento — a diferença entre coletas era da box.
resource: docs/benchmarks/m168-streaming-topk-verdict.md
tags: [benchmark, deriva, rigor]
timestamp: 2026-07-30T00:00:00Z
---

# O mesmo binário deu −0,6% e +2,3% em coletas diferentes da mesma box

## O controle

Reconstruí o binário da coleta A (de `6133b9f`) e rodei **intercalado** com o de F, numa única janela —
convertendo `aaabbb` em `ababab` no nível de coleta, como o Georges 2007 prescreve.

| | efeito |
|---|---|
| binário A, coleta original (14:46) | **−0,6%** |
| **binário A, agora, intercalado** | **+2,3%** |
| binário F, agora, intercalado | +4,7% |

**2,9 pontos de deriva no mesmo código.** Pareado por slot: 3 consultas estreitas **21/35, p=0,31**; q23 **7/12,
p=0,77** — nenhuma diferença de código detectável.

## A posição publicável que sobrou

> Consultas de projeção estreita economizam **31,6× / 21,9× / 31,6×** de memória; o custo de tempo, se existe, é
> **menor que a variação entre duas medições do mesmo código**.

O q23 sobreviveu porque é intra-coleta: **72/72 em seis coletas**, 12/12 em cada uma, magnitude **13,6%** (pool
dos pares aquecidos — não os 17,7% da coleta mais lisonjeira).

## O diagnóstico que a razão pareada deu

| | rho de Spearman vs ordem |
|---|---|
| `stream` absoluto | **+1,00** |
| **efeito** (razão pareada) | **+0,71** (crítico a n=6 é 0,886) |

Absolutos derivam; o efeito não. O pareamento **estava** funcionando — negando a alegação de "ordem e efeito
perfeitamente confundidos" que eu havia publicado antes de fazer a conta.

## Instância anterior, MUITO maior — M46 (+122%)

O M168 não foi a primeira vez, e a primeira foi pior por uma ordem de grandeza. No **M46**, numa dev box saturada
(**load 18-36 em 12 cores**), o **controle pgvector** — binário **inalterado** — derivou **+122%** entre corridas.

| | deriva do controle |
|---|---|
| M46, dev box saturada (load 18-36 / 12 cores) | **+122%** |
| M168, e2e-runner (load 21 / 8 cores) | +2,9 pontos percentuais |

Com +122% de ruído no controle, **nenhum Δ de QPS é atribuível ao código** — foi exatamente essa a conclusão, e o
ganho de QPS do M46 foi declarado honest-negative (ADR-2) em vez de publicado. O que o M46 entregou de real
(pré-dimensionamento das estruturas per-query + reuso do scratch de vizinhos) foi **provado recall-neutro**, que
é uma propriedade que não depende de relógio.

**A lição que atravessa os dois:** o controle de binário-inalterado não é zelo, é o instrumento que mede o piso
de ruído. Sem ele, qualquer Δ menor que a deriva vira alegação — e a deriva pode ser 40× maior do que se imagina.

## Relacionados

- [technique/desenho-ababab](../techniques/desenho-ababab.md)
- [failure-mode/estatistica-que-nao-sustenta-a-alegacao](../failure-modes/estatistica-que-nao-sustenta-a-alegacao.md)
