#!/usr/bin/env python3
"""M138 — the measurement the M53 never did: FUSION-with-BM25 vs FUSION-with-ts_rank_cd.

The M53 measured the isolated legs (ts_rank_cd 0.0703, BM25 0.6881, vector 0.7296 nDCG@10 on BEIR
scifact) and the ts_rank_cd FUSION (≈ vector). It never measured the FUSION with BM25. This harness
closes that gap and, per plan ADR-1, DECIDES whether the shipped lexical default should flip.

Why the leg gap does NOT settle it (plan ADR-1): RRF fuses by rank, not score, so a 9.8×-stronger leg
can leave the fusion unmoved — the M53 already saw the ts_rank_cd fusion tie the pure vector. So the
gate is the FUSION delta with paired significance, not the leg delta. Honest-negative (no flip) is a
valid outcome, not a failure.

Honesty rails (CLAUDE.md TheoDB rules 5 + 7, public-copy.md), inherited from run_m53_hybrid_beir.py:
  * No OPENAI_API_KEY → status UNBENCHMARKED, clean exit (no fabricated numbers).
  * pg_textsearch not loaded → status SKIPPED_NO_BM25 (the bm25 fusion cannot be measured honestly).
  * Embeddings pre-warmed once and cached; the DB retriever run is offline + deterministic.
  * The decision is a paired significance test over the per-query nDCG@10, seed-pinned (reproducible).

`decide_flip` is a PURE function (no DB) so the gate logic is unit-tested offline
(theodb_bench/test_m138_decision.py), the same discipline M134 used for egress policy.
"""
import argparse
import json
import os
import sys
import time

from theodb_bench.beir import load_beir_dataset
from theodb_bench.db import VectorDB
from theodb_bench.hybrid import rrf_fuse
from theodb_bench.metrics import ndcg_at_k, recall_at_n
from theodb_bench.openai_embed import CachedOpenAIEmbedder
from theodb_bench.significance import paired_significance

PGHOST = os.environ.get("PGHOST", "localhost")
PGPORT = os.environ.get("PGPORT", "5432")
PGUSER = os.environ.get("PGUSER", "postgres")
PGPASSWORD = os.environ.get("PGPASSWORD", "postgres")
PGDATABASE = os.environ.get("PGDATABASE", "postgres")


def decide_flip(bm25_ndcgs, tsrank_ndcgs, *, alpha: float = 0.05) -> dict:
    """Decide whether the lexical default should flip to BM25 (plan ADR-1 — the milestone gate).

    Flip IFF the BM25 fusion is BETTER (mean per-query nDCG@10 delta > 0) AND that advantage is
    significant at `alpha` (paired permutation p < alpha). A worse fusion, or a tie the RRF washed
    out, is an honest-negative → no flip. `bm25_ndcgs` / `tsrank_ndcgs` are per-query, same order.
    """
    sig = paired_significance(bm25_ndcgs, tsrank_ndcgs)  # a=bm25, b=tsrank → mean_diff = bm25 − tsrank
    mean_diff = sig["mean_diff"]
    p = sig["p_permutation"]
    return {
        "flip": bool(mean_diff > 0 and p < alpha),
        "mean_diff": mean_diff,
        "p": p,
        "wins": sig["wins"],
        "losses": sig["losses"],
        "ties": sig["ties"],
        "cohens_dz": sig["cohens_dz"],
        "alpha": alpha,
    }


def _dsn() -> str:
    return f"host={PGHOST} port={PGPORT} dbname={PGDATABASE} user={PGUSER} password={PGPASSWORD}"


def _score_ranked(ranked_by_qid: dict, dataset) -> dict:
    """Per-query nDCG@10 + Recall@100 from precomputed rankings. Returns aligned {qids, ndcg10, recall100}."""
    qids, ndcgs, recalls = [], [], []
    for qid in dataset.queries:
        ranked = ranked_by_qid[qid]
        qids.append(qid)
        ndcgs.append(ndcg_at_k(ranked, dataset.qrels.get(qid, {}), 10))
        recalls.append(recall_at_n(ranked, dataset.qrels.get(qid, {}), 100))
    return {"qids": qids, "ndcg10": ndcgs, "recall100": recalls}


def _mean(xs) -> float:
    return round(sum(xs) / len(xs), 6)


