---
type: Measurement
title: gate de escala do ClickBench — BLOQUEADO por defeito no colunar
description: Valida o pipeline em escala numa máquina barata ANTES de gastar infraestrutura cara, e o gate faz exatamente seu trabalho ao barrar por um defeito real.
resource: git:f7c7b93:docs/benchmarks/clickbench-scale-gate-2026-07-24.md
tags: [benchmark, gate, escala, custo, bloqueio]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: cbgate
    resource: git:f7c7b93:docs/benchmarks/clickbench-scale-gate-2026-07-24.md
    title: Gate de escala do ClickBench — BLOQUEADO
    last_modified: 2026-07-24
---

**Objetivo declarado:** validar, **antes de gastar infraestrutura cara**, que o pipeline roda ponta a
ponta em escala.

**Veredito: BLOQUEADO** por um defeito real no caminho colunar.

# Por que um gate barato antes de um caro

A medição roda numa máquina **dedicada, efêmera e destruída ao final** — barata em relação ao hardware
canônico do benchmark.

**Descobrir um defeito bloqueante numa máquina barata é o cenário de sucesso do gate.** Descobri-lo
depois de provisionar a infraestrutura cara custaria a infraestrutura mais o tempo, e provavelmente
pressão para "aproveitar" a máquina rodando algo.

É o mesmo raciocínio de escalonamento de custo que os gates anti-sunk-cost aplicam ao código:
[medir antes de construir](/benchmarks/m40-ceiling-probe.md).

# O que o bloqueio produziu

Um **defeito nomeado e rastreado**, em vez de uma execução parcial reportada como sucesso — e a
sequência natural: corrigir, e então
[re-rodar o gate](/benchmarks/clickbench-1m-postfix-2026-07-24.md), que destravou.

# A disciplina de custo

O planejamento de infraestrutura correspondente está em
[orçamento](/benchmarks/clickbench-official-budget.md), com preços **consultados por API, não
estimados** — a mesma exigência de evidência primária aplicada a dinheiro.
