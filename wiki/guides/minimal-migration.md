---
type: Guide
title: Migração mínima — PostgreSQL vanilla para TheoDB
description: Usa pg_dump e pg_restore padrão, sem ferramenta especial, com um checksum de linha inteira como oráculo de integridade e flags que fazem a restauração falhar em vez de restaurar parcialmente.
resource: git:f7c7b93:docs/migration/minimal-migration.md
tags: [guia, migracao, pg-dump, pg-restore, integridade, wire-compat]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: minmig
    resource: git:f7c7b93:docs/migration/minimal-migration.md
    title: Minimal migration — vanilla PostgreSQL → TheoDB
---

O TheoDB é **wire-compatible** com o PostgreSQL — invariante do
[ADR 0001](/decisions/0001-no-engine-fork.md) —, então **não existe ferramenta especial**: o caminho é
`pg_dump` e `pg_restore` padrão, com o dado vetorial e os índices vindo intactos. Provado ponta a ponta
por um smoke em CI.

> Use um cliente `pg_dump`/`pg_restore` de versão **maior ou igual à do servidor de origem** — um cliente
> antigo não faz dump de servidor novo.

# 0. Pré-voo — comparar versões de extensão

Uma restauração **falha** se a origem usa uma versão mais nova que o destino:

```bash
psql -h SRC -tAc "SELECT extversion FROM pg_extension WHERE extname='vector';"
psql -h DST -tAc "SELECT extversion FROM pg_extension WHERE extname='vector';"
```

Se a origem for mais nova, atualize a extensão do destino antes.

# 1. Checksum de baseline — o oráculo de integridade

Este é o passo que distingue uma migração verificada de uma migração esperançosa. O hash cobre a **linha
inteira**, ordenada por id, então **qualquer** mudança em **qualquer** coluna é pega:

```bash
psql -h SRC -tAc \
  "SELECT md5(string_agg(id::text || '|' || title || '|' || embedding::text, ',' ORDER BY id)) FROM items;"
```

Guarde o valor.

# 2. Migrar

**Formato custom (recomendado):**

```bash
pg_dump -Fc -h SRC -U postgres -d SRC_DB -f db.dump
pg_restore --no-owner --exit-on-error -h DST -U postgres -d DST_DB db.dump
```

**`--exit-on-error` é obrigatório na prática**: sem ele, o `pg_restore` **pula erros e sai com código
zero** — uma restauração parcial silenciosa que parece sucesso. `--no-owner` evita erros quando os papéis
da origem não existem no destino. Para bases grandes, o formato custom também permite `-j N` para
restauração paralela.

**SQL puro (mais simples, pipeável):**

```bash
pg_dump -h SRC -U postgres -d SRC_DB | psql -h DST -U postgres -d DST_DB -v ON_ERROR_STOP=1
```

**`-v ON_ERROR_STOP=1` cumpre o mesmo papel:** aborta no primeiro erro em vez de tolerar carga parcial.

Os dois caminhos emitem `CREATE EXTENSION IF NOT EXISTS vector` — idempotente — e recriam **todos** os
índices reconstruindo-os a partir do dado restaurado: os vetoriais `USING hnsw` e `USING ivfflat`, e os
btree. A extensão é instalada no destino pela própria restauração, porque o binário é distribuído junto
do banco — o `CREATE EXTENSION` não falha por ausência de artefato.

# 3. Verificar

```bash
# o checksum precisa ser IGUAL ao do passo 1
psql -h DST -tAc "SELECT md5(string_agg(id::text || '|' || title || '|' || embedding::text, ',' ORDER BY id)) FROM items;"

# as definições de índice precisam ter o mesmo access method e opclass
psql -h DST -c "\d items"
```

# Diagnóstico

| Sintoma | Causa | Correção |
|---|---|---|
| `type "vector" does not exist`, ou erro de opclass | versão de extensão incompatível — origem mais nova | atualize a extensão do destino primeiro |
| Restauração muito lenta ou travada | restauração de statement único mais rebuild de índice em escala | formato custom com `-j N` |
| `must be owner of …` | papéis da origem ausentes no destino | `--no-owner`, e `--no-acl` se houver ACL referenciando papéis inexistentes |

# Ressalva de drift

O documento de origem sugere, como passo opcional pós-migração, criar um índice `USING diskann` via uma
extensão de terceiro. **Essa extensão foi removida** ([ADR 0029](/decisions/0029-m70-drop-pgvector.md)).
Os índices disponíveis hoje são os próprios — ver [HNSW](/features/02-indice-hnsw.md) e
[IVFFlat](/features/03-indice-ivfflat.md), com a escolha de default registrada em
[decisão de índice](/decisions/m2-index-decision.md).

Se a origem usa o tipo `vector` do [pgvector](/technologies/pgvector.md) e o destino é uma versão sem
ele, o caminho não é este — é
[a migração de tipo](/guides/pgvector-migration.md), que exige janela de manutenção.
