---
type: Invariant
title: maintenance_work_mem não limita o pico de RSS quando o trabalho é feito em Rust
description: O malloc do Rust acontece FORA dos memory contexts do PostgreSQL, então o knob não capa nada — o working set precisa ser medido, não presumido.
resource: docs/benchmarks/m55-vacuum-wall.md
tags: [postgres, pgrx, memoria, rust]
timestamp: 2026-07-30T00:00:00Z
---

# `maintenance_work_mem` **não** limita o pico de RSS quando o trabalho é feito em Rust

## O invariante

`maintenance_work_mem` governa alocações feitas nos **memory contexts do PostgreSQL** (`palloc`). Código Rust
numa extensão pgrx aloca pelo **alocador do Rust** (`malloc`), **fora** desses contexts — o knob não vê e não capa.

Medido no fold whole-index do M55 a 100k×768d: **VmHWM de 1442,7 MB**, com o artefato declarando explicitamente
*"`maintenance_work_mem` does NOT bound peak_private_rss: the Rust fold mallocs OUTSIDE Postgres memory
contexts"*.

## Por que importa mais do que parece

Um operador que dimensiona a caixa pelo `maintenance_work_mem` está dimensionando pelo **orçamento errado** — e o
sintoma é OOM-kill do backend, não erro tipado. É a mesma família do
[amplificacao-maintenance-work-mem](../measurements/amplificacao-maintenance-work-mem.md), visto do outro lado:
lá o knob é respeitado no gatilho e estourado no flush; aqui ele **nunca governou** o caminho.

## Corolário de instrumentação

Como o knob não capa, o working set **tem de ser medido** (`VmHWM`, `peak_private_rss`) — presumir a partir da
configuração é presumir a partir de um número que não governa nada.

E cuidado com a projeção: o mesmo artefato extrapola 1M linearmente a partir de **um único ponto** e marca a
projeção como *"NEVER measured — do not report it as fact"*.

## Relacionados

- [measurement/amplificacao-maintenance-work-mem](../measurements/amplificacao-maintenance-work-mem.md)
- [failure-mode/instrumento-cego-a-arquitetura](../failure-modes/instrumento-cego-a-arquitetura.md)
