# Review — aliases de AM/opclasse pgvector (#182)

**Data:** 2026-07-24 · **Branch:** develop · **Issue:** #182 · **ADR:** `docs/adr/0058` (adendo v0.6.0)

## Verdict: **READY_TO_MERGE**

Sem BLOCKER. A mudança é rotulagem de catálogo sobre implementação já existente e provada, com o teste
decisivo (migration real de uma aplicação) verde e não-vacuidade demonstrada.

## O que mudou

`vector` 0.5.1 → 0.6.0, com `sql/vector--0.5.1--0.6.0.sql` (script de upgrade obrigatório — a limitação
nº 3 da ADR-0058 previa exatamente isto). Adiciona:

- AM `hnsw` sobre o **mesmo** handler own-code `theodb_hnsw_amhandler`;
- opclasses `vector_l2_ops` (DEFAULT), `vector_cosine_ops`, `vector_ip_ops`, reusando os mesmos operadores
  (`<->`/`<=>`/`<#>`, strategy 1, `FOR ORDER BY float_ops`) e funções de suporte já declarados pelas
  opclasses `theodb_hnsw_*_ops`.

Nada reimplementado (Regra 9). O harness asserta que `hnsw` e `theodb_hnsw` compartilham o `amhandler`,
travando o surgimento de uma segunda implementação divergente.

## Evidência decisiva (droplet PG 18.4)

Migration versionada real do `theo-memory` (`0000_crazy_mimic.sql`), **sem alterar uma linha da app**:

| Momento | Resultado |
|---|---|
| antes de #181 | falha na linha 6 (`CREATE EXTENSION IF NOT EXISTS vector`) |
| depois de #181 | falha na linha 44 (`CREATE INDEX ... USING hnsw`) |
| **depois de #182** | **`MIGRATION_EXIT=0`** — 3 tabelas, 2 índices `USING hnsw` criados |

**Não-vacuidade:** instalando `vector VERSION '0.5.1'` (sem aliases), a mesma migration volta a falhar na
linha 44 com `access method "hnsw" does not exist`.

## Correção do falso-verde do review anterior

O review do #181 apontou que o harness se anunciava como "bootstrap REAL de app pgvector" usando
`USING theodb_hnsw` — verde sobre um drop-in quebrado. **Corrigido:** o harness passou a usar a sintaxe da
aplicação (`USING hnsw (col vector_cosine_ops)`), e ganhou asserções de handler compartilhado, das 3
opclasses, de `extversion = 0.6.0` e do upgrade `0.5.1 → 0.6.0`. `make check-compat` → exit 0.

## Limitação declarada

O AM `ivfflat` não recebeu alias — apps que escrevem `USING ivfflat (...) WITH (lists=…)` continuam
falhando. `hnsw` foi priorizado por ser o que as capabilities theo-data declaram. Registrado na ADR.

## Gates — verdes

Harness exit 0 com não-vacuidade · upgrade path presente (disciplina M137) · sem secrets · sem commit em
`main` · sem trailer de coautoria · CHANGELOG atualizado.

**Verdict:** READY_TO_MERGE
