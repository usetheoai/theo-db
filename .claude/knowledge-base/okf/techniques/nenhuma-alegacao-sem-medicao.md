---
type: Technique
title: Nenhuma alegação entra em documento antes da medição que a sustenta
description: A regra-mãe do método deste projeto — e a metade que falha na prática é 'vale também para as alegações que me favorecem'.
resource: knowledge-base/discoveries/blueprints/m168-drift-desk-check.md
tags: [metodo, honestidade, regra]
timestamp: 2026-07-30T00:00:00Z
---

# Nenhuma alegação entra em documento antes da medição que a sustenta

## A regra

> Nenhuma alegação — minha ou de um revisor — entra em documento, ADR, issue ou código antes de eu **reproduzir
> a medição que a sustenta**. Vale igualmente para as que me contradizem e para as que me favorecem.

## Por que ela nasceu

Quatro rodadas consecutivas do M168 em que a **correção** de um defeito introduzia o defeito seguinte. O padrão
era sempre o mesmo: aceitar um diagnóstico bem-argumentado e escrevê-lo, em vez de fazer a conta que o testaria.

O desk-check que a formalizou está em `knowledge-base/discoveries/blueprints/m168-drift-desk-check.md`.

## A assimetria que importa

Na sessão em que a regra foi aplicada com disciplina, ela derrubou **sete** alegações — e a distribuição é o
achado:

| Direção da alegação | Quantas caíram |
|---|---|
| me **favorecia** (destravava gate, validava hipótese, dispensava trabalho) | a maioria |
| me **contradizia** | uma (o "perfeitamente confundido" do revisor, negado por rho=+0,71) |

Alegação conveniente passa pelo filtro porque ninguém quer testá-la. É por isso que a regra precisa dizer
explicitamente "vale para as que me favorecem" — senão ela se auto-desliga.

## Como aplicar

Antes de escrever a frase, pergunte: **qual comando eu rodaria para provar isto errado?** Se não houver comando,
a frase é hipótese e tem de ser marcada como tal (`UNBENCHMARKED`). Se houver, rode.

## Relacionados

- [failure-mode/diagnostico-aceito-sem-reproduzir](../failure-modes/diagnostico-aceito-sem-reproduzir.md)
- [technique/medir-antes-de-filar](medir-antes-de-filar.md)
