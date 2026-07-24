---
scenario: theo-data-capability-on-theodb
date: 2026-07-24
operator: paulohenriquevn
outcome: fail
summary: O dogfood descobriu que NENHUMA capability theo-data consegue sequer inicializar contra o TheoDB — `CREATE EXTENSION IF NOT EXISTS vector`, presente no bootstrap oficial do theo-memory e do theo-rag, falha com "extension vector is not available", embora o tipo public.vector e o operador <-> funcionem (drop-in incompleto do M70; issue #181).
---

# Anchor failure — o bootstrap de uma capability real não sobe contra o TheoDB (blocker do M141)

A metade de *freshness* passou (`2026-07-21-anchor-freshness-pass.md`) e a de *query* passou
(`2026-07-20-anchor-smoke.md`). O passo seguinte do anchor — **apontar uma capability theo-data real
para o TheoDB** — foi tentado hoje e falhou **no primeiro comando**, antes de qualquer query.

Este é o valor do dogfood em estado puro: 109+ artefatos de benchmark nunca teriam achado isto, porque
nenhum deles inicializa uma aplicação real. A tentativa de migrar o retrieval de uma app é que acha.

## O que foi exercitado

Levantar um TheoDB self-hosted (droplet e2e-runner, PG 18.4 pgrx-install, `theodb_rs` @ develop
pós-v0.136.0, `shared_preload_libraries='theodb_rs'`) e executar o **bootstrap oficial** que o
`theo-memory` usa (`package.json:30`, script `db:push`) e que o `theo-rag` replica.

## Resultado — fail

| Passo | Observado |
|---|---|
| `SELECT name FROM pg_available_extensions WHERE name IN ('vector','theodb_rs')` | só `theodb_rs` |
| `CREATE EXTENSION theodb_rs CASCADE` | OK |
| **`CREATE EXTENSION IF NOT EXISTS vector`** (o bootstrap real da app) | **`ERROR: extension "vector" is not available`** |
| `SELECT typname, nspname FROM pg_type … WHERE typname='vector'` | `vector\|public` ✅ |
| `CREATE TABLE t(id int, e vector(3)); SELECT e <-> '[1,2,4]'::vector` | `1` ✅ |

O **tipo** e o **operador** são drop-in exatamente como a ADR-0029 § D2 prometeu. O que falta é o
objeto de extensão nominal que todo tooling pgvector exige no bootstrap.

## Impacto

Bloqueia o M141 por completo: `theo-memory` e `theo-rag` (ambos hoje em `ankane/pgvector:v0.5.1`)
executam `CREATE EXTENSION IF NOT EXISTS vector` no `db:push` e em ≥7 arquivos de teste de integração.
Nenhum dos dois consegue **inicializar** contra o TheoDB — logo os "≥30 dias de tráfego real" nem podem
começar a contar. O gate do M141 não era só a decisão humana de migrar; havia um bloqueio técnico real,
não medido até hoje.

## Diagnóstico honesto

A compatibilidade do M70 foi entregue no nível **SQL/tipos** e validada por benchmark; o nível
**tooling/drivers** (um dos 7 níveis de compatibilidade da skill `theodb-evolution`) nunca foi exercitado
porque nenhuma aplicação real foi apontada para o banco. "PostgreSQL-compatible" verificado só por query
é exatamente o anti-pattern que a skill chama de *"compatível como vibe"*.

## Encaminhamento

Filed: [#181](https://github.com/usetheodev/theo-db/issues/181) com repro completo, a evidência medida e o
fix sugerido (extension shim `vector` — feature nativa do PostgreSQL, sem reimplementar nada, já que o
tipo/operadores/opclasses já são providos pelo `theodb_rs`).

Nota de escopo: fechar #181 remove o bloqueio **técnico**; os ≥30 dias de tráfego real, a dependência do
time e o ≥2º operador continuam sendo evidência operacional humana que nenhuma sessão produz.
