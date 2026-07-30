---
type: Honest Negative
title: SBQ: tese de ≥2× QPS falsificada — 0,31 a 0,77× do f32
description: Sete configurações medidas; a vantagem do SBQ só existe sob pressão de RAM, não in-RAM. ADR-0018.
resource: docs/adr/0018
tags: [vetorial, quantizacao, veredito]
timestamp: 2026-07-30T00:00:00Z
---

# SBQ: tese de ≥2× QPS **falsificada** — 0,31 a 0,77× do f32

## O veredito (M57, v0.49.0, ADR-0018)

A hipótese era **≥2×** de QPS sobre f32. Medido em **7 configurações**: SBQ entrega **0,31 a 0,77×** — ou seja,
é consistentemente **mais lento**.

Comparação in-RAM a 5k: SBQ 1480 qps · f32 1582 · pgvector 1641.

## A nuance que salva a técnica

SBQ **não** é inútil — a vantagem dele é **memória**, e ela só aparece **sob pressão de RAM**, quando o f32 não
cabe. Medir in-RAM responde a pergunta errada: ali o f32 sempre ganha, porque a quantização só adiciona trabalho
de decodificação sem economizar o acesso que não estava custando nada.

É o mesmo padrão que o RaBitQ mostrou depois: **o quantizador permissivo dá memória, não QPS.**

## Como usar este registro

Antes de propor "quantizar para acelerar", pergunte **em qual regime** o ganho apareceria — e se o benchmark
planejado está nesse regime. Se o dataset cabe em RAM, o experimento não pode mostrar a vantagem, e o resultado
será um negativo previsível.

## Relacionados

- [failure-mode/dados-sinteticos-degenerados](../failure-modes/dados-sinteticos-degenerados.md)
- [honest-negative/superioridade-vetorial-vs-scann](superioridade-vetorial-vs-scann.md)
