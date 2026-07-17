---
slug: m111-vector-nodes-flow
milestone_id: M111
created_at: 2026-07-16
goal: Ship `theodb.graph_rag_search(query_embedding, ws, coll, k_entry, max_hops)` — the vector-entry→bounded-traversal→rerank flow over graph-node embeddings — proven structurally (set-hash) and measured by a stratified real-embedding recall@k eval where graph×vector ≥ pure-vector on multi-hop.
---

# M111 — vector-on-nodes + vector-entry→traversal→rerank flow

## Goal
Ship `theodb.graph_rag_search(query_embedding, ws, coll, k_entry, max_hops)` (vector-entry → bounded-traversal →
rank) over `graph_nodes.embedding`, proven structurally (hermetic set-hash) and measured by a stratified
real-embedding recall@k eval where graph×vector ≥ pure-vector on multi-hop (honest-negative on local-fact valid).

## Context
Consumes the M111 blueprint (SHIPPABLE_WITH_CAVEATS). Reuses M108/M109 traversal, M110 nodes/edges, `public.vector`
`<=>`, `ai.embed`, `ai.rerank`. ADR-1 pre-computed-embedding-in (hermetic), ADR-2 cosine-entry + proximity×weight
rank, ADR-3 stratified eval with local-fact honest-negative.

## Baseline Context
| File | LoC | Why |
|---|---|---|
| `theodb_rs/src/graph_rag.rs` | 0 (NEW) | ALTER graph_nodes ADD embedding; `graph_embed_nodes`; `graph_rag_search`; tests + eval |
| `theodb_rs/src/lib.rs` | — | `mod graph_rag;` |
| `docs/benchmarks/m111-graphrag-flow.{md,json}` | 0 (NEW) | stratified recall@k artifact |
| `CHANGELOG.md` | — | Rule 6 |
Reuse: `graph_extract.rs` (`graph_nodes`/`graph_edges`), `graph.rs` (`graph_expand`), `dtype.rs` (`<=>`),
`embed.rs` (`ai.embed`), `api.rs` (`ai.rerank`).

## Prior Art & Related Work
HippoRAG (vector-entry→graph), LazyGraphRAG (bounded traversal+rerank ≫ community cost), GraphRAG stratified eval.
Internal: M108/M109/M110.

## ADRs (from blueprint)
ADR-1 pre-computed embedding arg (hermetic; *rejected* raw-text-in). ADR-2 cosine-entry + proximity×weight rank
(*rejected* PPR = M112). ADR-3 stratified eval, local-fact honest-negative (*rejected* single-number recall).

## Phase 1 — vector-on-nodes + flow
### T1.1 — `embedding` column + `theodb.graph_embed_nodes(ws,coll,model)` + `theodb.graph_rag_search(...)`
#### Why this step
The composed in-DB flow (the payoff). Pre-computed-embedding arg → hermetically testable. Reuses vector `<=>` +
`graph_expand` + node/edge tables.
#### Files to edit
`theodb_rs/src/graph_rag.rs` (NEW), `theodb_rs/src/lib.rs`
#### TDD
- `m111_flow_structural_set_hash`: insert 5 nodes with hand-crafted 4-dim embeddings + a chain of edges; a query
  vector closest to node A; `graph_rag_search(qvec,…,k_entry=1,max_hops=2)` returns the chunk-set of A + its ≤2-hop
  neighbors (set-hash vs the expected). GWT: given node embeddings + edges, when search, then the ranked
  chunk-set == entry∪reachable.
- `m111_flow_multihop_finds_neighbor_chunk`: gold chunk belongs to a 1-hop neighbor, NOT the entry entity's chunk
  → graph flow includes it (entry-only vector would miss it). Proves the traversal adds recall.
- `m111_flow_k_entry_and_hops_bounds`: k_entry=2 seeds two entries; max_hops=0 returns only entry chunks.
- `m111_flow_empty_and_isolation`: no nodes / wrong workspace → empty, no panic; tenant-scoped.
#### Concurrency tests
(none — single-threaded).
#### Failure scenarios
Unset embedding endpoint → `graph_embed_nodes` raises typed (embed.rs). Null query vector → typed error.
#### Acceptance
Structural tests GREEN; flow returns correct ranked chunk-set.

## Phase 2 — stratified real-embedding eval (the gate)
### T2.1 — `m111_eval_stratified_recall` (real embeddings, honest-negative on local-fact)
#### Why this step
The measurement gate (ADR-3): graph×vector ≥ pure-vector on multi-hop. Real embeddings (Rule 5, no synthetic).
#### Files to edit
`theodb_rs/src/graph_rag.rs` (eval `#[pg_test]`, gated on `theodb.embedding_endpoint` set — SKIP + WARN if unset)
#### Deep dep analysis
Compact labeled corpus (~12 chunks) + queries tagged local_fact/multi_hop/global with gold chunk-ids. Embed
chunks (pure-vector baseline) + node names (graph entry) via `ai.embed`. Measure recall@k per stratum for
(a) pure-vector-over-chunks, (b) graph_rag_search. Writes `docs/benchmarks/m111-graphrag-flow.json`.
#### TDD
- `m111_eval_stratified_recall`: asserts recall_multihop(graph) ≥ recall_multihop(vector) − ε; records all
  strata honestly (local-fact may favor vector — VALID). SKIPs (WARN, no fail) when no endpoint configured.
#### Acceptance
Eval artifact written with per-stratum recall@k + methodology + source; multi-hop gate met OR honest-negative
documented with the number.

## Phase 3 — Integration Validation
Full `cargo pgrx test pg17` GREEN (0 regression). M111 tests GREEN. Eval artifact present. CHANGELOG updated.

## Coverage Matrix
| Goal claim | Tasks |
|---|---|
| vector index on graph nodes | T1.1 |
| composed vector-entry→expand→rerank flow | T1.1 |
| stratified eval, source identified, zero fabricated | T2.1 |
| graph×vector ≥ vector on multi-hop (or honest-negative) | T2.1 |

## Drawbacks & Risks
| Risk | Sev | Mitigation | Owner |
|---|---|---|---|
| Eval needs real labeled corpus not synthetic | MEDIUM | compact real-embedding corpus + identified gold; large-benchmark eval is follow-on | impl |
| Gain depends on graph quality (M110) | MEDIUM (accepted) | gate measures the flow, not a bad graph (honest) | impl |
| Paid embed API in a test | LOW | eval SKIPs when endpoint unset; structural tests hermetic | impl |

## Unresolved Questions
- Large public-benchmark eval (MuSiQue/2Wiki full) deferred; the compact real-embedding eval proves the multi-hop
  advantage directionally — honestly flagged, not hidden.

## Global DoD
TDD per task; set-hash + real-embedding eval; no new crate; full suite GREEN 0 regression; eval artifact (Rule 5);
CHANGELOG; commits without Co-Authored-By; develop.
