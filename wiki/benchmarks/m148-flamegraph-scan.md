---
type: Measurement
title: m148 — flamegraph do scan colunar: veredito de priorização
description: Um spike de profiling que serve de gate para três milestones seguintes, decidindo o que otimizar antes de otimizar — e declarando o confounder do próprio build.
resource: git:f7c7b93:docs/benchmarks/m148-flamegraph-scan.md
tags: [benchmark, profiling, flamegraph, priorizacao, confounder, m148]
milestone: M148
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m148
    resource: git:f7c7b93:docs/benchmarks/m148-flamegraph-scan.md
    title: M148 — Flamegraph do scan colunar
    last_modified: 2026-07-24
---

**Um spike de medição que é o gate de três milestones seguintes** — ele decide **o que** otimizar antes
que qualquer otimização seja escrita.

É o mesmo padrão que [m31b](/benchmarks/m31b-simd-distance.md) usou para descobrir que a distância era só
55% do custo, e que [m36](/benchmarks/archive/m36-scan-optimization.md) usou para descobrir que a distância não
era o gargalo — **medir antes de otimizar, repetidamente, porque a intuição erra**.

# O método

Profiling com captura de pilha completa sobre o backend em execução, exigindo um build **com informação
de depuração e ponteiros de frame preservados** — sem isso, as pilhas seriam inúteis.

Dataset **real** de 105 colunas, e não sintético, porque o comportamento de um scan colunar depende
fortemente da largura e da variedade de tipos.

# As duas ressalvas declaradas

**O confounder do build:** o PostgreSQL usado tem asserções e depuração ativadas — o que **distorce os
tempos absolutos**. Para um profile de **proporções relativas** isso é aceitável; para números absolutos,
não seria, e o documento diz isso.

**A amostragem do dataset:** por que as primeiras linhas foram usadas em vez de amostra aleatória está
justificado no próprio artefato, em vez de passar como detalhe.

# O que ele destravou

Os milestones de [projection pushdown](/benchmarks/m149-projection-pushdown.md),
[filtragem por zone-map](/benchmarks/m150-chunk-group-filtering.md) e
[ampliação de cobertura](/benchmarks/m151-datafusion-coverage.md) — todos escolhidos **porque o profile
os apontou**, e não porque pareciam boas ideias.
