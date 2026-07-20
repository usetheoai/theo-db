---
scenario: theo-data-capability-on-theodb
date: 2026-07-20
operator: paulohenriquevn
outcome: pass
summary: Anchor QUERY path (theodb vectorizer column + ai.hybrid_search_rrf) exercised end-to-end on a self-hosted TheoDB with real OpenAI embeddings — a genuine two-leg FTS+vector RRF fusion proven (a doc ranked by BOTH legs scores 1/61+1/61=0.032787, above the single-leg ceiling).
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
4. `ai.hybrid_search_rrf('docs','id','body_tsv','embedding', query_text => 'vector search', k=>60, …)` fuses the
   FTS leg and the **real-vector** leg via RRF. Query = `'vector search'` (its lexemes `vector & search` match the
   FTS `plainto_tsquery` AND-filter on the HNSW/ANN doc, so BOTH legs are alive):

   ```
    id | rrf_score |                        body
   ----+-----------+----------------------------------------------------
    2  |  0.032787 | HNSW graph index enables approximate nearest neigh…   ← in BOTH legs: 1/61 + 1/61
    3  |  0.016129 | Reciprocal rank fusion combines a lexical BM25 leg…    (vector leg only: 1/62)
    5  |  0.015873 | A background worker keeps the embedding column fre…    (vector leg only: 1/63)
    1  |  0.015625 | PostgreSQL vacuum reclaims dead tuples and prevent…    (vector leg only: 1/64)
    4  |  0.015385 | Write-ahead logging guarantees crash-safe durabili…   (vector leg only: 1/65)
   ```

   **Two-leg fusion is proven, not assumed:** doc 2 scores `0.032787 = 1/(60+1) + 1/(60+1)` — it is rank-1 in the
   FTS leg AND rank-1 in the vector leg, so RRF **sums** the two terms. That sum is above the single-leg rank-1
   ceiling `1/61 ≈ 0.016393` (the smoke asserts `max_score > 1/61`). The other docs score a single `1/(60+rank)`
   term (vector leg only). Leg liveness measured directly: `FTS matches=1, vector-embedded=5`.

   > An earlier version of this run used the query "how does the index keep vector search fast"; under
   > `plainto_tsquery` AND-semantics NO doc contained all its lexemes, so the FTS leg was empty and the "fusion"
   > was vector-leg-only (caught in review). The query above fixes that; the smoke now asserts both legs are
   > non-empty AND that the max score exceeds the single-leg ceiling, so a vector-leg-only or FTS-leg-only result
   > would FAIL the gate.

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
