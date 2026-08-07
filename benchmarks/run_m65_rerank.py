#!/usr/bin/env python3
"""M65 — ai.rerank cross-encoder measured on real BEIR: nDCG@10/MRR@10 WITH vs WITHOUT rerank.

The `ai.rerank` surface running is NOT the milestone — the milestone is whether reranking the retrieval
top-k IMPROVES nDCG@10 on real labelled data (BEIR/SciFact). The literature is explicit that the gain is
NOT universal (off-the-shelf cross-encoders degraded nDCG −0.3% to −3.1% on out-of-distribution corpora),
so this harness measures the REAL delta and the DoD accepts an honest-negative.

Two arms over the SAME retrieval top-k (the rerank only reorders — it cannot add evidence, so Recall@k is
conserved by construction, a sanity check):

  A_baseline — nDCG@10 / MRR@10 over the top-K ranked by vector distance (theodb_hnsw).
  B_rerank   — rerank the top-K document TEXTS via ai.rerank (cross-encoder) → new top-10 → nDCG@10/MRR@10.

The claim-bearing arithmetic (mrr_at_k, rerank_verdict) is pure stdlib so it is unit-testable WITHOUT a
container or a model (`benchmarks/tests/test_run_m65_rerank.py`); ndcg_at_k / recall_at_n are reused from
`theodb_bench.metrics` (Rule 9 — do not reinvent metrics). Mirrors `run_m53_hybrid_beir.py`.
"""
import argparse
import json
import os
import statistics
import time

from theodb_bench.metrics import latency_percentiles, ndcg_at_k, recall_at_n


# ── Pure arithmetic (unit-testable WITHOUT a container / model) ─────────────────────────────────

def mrr_at_k(ranked_ids, qrels, k):
    """Mean Reciprocal Rank contribution for ONE query: 1/rank of the first relevant doc (grade > 0)
    within the top-k, else 0.0. The caller averages over queries. BEIR-style graded qrels."""
    if k <= 0:
        raise ValueError(f"k must be > 0 (got {k})")
    relevant = {doc_id for doc_id, grade in qrels.items() if grade > 0}
    for i, doc_id in enumerate(ranked_ids[:k]):
        if doc_id in relevant:
            return 1.0 / (i + 1)
    return 0.0


def rerank_verdict(baseline_ndcg, rerank_ndcg, noise_tol=0.005):
    """Honest per-axis verdict of rerank vs baseline on nDCG@10. delta = rerank − baseline.

    PASS            — delta > noise_tol (rerank measurably improves).
    HONEST_NEGATIVE — delta <= noise_tol (no improvement OR regression) — reported with the number, NOT spin.
    """
    delta = round(rerank_ndcg - baseline_ndcg, 6)
    if delta > noise_tol:
        status = "PASS"
    else:
        status = "HONEST_NEGATIVE"
    return {
        "axis": "ndcg_at_10",
        "baseline": round(baseline_ndcg, 6),
        "rerank": round(rerank_ndcg, 6),
        "delta": delta,
        "noise_tol": noise_tol,
        "status": status,
        "note": "rerank only reorders the SAME top-k (Recall@k conserved); the milestone is whether it "
                "improves nDCG@10 on real BEIR. HONEST_NEGATIVE if delta <= noise (literature: gain is "
                "not universal, cross-encoders can regress on out-of-distribution corpora).",
    }


# ── DB + model integration (needs a container + a rerank endpoint) ──────────────────────────────

PGHOST = os.environ.get("PGHOST", "localhost")
PGPORT = os.environ.get("PGPORT", "28817")
PGUSER = os.environ.get("PGUSER", "theo")
PGPASSWORD = os.environ.get("PGPASSWORD", "")
PGDATABASE = os.environ.get("PGDATABASE", "postgres")


def _conn():
    import psycopg2
    kw = dict(host=PGHOST, port=PGPORT, user=PGUSER, dbname=PGDATABASE, connect_timeout=15)
    if PGPASSWORD:
        kw["password"] = PGPASSWORD
    c = psycopg2.connect(**kw)
    c.autocommit = True
    return c


def _load():
    return round(os.getloadavg()[0], 2)


def _index_corpus(cur, dataset, embed_fn, dim):
    """Create the eval table, insert the corpus with embeddings, build the theodb_hnsw index."""
    cur.execute("DROP TABLE IF EXISTS m65_rerank")
    cur.execute(f"CREATE TABLE m65_rerank (doc_id text PRIMARY KEY, content text, emb vector({dim}))")
    for doc_id, content in dataset.corpus.items():
        vec = "[" + ",".join(f"{x:.6f}" for x in embed_fn(content)) + "]"
        cur.execute("INSERT INTO m65_rerank VALUES (%s, %s, %s)", (doc_id, content, vec))
    cur.execute("CREATE INDEX m65_idx ON m65_rerank USING theodb_hnsw (emb theodb_hnsw_cosine_ops)")
    cur.execute("SET theodb_hnsw.ef_search = 100")
    cur.execute("ANALYZE m65_rerank")


def _retrieve_topk(cur, qvec, k):
    """Retrieve the top-k (doc_id, content) by vector distance (theodb_hnsw)."""
    cur.execute("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on")
    cur.execute(
        f"SELECT doc_id, content FROM m65_rerank ORDER BY emb <=> %s LIMIT {k}", (qvec,)
    )
    return cur.fetchall()


def _rerank(cur, query, docs):
    """Call ai.rerank(query, docs[]) → list of (input_idx, score) sorted by score DESC (the surface does
    the sort). Returns the reordered indices into `docs`."""
    cur.execute(
        "SELECT idx, score FROM ai.rerank(%s, %s::text[], NULL, NULL) ORDER BY score DESC",
        (query, docs),
    )
    return [row[0] for row in cur.fetchall()]


