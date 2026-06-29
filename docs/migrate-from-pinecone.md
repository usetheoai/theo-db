# Migrate from Pinecone to TheoDB

Move your vectors **and their metadata** into a regular PostgreSQL table, so you can `JOIN` them with your
operational data and run AI in one transactional SQL — and drop the separate vector service.

> No performance claim here — the win is **one system, transactional consistency, no ETL** (see
> [`unification-1-vs-2-systems.md`](unification-1-vs-2-systems.md)). Reproducible benchmarks, when published,
> live under `docs/benchmarks/`.

## 1. Export from Pinecone

Use the Pinecone client to fetch your vectors and dump them as a JSON array of records. Each record is the
Pinecone `Vector` shape — `{ "id": str, "values": [float, …], "metadata": { … } }`:

```json
[
  {"id": "doc-1", "values": [0.12, 0.04, ...], "metadata": {"category": "shoes", "in_stock": true}},
  {"id": "doc-2", "values": [0.91, 0.10, ...], "metadata": {"category": "boots", "in_stock": false}}
]
```

(Dense vectors. Sparse vectors are not imported yet — a documented follow-up.)

## 2. Create the target table in TheoDB

Map the Pinecone fields to columns. Metadata goes to `jsonb` (and can be promoted to real relational columns
you can `JOIN`/filter on):

```sql
CREATE EXTENSION IF NOT EXISTS theodb CASCADE;

CREATE TABLE items (
  id        text PRIMARY KEY,     -- Pinecone id
  embedding vector(3),            -- Pinecone values — toy dim 3 here; use YOUR model's dim (e.g. vector(1536))
  metadata  jsonb                 -- Pinecone metadata (queryable; promote hot keys to columns)
);
```

> The example below uses `vector(3)` so it runs as-is. In production set the dimension to your embedding
> model's (e.g. `vector(1536)` for OpenAI `text-embedding-3-small`); the `values` array length must match.

| Pinecone field | TheoDB column |
|---|---|
| `id` | `id text` |
| `values` | `embedding vector(N)` |
| `metadata` | `metadata jsonb` (or promoted columns) |

## 3. Import — `theodb.import_pinecone`

Pass the exported JSON array as `jsonb`. Native jsonb parsing — no extra dependency, runs in the database:

```sql
SELECT theodb.import_pinecone(
  'items'::regclass,
  '[{"id":"doc-1","values":[0.12,0.04,0.0],"metadata":{"category":"shoes"}}]'::jsonb
);   -- returns the number of rows inserted
```

Signature: `theodb.import_pinecone(target regclass, export jsonb, id_col text DEFAULT 'id', embedding_col text
DEFAULT 'embedding', metadata_col text DEFAULT 'metadata') RETURNS integer`. It fails fast (`SQLSTATE 22023`)
on a non-array export or a record missing `id`/`values` — no partial corrupt insert. The FUNCTION is
**all-or-nothing**: the whole import runs in one transaction (best for small/atomic imports).

### Large exports — `theodb.import_pinecone_chunked` (PROCEDURE, per-batch COMMIT)

The FUNCTION holds the whole export in ONE transaction — for a large export that is unbounded memory/WAL.
For large migrations use the PROCEDURE, which ingests in `chunk_size` batches with a COMMIT per batch:

```sql
-- CALL (not SELECT) — and in autocommit (no surrounding BEGIN), because the procedure COMMITs per chunk.
CALL theodb.import_pinecone_chunked(
  'items'::regclass,
  '[{"id":"doc-1","values":[0.12,0.04,0.0],"metadata":{"category":"shoes"}}, ...]'::jsonb,
  1000               -- chunk_size (default 1000)
);
```

Signature: `theodb.import_pinecone_chunked(target regclass, export jsonb, chunk_size int DEFAULT 1000,
id_col text DEFAULT 'id', embedding_col text DEFAULT 'embedding', metadata_col text DEFAULT 'metadata')`.
Same fail-fast validation + injection-safe dynamic SQL as the FUNCTION. **It is NOT all-or-nothing**: a
mid-run failure leaves already-COMMITted batches persisted (bounded footprint + partial progress survives an
abort). Choose the FUNCTION for small/atomic imports, the PROCEDURE for large ones.

## 4. Now it's unified

Your vectors are relational rows — `JOIN`, filter, and run AI in one SQL:

```sql
SELECT i.id, i.metadata->>'category'
FROM items i
WHERE i.metadata->>'category' = 'shoes'        -- relational/JSON filter, no second system
ORDER BY i.embedding <=> '[0.1,0.2,0.0]'::vector
LIMIT 5;
```

See [`quickstart.md` § Unified query](quickstart.md) for the full vector + JOIN + AI example, and remember
`SET hnsw.iterative_scan = strict_order` to preserve recall under selective filters.
