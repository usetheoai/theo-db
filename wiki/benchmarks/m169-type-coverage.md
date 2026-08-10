---
type: Measurement
title: m169 — matriz de tipos, execução com controle positivo
description: Reexecução da matriz sobre o binário do milestone, novamente com divergência semeada para provar que o oráculo detecta erro.
resource: git:f7c7b93:docs/benchmarks/m169-type-coverage.md
tags: [benchmark, tipos, controle-positivo, gate-recorrente, m169]
milestone: M169
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m169tc
    resource: git:f7c7b93:docs/benchmarks/m169-type-coverage.md
    title: M169 — type-coverage A/B run
---

**35 de 35 casos como esperado**, com **divergência semeada detectada** — o oráculo pega um resultado
errado.

# O ponto: este gate é recorrente, não pontual

Esta matriz aparece **de novo**, sobre o binário deste milestone. Ela já aparecera em
[m163](/benchmarks/m163-type-coverage-verdict.md),
[m166](/benchmarks/m166-type-coverage.md) e [m167](/benchmarks/m167-type-coverage.md).

**Um gate que roda uma vez encontra os bugs de uma vez.** O que fecha a classe de defeito é ele
**re-rodar a cada mudança que toca os caminhos de admissão** — que foi a conclusão do m163 ao diagnosticar
por que bugs de classe de tipo sobreviviam repetidamente.

# O controle positivo, de novo

**Semear um erro e verificar que ele é detectado** não é ritual: um harness pode quebrar entre execuções —
uma comparação que passa a comparar a coisa errada, um caminho que deixa de rodar — e reportar verde
perfeito.

**Revalidar o instrumento a cada execução** é o que faz a série de verdes significar algo.

# Contexto

É o artefato de gate exigido pela mudança de roteamento deste milestone, cujos números de conclusão estão
em [m169 t41](/benchmarks/m169-t41.md).
