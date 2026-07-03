# TheoDB Quickstart — all 12 capabilities via `CREATE EXTENSION theodb`

This guide takes you from a running TheoDB container to every `docs/features/` capability through a single
extension. No init-scripts, no manual wiring — one `CREATE EXTENSION theodb CASCADE`.

> No performance numbers appear here — reproducible benchmarks live under `docs/benchmarks/` when published
> (CLAUDE.md TheoDB rule 5).

## 0. Run TheoDB and install the extension

```bash
docker run -d --name theodb -e POSTGRES_PASSWORD=postgres -p 5432:5432 ghcr.io/usetheodev/theo-db:latest
```

```sql
-- Installs the whole AI + vector surface; CASCADE pulls vector and vectorscale.
CREATE EXTENSION IF NOT EXISTS theodb CASCADE;
```

The bundled image runs this automatically on first init. On any other PostgreSQL 17, run it yourself
(requires superuser — the `theodb` extension is marked `superuser` in its control file).

```sql
-- Seed a small products table used by the examples below.
CREATE TABLE products (
  id            bigserial PRIMARY KEY,
  description   text,
  category_id   int,
  description_tsv tsvector GENERATED ALWAYS AS (to_tsvector('english', coalesce(description,''))) STORED,
  embedding     vector(3)            -- toy 3-dim vectors for the walkthrough
);
INSERT INTO products (description, category_id, embedding) VALUES
  ('red running shoes',     1, '[1,0,0]'),
  ('blue running shoes',    1, '[0.9,0.1,0]'),
  ('waterproof hiking boots',1, '[0,1,0]'),
  ('cotton hoodie',         2, '[0,0,1]');
```

## 1. Vector similarity search (feature 01)

```sql
SELECT id, description FROM products ORDER BY embedding <=> '[1,0,0]'::vector LIMIT 3;
```

## 2. HNSW index (feature 02)

```sql
CREATE INDEX products_hnsw ON products USING hnsw (embedding vector_cosine_ops);
```

## 3. IVFFlat index (feature 03)

```sql
CREATE INDEX products_ivfflat ON products USING ivfflat (embedding vector_cosine_ops) WITH (lists = 1);
```

## 4. IVF family / tuning probes (feature 04)

```sql
SET ivfflat.probes = 1;  -- recall/latency knob for the IVFFlat family
SELECT id FROM products ORDER BY embedding <=> '[0.9,0.1,0]'::vector LIMIT 2;
```

## 5. ScaNN-quality ANN — StreamingDiskANN (feature 05)

```sql
-- The permissive ScaNN-quality index (pgvectorscale). See docs/adr/0004-scann-fork-decision.md.
CREATE INDEX products_diskann ON products USING diskann (embedding vector_cosine_ops);
```

## 6. Hybrid search — FTS + vector fused by RRF (feature 06)

```sql
SELECT * FROM ai.hybrid_search(jsonb_build_object(
  'table','products', 'id_col','id', 'content_tsv_col','description_tsv',
  'vector_col','embedding', 'query_text','running', 'query_vector','[1,0,0]', 'result_limit', 3));
```

## 7. Generative AI in SQL — configure once, then call (feature 07)

```sql
-- Point at any OpenAI-compatible chat endpoint (the DB ships no model).
SET theodb.llm_endpoint = 'https://api.openai.com/v1/chat/completions';
SET theodb.llm_model    = 'gpt-4o-mini';
SET theodb.llm_api_key  = '...';                 -- your key (never stored by TheoDB)
SELECT ai.generate('Write a one-line tagline for red running shoes.');
```

## 8. Accelerated / batch generation (feature 08)

```sql
SELECT ai.generate_batch(ARRAY['Capital of France?', '2+2?', 'Antonym of hot?']);
```

## 9. Rank results by relevance (feature 09)

```sql
SELECT ai.rank('best shoes for marathon training', ARRAY['red running shoes','cotton hoodie']);
```

## 10. Sentiment analysis (feature 10)

```sql
SELECT ai.analyze_sentiment('These shoes are fantastic, best purchase this year!');
```

## 11. Content summarization — scalar and aggregate (feature 11)

```sql
SELECT ai.summarize('A long product review text goes here ...');
SELECT ai.agg_summarize(description) FROM products;   -- aggregate over many rows
```

## 12. Natural language → SQL, safely (feature 12)

```sql
-- Anti-injection: generates + validates a single read-only SELECT over an allowlist, then runs it sandboxed.
SELECT ai.nl_query('how many products are in category 1?', ARRAY['products']);
```

## Unified query (the differentiator)

The point of TheoDB: vector search, your **relational** data, and **AI** in **one transactional SQL** — no
ETL, no second system. The embedding and the business row are the same row (consistent by construction).

```sql
-- vector ORDER BY + relational JOIN + filter + AI, one statement, one transaction
SET hnsw.iterative_scan = strict_order;   -- preserves recall under a selective filter (see below)
SELECT p.id, p.description,
       ai.summarize(p.description) AS gist          -- AI leg (same instance)
FROM products p
JOIN inventory i ON i.product_id = p.id             -- relational JOIN (operational data)
WHERE i.in_stock AND p.category_id = 3              -- relational filter
ORDER BY p.embedding <=> '[0.1,0.2,...]'::vector    -- vector leg (pgvector)
LIMIT 5;
```

A pure vector DB (e.g. Pinecone) cannot do the `JOIN`/`WHERE` against your relational data in the same query —
you would run two systems and merge in the app, risking staleness. Here it is one consistent SQL.

### Filtered vector search — preserve recall

With approximate indexes, a selective `WHERE` can return **fewer than `LIMIT`** rows (the filter is applied
after the index scan — "over-filtering"). Enable iterative scans so the index is scanned until `k` results are
found, in exact order:

```sql
SET hnsw.iterative_scan = strict_order;   -- or relaxed_order; bounded by hnsw.max_scan_tuples
```

For categorical low-cardinality filters, `pgvectorscale` label-filtering (a `smallint[]` label column) is an
in-index alternative. Prove the index is used with `EXPLAIN (ANALYZE, BUFFERS) SELECT … ORDER BY embedding <=> …`.

## Upgrades

```sql
ALTER EXTENSION theodb UPDATE;   -- chains theodb--X--Y.sql scripts to the newest installed version
```

## Notes

- Features 01–05 (vector + indexes) need only `vector` + `vectorscale`.
- Features 06–12 (the `ai.*` surface) are served by the Rust `theodb_rs` extension (no `plpython3u` since
  M19) and need a configured LLM endpoint. Because there is no untrusted-language dependency anymore, the
  `ai.*` surface also works on managed PostgreSQL that does not enable `plpython3u`.
