---
type: Measurement
title: m131 — o bug destravado, e o diagnóstico original refutado
description: Um backtrace ao vivo do processo travado falsificou as três afirmações do relato original do defeito — não era planner, não era quadrático, não era o que se supunha.
resource: git:f7c7b93:docs/benchmarks/m131-columnar-agg-accelerated.md
tags: [benchmark, bug, diagnostico, gdb, clickbench, m131]
milestone: M131
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m131
    resource: git:f7c7b93:docs/benchmarks/m131-columnar-agg-accelerated.md
    title: M131 — columnar-aggregate pushdown unblocked
    last_modified: 2026-07-21
---

# O defeito, e por que o diagnóstico original estava errado

O issue relatava um travamento **de planner**, ininterruptível, de custo **quadrático no número de
colunas**, em tabelas largas de tipos mistos.

**Um backtrace ao vivo do backend travado falsificou as três afirmações.**

Esse é o ponto central do artefato: **o relato de um defeito é uma hipótese, não um fato.** Um sintoma
observado — a query trava — é compatível com muitas causas, e a explicação que ocorre primeiro
frequentemente está errada.

**Anexar um depurador ao processo travado** é o instrumento que resolve isso, e é qualitativamente
diferente de raciocinar sobre o código.

# O que se ganhou

Com a causa real estabelecida, o pushdown de agregados foi **destravado**, e o ClickBench pôde ser medido
**acelerado** — o que o [m128](/benchmarks/m128-clickbench-columnar.md) tivera de deixar como follow-up.

E a medição vem com **controle**: um artefato com a aceleração ligada e outro com ela desligada, sobre a
mesma tabela — isolando a variável, como em
[m114](/benchmarks/m114-columnar-aggregate-verdict.md).

# Contexto

A feature é [analítico colunar](/features/14-analitico-colunar.md), e o pushdown continua sendo **opt-in
com default desligado** — decisão registrada na própria feature.
