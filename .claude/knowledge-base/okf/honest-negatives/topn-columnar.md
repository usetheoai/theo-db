---
type: Honest Negative
title: Top-N colunar: cobertura ZERO, não rotear
description: O PostgreSQL já usa top-N heapsort (equivalente ao TopK do DataFusion); Sort não é o gargalo — o custo é materialização.
tags: [colunar, topk, veredito]
timestamp: 2026-07-30T00:00:00Z
---

# Top-N colunar: cobertura **zero**, não rotear

## O veredito (M155)

O PostgreSQL **já** usa top-N heapsort, que é equivalente ao TopK do DataFusion. Logo:

- `Sort` **não é o gargalo**;
- o custo real é **materialização** (o achado do M148);
- a cobertura de consultas que ganhariam algo é **0**.

Rotear seria acrescentar caminho, risco e superfície de manutenção por ganho medido nulo.

## O mandato que este registro honra

> *"nunca mascare números"* — mandato do owner.

Um milestone que produz zero é resultado, e o registro dele evita que a ideia volte com aparência de novidade.

## Relacionados

- [honest-negative/bm25-na-fusao-rrf](bm25-na-fusao-rrf.md)
