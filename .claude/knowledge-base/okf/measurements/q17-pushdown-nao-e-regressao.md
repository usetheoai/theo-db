---
type: Measurement
title: O pushdown agregado NÃO é a regressão de memória do q17 a 100M
description: Isolado em box ociosa, com RSS do backend amostrado durante: 4,58 GB com pushdown ON vs 4,57 GB OFF. O OOM de 12,3 GB vinha do oráculo do harness.
tags: [memoria, colunar, agregado, honestidade]
timestamp: 2026-07-30T00:00:00Z
---

# O pushdown agregado **não** é a regressão de memória do q17 a 100M

## A medição

`SELECT UserID, SearchPhrase, COUNT(*) FROM hits GROUP BY UserID, SearchPhrase LIMIT 10` sobre 100M, box ociosa,
RSS do **backend** amostrado a cada 3 s **durante**:

| Braço | wall | pico | desfecho |
|---|---|---|---|
| `enable_columnar_agg=off` | 1800 s (cortado) | 4,57 GB (597 amostras) | sem linhas |
| `enable_columnar_agg=on` | **2127 s** | **4,58 GB** (705 amostras) | **(10 rows)** — `[VALIDO]` |

## O que isso corrige

Eu estava a um passo de filar "o pushdown agregado é regressão de memória a 100M". Os dois braços são
indistinguíveis, e ambos a **um terço** dos 12,3 GB do OOM observado.

## A causa real dos dois OOMs

O oráculo A/B do harness (`run_m128_clickbench.py:283`) **remove o `LIMIT`** — por razão correta, empates tornam
o corte arbitrário — e depois faz `fetchall()`. O `LIMIT 10` limita a materialização no backend; a versão sem
limite não limita nenhum dos dois lados:

| OOM | processo | `anon-rss` |
|---|---|---|
| 06:11:01 | `postgres` | 12,3 GB — todos os grupos materializados no backend |
| 06:12 | `python3` | 32,2 GB — `fetchall()` dos mesmos grupos |

## O que fica em aberto, marcado UNBENCHMARKED

Numa agregação de alta cardinalidade **sem `LIMIT`**, o nosso caminho materializa o resultado inteiro no backend
(`rows: Vec<Vec<(Datum,bool)>>` → `Box::into_raw`) onde o PostgreSQL faria streaming. Os 12,3 GB são
*consistentes* com isso, mas **não são prova** — a corrida que os produziu tinha o harness competindo.

## Relacionados

- [failure-mode/oraculo-de-correcao-que-nao-escala](../failure-modes/oraculo-de-correcao-que-nao-escala.md)
- [technique/medir-antes-de-filar](../techniques/medir-antes-de-filar.md)
