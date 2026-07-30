---
type: Invariant
title: UNLOGGED é truncada por crash recovery — sempre, sem aviso
description: Uma tabela UNLOGGED perde 100% do conteúdo quando o cluster reinicializa após crash. Se ela é a fonte de um A/B, o oráculo passa a comparar contra vazio.
tags: [postgres, storage, recovery]
timestamp: 2026-07-30T00:00:00Z
---

# `UNLOGGED` é truncada por crash recovery — sempre, sem aviso

## O invariante

`CREATE UNLOGGED TABLE` troca durabilidade por velocidade: sem WAL. A contrapartida é que **qualquer** crash
recovery a trunca. Não há recuperação parcial e não há aviso no caminho de leitura — a tabela simplesmente fica
vazia e continua consultável.

## O que custou

No M169, `hits_heap` (66 GB, ~1,5 h de `COPY`) foi truncada **duas vezes**, por dois crashes distintos. Na
segunda, o A/B do q17 rodou contra ela e devolveu `(0 rows)` em 10,354 ms — que eu registrei como dado antes de
perceber.

## Quando `UNLOGGED` continua certo

Para carga em massa descartável é a escolha certa: evita a tempestade de WAL. O erro não foi usar `UNLOGGED`; foi
usá-la como **fonte de verdade de um oráculo** num ambiente que já havia crashado.

## Regra derivada

- Tabela de comparação de A/B: **`LOGGED`**, mesmo custando WAL. `max_wal_size` alto resolve o checkpoint storm
  sem sacrificar persistência.
- Todo oráculo que lê de uma tabela precisa de não-vacuidade sobre a **contagem** dela, não só sobre o resultado.

## Contraprova útil

Nos mesmos dois crashes, a tabela colunar (`persistence=p`) sobreviveu com os 16 GB intactos — evidência direta
de crash-safety do AM.

## Relacionados

- [failure-mode/medicao-vacuosa-aceita](../failure-modes/medicao-vacuosa-aceita.md)
