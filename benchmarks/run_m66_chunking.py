#!/usr/bin/env python3
"""M66 — chunking strategies measured on real BEIR: doc-recall@k / nDCG@10 per strategy.

Chunking DOMINATES RAG quality — but "better" is corpus-dependent (the literature shows the same strategy
winning on one corpus and losing on another). So this harness MEASURES the real per-strategy delta rather
than asserting a winner. For each strategy the corpus is chunked (via the `theodb.chunk` SQL function — the
Rust chunker under test), each chunk is embedded (OpenAI, cached), indexed in a per-strategy chunk table,
and retrieved. A document is "recalled" if ANY of its chunks lands in the top-k (BEIR qrels are per-doc).

Honesty (blueprint (c) — the Vecta method): strategies produce chunks of very different sizes, so a fixed
top-k is UNFAIR. We use k-adaptive (`k ≈ target_tokens / avg_chunk_size`) to equalise the context budget
before comparing. Recall alone misleads (over-retrieval inflates it), so nDCG@10 is reported alongside.

The claim-bearing arithmetic (doc_recall_at_k, k_adaptive, chunk_verdict) is pure stdlib so it is
unit-testable WITHOUT a container (`benchmarks/tests/test_run_m66_chunking.py`); ndcg_at_k is reused from
`theodb_bench.metrics` (Rule 9). Mirrors `run_m65_rerank.py`.
"""
import argparse
import json
import os
import statistics

from theodb_bench.metrics import ndcg_at_k


# ── Pure arithmetic (unit-testable WITHOUT a container / model) ─────────────────────────────────

def doc_recall_at_k(ranked_doc_ids, qrels, k):
    """A doc is recalled if it appears in the top-k RANKED DOCS (deduped from chunk hits). Returns the
    fraction of relevant docs (grade > 0) present in the top-k. BEIR qrels are per-doc."""
    if k <= 0:
        raise ValueError(f"k must be > 0 (got {k})")
    relevant = {d for d, g in qrels.items() if g > 0}
    if not relevant:
        return 0.0
    topk = ranked_doc_ids[:k]
    hit = sum(1 for d in topk if d in relevant)
    return hit / len(relevant)


def k_adaptive(target_tokens, avg_chunk_chars, chars_per_token=4):
    """Equalise the context budget across strategies (blueprint (c), the Vecta method): the number of
    chunks to retrieve so all strategies deliver ~`target_tokens` of context. Larger chunks → fewer k."""
    if avg_chunk_chars <= 0:
        raise ValueError("avg_chunk_chars must be > 0")
    avg_chunk_tokens = max(1.0, avg_chunk_chars / chars_per_token)
    return max(1, round(target_tokens / avg_chunk_tokens))


def chunk_verdict(per_strategy, noise_tol=0.005):
    """Rank strategies by nDCG@10 and report the delta of the best vs the baseline (recursive). Honest:
    reports the ranking per-corpus (NOT a universal winner) + HONEST_NEGATIVE if the spread is within noise.
    `per_strategy` = {name: {ndcg10, doc_recall}}."""
    baseline = per_strategy.get("recursive", {}).get("ndcg10")
    ranked = sorted(per_strategy.items(), key=lambda kv: kv[1].get("ndcg10", 0.0), reverse=True)
    best_name, best = ranked[0]
    spread = round(best["ndcg10"] - ranked[-1][1]["ndcg10"], 6)
    status = "STRATEGY_MATTERS" if spread > noise_tol else "HONEST_NEGATIVE"
    return {
        "axis": "ndcg_at_10_per_strategy",
        "ranking": [(n, round(m.get("ndcg10", 0.0), 6)) for n, m in ranked],
        "best": best_name,
        "baseline_recursive": round(baseline, 6) if baseline is not None else None,
        "spread": spread,
        "noise_tol": noise_tol,
        "status": status,
        "note": "strategy ranking is CORPUS-DEPENDENT (literature: config X wins on corpus Y, loses on Z). "
                "HONEST_NEGATIVE if the spread is within noise (strategy choice did not move recall here).",
    }


# ── DB + model integration (needs a container + OpenAI key) ─────────────────────────────────────

PGHOST = os.environ.get("PGHOST", "localhost")
PGPORT = os.environ.get("PGPORT", "28817")
PGUSER = os.environ.get("PGUSER", "theo")
PGDATABASE = os.environ.get("PGDATABASE", "postgres")


def _conn():
    import psycopg2
    c = psycopg2.connect(host=PGHOST, port=PGPORT, user=PGUSER, dbname=PGDATABASE, connect_timeout=15)
    c.autocommit = True
    return c


def _load():
    return round(os.getloadavg()[0], 2)


def _chunk_via_sql(cur, content, strategy, size, overlap):
    """Chunk `content` using the Rust chunker under test (theodb.chunk) — this is the SUT."""
    cur.execute("SELECT theodb.chunk(%s, %s, %s, %s)", (content, strategy, size, overlap))
    row = cur.fetchone()
    return row[0] if row and row[0] else []


