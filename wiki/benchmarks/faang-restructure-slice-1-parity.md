---
type: Measurement
title: reestruturação fatia 1 — paridade de latência num refactor puro
description: Prova que uma divisão de módulos não muda o caminho de instruções compilado, com evidência independente da medição de latência.
resource: git:f7c7b93:docs/benchmarks/faang-restructure-slice-1-parity.md
tags: [benchmark, refactor, paridade, modularizacao]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: faang1
    resource: git:f7c7b93:docs/benchmarks/faang-restructure-slice-1-parity.md
    title: FAANG Restructure Slice 1 — latency parity
    last_modified: 2026-06-29
---

Prova que uma divisão de módulos é **refactor puro, sem regressão de latência**.

# As duas evidências, e por que a segunda é a forte

> **Isto é um refactor, não uma feature.** A divisão relocaliza código entre módulos; **o caminho de
> instruções compilado é inalterado — provado independentemente**.

A medição de latência é a evidência **fraca** aqui: ela mostra que os tempos são compatíveis, mas está
sujeita à variância da máquina, e um refactor pequeno produziria diferenças menores que o ruído.

**A evidência forte é estrutural** — que o binário resultante executa o mesmo caminho.

Isso antecipa o padrão que os refactors posteriores levariam ao extremo:
[m126](/benchmarks/m126-hnsw-split-byteidentical.md) provando identidade textual dos módulos mais A/B
sobre o mesmo índice, e [m147](/benchmarks/m147-ab-byte-identical.md) provando byte-identidade sobre três
eixos de limpeza.

**Refactors precisam de prova estrutural, não de benchmark** — porque um benchmark neutro é compatível
tanto com "nada mudou" quanto com "duas mudanças se cancelaram".

# Escopo

A fatia também fixou a versão do ferramental de compilação — o que é pré-requisito para qualquer
comparação posterior significar alguma coisa, já que uma mudança de compilador altera o binário sem
alterar o código.
