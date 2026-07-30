---
type: Failure Mode
title: O instrumento não observa a coisa que se quer medir
description: Escolher um contador que a arquitetura do sistema torna estruturalmente incapaz de ver o fenômeno — e ler o zero dele como ausência do fenômeno.
resource: rules/discover-phd-rigor.md#R3.1
tags: [medicao, instrumentacao, R3.1]
timestamp: 2026-07-30T00:00:00Z
---

# O instrumento não observa a coisa que se quer medir

## Assinatura

O contador devolve zero, ou um valor modesto, **e a consulta claramente fez o trabalho**. O zero é lido como
"não aconteceu" quando significa "este instrumento não consegue ver isso".

## Casos pagos

| Caso | Instrumento escolhido | Por que era cego |
|---|---|---|
| M162 | `shared_blks_read` como oráculo de I/O-vs-CPU | `theodb_columnar` decodifica stripes **fora** do buffer manager do PostgreSQL — o contador lê ~0 enquanto o scan lê dezenas de GB do disco |
| M169 | `/usr/bin/time -v` → `Maximum resident set size` | mediu o **psql**, não o backend. O trabalho e a memória estão do outro lado do socket |
| M169 | `ps -o rss` amostrado **depois** da consulta | o pico já tinha sido liberado. Amostrar um pico exige amostrar *durante* |
| M169 (previsto) | `PeakTrackingPool` + spill do DataFusion para medir pico do agregado | cegos ao `rows: Vec<Vec<(Datum,bool)>>`, que é `malloc` do Rust — não é consumidor da pool nem aparece em `pg_backend_memory_contexts` |

## Custo

O M162 teve o veredito corrigido depois de publicado. O M169 gastou quatro tentativas de medição antes de um
número válido.

## Como evitar

Antes de travar um DoD que cite um instrumento, responda: **"por qual caminho o fenômeno chega até este
contador?"** Se não houver caminho, o instrumento não serve — mesmo que o nome dele sugira que sim.

Isto virou regra do projeto: `rules/discover-phd-rigor.md` **R3.1** — *instrument-validates-against-architecture*.
Se nenhum instrumento válido existir para a arquitetura, declare `INSTRUMENT-UNAVAILABLE` e **não** vista uma
inferência de medição.

## Relacionados

- [technique/gate-de-nao-vacuidade](../techniques/gate-de-nao-vacuidade.md) — o gate que pega o caso degenerado
- [failure-mode/medicao-vacuosa-aceita](medicao-vacuosa-aceita.md)
