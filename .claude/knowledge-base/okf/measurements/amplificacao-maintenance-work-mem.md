---
type: Measurement
title: flush_pending consome ≈ maintenance_work_mem × 7
description: Medido por OOM real: mwm=2GB produziu 23,4 GB de anon-rss; mwm=128MB completou a carga de 100M. A fórmula previu os dois.
resource: https://github.com/usetheodev/theo-db/issues/221
tags: [memoria, colunar, escrita, issue-221]
timestamp: 2026-07-30T00:00:00Z
---

# `flush_pending` consome ≈ `maintenance_work_mem × 7`

## O número

| `mwm` | pico anônimo previsto | observado |
|---|---|---|
| 2 GB | ~16 GB | **OOM com 23,4 GB de `anon-rss`** (+ 8,6 GB de shmem) |
| 128 MB | ~510 MB | carga de 99.997.497 linhas **completou** |

## A decomposição (hits do ClickBench: ~700 B/linha, 105 colunas)

| Termo | Fórmula | a `mwm=2GB` |
|---|---|---|
| linhas pendentes | `mwm` | 2,0 GB |
| células | `(mwm / 700) × 105` | **305M** |
| cabeçalhos `Option<Vec<u8>>` | células × 24 B | 7,3 GB |
| cópia do payload | ≈ `mwm` | 2,0 GB |
| overhead do alocador (≥16 B/aloc) | células × 16 B | 4,9 GB |
| **total** | ≈ **`mwm × 7`** | **~16 GB** |

## A causa

O **gatilho** do flush está correto (`columnar.rs:1866` checa o orçamento antes de empilhar a linha). O **flush**
não: `flush_pending` (`:1958`) chama `deform_rows_into_columns(&rows, …)` sobre o conjunto pendente **inteiro**
antes de escrever o primeiro chunk-group, e o retorno é `Vec<Vec<Option<Vec<u8>>>>` — 24 B de cabeçalho **mais
uma alocação de heap por célula**.

## O fix, verificado (não só proposto)

Mover a transposição para dentro do laço por chunk-group: `&rows[lo..hi]` (10.000 linhas → 1,05M células ≈ 25 MB,
independente de `mwm`). Dois pontos que poderiam invalidá-lo, checados:

- `encode_column` (`columnar_codec.rs:210`) calcula `null_count`/`has_nulls`/min-max **só do slice recebido** — e
  o min/max do `ChunkDirEntry` é por chunk-group **por desenho**, é o que o zone-map skip consome.
- `columns` não é lido em nenhum ponto **depois** do laço; `row_count` vem de `rows.len()`, independente.

Nenhum byte gravado muda ⇒ o gate natural é A/B de byte-identidade.

## Relacionados

- [failure-mode/config-do-operador-que-inviabiliza-a-medicao](../failure-modes/config-do-operador-que-inviabiliza-a-medicao.md)
- [invariant/chunk-group-e-a-unidade-de-tudo](../invariants/chunk-group-e-a-unidade-de-tudo.md)
