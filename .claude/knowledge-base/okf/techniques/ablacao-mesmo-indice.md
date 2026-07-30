---
type: Technique
title: Para medir um kernel, varie só o kernel
description: Comparar builds diferentes, boxes diferentes ou índices diferentes mede a soma das mudanças; a ablação sobre o MESMO artefato mede a mudança.
tags: [benchmark, ablacao, rigor]
timestamp: 2026-07-30T00:00:00Z
---

# Para medir um kernel, varie só o kernel

## O caso — FastScan 1-bit (E2)

Medido cross-box, o ganho parecia **2,8×**. Medido por **ablação sobre o mesmo índice** — trocando apenas o
kernel de scoring, com tudo mais idêntico — o ganho real era **1,2×**. Modesto, e a decisão de produto mudou.

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