def run(name, k, runs, dim, model, cache_dir, limit_queries=None):
    from theodb_bench.beir import load_beir_dataset
    from theodb_bench.openai_embed import CachedOpenAIEmbedder

    base = {"dataset": name, "top_k": k, "runs": runs, "embed_model": model, "dim": dim,
            "limit_queries": limit_queries}
    if not os.environ.get("OPENAI_API_KEY"):
        return {**base, "status": "UNBENCHMARKED",
                "reason": "OPENAI_API_KEY absent — no embeddings fetched"}

    dataset = load_beir_dataset(name, cache_dir=cache_dir, split="test", limit_queries=limit_queries)
    embedder = CachedOpenAIEmbedder(model=model, dim=dim, cache_dir=cache_dir)
    embedder.warm(list(dataset.corpus.values()) + list(dataset.queries.values()))
    embed_fn = embedder.as_embed_fn()

    conn = _conn()
    cur = conn.cursor()
    # Point ai.rerank at the configured cross-encoder endpoint (the rerank_server / a real provider).
    rerank_endpoint = os.environ.get("THEODB_RERANK_ENDPOINT")
    if not rerank_endpoint:
        conn.close()
        raise SystemExit("THEODB_RERANK_ENDPOINT not set — cannot call ai.rerank (fail-fast, no lixo).")
    cur.execute("SET theodb.rerank_endpoint = %s", (rerank_endpoint,))
    _index_corpus(cur, dataset, embed_fn, dim)

    per_run = []
    loads = []
    for _ in range(runs):
        loads.append(_load())
        ndcg_a, ndcg_b, mrr_a, mrr_b, rec_a, rec_b = [], [], [], [], [], []
        rerank_lat = []
        for qid, qtext in dataset.queries.items():
            qrels = dataset.qrels.get(qid, {})
            if not qrels:
                continue
            qvec = "[" + ",".join(f"{x:.6f}" for x in embed_fn(qtext)) + "]"
            topk = _retrieve_topk(cur, qvec, k)
            ids = [r[0] for r in topk]
            docs = [r[1] for r in topk]
            # Arm A — vector order.
            ndcg_a.append(ndcg_at_k(ids, qrels, 10))
            mrr_a.append(mrr_at_k(ids, qrels, 10))
            rec_a.append(recall_at_n(ids, qrels, k))
            # Arm B — rerank the same top-k.
            t0 = time.perf_counter()
            order = _rerank(cur, qtext, docs)
            rerank_lat.append((time.perf_counter() - t0) * 1000.0)
            reranked_ids = [ids[i] for i in order]
            ndcg_b.append(ndcg_at_k(reranked_ids, qrels, 10))
            mrr_b.append(mrr_at_k(reranked_ids, qrels, 10))
            rec_b.append(recall_at_n(reranked_ids, qrels, k))
        perc = latency_percentiles(rerank_lat) if rerank_lat else {"p50": 0.0, "p95": 0.0}
        per_run.append({
            "A_ndcg10": round(statistics.mean(ndcg_a), 6), "B_ndcg10": round(statistics.mean(ndcg_b), 6),
            "A_mrr10": round(statistics.mean(mrr_a), 6), "B_mrr10": round(statistics.mean(mrr_b), 6),
            "A_recall_k": round(statistics.mean(rec_a), 6), "B_recall_k": round(statistics.mean(rec_b), 6),
            "rerank_p50_ms": round(perc["p50"], 3), "rerank_p95_ms": round(perc.get("p95", 0.0), 3),
            "n_queries": len(ndcg_a),
        })
    cur.close()
    conn.close()

    def _agg(key):
        vals = [r[key] for r in per_run]
        return round(statistics.mean(vals), 6)

    a_ndcg, b_ndcg = _agg("A_ndcg10"), _agg("B_ndcg10")
    return {
        **base, "status": "MEASURED", "load_per_run": loads, "per_run": per_run,
        "per_arm": {
            "A_baseline": {"ndcg10": a_ndcg, "mrr10": _agg("A_mrr10"), "recall_k": _agg("A_recall_k")},
            "B_rerank": {"ndcg10": b_ndcg, "mrr10": _agg("B_mrr10"), "recall_k": _agg("B_recall_k"),
                         "rerank_p50_ms": _agg("rerank_p50_ms"), "rerank_p95_ms": _agg("rerank_p95_ms")},
        },
        "recall_conserved": abs(_agg("A_recall_k") - _agg("B_recall_k")) < 1e-6,
        "verdict": rerank_verdict(a_ndcg, b_ndcg),
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dataset", default="scifact")
    ap.add_argument("--top-k", type=int, default=50)
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--dim", type=int, default=1536)
    ap.add_argument("--model", default="text-embedding-3-small")
    ap.add_argument("--cache-dir", default="/tmp/m65-beir-cache")
    ap.add_argument("--limit-queries", type=int, default=None, help="cap queries for CPU-rerank tractability")
    ap.add_argument("--out", default="benchmarks/artifacts/m65-rerank.json")
    a = ap.parse_args()
    res = run(a.dataset, a.top_k, a.runs, a.dim, a.model, a.cache_dir, a.limit_queries)
    os.makedirs(os.path.dirname(a.out), exist_ok=True)
    with open(a.out, "w") as f:
        json.dump(res, f, indent=2)
    print(json.dumps(res.get("verdict", {"status": res.get("status")}), indent=2))
    print(f"\nwrote {a.out}")


if __name__ == "__main__":
    main()
