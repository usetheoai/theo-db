---
type: Guide
title: Migrar do Pinecone para o TheoDB
description: Traz vetores e metadados para uma tabela PostgreSQL comum; a escolha entre função atômica e procedure com commit por lote é a decisão operacional que importa.
resource: git:f7c7b93:docs/migrate-from-pinecone.md
tags: [guia, migracao, pinecone, import, jsonb]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: pinecone
    resource: git:f7c7b93:docs/migrate-from-pinecone.md
    title: Migrate from Pinecone to TheoDB
---

Migrar do Pinecone é a **north-star metric** declarada no
[ADR 0005](/decisions/0005-unification-as-differentiator.md) — é a prova de que a unificação OSS ocupa o
vácuo. **Não há alegação de performance aqui:** o ganho é um sistema só, consistência transacional e
ausência de ETL, como detalhado em [um sistema contra dois](/guides/unification-1-vs-2-systems.md).

# 1. Exportar

Cada registro segue a forma do Pinecone:

```json
[
  {"id": "doc-1", "values": [0.12, 0.04, ...], "metadata": {"category": "shoes", "in_stock": true}},
  {"id": "doc-2", "values": [0.91, 0.10, ...], "metadata": {"category": "boots", "in_stock": false}}
]
```

**Vetores densos.** Vetores esparsos ainda não são importados — follow-up documentado.

# 2. Criar a tabela alvo

```sql
CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE;

CREATE TABLE items (
  id        text PRIMARY KEY,   -- id do Pinecone
  embedding vector(1536),       -- a dimensão do SEU modelo
  metadata  jsonb               -- metadados; promova as chaves quentes a colunas reais
);
```

| Campo Pinecone | Coluna |
|---|---|
| `id` | `id text` |
| `values` | `embedding vector(N)` |
| `metadata` | `metadata jsonb`, ou colunas promovidas |

**O tamanho do array `values` precisa casar com a dimensão declarada.**

# 3. Importar — e a escolha que importa

Há **duas** superfícies, e escolher errado é o principal risco operacional desta migração.

**Função — tudo ou nada, para importações pequenas:**

```sql
SELECT theodb.import_vectors(
  'items'::regclass,
  '[{"id":"doc-1","values":[0.12,0.04,0.0],"metadata":{"category":"shoes"}}]'::jsonb
);   -- retorna quantas linhas inseriu
```

A importação inteira roda **numa transação só**. Isso é atômico — e, para um export grande, significa
**memória e WAL ilimitados**.

**Procedure — commit por lote, para migrações grandes:**

```sql
-- CALL, não SELECT — e em autocommit, sem BEGIN em volta, porque ela commita por chunk.
CALL theodb.import_vectors_chunked('items'::regclass, '[...]'::jsonb, 1000);
```

**Ela NÃO é tudo ou nada:** uma falha no meio deixa os lotes já commitados persistidos — o que dá
footprint limitado e preserva progresso parcial diante de um aborto.

Ambas validam **fail-fast** (erro tipado) num export que não seja array ou num registro sem `id` ou
`values`, sem inserção parcial corrompida, e ambas usam SQL dinâmico seguro contra injeção.

**Regra prática:** função para importações pequenas e atômicas; procedure para grandes.

# 4. Agora está unificado

```sql
SELECT i.id, i.metadata->>'category'
FROM items i
WHERE i.metadata->>'category' = 'shoes'          -- filtro relacional, sem segundo sistema
ORDER BY i.embedding <=> '[0.1,0.2,...]'::vector
LIMIT 5;
```

O exemplo completo com JOIN e IA está no [quickstart](/guides/quickstart.md).

# Ressalva de drift

O documento de origem lembra de usar `SET hnsw.iterative_scan` para preservar recall sob filtro seletivo.
Essa GUC vinha de extensão **removida** ([ADR 0029](/decisions/0029-m70-drop-pgvector.md)); o mecanismo
atual é o filtro inline do [ADR 0040](/decisions/0040-m90-inline-label-filter-verdict.md).
