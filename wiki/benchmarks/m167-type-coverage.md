---
type: Measurement
title: m167 — execução da matriz de tipos, com controle positivo
description: 35 de 35 casos como esperado — e, crucialmente, uma divergência semeada de propósito para provar que o oráculo detecta resultado errado.
resource: git:f7c7b93:docs/benchmarks/m167-type-coverage.md
tags: [benchmark, tipos, controle-positivo, oraculo, m167]
milestone: M167
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m167tc
    resource: git:f7c7b93:docs/benchmarks/m167-type-coverage.md
    title: M167 — type-coverage A/B run
---

**35 de 35 casos como esperado.**

# O controle positivo — o detalhe que vale o artefato

> **Controle positivo: divergência semeada detectada — o oráculo pega um resultado errado.**

Isto é o que separa uma suíte que passa de uma suíte que **prova algo**.

Uma matriz que reporta zero divergências pode significar duas coisas: **o código está correto**, ou **o
oráculo não detecta nada**. Um harness quebrado — comparando a coisa errada, ou comparando nada — reporta
verde perfeito.

**Semear deliberadamente um resultado errado e verificar que o oráculo o pega** elimina a segunda
possibilidade. É a mesma ideia do **teste que falha antes de passar**: sem ver o vermelho, o verde não
informa.

Vale notar que essa mesma preocupação aparece em outras formas no repositório — no guard não-vacuoso que
exige um número mínimo de crashes reais antes de aceitar um teste de crash-safety
([ADR 0014](/decisions/0014-m48-crash-safe-fold-reclaim-mechanism.md)), e no harness de compatibilidade
que **falha sem o shim** para provar não-vacuidade
([ADR 0058](/decisions/0058-pgvector-compat-shim.md)).

**Provar que o instrumento detecta falha é pré-requisito de confiar que ele não detectou nenhuma.**

# Contexto

É o artefato de gate exigido pelo [veredito de projeção](/benchmarks/m167-projection-topk-verdict.md),
seguindo a matriz definida em [m163](/benchmarks/m163-type-coverage-verdict.md).
