---
type: Measurement
title: head-to-head fresco contra o ClickHouse — o gap caiu pela metade
description: Substitui a razão obsoleta por uma atual, com o harness endurecido para que uma query recusada não passe como verde trivial.
resource: git:f7c7b93:docs/benchmarks/clickbench-fresh-vs-clickhouse-2026-07-27.md
tags: [benchmark, clickhouse, gap, harness-endurecido, atualizacao]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: cbfresh
    resource: git:f7c7b93:docs/benchmarks/clickbench-fresh-vs-clickhouse-2026-07-27.md
    title: ClickBench head-to-head vs ClickHouse — fresh measurement
    last_modified: 2026-07-27
---

**O gap caiu aproximadamente pela metade** desde a medição anterior, depois das otimizações de decode e
de cobertura de roteamento.

# Por que re-medir importa

O documento diz que **substitui a razão obsoleta por uma atual**.

Um número de comparação envelhece assim que o próprio sistema muda — e continuar citando a razão antiga
seria reportar um estado que não existe mais. **Manter a comparação viva é parte de mantê-la honesta.**

# O harness endurecido — a correção que impede o falso verde

> O roteamento agora é **asserido por query** — uma agregação **recusada** não pode mais passar como
> verde trivial por divergência zero.

Esta é a mesma lição que o [m161](/benchmarks/m161-expr-routing-verdict.md) nomeou: **se a otimização não
foi aplicada, os dois braços são idênticos e a comparação é vazia**.

Endurecer o harness **entre duas medições** significa que a medição nova é mais confiável que a antiga —
e vale registrar que parte da melhora do gap poderia, em princípio, vir de queries que antes contavam
como roteadas sem estar. **Asserir o roteamento remove essa dúvida.**

# Contexto

A medição anterior é [m159](/benchmarks/m159-clickhouse-gap-verdict.md), que estabeleceu o primeiro
baseline real do adversário; a medição em escala maior é
[m162](/benchmarks/m162-100m-gap-verdict.md).