def run(dataset_name: str, model: str, dim: int, cache_dir: str = "benchmarks/.cache") -> dict:
    if not os.environ.get("OPENAI_API_KEY"):
        return {"status": "UNBENCHMARKED", "reason": "no OPENAI_API_KEY — embeddings unavailable"}

    dataset = load_beir_dataset(dataset_name, cache_dir)
    embedder = CachedOpenAIEmbedder(model=model, dim=dim, cache_dir=cache_dir)
    # Pre-warm ONCE (the only OpenAI network hop), then score offline + deterministic.
    embedder.warm(list(dataset.corpus.values()) + list(dataset.queries.values()))
    embed_fn = embedder.as_embed_fn()
    table = "m138_eval"

    db = VectorDB(_dsn()).connect()
    db.ensure_extension()
    if not db.pg_textsearch_available():
        return {
            "status": "SKIPPED_NO_BM25",
            "reason": "pg_textsearch not installed or not in shared_preload_libraries — "
            "the BM25 fusion cannot be measured; run on the throwaway image with the preload set",
        }

    db.create_documents_table(table, dim)
    db.load_documents(table, {d: (c, embed_fn(c)) for d, c in dataset.corpus.items()})
    db.ensure_bm25_extension()
    db.create_bm25_index(table)

    load = round(os.getloadavg()[0], 2)
    t0 = time.time()

    # Collect the three REAL per-leg rankings from the DB (top-100 each), then fuse the two hybrids with
    # the SAME rrf_fuse twin (k=60). Fusing both hybrids identically is the apples-to-apples the decision
    # needs; the twin is byte-identical to the in-DB ai.hybrid_search_rrf (ADR D2), so the fused quality
    # equals the product's. (The in-DB lexical_engine='bm25' template itself carries a separately-tracked
    # bug on pg_textsearch 1.3.1 — the bare `<@> $bind` form needs `to_bm25query($bind, idx)`; measuring
    # via the proven twin decides the milestone without gating on that fix. See docs/benchmarks/m138.)
    K = 60
    TOP = 100
    vec_rank, ts_rank, bm_rank = {}, {}, {}
    for qid, qtext in dataset.queries.items():
        qvec = embed_fn(qtext)
        vec_rank[qid] = db.vector_query_docs(table, qvec, TOP)
        ts_rank[qid] = db.fts_query(table, qtext, TOP)
        bm_rank[qid] = db.bm25_query(table, qtext, TOP)

    hybrid_ts_ranked = {q: [d for d, _ in rrf_fuse([vec_rank[q], ts_rank[q]], k=K)][:TOP] for q in dataset.queries}
    hybrid_bm_ranked = {q: [d for d, _ in rrf_fuse([vec_rank[q], bm_rank[q]], k=K)][:TOP] for q in dataset.queries}

    vec = _score_ranked(vec_rank, dataset)
    ts_leg = _score_ranked(ts_rank, dataset)
    bm_leg = _score_ranked(bm_rank, dataset)
    hybrid_tsrank = _score_ranked(hybrid_ts_ranked, dataset)
    hybrid_bm25 = _score_ranked(hybrid_bm_ranked, dataset)

    decision = decide_flip(hybrid_bm25["ndcg10"], hybrid_tsrank["ndcg10"])

    return {
        "status": "MEASURED",
        "dataset": dataset_name,
        "model": model,
        "dim": dim,
        "k_rrf": K,
        "top": TOP,
        "fusion_method": "rrf_fuse twin (byte-identical to in-DB ai.hybrid_search_rrf, ADR D2)",
        "n_queries": len(dataset.queries),
        "n_docs": len(dataset.corpus),
        "loadavg": load,
        "elapsed_s": round(time.time() - t0, 2),
        "vector": {"ndcg10": _mean(vec["ndcg10"]), "recall100": _mean(vec["recall100"])},
        "leg_tsrank": {"ndcg10": _mean(ts_leg["ndcg10"]), "recall100": _mean(ts_leg["recall100"])},
        "leg_bm25": {"ndcg10": _mean(bm_leg["ndcg10"]), "recall100": _mean(bm_leg["recall100"])},
        "hybrid_tsrank": {
            "ndcg10": _mean(hybrid_tsrank["ndcg10"]),
            "recall100": _mean(hybrid_tsrank["recall100"]),
        },
        "hybrid_bm25": {
            "ndcg10": _mean(hybrid_bm25["ndcg10"]),
            "recall100": _mean(hybrid_bm25["recall100"]),
        },
        "decision": decision,
    }


def main() -> int:
    ap = argparse.ArgumentParser(description="M138 — BM25 fusion vs ts_rank_cd fusion (decision gate)")
    ap.add_argument("--dataset", default="scifact")
    ap.add_argument("--model", default="text-embedding-3-small")
    ap.add_argument("--dim", type=int, default=1536)
    ap.add_argument("--cache-dir", default="benchmarks/.cache")
    ap.add_argument("--out", default=None, help="write JSON here in addition to stdout")
    args = ap.parse_args()

    result = run(args.dataset, args.model, args.dim, args.cache_dir)
    payload = json.dumps(result, indent=2)
    print(payload)
    if args.out:
        with open(args.out, "w") as fh:
            fh.write(payload)

    if result["status"] == "MEASURED":
        d = result["decision"]
        print(
            f"\nvetor puro nDCG@10 = {result['vector']['ndcg10']}",
            file=sys.stderr,
        )
        print(
            f"hybrid(ts_rank_cd) nDCG@10 = {result['hybrid_tsrank']['ndcg10']}  "
            f"hybrid(bm25) nDCG@10 = {result['hybrid_bm25']['ndcg10']}",
            file=sys.stderr,
        )
        print(
            f"paired p = {d['p']:.4g}  mean_diff = {d['mean_diff']:.4g}  "
            f"wins/losses/ties = {d['wins']}/{d['losses']}/{d['ties']}  → FLIP = {d['flip']}",
            file=sys.stderr,
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
