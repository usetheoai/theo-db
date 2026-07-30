---
type: Invariant
title: BackgroundWorker::transaction faz PushActiveSnapshot por todo o closure
description: Uma chamada HTTP dentro do closure segura backend_xmin pelo tempo inteiro da chamada, atrasando autovacuum.
tags: [pgrx, bgworker, mvcc, vacuum]
timestamp: 2026-07-30T00:00:00Z
---

# `BackgroundWorker::transaction` faz `PushActiveSnapshot` por **todo** o closure

## O invariante

`pgrx` (`bgworkers.rs:335-343`) empurra um snapshot ativo que dura o closure inteiro. Qualquer I/O lento lá dentro
— uma chamada HTTP de embedding de ~90 s — **segura `backend_xmin`** por esse tempo, e um `backend_xmin` preso
atrasa o autovacuum local.

O sintoma não é erro: é bloat que aparece depois, longe da causa.

## O fix estrutural (M122)

Dividir em **três fases**, com transação só onde é necessária:

| Fase | O que faz | Transação |
|---|---|---|
| A | `_vectorizer_read_batch` — lê conteúdo + config, commita | sim, curta |
| B | `embed::run_batch_resolved` — HTTP **sem** txn e **sem** SPI | **não** |
| C | `_vectorizer_write_batch` — UPDATE idempotente + `mark_done` owner-guarded | sim, nova |

Detalhe que faz a divisão funcionar: `resolve_cfg` usa `guc()`, que é SPI — então ele **tem** de ficar na fase A.

Crash entre B e C é tratado por lease-expiry (at-least-once, ADR-0008).

## Como isto foi provado — e por que a prova importa

Não foi por leitura: **fonte do pgrx + medição** — worker com `0/28` held num embed real de 8 s, contra um
controle com `held age=48`. Um no-op alegado sem medição teria sido indistinguível.

## Relacionados

- [invariant/pgrx-spi-nao-e-read-only](pgrx-spi-nao-e-read-only.md)
- [invariant/worker-nao-ve-set-de-sessao](worker-nao-ve-set-de-sessao.md)
