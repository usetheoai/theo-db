---
scenario: theo-data-capability-on-theodb
date: 2026-07-20
operator: paulohenriquevn
outcome: pass
summary: Anchor QUERY path (theodb vectorizer column + ai.hybrid_search_rrf) exercised end-to-end on a self-hosted TheoDB with real OpenAI embeddings — fused FTS+vector top-5 returned, vector leg semantically alive.
---

# Anchor smoke — query path on self-hosted TheoDB (wired evidence)

**What ran:** `benchmarks/dogfood_anchor_smoke.sh` against a self-hosted TheoDB (PG17.10 pgrx-install +
`theodb_rs` @ v0.111.0, `shared_preload_libraries = 'theodb_rs'`, OpenAI `text-embedding-3-small`, dim 1536).

**Result: PASS** — `SMOKE_RESULT: PASS (query_path_fused_rows=5, embedded=5/5, async_worker_embedded=no)`.

## What was proven (the anchor QUERY path, with REAL embeddings)

1. `CREATE EXTENSION theodb_rs` — the own `vector` type + the `ai.*` surface + `theodb.*` are available.
2. `theodb.create_vectorizer('docs','id','body','docs','embedding', 'text-embedding-3-small', 1536, 'fixed', 512, 64)`
   creates the vectorizer + the ON-DML enqueue trigger; INSERTs enqueue jobs into `theodb.vectorizer_queue`.
3. `theodb.embed(body)` embeds real content via OpenAI (valid key, endpoint reachable, SSRF guard enforced).
4. `ai.hybrid_search_rrf('docs','id','body_tsv','embedding', query_text => '…', k=>60, …)` fuses the FTS leg and
   the **real-vector** leg via RRF and returns a ranked top-5:

   ```
    id | rrf_score |                         body
   ----+-----------+------------------------------------------------------
    2  |   0.01639 | HNSW graph index enables approximate nearest neighbo…   ← query "keep vector search fast"
    3  |   0.01613 | Reciprocal rank fusion combines a lexical BM25 leg a…
    5  |   0.01587 | A background worker keeps the embedding column fresh…
    1  |   0.01563 | PostgreSQL vacuum reclaims dead tuples and prevents …
    4  |   0.01538 | Write-ahead logging guarantees crash-safe durability…
   ```

   The vector leg is semantically alive — the query "how does the index keep vector search fast" ranks the
   HNSW/ANN doc first, which lexical-only FTS would not guarantee.

## Honest scope

- This is **`wired`** evidence, not `running`: the anchor path is exercised on self-hosted infra with real
  embeddings, but this is a smoke, not sustained real product traffic (≥30 days). The `running` flip stays
  operational/cross-repo (a capability must migrate its production retrieval here).
- The async freshness worker did NOT embed in this run (`async_worker_embedded=no`) — a real failure mode recorded
  separately (`2026-07-20-anchor-failure-modes.md`, issue #132). The query path was proven with a session
  `theodb.embed` backfill (worker-independent), which is the honest state: the retrieval works; the async
  freshness has a known gap.

## Reproduction

`bash benchmarks/dogfood_anchor_smoke.sh` with `OPENAI_API_KEY` set + a self-hosted TheoDB per
`docs/ops/self-host-quickstart.md`. No secrets in this file.
