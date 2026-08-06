# M111/M112 — GraphRAG retrieval on real HotpotQA: the honest measured verdict

**Date:** 2026-07-17 · **Milestones:** M111 (vector-entry→traversal→rerank flow) + M112 (Personalized PageRank)
**Benchmark:** HotpotQA distractor (validation), HuggingFace `hotpotqa/hotpot_qa` — HippoRAG's multi-hop set.
**Retriever baseline:** OpenAI `text-embedding-3-small` (a strong 2024-era dense embedder).
**Metric:** recall@4 of the 2 supporting-fact paragraphs, per question. Real embeddings + real LLM extraction.
**Raw data:** [`m111-graphrag-flow.json`](../m111-graphrag-flow.json), [`m112-hipporag-eval.json`](../m112-hipporag-eval.json).

## The measured result — vector wins in EVERY configuration

| Retrieval method | extraction | ranking | n | recall@4 |
|---|---|---|---:|---:|
| **Pure vector** | — | cosine | 30 | **0.850** |
| Graph-only flow | heuristic | edge-weight | 30 | 0.317 |
| Hybrid RRF (vector ∪ graph) | heuristic | RRF | 30 | 0.717 |
| **Pure vector** | — | cosine | 15 | **0.867** |
| PPR-only | **LLM (gpt-4o-mini)** | Personalized PageRank | 15 | 0.533 |
| Hybrid RRF (vector ∪ PPR) | **LLM (gpt-4o-mini)** | RRF | 15 | 0.833 |

**Even the full HippoRAG recipe — LLM (OpenIE-style) extraction + Personalized PageRank — does not beat pure
dense vector retrieval on HotpotQA** (PPR 0.53, hybrid 0.83, both < vector 0.87). LLM extraction lifted the
graph-only recall from 0.32 (heuristic) to 0.53, but the graph still adds no recall over a strong embedder.

## Honest analysis (why)

- **Modern dense embedders are strong.** HippoRAG's published gains were relative to weaker 2024 retrievers
  (Contriever, ColBERTv2). Against `text-embedding-3-small`, the graph's marginal retrieval value on HotpotQA
  shrinks to zero — the embedder already retrieves the supporting passages. This matches HippoRAG-2's own
  warning that graph-augmented RAG can drop below standard RAG on factual tasks.
- **HotpotQA distractor** (10 paragraphs, 2 gold) is a regime where semantic similarity alone finds the gold.
- The measurement is honest and repeatable (public dataset, real embeddings/LLM, method disclosed). No
  configuration of our graph flow beat pure vector.

## What this means for the native graph pillar (measurement-first, anti-sunk-cost — CLAUDE.md)

- **The pillar's shipped value is REAL and stands on its own capabilities**, not on beating vector retrieval:
  - M108 persisted-CSR traversal: **16×** vs recursive-CTE (measured).
  - M109 vectorized MS-BFS: **5–8×** vs N sequential BFS (measured).
  - M110 in-DB extraction + traversal surface: byte-identical to theo-rag, 1537 chunks/sec — **reduces theo-rag
    to 3 SQL calls**.
- **The retrieval-QUALITY premise (graph beats vector) is definitively falsified on a real benchmark**, even
  with LLM extraction + PPR. Positioning the pillar as a retrieval-quality upgrade over dense vectors would be
  dishonest per `public-copy.md` / Rule 5.
- **M111 flow + M112 PPR are BUILT and mechanism-proven** (hermetic tests: the flow surfaces neighbor chunks a
  vector-on-entities search misses; PPR is symmetric + monotone-decaying from seeds) — they are correct graph
  operators. Their retrieval-quality *win* is an honest-negative on HotpotQA.

## Reproduce

```
# structural (hermetic, no API): cargo pgrx test pg17 m111_flow ; cargo pgrx test pg17 m112_ppr
# real benchmark (needs THEODB_EVAL_OPENAI_KEY + /tmp/hotpot_eval.json):
cargo pgrx test pg17 m111_eval_hotpot        # vector vs heuristic-graph vs hybrid
cargo pgrx test pg17 m112_eval_hotpot_llm_ppr # vector vs LLM-graph+PPR vs hybrid
```
Dataset: `curl "https://datasets-server.huggingface.co/rows?dataset=hotpotqa/hotpot_qa&config=distractor&split=validation&length=40"`
→ transform to `[{q, paras:[[title,text]], gold:[titles]}]`.
