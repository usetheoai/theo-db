---
type: Measurement
title: m100 — executor vetorizado: a comparação é contra o próprio scan, não contra o heap
description: Escolhe deliberadamente comparar as duas formas de agregar os MESMOS dados colunares, e trata o heap como contexto — porque comparar storages diferentes mediria outra coisa.
resource: git:f7c7b93:docs/benchmarks/m100-datafusion-executor.md
tags: [benchmark, datafusion, custom-scan, vetorizado, escopo-de-comparacao, m100]
milestone: M100
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m100
    resource: git:f7c7b93:docs/benchmarks/m100-datafusion-executor.md
    title: M100 — DataFusion vectorized CustomScan aggregate
    last_modified: 2026-07-16
---

# A escolha de comparação, declarada no topo

O ganho reivindicado é o do **executor vetorizado contra o scan linha a linha do
[milestone anterior](/benchmarks/m99-columnar-tam.md), sobre OS MESMOS dados colunares** — ou seja, **as
duas formas de agregar a mesma tabela**.

**A comparação contra o heap é contexto apenas** — storage diferente.

Isso importa: comparar execução vetorizada sobre colunar contra execução linha a linha sobre heap
misturaria **duas variáveis** (formato e executor) e atribuiria o resultado à errada. **Isolar o executor
exige manter o storage constante.**

E o teto: **não é claim de superioridade contra o motor in-core de uma referência de mercado** — a mesma
disciplina que o [ADR 0035](/decisions/0035-m73-northstar-vector-verdict.md) firmou no pilar vetorial.

# O escopo desta fatia

Cobre agregações simples **sem agrupamento e sem filtro**. Agrupamento e pushdown de filtro **alargam o
ganho em fatias posteriores** — e de fato alargaram, conforme os verdicts de
[agrupamento](/benchmarks/columnar-groupby-verdict.md) e de
[zone-map](/benchmarks/columnar-zonemap-verdict.md).

Declarar o escopo estreito da fatia é o que impede o número de ser extrapolado para formas de query que
ele não exercitou — a mesma armadilha que o
[ADR 0059](/decisions/0059-m169-fail-open-cobre-falha-de-spill.md) encontraria mais tarde, quando uma
forma não testada regrediu.

# O par

Storage e execução formam a costura decidida no
[ADR 0042](/decisions/0042-m99-own-code-columnar-tam.md); a feature resultante é
[analítico colunar](/features/14-analitico-colunar.md).
