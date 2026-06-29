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
  embedding vector(1536),         -- Pinecone values (match your model's dimension)
  metadata  jsonb                 -- Pinecone metadata (queryable; promote hot keys to columns)
);
```

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
on a non-array export or a record missing `id`/`values` — no partial corrupt insert.

For large exports, import in batches (call once per chunk of the array).

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
