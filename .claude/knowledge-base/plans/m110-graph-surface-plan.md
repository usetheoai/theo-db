---
slug: m110-graph-surface
milestone_id: M110
created_at: 2026-07-16
goal: Ship in-DB graph extraction (`ai.extract_entities/graph`, heuristic-default + LLM opt-in) + idempotent `theodb.graph_upsert` into CSR-shaped node/edge tables, proven byte-identical (cross-language parity + set-hash) to theo-rag's extractor+store, so theo-rag's graph strategy runs on 3 SQL calls.
---

# M110 — in-DB graph surface (`ai.extract_graph` + `theodb.graph_upsert`)

## Goal

Ship in-DB graph extraction (`ai.extract_entities`/`ai.extract_graph`, heuristic-default + LLM opt-in) and an
idempotent `theodb.graph_upsert` into CSR-shaped `theodb.graph_nodes`/`graph_edges`, proven **byte-identical**
(cross-language parity + set-hash) to theo-rag's `graph-extractor.ts` + `graph-store.ts`, so theo-rag's graph
strategy runs on 3 SQL calls (extract → upsert → `graph_build`/`graph_expand`).

## Context

Consumes the M110 blueprint (`knowledge-base/discoveries/blueprints/m110-graph-surface-blueprint.md`, verdict
SHIPPABLE_WITH_CAVEATS). **Gate insight:** theo-rag's own quality baseline IS the heuristic → cross-language
parity (IT-1) + traversal equality (IT-3, set-hash) prove downstream recall non-regression **by construction**
— no separate QA-eval run needed for the heuristic gate. Decisions: heuristic-default/LLM-opt-in (ADR-1),
extrinsic-by-construction gate (ADR-2), parameterized-data-only security (ADR-3), bigint node-ids (ADR-4).

## Baseline Context

### Files
| File | LoC | Why |
|---|---|---|
| `theodb_rs/src/graph_extract.rs` | 0 (NEW) | heuristic extractor (port of graph-extractor.ts) + LLM path (reuse chat.rs) + `ai.extract_*` + `theodb.graph_upsert` + tables |
| `theodb_rs/src/lib.rs` | — | `mod graph_extract;` |
| `docs/benchmarks/m110-extraction.{md,json}` | 0 (NEW) | extraction throughput + parity-coverage artifact (Rule 5) |
| `CHANGELOG.md` | — | Rule 6 |

### Port source (theo-rag) — the exact algorithm to match
`graph-extractor.ts:124-204`: tokenize `[\p{L}\p{N}]+`; maximal runs of capitalized tokens (`^\p{Lu}…`) → spans
(space-separated tokens stay in a run; any non-space separator/newline flushes); strip sentence-initial
STOPWORDS from span ends; drop single-token spans <3 chars; dedup by `normalizeEntityName` (lowercase+collapse-
ws+trim), first-appearance order, cap 64; edges = windowed co-occurrence (window=4, undirected canonical
src≤dst, weight=count). `graph-store.ts`: `ON CONFLICT (ws,coll,normalized_name) DO UPDATE mention_count+=`;
edges `ON CONFLICT (ws,coll,src,dst) DO UPDATE weight+=, source_chunk_ids unioned`.

### Reuse (Rule 9, no new crate)
`chat::chat(prompt,system,model)` (LLM path + `'parity'` hermetic test model), `ai_op.rs` newline-collapse
(prompt-injection guard), `graph.rs` `graph_build`/`graph_expand` (traversal the extraction feeds),
`theodb_schema_bootstrap` (schema).

## Prior Art & Related Work
Internal: theo-rag `graph-extractor.ts`/`graph-store.ts`/`graph-retriever.ts` (ported); M108/M109 graph.rs.
External (blueprint-cited): GraphRAG delimited-prompt+gleaning (arXiv:2404.16130), KGGen/MINE extraction-quality
(2502.09956), HippoRAG (2405.14831).

