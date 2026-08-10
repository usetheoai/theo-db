---
type: Measurement
title: m48 — caracterização do fold de VACUUM
description: Caracterização, não comparação competitiva; mostra que o custo do pending é varredura linear que aparece no p50 e não nas páginas do grafo, e mede o WAL que o milestone seguinte buscaria reduzir.
resource: git:f7c7b93:docs/benchmarks/m48-am-maintenance.md
tags: [benchmark, vacuum, fold, wal, caracterizacao, m48]
milestone: M48
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m48
    resource: git:f7c7b93:docs/benchmarks/m48-am-maintenance.md
    title: M48 — VACUUM fold maintenance benchmark
---

**Caracterização, não comparação competitiva** — o documento se classifica assim, o que muda o que se
pode concluir dele.

E ele começa registrando a **carga da máquina no pré-voo**, com um guard que **aborta a medição se a
carga exceder metade dos núcleos** — mecanização direta da lição que o
[m46](/benchmarks/m46-highrecall-qps.md) aprendeu.

# Degradação por pending, e onde ela aparece

O custo da região pending é uma **varredura linear** somada à travessia do grafo. **Ela aparece no p50 e
na contagem de páginas de pending — NÃO nas páginas lidas do grafo**, que ficam praticamente constantes,
porque a travessia independe do pending.

| pending alvo | p50 antes | p50 depois | foldou? |
|---|---|---|---|
| 0 | 0,286 ms | 0,274 ms | não |
| 8 | 0,493 ms | 0,437 ms | não |
| 16 | 0,637 ms | 0,578 ms | não |
| **64** | **1,589 ms** | **0,284 ms** | **sim** |

O fold só dispara acima do limiar; quando dispara, elimina a região pending e **o p50 volta ao
baseline**.

**Saber em qual métrica um custo aparece** é o que permite diagnosticar — e é a mesma distinção que o
[runbook de diagnóstico](/runbooks/vector-scan-diagnostics.md) usa entre páginas lidas e candidatos
vistos.

# Volume de WAL — insumo para a decisão seguinte

Abaixo do limiar, o VACUUM não dispara o fold e é **barato**, com WAL de índice próximo de zero. Acima, o
fold **reescreve o índice inteiro**.

**É esse custo de WAL da reescrita em sombra que o milestone seguinte busca reduzir** — a medição existe
para alimentar a decisão de [manutenção a escala](/decisions/0017-m55-index-maintenance-at-scale.md),
não para declarar vitória.

# Relacionado

A crash-safety do fold caracterizado aqui é o
[ADR 0014](/decisions/0014-m48-crash-safe-fold-reclaim-mechanism.md).
