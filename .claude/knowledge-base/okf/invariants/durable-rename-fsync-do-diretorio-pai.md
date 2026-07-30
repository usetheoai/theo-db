---
type: Invariant
title: durable_rename emite 5 fsyncs em ordem estrita e o do diretório-pai é o load-bearing
description: Sem o fsync do diretório-pai o rename se perde; e durable_rename NÃO faz PANIC — repassa o elevel do caller.
tags: [postgres, durabilidade, fsync, recovery]
timestamp: 2026-07-30T00:00:00Z
---

# `durable_rename` emite 5 fsyncs em ordem estrita — e o do diretório-pai é o load-bearing

## O invariante (verificado em PG 17.10, `e99fb32`)

`durable_rename` não é "rename + um fsync". São **cinco fsyncs em ordem estrita**, e o que carrega a garantia é
o **fsync do diretório-pai**: sem ele, o *rename* pode se perder num crash, mesmo com o arquivo inteiro
sincronizado. O dado sobrevive e a entrada de diretório não.

É a assimetria que engana: sincronizar o conteúdo parece o trabalho importante, e é o **metadado do diretório**
que decide se o arquivo existe depois do crash.

## Lição separada, do mesmo arquivo

`durable_rename` **não faz PANIC** — ele repassa o `elevel` que o caller passou. O PANIC vive em
`data_sync_elevel()`. Quem chama com `elevel` brando recebe uma falha branda numa operação de durabilidade, o que
é quase sempre o oposto da intenção.

## Por que isto vale registrar

Quando um AM próprio implementa persistência (stripes, diretórios, arquivos de índice), a tentação é replicar o
padrão "escreve + fsync". Replicar sem o fsync do diretório-pai produz um sistema que passa em todo teste que
não seja um crash real — exatamente a classe que `unlogged-truncado-por-recovery` documenta pelo outro lado.

## Relacionados

- [invariant/unlogged-truncado-por-recovery](unlogged-truncado-por-recovery.md)
