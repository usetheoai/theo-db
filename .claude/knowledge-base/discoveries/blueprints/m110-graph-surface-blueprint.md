---
slug: m110-graph-surface
milestone_id: M110
date: 2026-07-16
cycle: discover
verdict: SHIPPABLE_WITH_CAVEATS
---

# M110 Blueprint — in-DB graph surface (`ai.extract_graph` + `theodb.graph_expand`) theo-rag adopts

Two independent council agents (ai-in-db + research-adr), R0 web evidence (GraphRAG arXiv:2404.16130,
HippoRAG 2405.14831 + HippoRAG-2 2502.14802, KGGen/MINE 2502.09956, GPT-NER 2304.10428, GLiNER 2311.08526,
GraphRAG dataflow, PostgresML token-classification). Lens: **graph QUALITY ≠ traversal speed** — a fast engine
over a bad graph is worthless; the milestone spine is the extraction-quality gate.

## Ground truth (real code)

- **Port source (theo-rag):** `packages/core/src/domain/extraction/graph-extractor.ts` (heuristic: capitalized-run
  entities + windowed co-occurrence edges, pure/dependency-free), `llm-entity-extractor.ts`/`llm-graph-extractor.ts`
  (MIT-ported GraphRAG delimited prompt `<|>`/`##`/`<|COMPLETE|>` + bounded gleaning), `graph-store.ts` (idempotent
  `ON CONFLICT … weight+=EXCLUDED.weight` upsert), `graph-retriever.ts` (anchor-match → ≤3-hop CTE walk).
  **theo-rag's gate is stratified recall@k** (`eval/run-graphrag-baseline.ts`, strata local_fact/multi_hop/
  global_sensemaking) — **NO entity-F1 gate exists anywhere**.
- **Reuse targets (theodb, Rule 9):** `ai_op.rs` (ai.* schema, `ai_generate_batch`, `ai.call_count` budget,
  newline-collapse prompt-injection guard), `chat.rs` (`chat`, parity test model `'parity'`), `nl.rs` (L1/L2/L4
  NL→SQL security posture), `graph.rs` (M108/M109 `graph_build`/`graph_expand`/`graph_expand_multi` — extraction feeds).

## Coverage Corner 1 — Integration Tests

- **IT-1 cross-language parity:** `ai.extract_entities/graph(text)` returns the SAME normalized entity/edge set as
  theo-rag `graph-extractor.ts` on a ~50-chunk fixture (the `chat.rs` parity pattern; port-correctness gate).
- **IT-2 idempotent upsert:** re-ingest a chunk twice → mention_count/weight doubled, ZERO new rows (`ON CONFLICT`).
- **IT-3 extraction→CSR→expand E2E:** `ai.extract_graph` → upsert → `graph_build` → `graph_expand` returns the
  chunks the theo-rag recursive-CTE returns (SET-HASH oracle `bit_xor(hashint8)`).
- **IT-4 LLM opt-in budget:** `use_llm=>true` issues a BOUNDED round-trip count (`ai.call_count` before/after),
  fail-soft on endpoint failure (deterministic `'parity'` model in tests — never paid/flaky).
- **IT-5 tenant isolation:** `graph_nodes`/`graph_edges` carry `workspace_id`+`collection_id`; two workspaces never cross.
- **IT-6 THE quality gate:** stratified recall@k of the in-DB graph ≥ theo-rag heuristic baseline − ε (a regression
  is a valid NO-GO). Extrinsic, not intrinsic.

## Coverage Corner 2 — Dependencies

**No new crate** — everything is a port or reuse. LLM HTTP + budget = `chat.rs`/`ai_op.rs`; batched inference =
`ai_generate_batch`; heuristic = pure Rust port (no regex crate, `nl.rs` stdlib-scan precedent); traversal =
M108/M109; schema bootstrap = `theodb_schema_bootstrap`. REJECTED: spaCy/GLiNER/NER-model as a hard dep (Rule 9 /
D1 license risk) — recorded as a future opt-in path (PostgresML `token-classification` exists, MIT, but heavy).

## Coverage Corner 3 — Tools

