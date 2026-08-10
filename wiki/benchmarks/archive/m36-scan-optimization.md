---
type: Measurement
title: m36 — heap top-K lazy no scan, depois do profile falsificar a premissa
description: O gate mediu que a distância era só 14–15% do custo, não o gargalo; o milestone foi re-escopado para atacar a ordenação, que colapsou 10–13×.
resource: git:f7c7b93:docs/benchmarks/archive/m36-scan-optimization.md
tags: [benchmark, scan, heap, profiling, honest-negative, m36]
milestone: M36
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m36
    resource: git:f7c7b93:docs/benchmarks/archive/m36-scan-optimization.md
    title: M36 — Otimização do scan do índice
---

**O gate measurement-first FALSIFICOU a premissa original do milestone.**

O profiler mostrou que a **distância em precisão plena é ~14–15% do custo de scan** — **não o gargalo**.
Os gargalos medidos são **I/O de página, com 44–51%**, e **ordenação de TODOS os candidatos, com 35–41%**.

O milestone foi então **re-escopado** para atacar a ordenação. Medir antes de otimizar mudou o alvo.

# A mudança

Substituir a ordenação de todos os candidatos, O(C·log C), por um **heap min lazy**: heapify O(C) na
abertura do scan e um pop O(log C) por tupla entregue. Como o executor puxa ~k vezes para um `LIMIT k`, o
custo total vira **O(C + k·log C)**.

**O top-K emitido é byte-idêntico ao da ordenação completa** — logo o recall é inalterado por construção,
não por medição.

# O achado estável

O profiler mostra a fase caindo de **~10.000–15.000 µs** para **~760–1.130 µs** — **10 a 13× menos**
naquela fase.

Isso é **algorítmico e robusto**: a complexidade mudou, e o custo por pop migrou para o caminho de
entrega, limitado pelo `LIMIT`.

# Ressalva de hardware, declarada

A medição roda numa **CPU móvel, single-thread, com throttling térmico e variância alta entre runs** — os
números absolutos **subestimam** um servidor.

Por isso o achado reportado é o **da fase medida pelo profiler**, que é estável, e não um número
ponta a ponta que a variância da máquina não sustentaria.
