---
type: Honest Negative
title: Uma perna BM25 9,8× mais forte NÃO vence na fusão RRF
description: O RRF premia complementaridade, não força individual — trocar a perna lexical por uma muito melhor não melhorou a fusão.
tags: [lexical, rrf, hibrida, veredito]
timestamp: 2026-07-30T00:00:00Z
---

# Uma perna BM25 9,8× mais forte **não** vence na fusão RRF

## O veredito (M138)

Substituir a perna lexical por um BM25 próprio, medido **9,8× mais forte** em lexical puro, **não** melhorou a
fusão híbrida. Default mantido em `ts_rank_cd`.

## A razão, e ela é do algoritmo

O **RRF premia complementaridade**, não força individual. Uma perna melhor que erra nos **mesmos** documentos que
a outra não acrescenta informação à fusão. O ganho viria de errar em documentos **diferentes**.

## Por que este é o negativo mais útil da série

Ele contradiz a intuição mais natural do domínio ("melhorar um componente melhora o todo") e teria custado um
milestone inteiro de otimização de BM25 sem ganho de produto. Bug relacionado: #146.

## Correlato — M140.1

Em lexical **puro**, o engine próprio bate `ts_rank`, mas a magnitude é **modesta** (m=1, +13%) e o storage do
heap é ~3,5× menor. Registrado com honestidade em vez de arredondado para cima.
