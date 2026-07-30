---
type: Measurement
title: Há TRÊS contadores chamados cobertura no ClickBench, e eles não se contradizem
description: 35/43 (pushdown sob GUC default), 31/43 (só agg) e os intermediários históricos medem coisas diferentes — confundi-los é a leitura errada mais provável.
resource: docs/benchmarks/m161-expr-routing-verdict.md
tags: [clickbench, colunar, cobertura, vocabulario]
timestamp: 2026-07-30T00:00:00Z
---

# Há **três** contadores chamados "cobertura" no ClickBench, e eles não se contradizem

## Os três

| Contador | O que conta | Último valor |
|---|---|---|
| `Custom Scan (theodb_columnar_*)` sob **GUC default** | união **agg ∪ projeção** | **35/43** (M161) |
| `columnar_customscan_count` | a mesma união, nome do harness | 32/43 (M159) |
| `agg_routed` | **só** o agregado | 30 → **31/43** (M165) · 32 (M166) |

O M154 é explícito sobre a composição: **18 = 13 agg + 5 projeção**.

## Por que registrar isto

"35/43" e "31/43" aparecem em artefatos do mesmo mês e **parecem** contradição. Não são: medem populações
diferentes. Qualquer um que consulte a série sem saber disso vai concluir que um dos dois está errado — e essa é
a leitura errada mais provável do pilar colunar.

## E os intermediários estão todos superados

6 → 11 → 14 → 18 → 21 → 31 → 32 → **35**. Citar qualquer número anterior a 35 como "a cobertura" é citar um
estado histórico. O M151 registra o cuidado inverso, e vale repetir: *"reportar o 14 como 'ganho do M151' seria
desonesto"* — o salto 6→11 era do M149, já released.

## Relacionados

- [honest-negative/min-max-texto-e-colacao](../honest-negatives/min-max-texto-e-colacao.md) — o que impede 43/43
- [measurement/gap-vs-clickhouse-m159](gap-vs-clickhouse-m159.md)