`cargo pgrx test` / `#[pg_test]` (deterministic `'parity'` LLM model); SET-HASH oracle (IT-3); theo-rag TS fixture
harness (dump graph-extractor.ts entities/edges JSON → IT-1 golden set); `ai.call_count` (IT-4 budget + wiring
metric); labeled quality corpus (theo-rag's stratified gold-chunk corpus; optional external anchor = MINE);
`docs/benchmarks/m110-extraction-quality.*` (Rule 5 artifact).

## Coverage Corner 4 — Techniques (SOTA)

- **Heuristic quality is a FLOOR, honestly.** MINE (KGGen, arXiv:2502.09956): OpenIE 29.84% vs GraphRAG-LLM 47.80%
  vs KGGen 66.07% — LLM ≈ 2× heuristic; theo-rag's co-occurrence heuristic is BELOW OpenIE. LLM helps
  multi-hop/global, NOT local-fact (GraphRAG global-sensemaking thesis; HippoRAG-2 warns a bad graph can DROP
  below plain vector RAG). → heuristic-default is defensible ONLY because the gate measures good-enough-vs-baseline.
- **Gate = extrinsic stratified recall@k** (GraphRAG/HippoRAG/theo-rag all use downstream, never entity-F1;
  extraction-F1 needs a gold KG we don't have and doesn't predict retrieval).
- **LLM path = ported GraphRAG prompt** (delimited tuples, types [organization,person,geo,event], temp 0) +
  bounded gleaning (logit-bias Y/N continuation, maxGleanings≈1, cost-metered; recall lever ~2× entity refs at
  large chunks).
- **Idempotent upsert = normalize-then-ON CONFLICT** (nodes: normalized_name; edges: unordered normalized pair,
  weight-accumulate) + orphan-edge filter. HippoRAG synonymy-merge (cosine>τ) DEFERRED to M111 (gated on eval).

## ADRs

- **ADR-1 heuristic-default, LLM opt-in.** Alt: LLM-default (rejected — ~75% indexing cost, paid/flaky, breaks
  parity-first); GLiNER-default (rejected M110 — ML dep/license; future upgrade). Honest ceiling: heuristic
  recall < LLM (§Corner 4) — accepted because the gate decides good-enough.
- **ADR-2 extrinsic recall@k is the gate, not intrinsic F1.** Alt: entity-F1-vs-gold-KG (rejected — no gold KG,
  doesn't predict retrieval); "it builds a graph, ship it" (rejected — the exact failure the lens exists to catch).
- **ADR-3 parameterized-data-only; NO query generation from untrusted text.** Alt: `extract_and_query` convenience
  (rejected — reopens NL→SQL injection; if ever needed inherit `nl.rs` L1/L2/L4 + council-security). LLM output is
  parsed-never-executed; prompt-injection blast radius = the row's own graph rows.
- **ADR-4 bigint node-ids (CSR-compatible), normalized_name identity.** Alt: UUID (rejected — CSR needs dense
  non-negative ints); reuse M108 generic edge table (rejected — extraction needs typed cols). Dedicated
  `graph_nodes`/`graph_edges` that are already CSR-shaped.

## Recommended API + schema

```sql
ai.extract_entities(text, use_llm bool DEFAULT false, model text DEFAULT NULL)
  RETURNS TABLE(name text, normalized_name text, type text, mention_count int);
ai.extract_graph(text, use_llm bool DEFAULT false, model text DEFAULT NULL)
  RETURNS TABLE(src_normalized text, dst_normalized text, weight int, description text);
theodb.graph_upsert(workspace_id text, collection_id text, source_chunk_id text, text text,
                    use_llm bool DEFAULT false) RETURNS bigint;  -- rows affected (idempotent extract+upsert)
-- tables: theodb.graph_nodes(id bigserial, workspace_id, collection_id, name, normalized_name,
--   type default 'entity', mention_count, UNIQUE(ws,coll,normalized_name));
--   theodb.graph_edges(id, ws, coll, src_id bigint, dst_id bigint, weight, description,
--   source_chunk_ids text[], UNIQUE(ws,coll,src_id,dst_id) canonical src≤dst)
-- all REVOKE ALL FROM PUBLIC.
```

## Security (Risk a)

Parameterized `INSERT … ON CONFLICT` (unnest($n::text[])) — extraction emits DATA, never generated SQL.
`graph_build` identifier args via `format('%I')` (M108). LLM path: newline-collapse (`ai_op.rs`), operator-config
endpoint (no SSRF), output parsed-never-executed, REVOKE-from-PUBLIC + "never GRANT to isolated role" COMMENT.
M110 ships NO query-generating surface (YAGNI + security). council-security signs off before any such extension.

## theo-rag integration proof (the payoff)

theo-rag `graph` strategy sheds its `extraction/` + `graph-store/` modules + hand-written recursive CTE, replaced
by 3 SQL calls: `ai.extract_graph`→`theodb.graph_upsert` (ingest), `theodb.graph_build` (once), and anchor→
`theodb.graph_expand_multi` (retrieve). Proof = IT-3 SET-HASH equality + IT-6 recall non-regression: same answers,
faster traversal, less theo-rag code.

## Honest caveats

- Heuristic recall/precision < LLM (~2× on MINE); co-occurrence edges carry no relation label. A bad graph can
  REDUCE recall (HippoRAG-2) → ADR-2 makes extrinsic recall a hard NO-GO. Expect the heuristic to struggle on
  multi-hop/global; the honest outcome there is "ship heuristic for local-fact, enable the (built) LLM path where
  the eval proves it's needed" — never "extraction is fine" without the stratified number in docs/benchmarks/.
- The extrinsic quality benchmark (IT-6) is the heavy part: needs theo-rag's stratified corpus + an eval run
  (OpenAI key for the LLM path). This is the milestone's true cost, not the mechanical extraction port.
