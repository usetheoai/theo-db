"""Hybrid-search fusion + the 3-retriever BEIR-style eval driver (M7-S1).

`rrf_fuse` is the OFFLINE Python twin of the in-DB `ai.hybrid_search_rrf` SQL function: identical
Reciprocal Rank Fusion formula `score(d) = sum_legs 1/(k + rank)` (Cormack et al. 2009, k=60 default),
so offline scoring of the harness matches what the database returns (plan ADR D2 — one fusion definition).
"""
from __future__ import annotations

from collections import defaultdict


def rrf_fuse(leg_rankings: list, k: int = 60, weights: list | None = None) -> list:
    """Fuse N ranked id-lists via (weighted) RRF. Returns [(id, score), ...] sorted score desc, id asc.

    leg_rankings: list of ranked lists; each is [id_1, id_2, ...] best-first (1-based rank).
    An id absent from a leg contributes 0 for that leg (mirrors the SQL COALESCE empty-leg handling).
    weights: optional per-leg weight (M106 — twin of the in-DB `vector_weight`/`text_weight`). Default is
    1.0 for every leg (byte-identical to the pre-M106 unweighted fusion): `score(d) = sum_legs w_leg/(k+rank)`.
    Each weight must be a finite number >= 0 (a negative weight would invert the fusion).
    """
    if k <= 0:
        raise ValueError(f"k must be > 0 (got {k})")
    if weights is None:
        weights = [1.0] * len(leg_rankings)
    if len(weights) != len(leg_rankings):
        raise ValueError(f"weights length {len(weights)} != legs {len(leg_rankings)}")
    for w in weights:
        if w != w or w in (float("inf"), float("-inf")) or w < 0:  # finite and >= 0
            raise ValueError(f"weight must be a finite number >= 0 (got {w})")
    scores: dict = defaultdict(float)
    for leg, w in zip(leg_rankings, weights):
        for rank, doc_id in enumerate(leg, start=1):
            scores[doc_id] += w * (1.0 / (k + rank))
    # deterministic tie-break: score desc, then id asc
    return sorted(scores.items(), key=lambda kv: (-kv[1], str(kv[0])))


def run_three_retrievers(db, dataset, embed_fn, dim: int, table: str = "hybrid_eval",
                         k_rrf: int = 60, top: int = 100, include_bm25: bool = False,
                         return_per_query: bool = False) -> dict:
    """Load the labelled corpus into a documents table, then score pure-vector / pure-FTS / RRF-hybrid
    retrievers with nDCG@10 + Recall@100 (BEIR methodology). Returns {name: {ndcg10, recall100}}.
    When include_bm25=True, also scores a `bm25` leg via pg_textsearch (requires the throwaway bm25 image).

    When return_per_query=True, the result also carries a `_per_query` key holding, per retriever, the
    aligned per-query score lists ({"qids", "ndcg10", "recall100"}) — so a caller can audit the score
    DISTRIBUTION (mean can hide a cherry-picked query) instead of trusting the aggregate alone. Callers
    that ignore the flag are unaffected: the default return shape is unchanged.

    embed_fn(text) -> vector (deterministic lexical embedder for CI, or theodb.embed for the real eval).
    """
    from theodb_bench.metrics import ndcg_at_k, recall_at_n

    db.create_documents_table(table, dim)
    db.load_documents(table, {doc_id: (content, embed_fn(content)) for doc_id, content in dataset.corpus.items()})

    retrievers = {
        "vector": lambda qtext, qvec: db.vector_query_docs(table, qvec, top),
        "fts": lambda qtext, qvec: db.fts_query(table, qtext, top),
        "hybrid": lambda qtext, qvec: db.hybrid_rrf_docs(table, qtext, qvec, k_rrf, top),
    }

    # M7-S2 (measurement-first): optionally add the pg_textsearch BM25 leg (throwaway image only).
    if include_bm25:
        db.ensure_bm25_extension()
        db.create_bm25_index(table)
        retrievers["bm25"] = lambda qtext, qvec: db.bm25_query(table, qtext, top)

    out: dict = {}
    per_query: dict = {}
    for name, retrieve in retrievers.items():
        qids, ndcgs, recalls = [], [], []
        for qid, qtext in dataset.queries.items():
            qvec = embed_fn(qtext)
            ranked = retrieve(qtext, qvec)
            qids.append(qid)
            ndcgs.append(ndcg_at_k(ranked, dataset.qrels.get(qid, {}), 10))
            recalls.append(recall_at_n(ranked, dataset.qrels.get(qid, {}), 100))
        out[name] = {
            "ndcg10": sum(ndcgs) / len(ndcgs),
            "recall100": sum(recalls) / len(recalls),
        }
        per_query[name] = {"qids": qids, "ndcg10": ndcgs, "recall100": recalls}
    if return_per_query:
        out["_per_query"] = per_query
    return out
