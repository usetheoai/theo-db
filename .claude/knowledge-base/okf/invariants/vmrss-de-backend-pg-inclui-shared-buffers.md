---
type: Invariant
title: O VmRSS de um backend PostgreSQL inclui os shared_buffers que ele tocou — meça RssAnon
description: Um backend varrendo uma tabela grande vê o RSS subir até o tamanho de shared_buffers sem alocar nada próprio; ler isso como crescimento de memória do código é o erro.
resource: theodb_rs/src/am/columnar.rs
tags: [memoria, postgres, medicao, rss]
timestamp: 2026-07-30T00:00:00Z
---

# O `VmRSS` de um backend PostgreSQL inclui os `shared_buffers` que ele **tocou**

## O invariante

`shared_buffers` é memória **compartilhada**, mapeada em todo backend. O `VmRSS` de um processo conta as páginas
dessa região que **aquele processo já tocou**. Logo, um backend que varre uma tabela maior que o cache vê o
`VmRSS` subir **monotonicamente até ~`shared_buffers`** — sem alocar um único byte próprio.

O sintoma é indistinguível de um vazamento: RSS que sobe e não desce, num comando que não deveria acumular nada.

## O caso (M169, 2026-07-30) — a alegação que quase publiquei

`select count(*) from hits` (16 GB colunar, 100M linhas), plano `Aggregate → Seq Scan`, sem CustomScan:

| amostra | `VmRSS` | `RssAnon` | `RssShmem` |
|---|---|---|---|
| t=0 s | 3,75 GB | **0,18 GB** | 3,56 GB |
| t=20 s | 3,80 GB | **0,19 GB** | 3,60 GB |

`shared_buffers` = 4 GB. **Todo** o crescimento era `RssShmem`; a alocação própria do backend ficou em ~180 MB e
plana. Eu estava a um passo de registrar "o `count(*)` a 100M cresce em memória" — que é falso, e teria mandado o
milestone caçar um defeito inexistente.

## A regra

| Quer saber | Leia |
|---|---|
| quanto o **código** alocou | **`RssAnon`** em `/proc/PID/status` |
| quanto do cache o backend tocou | `RssShmem` |
| o que o OOM-killer considera | `RssAnon` (+ swap) — é por isso que a fórmula do `maintenance_work_mem` fala em anon-rss |
| `VmRSS` | a soma — útil para pressão de máquina, **inútil** para atribuir memória a um caminho de código |

Duas amostras separando `RssAnon` de `RssShmem` custam uma linha de `awk` e decidem a questão. Uma série só de
`VmRSS` **não consegue** distinguir vazamento de cache tocado — nenhuma quantidade de pontos resolve.

## Consequência para comparações A/B

Uma comparação de dois braços com o **mesmo** `shared_buffers` continua válida — o termo compartilhado é comum
aos dois e se cancela no delta. O que fica errado é a **magnitude absoluta**: publicar "o caminho X usa 4,58 GB"
quando 4 GB são `shared_buffers` atribui ao código memória que não é dele.

## Relacionados

- [invariant/maintenance-work-mem-nao-capa-rss-de-rust](maintenance-work-mem-nao-capa-rss-de-rust.md) — o outro lado: o que o knob NÃO capa é justamente o `RssAnon`
- [measurement/q17-pushdown-nao-e-regressao](../measurements/q17-pushdown-nao-e-regressao.md)
- [failure-mode/instrumento-cego-a-arquitetura](../failure-modes/instrumento-cego-a-arquitetura.md)
