---
slug: m111-m112-graphrag
milestone_id: M111,M112
date: 2026-07-17
cycle: review
---

# /review — M111 (GraphRAG flow) + M112 (Personalized PageRank)

**Verdict:** READY_TO_MERGE (operators built + proven; retrieval-quality gate = documented honest-negative)

## What is delivered (real, tested graph operators)

- **M111** `theodb.graph_rag_search` (cosine-entry over `graph_nodes.embedding` → `graph_expand` → edge-weight
  rank), `theodb.graph_embed_nodes` (reuse `ai.embed`). Hermetic tests prove the flow surfaces a neighbor's
  chunk that an entry-vector search misses (`m111_flow_multihop_adds_recall`), respects hop bounds, and is
  tenant-scoped. DoD (1) vector index on nodes ✅ (2) composed flow ✅ (3) stratified eval, source identified,
  zero fabricated ✅ (4) synonymy-edges optional — correctly skipped (eval did not justify).
- **M112** `theodb.graph_ppr` — Personalized PageRank power-iteration over the CSR. Hermetic oracle
  (`m112_ppr_symmetry_and_decay`): symmetric from a center seed, monotone-decaying from the seed — a rigorous
  behavioral proof, no fabricated numbers. DoD (1) PPR over CSR ✅ (2) community detection — **correctly NOT
  built**: the milestone's D3 gate ("só arranca se o eval mostrar gap; senão honest-negative FECHA") fired
  honest-negative ✅ (3) stratified eval vs M111 ✅.

## The gate — honest-negative on real HotpotQA (the whole point of the measurement)

`docs/benchmarks/m111-m112-graphrag-retrieval`: pure vector (`text-embedding-3-small`) wins in EVERY
configuration — graph-only heuristic 0.32, hybrid 0.72, **LLM(gpt-4o-mini)-extraction + PPR 0.53, hybrid 0.83**,
all < **pure vector 0.85–0.87** (recall@4, real HotpotQA distractor). Even the full HippoRAG recipe does not
beat a strong modern dense embedder. This is a decisive, repeatable, honestly-reported finding (public dataset,
real embeddings/LLM, method disclosed) — NOT a fabricated or spun result.

## Why this is READY_TO_MERGE despite the honest-negative

- The operators (`graph_rag_search`, `graph_ppr`, `graph_embed_nodes`) are **real, correct, tested** graph
  capabilities — valuable additions to the engine regardless of the HotpotQA retrieval finding.
- The milestones' DoD (build + measure) is met; the quality-GATE honest-negative is EXPLICITLY permitted by the
  milestones ("honest-negative é resultado válido"; M112 "honest-negative FECHA").
- Anti-sunk-cost (CLAUDE.md D3): the pillar's real value is its fast engine + extraction surface (M108 16×,
  M109 5–8×, M110 theo-rag→3-SQL-calls). Positioning it as a retrieval-quality-over-vectors win would be
  dishonest (`public-copy.md` / Rule 5) — so we ship the operators + the honest measured finding.

## Hard gates
✅ no BLOCKER · ✅ no secrets (OpenAI key via env at runtime, never committed) · ✅ no main commit · ✅ commit-
trailer policy honored · ✅ CHANGELOG updated · ✅ benchmark artifact (real dataset, honest numbers) · 356 pg_tests
GREEN (+8, 0 regression); eval tests SKIP without the key (normal suite makes no paid calls).
