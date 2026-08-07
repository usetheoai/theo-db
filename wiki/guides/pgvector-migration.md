---
type: Guide
title: Migrar do pgvector para o tipo vector próprio
description: Playbook de janela de manutenção para bancos existentes; o cast binário grátis não se aplica porque os dois tipos ocupam o mesmo nome e não coexistem.
resource: git:f7c7b93:docs/ops/pgvector-migration.md
tags: [guia, migracao, pgvector, janela-de-manutencao, reindex, operacao]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: pgvecmig
    resource: git:f7c7b93:docs/ops/pgvector-migration.md
    title: Migração pgvector → tipo vector own-code
---

> **Instalações novas não precisam disto.** `CREATE EXTENSION theodb CASCADE` já instala o tipo próprio,
> sem pgvector. Este playbook é **só para upgrades** de bancos que já têm colunas `vector` do
> [pgvector](/technologies/pgvector.md).

# Por que não é um byte-cast direto

O [ADR 0028](/decisions/0028-m69-own-vector-type.md) provou que o layout on-disk é **byte-idêntico**, e o
cast binário sem função existe. Mas ele exige **os dois tipos coexistindo em schemas distintos** — e o
[ADR 0029](/decisions/0029-m70-drop-pgvector.md) colocou o tipo próprio em `public.vector`, **o mesmo
nome** do pgvector, justamente para ser drop-in.

**Os dois não coexistem.** Logo a migração usa um **intermediário neutro (`real[]`)**, que qualquer um dos
tipos converte. Isso **preserva os dados** — os floats sobrevivem — mas **reescreve o heap** das colunas,
e por isso exige janela.

# Procedimento

```sql
-- 0. BACKUP. É DDL de produção.

-- 1. Converter cada coluna para o intermediário neutro (reescreve o heap).
ALTER TABLE minha_tabela ALTER COLUMN emb TYPE real[] USING emb::real[];

-- 2. Remover o pgvector e o pgvectorscale — agora sem colunas dependentes.
--    Isto também dropa os índices ANN antigos; eles serão recriados no passo 5.
DROP EXTENSION IF EXISTS vectorscale CASCADE;
DROP EXTENSION IF EXISTS vector CASCADE;

-- 3. Instalar o TheoDB (provê public.vector próprio — agora sem colisão).
CREATE EXTENSION theodb CASCADE;

-- 4. Converter de volta. O cast rejeita NaN e infinito, e valida a dimensão.
ALTER TABLE minha_tabela ALTER COLUMN emb TYPE vector USING emb::vector;

-- 5. Recriar os índices ANN sobre os access methods próprios.
CREATE INDEX meu_indice_ann ON minha_tabela USING theodb_hnsw (emb);
```

# Ressalvas honestas

**A ordem importa e não é negociável.** Converter as colunas **antes** de dropar o pgvector; reinstalar
**antes** de converter de volta. Fora de ordem, o `DROP EXTENSION` falha por dependência ou o cast do
passo 4 não resolve.

**Janela de manutenção obrigatória.** Os dois `ALTER COLUMN TYPE` reescrevem o heap e pegam
`ACCESS EXCLUSIVE` na tabela. **Não é o cast O(1)** — os dados são preservados, mas há reescrita.

**REINDEX obrigatório.** As opfamilies do pgvector **não são** as do TheoDB — são access methods
distintos —, então o índice ANN é recriado, ao custo de um `CREATE INDEX`.

**Não é online.** Por causa da reescrita mais o reindex. Para tabelas enormes, considere abordagem por
partição ou cópia lado a lado.

# Dívida conhecida

Uma migração **byte-level sem reescrita**, aproveitando o layout idêntico, exigiria instalar o tipo
próprio num schema temporário durante a transição e movê-lo depois. **Isso não está implementado** — o
tipo é fixo em `public.vector`. Está rastreado como otimização, e o procedimento acima é o caminho
correto e seguro hoje.

# Se a aplicação não sobe

Se o problema não é o dado mas o **bootstrap** — `CREATE EXTENSION vector` falhando, ou
`CREATE INDEX … USING hnsw` não reconhecido —, a resposta é o shim de compatibilidade do
[ADR 0058](/decisions/0058-pgvector-compat-shim.md), que faz aplicações pgvector existentes subirem sem
alterar código.
