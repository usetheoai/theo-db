---
type: Measurement
title: m155 — spike de top-N: hipótese refutada, o PostgreSQL já fazia isso
description: A premissa do milestone era substituir uma ordenação completa que não existia — o motor já usava o mesmo algoritmo que se pretendia introduzir.
resource: git:f7c7b93:docs/benchmarks/m155-topn-spike.md
tags: [benchmark, spike, hipotese-refutada, top-n, m155]
milestone: M155
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m155
    resource: git:f7c7b93:docs/benchmarks/m155-topn-spike.md
    title: M155 — Spike Top-N
    last_modified: 2026-07-25
---

**O spike corrige a hipótese** — o terceiro da série a fazê-lo, depois de
[m148](/benchmarks/m148-flamegraph-scan.md) e [m152](/benchmarks/m152-routing-map.md).

# A premissa, e por que ela estava errada

O milestone existia para "rotear ao top-K e evitar a ordenação completa".

**Medição:** o PostgreSQL **já usa heapsort top-N** — um heap de complexidade proporcional a `n log k`,
**exatamente o algoritmo** que se pretendia introduzir.

**Não havia ordenação completa a evitar.** O trabalho teria substituído uma implementação por outra
equivalente.

# Como isso foi estabelecido

Lendo o **método de ordenação que o próprio plano reporta** ao executar. A informação estava disponível
o tempo todo, num campo que o `EXPLAIN` imprime — e bastou olhar.

**Spikes assim custam uma tarde e economizam um milestone.** É o mesmo padrão de
[m40](/benchmarks/m40-ceiling-probe.md): uma medição barata que responde "vale a pena?" antes de
"como?".

# O que sobrou de útil

Uma ressalva sobre **empates** no limite do top-k, que foi depois **neutralizada por desenho** no
milestone seguinte, ao escolher uma chave de ordenação única — ver
[m158](/benchmarks/m158-late-mat-verdict.md).

Ou seja: mesmo um spike que mata a própria premissa **produz conhecimento reutilizável**.
