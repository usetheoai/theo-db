"""Hybrid-search fusion + the 3-retriever BEIR-style eval driver (M7-S1).

`rrf_fuse` is the OFFLINE Python twin of the in-DB `ai.hybrid_search_rrf` SQL function: identical
Reciprocal Rank Fusion formula `score(d) = sum_legs 1/(k + rank)` (Cormack et al. 2009, k=60 default),
so offline scoring of the harness matches what the database returns (plan ADR D2 — one fusion definition).
"""
from __future__ import annotations

from collections import defaultdict


def rrf_fuse(leg_rankings: list, k: int = 60) -> list:
    """Fuse N ranked id-lists via RRF. Returns [(id, score), ...] sorted by score desc, id asc (stable).

    leg_rankings: list of ranked lists; each is [id_1, id_2, ...] best-first (1-based rank).
    An id absent from a leg contributes 0 for that leg (mirrors the SQL COALESCE empty-leg handling).
    """
    if k <= 0:
        raise ValueError(f"k must be > 0 (got {k})")
    scores: dict = defaultdict(float)
    for leg in leg_rankings:
        for rank, doc_id in enumerate(leg, start=1):
            scores[doc_id] += 1.0 / (k + rank)
    # deterministic tie-break: score desc, then id asc
    return sorted(scores.items(), key=lambda kv: (-kv[1], str(kv[0])))


def run_three_retrievers(db, dataset, embed_fn, dim: int, table: str = "hybrid_eval",
                         k_rrf: int = 60, top: int = 100, include_bm25: bool = False) -> dict:
    """Load the labelled corpus into a documents table, then score pure-vector / pure-FTS / RRF-hybrid
    retrievers with nDCG@10 + Recall@100 (BEIR methodology). Returns {name: {ndcg10, recall100}}.
    When include_bm25=True, also scores a `bm25` leg via pg_textsearch (requires the throwaway bm25 image).

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
    for name, retrieve in retrievers.items():
        ndcgs, recalls = [], []
        for qid, qtext in dataset.queries.items():
            qvec = embed_fn(qtext)
            ranked = retrieve(qtext, qvec)
            ndcgs.append(ndcg_at_k(ranked, dataset.qrels.get(qid, {}), 10))
            recalls.append(recall_at_n(ranked, dataset.qrels.get(qid, {}), 100))
        out[name] = {
            "ndcg10": sum(ndcgs) / len(ndcgs),
            "recall100": sum(recalls) / len(recalls),
        }
    return out
