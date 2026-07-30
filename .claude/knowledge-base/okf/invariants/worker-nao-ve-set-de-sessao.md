---
type: Invariant
title: Um background worker roda em backend próprio e não enxerga SET de sessão
description: Configurar GUCs com SET numa sessão psql não afeta o worker; ele precisa de ALTER SYSTEM, e shared_preload_libraries exige restart, não reload.
tags: [postgres, bgworker, guc, operacao]
timestamp: 2026-07-30T00:00:00Z
---

# Um background worker roda em backend próprio e **não** enxerga `SET` de sessão

## O invariante

O worker tem seu próprio backend e sua própria visão de GUCs. Consequências operacionais que já custaram
diagnóstico:

| O que se tenta | Resultado |
|---|---|
| `SET theodb.embedding_api_key = ...` numa sessão psql | o worker **não vê** |
| `ALTER SYSTEM SET ...` + `pg_reload_conf()` | o worker vê |
| adicionar a extensão a `shared_preload_libraries` + `reload` | **não basta** — exige **restart** |

## O sintoma que ele produz

`theodb.embed` funciona perfeitamente numa sessão fresca, e o worker dead-letra **todos** os embeds
(`state=failed`, `attempts=5`) com um `last_error` genérico. A assimetria "funciona na sessão, falha no worker"
é a assinatura — e foi exatamente o que gerou o bug **#132**.

## Armadilhas irmãs da mesma família (self-host)

- O PostgreSQL do `pgrx-install` **recusa rodar como root** — precisa de usuário próprio (`pgtest`), e o
  diretório precisa de `o+rX` para ele alcançar os binários.
- Um `postmaster` órfão de uma corrida que crashou **segura a porta**; `pkill` antes de re-rodar.
- `create_vectorizer` **não faz backfill** de linhas existentes — criar o vectorizer **antes** de carregar dados.

## Relacionados

- [invariant/bgworker-transaction-segura-snapshot](bgworker-transaction-segura-snapshot.md)
- [invariant/so-obsoleto-sob-shared-preload](so-obsoleto-sob-shared-preload.md)