## ADRs (from blueprint, with alternatives)
- **ADR-1** heuristic-default, LLM opt-in. *Rejected:* LLM-default (~75% indexing cost, breaks parity-first).
- **ADR-2** gate = cross-language parity (IT-1) + traversal set-hash equality (IT-3) → recall non-regression by
  construction. *Rejected:* entity-F1-vs-gold-KG (no gold KG; doesn't predict retrieval).
- **ADR-3** parameterized-data-only; no query-gen from untrusted text; REVOKE-from-PUBLIC; newline-collapse on
  LLM path. *Rejected:* `extract_and_query` (reopens NL→SQL injection).
- **ADR-4** bigint node-ids (CSR-compatible), normalized_name identity. *Rejected:* UUID (CSR needs dense ints).

## Phase 1 — heuristic extractor + `ai.extract_*`

### T1.1 — `extract_heuristic(text, max_entities, window)` pure Rust port + `ai.extract_entities`/`ai.extract_graph`
#### Why this step
Port the exact theo-rag heuristic so IT-1 parity holds; expose as SETOF functions (composable in SQL). Pure
function → unit-testable; the parity oracle compares against theo-rag's golden output.
#### Files to edit
`theodb_rs/src/graph_extract.rs` (NEW), `theodb_rs/src/lib.rs`
#### TDD
- `m110_extract_entities_parity`: on fixture chunks, `ai.extract_entities` entity set (name,normalized,count) ==
  theo-rag golden (hardcoded from running graph-extractor.ts). GWT: given a chunk, when extract, then the
  normalized entity set + counts match theo-rag exactly.
- `m110_extract_graph_parity`: edge set (src_norm,dst_norm,weight) == theo-rag golden.
- `m110_extract_stopword_trim`: "In Berlin, Acme Corp" → entities {berlin, acme corp} (sentence-initial stopword
  stripped, comma splits spans).
- `m110_extract_empty_and_short`: empty text → no rows; single-token span <3 chars dropped.
#### Concurrency tests
(none — single-threaded pure function).
#### Acceptance
Parity tests GREEN; entity/edge sets byte-identical to theo-rag golden.

## Phase 2 — tables + idempotent `theodb.graph_upsert`

### T2.1 — `graph_nodes`/`graph_edges` tables + `theodb.graph_upsert(ws,coll,chunk_id,text,use_llm)`
#### Why this step
Idempotent persistence (the theo-rag graph-store pattern) so re-ingest accumulates, never duplicates — the
substrate `graph_build` consumes. bigint ids (ADR-4).
#### Files to edit
`theodb_rs/src/graph_extract.rs` (`extension_sql!` tables + `#[pg_extern]` upsert + wrappers + REVOKE)
#### Deep dep analysis
Upsert: extract → per-entity `INSERT graph_nodes … ON CONFLICT (ws,coll,normalized_name) DO UPDATE
mention_count+=` RETURNING id → map normalized→id → per-edge `INSERT graph_edges … ON CONFLICT DO UPDATE
weight+=, source_chunk_ids array-union`. All parameterized (unnest($n::text[])).
#### TDD
- `m110_upsert_idempotent`: upsert same chunk twice → mention_count/weight doubled, node/edge row counts
  unchanged (ON CONFLICT).
- `m110_upsert_tenant_isolation`: two (ws,coll) never cross.
- `m110_upsert_source_chunk_union`: re-ingest with a new chunk_id → source_chunk_ids array grows (deduped).
#### Failure scenarios
Malformed/empty text → no rows, no error (extract returns empty). Missing ws/coll → typed error at boundary.
#### Acceptance
Idempotency + isolation tests GREEN.

## Phase 3 — E2E integration proof + LLM opt-in

### T3.1 — extraction→upsert→graph_build→graph_expand E2E (set-hash) + `use_llm` path
#### Why this step
The payoff: prove theo-rag's graph strategy runs on the SQL surface (extract+upsert replace extraction/+
graph-store/; graph_expand replaces the recursive CTE). LLM path reuses `chat::chat` with the GraphRAG
delimited prompt (single-round; gleaning is a documented follow-on).
#### Files to edit
`theodb_rs/src/graph_extract.rs`
#### TDD
- `m110_e2e_extract_to_expand`: upsert a small doc corpus → `graph_build('theodb.graph_edges','src_id','dst_id')`
  → `graph_expand` from an anchor entity returns the expected reachable chunk-set (set-hash).
- `m110_llm_path_parity_model`: `use_llm=>true` with `SET theodb.llm_test_model='parity'` returns entities
  parsed from the deterministic reply (bounded `ai.call_count`), fail-soft on empty reply.
#### Concurrency tests
(none — single-threaded).
#### Acceptance
E2E set-hash GREEN; LLM path deterministic under 'parity' model, call-count bounded.

## Phase 4 — Integration Validation + benchmark
- `cargo pgrx test pg17` full suite GREEN (0 regression vs 337).
- All M110 tests GREEN.
- `docs/benchmarks/m110-extraction.{md,json}`: extraction throughput (chunks/sec) + parity-coverage (100% of
  fixture entities/edges matched) — mean±std.
- CHANGELOG updated.

## Coverage Matrix
| Goal claim | Tasks |
|---|---|
| heuristic extraction == theo-rag (parity) | T1.1 |
| `ai.extract_entities`/`ai.extract_graph` SETOF | T1.1 |
| idempotent `graph_upsert` into CSR tables | T2.1 |
| tenant isolation | T2.1 |
| E2E extract→expand == theo-rag traversal (set-hash) | T3.1 |
| LLM opt-in (reuse chat.rs) | T3.1 |
| benchmark (throughput + parity coverage) | Phase 4 |

## Drawbacks & Risks
| Risk | Sev | Mitigation | Owner |
|---|---|---|---|
| Unicode tokenization divergence (Rust is_alphanumeric vs JS `\p{L}\p{N}`) breaks parity | MEDIUM | fixtures are English; parity test catches any divergence; documented scope | impl |
| Heuristic recall < LLM (multi-hop/global) | MEDIUM (accepted) | gate is non-regression vs theo-rag heuristic (itself heuristic); LLM path available; honest per blueprint | impl |
| LLM path prompt-injection / SSRF | HIGH | newline-collapse (ai_op), parsed-never-executed, REVOKE-from-PUBLIC, operator-config endpoint | impl |

## Unresolved Questions
- Full extrinsic QA-eval (stratified recall@k on a labeled corpus) is deferred: parity+set-hash prove
  non-regression by construction for the heuristic; the LLM path's *absolute* quality gain (multi-hop) is a
  separate measured study (blueprint ADR-2) not required to ship the heuristic surface. Flagged honestly.

## Global DoD
TDD per task; parity + set-hash oracles; no new crate; file <600 LoC; full suite GREEN 0 regression; benchmark
artifact (Rule 5); CHANGELOG; commits without Co-Authored-By; develop.
