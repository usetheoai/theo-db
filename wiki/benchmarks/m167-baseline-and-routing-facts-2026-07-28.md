---
type: Measurement
title: m167 — baseline e fatos de roteamento, antes de qualquer mudança de código
description: Estabelece o estado atual medido antes de otimizar, com as contagens de linha verificadas em vez de assumidas — a lição direta de um falso-verde anterior.
resource: git:f7c7b93:docs/benchmarks/m167-baseline-and-routing-facts-2026-07-28.md
tags: [benchmark, baseline, pre-mudanca, verificacao, m167]
milestone: M167
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m167b
    resource: git:f7c7b93:docs/benchmarks/m167-baseline-and-routing-facts-2026-07-28.md
    title: M167 — measured baseline and routing facts
    last_modified: 2026-07-28
---

**Baseline medido ANTES de qualquer mudança de código.**

Sem isso, o ganho de uma otimização é medido contra a memória de como as coisas eram — e a memória
favorece quem otimiza.

# As duas verificações que o cabeçalho carrega

**As contagens de linha das duas tabelas são verificadas por consulta**, e **não** pela linha de conclusão
do harness — o documento cita explicitamente **a lição do falso-100M**
([m162](/benchmarks/m162-100m-gap-verdict.md)), em que uma carga aparentemente concluída não continha o
dataset completo.

**O binário é identificado por versão e origem**, o que permite saber exatamente o que produziu os
números.

# A frase que resume o padrão

> **Cada número abaixo foi produzido por um comando na seção de reprodução. Nada aqui é estimado.**

É a formulação mais direta da regra que o repositório aplica: **medido ou não afirmado**.

# Enquadramento

**Não é o hardware canônico** — a ressalva padrão desta série, que impede os tempos de circularem como
comparáveis a leaderboard.

O veredito da otimização que este baseline habilita é
[m167 projeção](/benchmarks/m167-projection-topk-verdict.md).
