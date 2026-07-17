---
slug: m111-vector-nodes-flow
milestone_id: M111
date: 2026-07-16
cycle: discover
verdict: SHIPPABLE_WITH_CAVEATS
---

# M111 Blueprint — vector-on-nodes + vector-entry→traversal→rerank flow

Grounded in real code (M110 `graph_extract.rs` nodes/edges, `graph.rs` traversal, `dtype.rs` `public.vector`
+ `<=>` cosine, `embed.rs` `ai.embed`, `api.rs` `ai.rerank`) + prior research (HippoRAG vector-entry→PPR,
LazyGraphRAG, GraphRAG stratified eval; council dossiers from M110).

## The SOTA flow (HippoRAG/LazyGraphRAG, zero-copy single engine)

`vector-entry → bounded-traversal → rerank`: (1) embed query → cosine `<=>` top-k entry entities over
`graph_nodes.embedding` (reuse vector AM); (2) `graph_expand`/`graph_expand_multi` from the entry node-ids
(bounded ≤H hops, M108/M109); (3) collect `source_chunk_ids` from reached edges, rank by proximity + edge
weight; (4) optional `ai.rerank`. One in-DB path, zero copy — the payoff over theo-rag's multi-service flow.

## Coverage corners

- **Integration Tests:** (IT-1) hermetic structural — hand-crafted 4-dim node embeddings, known query vector →
  correct entry-node selection (cosine) + correct expansion + correct chunk collection (set-hash). (IT-2)
  stratified retrieval eval (real embeddings): recall@k of graph×vector vs pure-vector per stratum. (IT-3)
  entry-only vs expanded: multi-hop query where the answer chunk is NOT the entry entity's own chunk but a
  1-hop neighbor's — graph flow finds it, pure-vector-on-entities misses it.
- **Dependencies:** no new crate. `public.vector` + `<=>` (dtype.rs), `ai.embed` (embed.rs), `graph_expand`
  (graph.rs), `graph_nodes`/`graph_edges` (graph_extract.rs), `ai.rerank` (api.rs).
- **Tools:** `cargo pgrx test`; real OpenAI embeddings (`text-embedding-3-small`, endpoint reachable from the
  droplet, key from .env) for IT-2; set-hash oracle for IT-1.
- **Techniques (SOTA):** HippoRAG (vector-entry seeds → graph); LazyGraphRAG (bounded traversal + rerank beats
  expensive community summaries at 0.1% cost); GraphRAG/BenchmarkQED stratified eval (local/multi-hop/global);
  stratified honest-negative on local-fact is VALID (pure vector wins local factoid).

## ADRs

- **ADR-1** the flow takes a PRE-COMPUTED query embedding (`vector` arg), not raw text → hermetically testable
  (no embed call in structural tests) + composes with `ai.embed(query)` at the call site. *Rejected:*
  raw-text-in (couples the flow to the paid embed endpoint, untestable hermetically).
- **ADR-2** entry = cosine top-k over node embeddings; ranking = traversal-proximity × edge-weight (LazyGraphRAG
  bounded scoring), `ai.rerank` optional. *Rejected:* PPR for ranking (that's M112, gated on measured need).
- **ADR-3** the eval is stratified with an honest-negative allowance on local-fact (pure vector wins); the gate
  is "graph×vector ≥ pure-vector on multi-hop". *Rejected:* single-number recall (hides the stratum where graph
  helps vs hurts — the exact HippoRAG-2 warning).

## Honest caveats

- The stratified eval uses a SMALL real-embedding labeled corpus (compact but real, not synthetic embeddings —
  Rule 5). A large public-benchmark eval (MuSiQue/2Wiki) is a follow-on; the compact eval proves the flow's
  multi-hop advantage directionally with real embeddings + identified gold.
- Graph quality (M110) bounds the flow — the gate measures the flow, not a bad graph (blueprint honesty).
- `graph_build` tenant-scoping caveat (#118) carries over — eval uses a single workspace.
