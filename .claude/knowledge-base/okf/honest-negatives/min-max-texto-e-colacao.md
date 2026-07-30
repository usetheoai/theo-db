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

Regex, `ILIKE` e MIN/MAX de texto por colação são honest-negatives **estruturais** — vêm da semântica do
PostgreSQL, não de implementação faltante. Prometer 43/43 é prometer o que a linguagem proíbe.

> **RESSALVA acrescentada 2026-07-30 após review.** A versão anterior afirmava um teto de "**~35-39 de 43**".
> Esse intervalo **não tem derivação localizável** em nenhum artefato — o review não conseguiu distinguir entre
> conhecimento registrado só em transcript e extrapolação feita ao escrever. O que é **medido**: cobertura de
> **35/43** (M161), e q21/q22 declinam por colação (`plans/m166-string-agg-plan.md` ADR-2,
> `releases/v0.157.0-release.md:17`). O limite superior fica como **estimativa não derivada** até alguém
> enumerar quais das 8 restantes são estruturais e quais são apenas não-implementadas.

## A família de defeito

É a mesma classe do q17 e dos guards do M153/M156: sempre que o colunar compara texto, a pergunta é *"esta
operação é definida por bytes ou por colação?"*. Igualdade sob colação determinística: seguro. Ordem: não.

## Relacionados

- [invariant/chunk-group-e-a-unidade-de-tudo](../invariants/chunk-group-e-a-unidade-de-tudo.md)
