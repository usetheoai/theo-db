---
type: Measurement
title: m31b — distância SIMD, precedida de profiling
description: O profile mostrou que a distância era só 55% do custo, o que impediu de mirar o SIMD no lugar errado — e a solução fundiu decode e distância numa passada só.
resource: git:f7c7b93:docs/benchmarks/m31b-simd-distance.md
tags: [benchmark, simd, avx2, profiling, otimizacao, m31b]
milestone: M31b
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m31b
    resource: git:f7c7b93:docs/benchmarks/m31b-simd-distance.md
    title: M31b — SIMD vector distance
---

O melhor exemplo de **measurement-first aplicado à própria otimização**: medir antes de otimizar mudou o
desenho da otimização.

# Fase 0 — o profile que evitou o erro

Micro-benchmark do laço quente do scan, com build portátil equivalente ao da extensão:

| Componente | Custo | Fatia |
|---|---|---|
| **decode** — bytes de página para f32 | 1,96 ms | **45%** |
| **distância** — escalar | 2,34 ms | **55%** |
| total | 4,30 ms | 100% |

**Achado decisivo: a distância é só 55% do custo.** Portanto **vetorizar apenas a distância NÃO
alcançaria a referência** — cortar 55% pela metade renderia ~26% de ganho total.

**O profile impediu que o esforço de SIMD fosse mal direcionado.** Sem ele, a otimização teria sido feita,
teria funcionado no que atacava, e teria falhado o objetivo.

# O desenho que o profile produziu

Em vez de vetorizar a distância isolada, a implementação **lê os f32 DIRETAMENTE dos bytes da página**
com carga desalinhada, **fundindo decode e distância numa única passada SIMD** — o que elimina **os dois**
custos, e dispensa o buffer intermediário.

# Fase 1 — validação

Paridade contra o oráculo escalar em todas as dimensões varridas, **dentro de epsilon** — preservando
recall, **não bit-idêntico, e isso é por desenho**. O laço fundido mede **1,62× mais rápido** que o
caminho escalar no micro-benchmark.

# O que este milestone também corrigiu

Foi durante este trabalho que o profiler expôs a degenerescência de dados que
[retro-invalidou](/decisions/0012-benchmark-data-degeneracy.md) os números de latência anteriores. Com
dados genuinamente distintos, o quadro real é **melhor** do que o anterior sugeria: o índice próprio mede
**2,6× mais rápido que a referência** no regime uniforme com recall em paridade.

A narrativa dos "2,7× atrás" era **artefato de dados ruins** — e o registro corrige a história em vez de
sobrescrevê-la.
