---
type: Measurement
title: e2 — kernel FastScan de 1 bit: ganho modesto de 1,07 a 1,22×
description: Medido numa máquina dedicada sem roubo de CPU, com kill-switch que permite o A/B no mesmo binário — e o ganho pequeno é reportado como pequeno.
resource: git:f7c7b93:docs/benchmarks/e2-symqg-fastscan-verdict.md
tags: [benchmark, simd, fastscan, symqg, ablacao, sift1m]
dataset: SIFT1M
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: e2fs
    resource: git:f7c7b93:docs/benchmarks/e2-symqg-fastscan-verdict.md
    title: E2 — FastScan 1-bit SIMD sign kernel
    last_modified: 2026-07-18
---

**Ganho medido: 1,07 a 1,22×** — **modesto**, e reportado como tal.

# As condições de medição

Tudo numa **única máquina dedicada, sem roubo de CPU**. Para um ganho da ordem de 10–20%, essa condição
não é luxo: numa máquina compartilhada, o efeito **seria menor que a variância** e não poderia ser
atribuído — que foi exatamente o que aconteceu em [m46](/benchmarks/m46-highrecall-qps.md) e
[m38](/benchmarks/m38-copy-free-scan.md).

**Quanto menor o efeito, mais silenciosa precisa ser a máquina.** Escolher a condição de medição em
função do tamanho do efeito esperado é o que torna a medição capaz de responder.

# O kill-switch como instrumento

O GUC de A/B permite ligar e desligar o kernel **no mesmo binário**, isolando a variável sem trocar o
build — o padrão que [m160](/benchmarks/m160-decode-zerocopy-verdict.md) explicita.

# O contexto que enquadra o ganho

Este kernel acelera um índice que, por sua vez, **é 2,6 a 3,9× mais lento** que a alternativa
([e2 in-PG](/benchmarks/e2-symqg-inpg-verdict.md)).

**Um ganho de 20% sobre um caminho que perde por 3× não muda a recomendação** — e é por isso que a
feature correspondente segue marcada como experimental, com orientação explícita de usar outro default.

# Licença

Clean-room a partir dos papers; a referência de licença restritiva foi estudo apenas.
