---
type: Measurement
title: m165 — coluna constante na projeção: uma query sai de 152× para 10×
description: Uma limitação pequena da classificação de saída mantinha uma query inteira fora do caminho acelerado, custando ~23× de latência.
resource: git:f7c7b93:docs/benchmarks/m165-const-out-verdict.md
tags: [benchmark, columnar, roteamento, constante, clickhouse, m165]
milestone: M165
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m165
    resource: git:f7c7b93:docs/benchmarks/m165-const-out-verdict.md
    title: M165 — const-out verdict
---

# O resultado

| Métrica | antes | **depois** |
|---|---|---|
| razão contra o adversário | 152,44× | **10,19×** |
| tempo na mesma engine | 28,5 s | **1,22 s** |
| classe da query | não roteada | **roteada** |
| queries roteadas | 30/43 | **31/43** |

**Cerca de 15× mais perto do adversário, e ~23× mais rápida na mesma engine.**

# A lição de proporção

**A causa era uma limitação pequena:** a classificação de colunas de saída não aceitava uma **constante
projetada**. Uma query que seleciona uma constante junto com colunas reais era recusada **inteira**.

O custo dessa limitação era **23× de latência numa query**, e a razão contra o adversário passava de dez
para cento e cinquenta.

**Recusas de roteamento não custam proporcionalmente à sua causa** — elas custam a diferença inteira
entre o caminho acelerado e o caminho lento. É o mesmo padrão de
[m156](/benchmarks/m156-text-where.md), onde fechar uma lacuna de predicado rendeu dez queries de uma vez.

Isso é o que torna o [mapa de roteamento](/benchmarks/m152-routing-map.md) tão rentável: **cada razão de
recusa na lista é potencialmente um ganho desproporcional ao esforço**.

# Contexto

A medição de gap contra a qual a razão é calculada é
[m159](/benchmarks/m159-clickhouse-gap-verdict.md) a 1M e
[m162](/benchmarks/m162-100m-gap-verdict.md) a 100M.
