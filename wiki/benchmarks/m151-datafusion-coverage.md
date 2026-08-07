---
type: Measurement
title: m151 — ampliar a cobertura do caminho vetorizado
description: Roteia mais formas de agregado pelo caminho acelerado, com A/B por query sobre as 43 do benchmark e a ressalva de que os tempos não são comparáveis a leaderboard.
resource: git:f7c7b93:docs/benchmarks/m151-datafusion-coverage.md
tags: [benchmark, columnar, cobertura, clickbench, m151]
milestone: M151
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m151
    resource: git:f7c7b93:docs/benchmarks/m151-datafusion-coverage.md
    title: M151 — Cobertura do CustomScan vetorizado
    last_modified: 2026-07-25
---

Amplia quais formas de agregado seguem pelo caminho acelerado — operadores e combinações de tipo que
antes eram recusadas e caíam no plano nativo.

# A métrica de cobertura

O número que importa neste milestone **não é latência: é quantas das 43 queries do benchmark passam a ser
roteadas**.

**Cobertura é a métrica certa quando o ganho por query já foi estabelecido** e o que resta é aplicá-lo a
mais casos. Otimizar o que já era rápido daria menos que rotear o que ainda era lento.

E o gate que acompanha cada ampliação é **divergência zero** — cada query roteada precisa dar resultado
**byte-idêntico** ao do heap, conforme o contrato estabelecido em
[m114](/benchmarks/m114-columnar-aggregate-verdict.md).

# As ressalvas

**Não é o hardware canônico**, então **os tempos não são comparáveis a leaderboard** — a mesma
declaração que [m128](/benchmarks/m128-clickbench-columnar.md) faz.

E o dataset é **subamostrado**, com a licença dele permitindo apenas uso em CI, nunca empacotamento —
uma das guardas registradas no [ADR 0050](/decisions/0050-official-benchmark-adopt-and-wrap.md).

# Continuação

A pergunta natural — **por que** as queries restantes não vetorizam — virou um spike próprio,
[m152](/benchmarks/m152-routing-map.md), que instrumentou o motivo de cada recusa em vez de supor.
