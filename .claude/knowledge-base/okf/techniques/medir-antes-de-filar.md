---
type: Technique
title: Rodar a medição que separa 'nosso defeito' de 'propriedade do problema' antes de abrir o issue
description: Um issue com diagnóstico errado custa mais que a hora de medição que o teria evitado.
tags: [issue, metodo, diagnostico]
timestamp: 2026-07-30T00:00:00Z
---

# Rodar a medição que separa "nosso defeito" de "propriedade do problema"

## O padrão

Antes de filar um achado de produto, identifique o **A/B mínimo** que distingue as duas explicações concorrentes,
e rode-o.

## Caso — M169, q17

**Observado:** backend OOM-killed com 12,3 GB de `anon-rss` numa agregação de alta cardinalidade a 100M.
**Hipótese tentadora:** "o pushdown agregado é regressão de memória".

O A/B mínimo era ON vs OFF **na mesma tabela**, em box ociosa, com o RSS do **backend** amostrado durante:

| Braço | pico | desfecho |
|---|---|---|
| `enable_columnar_agg=off` | 4,57 GB (597 amostras) | cortado |
| `enable_columnar_agg=on` | **4,58 GB** (705 amostras) | **(10 rows)** |

A hipótese caiu. A causa era o oráculo do harness, que remove o `LIMIT` e faz `fetchall()`.

## Caso inverso — #221, onde medir **confirmou**

Mesma disciplina, resultado oposto: a aritmética previu `mwm × 8`, e a recarga com `mwm=128MB` (previsão: ~510 MB
de pico) **completou**. Aí sim o issue tinha peso — com fix verificado por leitura de `encode_column` e do uso de
`columns`.

## Custo de não fazer

O #219 foi filado com diagnóstico falso e uma sugestão de fix **proibida pelo cabeçalho do próprio arquivo**.
Precisou de comentário de correção pública.

## Relacionados

- [technique/nenhuma-alegacao-sem-medicao](nenhuma-alegacao-sem-medicao.md)
- [measurement/q17-pushdown-nao-e-regressao](../measurements/q17-pushdown-nao-e-regressao.md)
