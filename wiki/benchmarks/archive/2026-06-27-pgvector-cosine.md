---
type: Measurement
title: baseline sintético em cosseno — a corrida que sinalizou o próprio dataset como inadequado
description: Provou que o harness funcionava e, ao mesmo tempo, que o dataset não podia decidir nada — sinalizar a própria inadequação é o resultado mais útil que ela deu.
resource: git:f7c7b93:docs/benchmarks/archive/2026-06-27-pgvector-cosine.md
tags: [benchmark, baseline, sintetico, limite-de-representatividade, arquivo]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: pgvcos
    resource: git:f7c7b93:docs/benchmarks/archive/2026-06-27-pgvector-cosine.md
    title: TheoDB vector benchmark — pgvector cosine
    last_modified: 2026-06-27
---

Baseline sobre dados **gaussianos sintéticos**, com métrica de cosseno.

# O resultado mais útil que ela produziu

Não foi um número — foi **a constatação de que ela própria não podia decidir**.

A [decisão de índice](/decisions/m2-index-decision.md) registra isso literalmente: o baseline sintético
**provou que o harness funciona e sinalizou o próprio dataset como não-representativo** para a técnica
sob avaliação.

Vetores uniformes de alta dimensão são **quase equidistantes**, e uma técnica de compressão perde
justamente as distinções finas de que esses dados dependem. Além disso, a escala era pequena demais para
que uma técnica projetada para disco tivesse onde ganhar.

**Um benchmark que reconhece o próprio limite de representatividade vale mais que um que conclui além
dele** — e a decisão esperou o [dataset real](/benchmarks/archive/2026-06-27-glove-25-angular.md), que
inverteu a leitura.

# A nota de método que veio junto

Foi aqui que se descobriu uma **assimetria de varredura** que congelava o recall de um dos índices num
platô falso: um parâmetro era mantido fixo enquanto o outro subia, de modo que a curva perdia recall
sem que o platô fosse real.

**Varrer parâmetros acoplados de forma independente produz pontos que não existem** — a mesma família de
defeito de harness que o [ADR 0012](/decisions/0012-benchmark-data-degeneracy.md) documentaria depois em
outra forma.
