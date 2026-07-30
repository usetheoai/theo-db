---
type: Honest Negative
title: DoD de ≤1,2× vs pgvector FALSIFICADO — page-native é 7-23× mais lento
description: O caminho page-native não alcança o alvo; o own-path fica em ~1,95× a recall 1.0. Registrado como ADR-0033.
resource: docs/adr/0033
tags: [vetorial, storage, veredito]
timestamp: 2026-07-30T00:00:00Z
---

# DoD de ≤1,2× vs pgvector **falsificado** — page-native é 7-23× mais lento

## O veredito (M118)

O DoD pedia ficar em **≤1,2×** do pgvector. Medido: o caminho **page-native** é **7-23× mais lento**; o own-path
fica em **~1,95×** a recall 1.0. DoD falsificado, registrado em ADR-0033.

## O achado de método embutido

O bug de recall que apareceu no caminho foi encontrado **por evidência**, não por inspeção — o que reforça que
recall é propriedade a medir, nunca a inferir do desenho.

## Correlato — E2 / SymQG in-PG

Mesmo padrão: o AM estava **correto**, e ainda assim o `hnsw` era **2,6-3,9× mais rápido** em warm. O "page tax"
é real e não desaparece com corretude de implementação. Gate não atingido; próximo lever identificado
(FastScan 1-bit SIMD) — e depois medido em **1,2×** por ablação mesmo-índice, contra os 2,8× que a comparação
cross-box sugeria.

## Relacionados

- [technique/ablacao-mesmo-indice](../techniques/ablacao-mesmo-indice.md)
