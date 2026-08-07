---
type: Guide
title: Self-host — subir o TheoDB com a superfície AI-native
description: Receita de self-host com vectorizer e busca híbrida, incluindo os três erros de configuração que travam quem faz isso pela primeira vez.
resource: git:f7c7b93:docs/ops/self-host-quickstart.md
tags: [guia, self-host, operacao, vectorizer, guc, troubleshooting]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: selfhost
    resource: git:f7c7b93:docs/ops/self-host-quickstart.md
    title: Self-host quickstart
---

Sobe um TheoDB próprio com a superfície que uma aplicação real usa para recuperação: coluna de embedding
mantida fresca por um [vectorizer](/features/16-vectorizer.md) e queries de
[busca híbrida](/features/06-busca-hibrida.md).

> **Escopo:** este é o self-host do **engine**. HA, replicação e control plane são preocupações de
> plataforma, fora deste repositório.

# 1. Build e instalação

```bash
cargo install --locked cargo-pgrx --version 0.19.0
cargo pgrx init --pg17 download

cd theodb_rs
cargo pgrx install --pg-config "$(cargo pgrx info path pg17)/bin/pg_config"
```

**Não rode o PostgreSQL como root** — o `initdb` e o servidor recusam. Use um usuário dedicado que seja
dono do diretório de dados.

# 2. Preload — sem isso o worker não roda

```
shared_preload_libraries = 'theodb_rs'
```

**Reinicie** depois de mudar isso; um reload **não basta** para `shared_preload_libraries`. Este custo
operacional é exatamente o que o [ADR 0016](/decisions/0016-m54-vectorizer-worker-mechanism.md) argumenta
já estar pago — e é o que justifica o worker in-process.

# 3. Configurar o provedor — no nível da instância, não da sessão

**Este é o erro mais comum.** O worker roda no **próprio backend**, então as GUCs precisam existir no
nível da **instância**:

```sql
ALTER SYSTEM SET theodb.embedding_endpoint = 'https://api.openai.com/v1/embeddings';
ALTER SYSTEM SET theodb.embedding_model    = 'text-embedding-3-small';
ALTER SYSTEM SET theodb.embedding_api_key  = '<CHAVE>';
SELECT pg_reload_conf();
```

Um `SET` de sessão **não é visto pelo worker**. E a chave nunca deve ir para versionamento — venha de um
gerenciador de segredos.

# 4. Extensão e vectorizer

```sql
CREATE EXTENSION theodb_rs;

CREATE TABLE docs (
  id        int PRIMARY KEY,
  body      text NOT NULL,
  body_tsv  tsvector GENERATED ALWAYS AS (to_tsvector('english', body)) STORED,
  embedding vector(1536)
);

-- CRIE O VECTORIZER ANTES de carregar conteúdo (ver diagnóstico)
SELECT theodb.create_vectorizer(
  'docs', 'id', 'body',           -- tabela, PK, coluna de conteúdo
  'docs', 'embedding',            -- alvo, coluna de embedding
  'text-embedding-3-small', 1536, -- modelo, dimensão
  'fixed', 512, 64);              -- estratégia de chunking, tamanho, overlap

INSERT INTO docs (id, body) VALUES (1, 'HNSW graph index enables approximate nearest neighbor search');
```

Acompanhar a fila drenar:

```sql
SELECT state, count(*) FROM theodb.vectorizer_queue GROUP BY 1;
SELECT count(*) FROM docs WHERE embedding IS NULL;   -- esperado: 0
```

# 5. Consultar

```sql
SELECT id, score
FROM ai.hybrid_search_rrf(
  'docs', 'id', 'body_tsv', 'embedding',
  query_text  => 'how does the index keep vector search fast',
  k => 60, per_leg_limit => 10, result_limit => 5);
```

O `query_text` alimenta a perna de full-text **e** é embedado para a perna vetorial; a
[RRF](/technologies/rrf.md) funde as duas.

# Diagnóstico

| Sintoma | Causa | Correção |
|---|---|---|
| Embedding fica `NULL` em linhas **pré-existentes** | `create_vectorizer` instala um trigger de DML e **não faz backfill** do que já existia | Crie o vectorizer **antes** de carregar, ou re-toque as linhas (`UPDATE … SET body = body`) para enfileirá-las |
| Jobs vão para `failed` no worker, mas `theodb.embed()` funciona na sessão | **Defeito conhecido** do caminho do worker em self-host, rastreado por issue | Enquanto não corrigido, faça backfill por sessão: `UPDATE docs SET embedding = theodb.embed(body)::vector WHERE embedding IS NULL;` — o caminho de **consulta** não é afetado |
| `endpoint must be http(s)://` | A guarda de SSRF rejeitou o endpoint — **fail-closed por desenho** | Use um endpoint `https://` |
| `theodb.embedding_endpoint is not set` | As GUCs foram definidas por sessão, e o worker não as vê | Defina no nível da instância (passo 3) |

A segunda linha é o tipo de honestidade que vale registrar: **um defeito conhecido, com workaround e
escopo de impacto declarado**, em vez de omissão.
