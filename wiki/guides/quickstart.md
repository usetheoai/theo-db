---
type: Guide
title: Quickstart — todas as capacidades por um CREATE EXTENSION
description: Do container à superfície completa com uma única extensão; inclui a query unificada que é o diferencial do produto, e uma nota de drift sobre trechos que envelheceram.
resource: git:f7c7b93:docs/quickstart.md
tags: [guia, quickstart, onboarding, sql, docker]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: quickstart
    resource: git:f7c7b93:docs/quickstart.md
    title: TheoDB Quickstart
---

> **Nota de drift, verificada contra as decisões.** O documento de origem foi escrito antes da remoção
> do [pgvector](/technologies/pgvector.md) e do [pgvectorscale](/technologies/pgvectorscale.md)
> ([ADR 0029](/decisions/0029-m70-drop-pgvector.md)). Três trechos dele **não valem mais**, e estão
> marcados abaixo. O restante do passo a passo segue válido.

# Subir e instalar

```bash
docker run -d --name theodb -e POSTGRES_PASSWORD=postgres -p 5432:5432 \
  ghcr.io/usetheodev/theo-db:latest
```

```sql
CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE;
```

A imagem roda isso automaticamente na primeira inicialização. Em qualquer outro PostgreSQL, execute
você mesmo — **exige superusuário**, porque a extensão é marcada como tal no control file.

> ⚠️ O texto original diz que o `CASCADE` puxa `vector` e `vectorscale`. **Não puxa mais**: ambos foram
> removidos, e a extensão provê o tipo e os access methods como **código próprio**.

```sql
CREATE TABLE products (
  id              bigserial PRIMARY KEY,
  description     text,
  category_id     int,
  description_tsv tsvector GENERATED ALWAYS AS (to_tsvector('english', coalesce(description,''))) STORED,
  embedding       vector(3)
);
INSERT INTO products (description, category_id, embedding) VALUES
  ('red running shoes',      1, '[1,0,0]'),
  ('blue running shoes',     1, '[0.9,0.1,0]'),
  ('waterproof hiking boots',1, '[0,1,0]'),
  ('cotton hoodie',          2, '[0,0,1]');
```

# O passo a passo

**Busca vetorial** — ver [feature 01](/features/01-busca-similaridade-vetorial.md):

```sql
SELECT id, description FROM products ORDER BY embedding <=> '[1,0,0]'::vector LIMIT 3;
```

**Índices** — ver [HNSW](/features/02-indice-hnsw.md) e [IVFFlat](/features/03-indice-ivfflat.md):

```sql
CREATE INDEX products_hnsw ON products USING theodb_hnsw (embedding theodb_hnsw_cosine_ops);
CREATE INDEX products_ivf  ON products USING theodb_ivfflat (embedding theodb_ivfflat_cosine_ops)
  WITH (lists = 1);
SET theodb_ivfflat.probes = 1;
```

> ⚠️ O original usa `USING hnsw (embedding vector_cosine_ops)` e `SET ivfflat.probes`. A sintaxe do
> pgvector **volta a funcionar** pelos aliases do
> [ADR 0058](/decisions/0058-pgvector-compat-shim.md) — mas apontando para o **mesmo handler próprio**.
> A forma nativa acima é a canônica.

> ⚠️ O original mostra `USING diskann` para "qualidade ScaNN". **Isso não existe mais** — o
> pgvectorscale foi removido. Ver [feature 05](/features/05-indice-scann.md) para o que é entregue hoje.

**Busca híbrida** — ver [feature 06](/features/06-busca-hibrida.md):

```sql
SELECT * FROM ai.hybrid_search(jsonb_build_object(
  'table','products', 'id_col','id', 'content_tsv_col','description_tsv',
  'vector_col','embedding', 'query_text','running', 'query_vector','[1,0,0]', 'result_limit', 3));
```

**Superfície de IA** — configure uma vez e chame; ver [feature 07](/features/07-funcoes-ia-sql.md):

```sql
SET theodb.llm_endpoint = 'https://api.openai.com/v1/chat/completions';
SET theodb.llm_model    = 'gpt-4o-mini';
SET theodb.llm_api_key  = '...';                 -- a chave nunca é armazenada pelo banco

SELECT ai.generate('Write a one-line tagline for red running shoes.');
SELECT ai.generate_batch(ARRAY['Capital of France?', '2+2?']);
SELECT ai.analyze_sentiment('These shoes are fantastic!');
SELECT ai.summarize('A long product review ...');
SELECT ai.agg_summarize(description) FROM products;
SELECT ai.nl_query('how many products are in category 1?', ARRAY['products']);
```

**O banco não embarca modelo nenhum** — aponta para qualquer endpoint compatível. É a independência de
modelo que o [ADR 0002](/decisions/0002-north-star-equal-or-superior-to-alloydb.md) lista como
superioridade estrutural.

# A query unificada — o diferencial

Este é o ponto do produto: vetor, dado **relacional** e **IA** numa **SQL transacional só**, sem ETL e
sem um segundo sistema. O embedding e a linha de negócio **são a mesma linha**, logo consistentes por
construção.

```sql
SELECT p.id, p.description,
       ai.summarize(p.description) AS gist          -- perna de IA
FROM products p
JOIN inventory i ON i.product_id = p.id             -- JOIN relacional
WHERE i.in_stock AND p.category_id = 3              -- filtro relacional
ORDER BY p.embedding <=> '[0.1,0.2,...]'::vector    -- perna vetorial
LIMIT 5;
```

Um banco puramente vetorial não faz o `JOIN` contra o dado relacional na mesma query — exigiria dois
sistemas e merge na aplicação, com risco de staleness. O racional completo está no
[ADR 0005](/decisions/0005-unification-as-differentiator.md), e a medição do ganho estrutural em
[m64](/benchmarks/m64-rag-over-sql.md).

# Busca vetorial filtrada — preservar recall

Com índices aproximados, um `WHERE` seletivo pode retornar **menos linhas que o `LIMIT`**, porque o
filtro é aplicado depois do scan do índice. O caminho correto hoje é o **filtro inline** decidido no
[ADR 0040](/decisions/0040-m90-inline-label-filter-verdict.md), que mediu +0,48 de recall e ~20× de QPS
contra o post-filter a ~1% de seletividade.

> ⚠️ O original sugere `SET hnsw.iterative_scan` e o label-filtering do pgvectorscale. Ambos vinham de
> extensões **removidas**. Ver [acelerar consultas](/features/08-acelerar-consultas.md) e o ADR 0040.

Prove que o índice é usado com `EXPLAIN (ANALYZE, BUFFERS)`.

# Upgrades

```sql
ALTER EXTENSION theodb_rs UPDATE;
```

Encadeia os scripts de upgrade até a versão instalada mais nova. A disciplina que torna isso confiável
está registrada em [m137](/benchmarks/m137-upgrade-chain.md).

# Nota que continua válida

A superfície `ai.*` **não depende de linguagem não confiável** — não usa `plpython3u` —, então funciona
também em PostgreSQL gerenciado que não a habilita.
