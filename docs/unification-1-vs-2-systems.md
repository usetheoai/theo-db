# Unification: one system vs two (simplicity & consistency)

This is **not a speed comparison** (TheoDB's vector performance is competitive, not a marketing number — see
`docs/adr/0005-unification-as-differentiator.md`). It compares **operational simplicity and data
consistency**: doing a filtered, AI-augmented vector search in **one** transactional SQL (TheoDB) vs **two**
systems (a vector DB + Postgres) glued in the application.

## The task

"Return the 5 most similar **in-stock** products in category 3, with an AI summary of each."

## TheoDB — one system, one transaction

```sql
SET hnsw.iterative_scan = strict_order;
SELECT p.id, ai.summarize(p.description) AS gist
FROM products p
JOIN inventory i ON i.product_id = p.id
WHERE i.in_stock AND p.category_id = 3
ORDER BY p.embedding <=> '[0.1,0.2,...]'::vector
LIMIT 5;
```

One statement. The vector, the relational `in_stock`/`category`, and the AI call are the **same transaction**
over the **same rows** — no staleness window, no sync job.

## Two systems — vector DB (Pinecone) + Postgres

```python
# 1. query the vector DB (system A) — vector only, metadata filter only
res = pinecone_index.query(vector=q, top_k=50, filter={"category_id": 3})
ids = [m.id for m in res.matches]

# 2. fetch authoritative relational state from Postgres (system B) — is it ACTUALLY in stock NOW?
rows = pg.execute("SELECT id, description FROM products p JOIN inventory i ON i.product_id=p.id "
                  "WHERE p.id = ANY(%s) AND i.in_stock", (ids,))

# 3. re-rank/merge in the app, re-apply top_k after the relational filter dropped some
final = merge_and_take(rows, ids, k=5)

# 4. call the LLM per row in the app
summaries = [llm.summarize(r.description) for r in final]

# ...plus: an ETL/sync job keeping Pinecone vectors in step with Postgres writes (eventual consistency)
```

## The honest scorecard

| Dimension | TheoDB (1 system) | Vector DB + Postgres (2 systems) |
|---|---|---|
| Systems to run/operate | **1** | 2 (+ a sync pipeline) |
| Moving parts for this query | 1 SQL statement | 2 queries + app merge + per-row LLM calls |
| Consistency of vector ↔ relational state | **transactional (0 staleness)** | eventual — the vector DB can be stale vs Postgres writes |
| Filter correctness | relational `WHERE` in the same plan (recall preserved via `iterative_scan`) | metadata filter in system A, re-filtered in app; `top_k` may under-return after the relational filter |
| Data to keep in sync | none (same rows) | embeddings duplicated; ETL required |

This is the unification moat: **fewer systems, no ETL, transactional consistency** — not a speed claim.
