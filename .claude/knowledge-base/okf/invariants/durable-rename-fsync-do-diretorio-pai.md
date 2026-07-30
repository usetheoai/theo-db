---
type: Invariant
title: durable_rename emite 4 fsyncs em ordem estrita e o do diretório-pai é o load-bearing
description: Sem o fsync do diretório-pai o rename se perde; e durable_rename NÃO faz PANIC — repassa o elevel do caller.
tags: [postgres, durabilidade, fsync, recovery]
timestamp: 2026-07-30T00:00:00Z
---

# `durable_rename` emite **4** fsyncs em ordem estrita — e o do diretório-pai é o load-bearing

> **CORRIGIDO 2026-07-30 após review.** Dizia "5 fsyncs". São **quatro**, verificados em
> `references/postgres/src/backend/storage/file/fd.c` (REL_17_STABLE @ `e99fb32`) — e o próprio doc-comment em
> `:770` os enumera. A conclusão load-bearing não muda.

## O invariante (verificado em PG 17.10, `e99fb32`)

`durable_rename` não é "rename + um fsync". São **quatro fsyncs em ordem estrita**:

| # | linha | o quê |
|---|---|---|
| 1 | `fd.c:793` | `fsync_fname_ext(oldfile, …)` — a origem, antes do rename |
| 2 | `fd.c:809` | `pg_fsync(fd)` — o destino, antes do rename |
| 3 | `fd.c:847` | `fsync_fname_ext(newfile, …)` — o destino, depois |
| 4 | `fd.c:850` | `fsync_parent_path(newfile, …)` — **o diretório-pai** |

E o que carrega a garantia é o **fsync do diretório-pai**: sem ele, o *rename* pode se perder num crash, mesmo com o arquivo inteiro
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