def _index_strategy(cur, dataset, embedder, dim, strategy, size, overlap):
    """Chunk every doc via theodb.chunk, embed each chunk, index in a per-strategy chunk table. Returns the
    average chunk length (chars) for k-adaptive. The CachedOpenAIEmbedder is cache-only after warm(), so the
    chunk texts (which differ per strategy) MUST be warmed before embed_fn is called."""
    tbl = f"m66_{strategy}"
    cur.execute(f"DROP TABLE IF EXISTS {tbl}")
    cur.execute(f"CREATE TABLE {tbl} (doc_id text, chunk_index int, emb vector({dim}))")
    # Pass 1 — chunk every doc, collect all chunk texts.
    doc_chunks = []  # (doc_id, chunk_index, chunk_text)
    for doc_id, content in dataset.corpus.items():
        for i, ch in enumerate(_chunk_via_sql(cur, content, strategy, size, overlap)):
            doc_chunks.append((doc_id, i, ch))
    # Warm ALL chunk embeddings for this strategy in one batched pass (fills the disk cache).
    embedder.warm([ch for _, _, ch in doc_chunks])
    embed_fn = embedder.as_embed_fn()
    total_chars = 0
    for doc_id, i, ch in doc_chunks:
        total_chars += len(ch)
        vec = "[" + ",".join(f"{x:.6f}" for x in embed_fn(ch)) + "]"
        cur.execute(f"INSERT INTO {tbl} VALUES (%s,%s,%s)", (doc_id, i, vec))
    cur.execute(f"CREATE INDEX {tbl}_idx ON {tbl} USING theodb_hnsw (emb theodb_hnsw_cosine_ops)")
    cur.execute("SET theodb_hnsw.ef_search = 100")
    cur.execute(f"ANALYZE {tbl}")
    return (total_chars / len(doc_chunks)) if doc_chunks else float(size)


def _retrieve_docs(cur, strategy, qvec, k_chunks):
    """Retrieve top chunks, dedup to ranked doc ids (first chunk hit determines doc rank)."""
    tbl = f"m66_{strategy}"
    cur.execute("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on")
    cur.execute(f"SELECT doc_id FROM {tbl} ORDER BY emb <=> %s LIMIT {k_chunks}", (qvec,))
    seen, ranked = set(), []
    for (doc_id,) in cur.fetchall():
        if doc_id not in seen:
            seen.add(doc_id)
            ranked.append(doc_id)
    return ranked


def run(name, size, overlap, dim, model, cache_dir, limit_queries, target_tokens):
    from theodb_bench.beir import load_beir_dataset
    from theodb_bench.openai_embed import CachedOpenAIEmbedder

    base = {"dataset": name, "chunk_size": size, "overlap": overlap, "embed_model": model, "dim": dim,
            "limit_queries": limit_queries, "target_tokens": target_tokens}
    if not os.environ.get("OPENAI_API_KEY"):
        return {**base, "status": "UNBENCHMARKED", "reason": "OPENAI_API_KEY absent"}

    dataset = load_beir_dataset(name, cache_dir=cache_dir, split="test", limit_queries=limit_queries)
    embedder = CachedOpenAIEmbedder(model=model, dim=dim, cache_dir=cache_dir)
    # Warm the query embeddings; chunk embeddings warm lazily inside _index_strategy (per-strategy chunks).
    embedder.warm(list(dataset.queries.values()))
    embed_fn = embedder.as_embed_fn()

    conn = _conn()
    cur = conn.cursor()
    strategies = ["fixed", "sentence", "recursive"]
    per_strategy = {}
    for strat in strategies:
        avg_chunk_chars = _index_strategy(cur, dataset, embedder, dim, strat, size, overlap)
        k = k_adaptive(target_tokens, avg_chunk_chars)
        ndcgs, recalls = [], []
        for qid, qtext in dataset.queries.items():
            qrels = dataset.qrels.get(qid, {})
            if not qrels:
                continue
            qvec = "[" + ",".join(f"{x:.6f}" for x in embed_fn(qtext)) + "]"
            # over-fetch chunks so the deduped doc list has ~k docs (k-adaptive on the doc budget).
            ranked_docs = _retrieve_docs(cur, strat, qvec, k * 3)
            ndcgs.append(ndcg_at_k(ranked_docs, qrels, 10))
            recalls.append(doc_recall_at_k(ranked_docs, qrels, k))
        per_strategy[strat] = {
            "ndcg10": round(statistics.mean(ndcgs), 6) if ndcgs else 0.0,
            # Per-query std of nDCG@10 — the honest error bar (council-benchmark: a Δ between strategies
            # smaller than ~std/sqrt(n) is within noise; adjacent strategies must clear it to be separable).
            "ndcg10_std": round(statistics.pstdev(ndcgs), 6) if len(ndcgs) > 1 else 0.0,
            "doc_recall": round(statistics.mean(recalls), 6) if recalls else 0.0,
            "avg_chunk_chars": round(avg_chunk_chars, 1), "k_adaptive": k, "n_queries": len(ndcgs),
        }
    cur.close()
    conn.close()
    return {**base, "status": "MEASURED", "load": _load(), "per_strategy": per_strategy,
            "verdict": chunk_verdict(per_strategy)}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dataset", default="nfcorpus")
    ap.add_argument("--chunk-size", type=int, default=512)
    ap.add_argument("--overlap", type=int, default=64)
    ap.add_argument("--dim", type=int, default=1536)
    ap.add_argument("--model", default="text-embedding-3-small")
    ap.add_argument("--cache-dir", default="/tmp/m66-beir-cache")
    ap.add_argument("--limit-queries", type=int, default=50)
    ap.add_argument("--target-tokens", type=int, default=2000)
    ap.add_argument("--out", default="docs/benchmarks/m66-chunking.json")
    a = ap.parse_args()
    res = run(a.dataset, a.chunk_size, a.overlap, a.dim, a.model, a.cache_dir, a.limit_queries, a.target_tokens)
    os.makedirs(os.path.dirname(a.out), exist_ok=True)
    with open(a.out, "w") as f:
        json.dump(res, f, indent=2)
    print(json.dumps(res.get("verdict", {"status": res.get("status")}), indent=2))
    print(f"\nwrote {a.out}")


if __name__ == "__main__":
    main()
