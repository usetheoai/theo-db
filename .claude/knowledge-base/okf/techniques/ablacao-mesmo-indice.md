---
type: Technique
title: Para medir um kernel, varie só o kernel
description: Comparar builds diferentes, boxes diferentes ou índices diferentes mede a soma das mudanças; a ablação sobre o MESMO artefato mede a mudança.
tags: [benchmark, ablacao, rigor]
timestamp: 2026-07-30T00:00:00Z
---

# Para medir um kernel, varie só o kernel

## O caso — FastScan 1-bit (E2)

Medido cross-box, o ganho parecia **2,4–2,8×**. Medido por **ablação sobre o mesmo índice** — trocando apenas o
kernel de scoring, com tudo mais idêntico — o ganho real é **1,07–1,22×** (`fastscan_speedup_by_ef`: 1,07 a
ef=40 → 1,22 a ef=640). Modesto, e a decisão de produto mudou.

> **CORRIGIDO 2026-07-30 (round 3).** Esta seção publicava **2,8×** e **1,2×** — os **dois topos** das faixas
> medidas, o que maximiza a "correção" narrada (2,33×) contra a leitura honesta. É exatamente o
> arredondamento-para-o-favorável que [estatistica-que-nao-sustenta-a-alegacao](../failure-modes/estatistica-que-nao-sustenta-a-alegacao.md)
> condena por nome — cometido na Technique que ensina rigor de ablação. Fonte:
> `docs/benchmarks/e2-symqg-fastscan-verdict.md:37,48`.

## Correlato — M46

Uma mudança que só altera alocação (`alloc-only`) exige **mesmo grafo**: reconstruir o índice entre os braços
mistura a variância de construção com o efeito da alocação. `criterion` sobre o grafo já construído é o desenho
correto.

## Regra

Antes de atribuir um número a uma mudança, liste **tudo** que difere entre os braços. Se a lista tem mais de um
item, o número é da lista, não do item.

## Relacionados

- [technique/desenho-ababab](desenho-ababab.md)
- [honest-negative/symqg-in-pg](../honest-negatives/symqg-in-pg.md)
