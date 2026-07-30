---
type: Honest Negative
title: MIN/MAX sobre texto não é roteável: byte-min ≠ collation-min
description: Determinismo de colação não basta — ordenar não é o mesmo que igualdade. Só C/POSIX é seguro, e as colunas default do ClickBench declinam.
tags: [colunar, colacao, veredito]
timestamp: 2026-07-30T00:00:00Z
---

# MIN/MAX sobre texto não é roteável: byte-min ≠ collation-min

## O veredito

`MIN(texto)` calculado por comparação de **bytes** não é o mesmo que `MIN` sob a **colação** do PostgreSQL. E a
armadilha fina, que já enganou uma vez:

> **colação determinística não basta** — determinismo garante que a *igualdade* é estável, não que a *ordem*
> coincide com a ordem de bytes.

Só `C`/`POSIX` é seguro. As colunas default do ClickBench usam colação que faz o gate **declinar** — logo q21 e
q22 (300× e 260× de ganho potencial) são **honest-negatives estruturais**.

## Consequência que precisa ser dita em voz alta

Existe um **teto realista de cobertura do ClickBench em ~35-39 de 43**. Regex, `ILIKE` e MIN/MAX de texto por
colação são honest-negatives estruturais — **nunca será 43/43**. Prometer cobertura total é prometer o que a
semântica do PostgreSQL proíbe.

## A família de defeito

É a mesma classe do q17 e dos guards do M153/M156: sempre que o colunar compara texto, a pergunta é *"esta
operação é definida por bytes ou por colação?"*. Igualdade sob colação determinística: seguro. Ordem: não.

## Relacionados

- [invariant/chunk-group-e-a-unidade-de-tudo](../invariants/chunk-group-e-a-unidade-de-tudo.md)
