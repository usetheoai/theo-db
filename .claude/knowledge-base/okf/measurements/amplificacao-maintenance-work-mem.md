---
type: Measurement
title: flush_pending consome ≈ maintenance_work_mem × 8
description: Medido por OOM real: mwm=2GB produziu 23,4 GB de anon-rss; mwm=128MB completou a carga de 100M. A fórmula dá a ordem de grandeza e SUBESTIMA o observado em 36% (base: previsto, unidades uniformizadas em GiB).
resource: https://github.com/usetheodev/theo-db/issues/221
tags: [memoria, colunar, escrita, issue-221]
timestamp: 2026-07-30T00:00:00Z
---

# `flush_pending` consome ≈ `maintenance_work_mem × 8`

> **CORRIGIDO 2026-07-30 após review.** Três defeitos numa tabela só: (a) o multiplicador é **8**, não 7 — a
> decomposição dá `1 + 3,6 + 1 + 2,4`, e os próprios valores publicados somam 16,2/2,0 = 8,1; (b) o "~510 MB"
> previsto para `mwm=128MB` **pertencia à coluna `mwm=64MB`** do issue #221 — troquei o cabeçalho e mantive o
> valor; (c) "a fórmula previu os dois" era falso: ela **subestima** o observado. O `×7` também está no
> **issue público #221** e foi corrigido lá por comentário.

## O número

| `mwm` | pico anônimo **previsto** (`×8`) | **observado** |
|---|---|---|
| 2 GiB | ~16,0 GiB | **OOM com 23,4 GB = 21,8 GiB de `anon-rss`** (+ 8,6 GB de shmem) — **36% acima do previsto** |
| 128 MiB | ~1,0 GiB | carga de 99.997.497 linhas **completou** (pico não instrumentado) |

**A fórmula dá a ordem de grandeza, não o número.** A 2 GiB ela prevê **16,0 GiB**; o observado foi **23,4 GB,
que são 21,8 GiB** — **36% acima**, na mesma unidade e com base no previsto. O resíduo **não está decomposto**.
(A base e a unidade estão declaradas porque a primeira correção publicava "31%" no frontmatter e "36%" no corpo:
dois números para a mesma comparação, um deles sem conversão GB→GiB.) É o suficiente para explicar o OOM e para dimensionar o knob; não é o
suficiente para ser citada como previsão. A linha de 128 MiB não tem pico observado a comparar: o que se sabe é
que a carga completou.

## A decomposição (hits do ClickBench: ~700 B/linha, 105 colunas)

| Termo | Fórmula | a `mwm=2GB` |
|---|---|---|
| linhas pendentes | `mwm` | 2,0 GiB |
| células *(contagem — não é termo da soma)* | `(mwm / 700) × 105` | **322M** |
| cabeçalhos `Option<Vec<u8>>` | células × 24 B | 7,2 GiB |
| cópia do payload | ≈ `mwm` | 2,0 GiB |
| overhead do alocador (≥16 B/aloc) | células × 16 B | 4,8 GiB |
| **total** | ≈ **`mwm × 8`** | **~16,0 GiB** |

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
