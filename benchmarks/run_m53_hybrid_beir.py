#!/usr/bin/env python3
"""M53 — REAL BEIR hybrid retrieval: nDCG@10 × Recall@100 for vector / FTS / RRF-hybrid (+ optional BM25).

The decision-grade real slice that the CI synthetic eval (theodb_bench.beir.synthetic_dataset) only
approximates: a genuine BEIR dataset (default `scifact`) embedded with a real OpenAI model, run through
the SAME in-DB `ai.hybrid_search_rrf` fusion the product ships. Retrieval numbers here are a claim, so
this harness measures them — it never invents them.

Honesty rails (CLAUDE.md TheoDB rules 5 + 7, public-copy.md):
  * No OPENAI_API_KEY  → status UNBENCHMARKED, clean exit (no fabricated numbers).
  * Embeddings are pre-warmed ONCE and cached to disk; the DB retriever run is offline + deterministic.
  * `--runs` repeats prove determinism (identical rankings ⇒ identical scores); any variance is reported,
    not hidden.
  * BM25 leg is included only when pg_textsearch is actually available; otherwise recorded as SKIPPED.
  * Box load is recorded per M46's lesson (external contention invalidates latency; here we only score
    quality, but we still surface load for the artifact reader).

Mirrors the run_m50_sota.py harness shape (argparse, PG* env, run()->dict, main() dumps JSON + prints).
"""
import argparse
import json
import os
import sys
import time

from theodb_bench.beir import load_beir_dataset
from theodb_bench.db import VectorDB
from theodb_bench.hybrid import run_three_retrievers
from theodb_bench.openai_embed import CachedOpenAIEmbedder

PGHOST = os.environ.get("PGHOST", "localhost")
PGPORT = os.environ.get("PGPORT", "5432")
PGUSER = os.environ.get("PGUSER", "postgres")
PGPASSWORD = os.environ.get("PGPASSWORD", "postgres")
PGDATABASE = os.environ.get("PGDATABASE", "postgres")

_RETRIEVERS = ("vector", "fts", "hybrid")


def _dsn() -> str:
    return (
        f"host={PGHOST} port={PGPORT} dbname={PGDATABASE} "
        f"user={PGUSER} password={PGPASSWORD}"
    )


def _load() -> float:
    return round(os.getloadavg()[0], 2)


def _aggregate_runs(run_results: list, names: list) -> dict:
    """Fold N identical-by-design runs into per-retriever {ndcg10, recall100, deterministic, spread}.

    `deterministic` is True iff every run produced byte-identical scores for that retriever; `spread`
    is the max−min across runs (0.0 when deterministic). We report both instead of silently averaging.
    """
    agg: dict = {}
    for name in names:
        ndcgs = [r[name]["ndcg10"] for r in run_results]
        recalls = [r[name]["recall100"] for r in run_results]
        spread_ndcg = round(max(ndcgs) - min(ndcgs), 6)
        spread_recall = round(max(recalls) - min(recalls), 6)
        agg[name] = {
            "ndcg10": round(sum(ndcgs) / len(ndcgs), 6),
            "recall100": round(sum(recalls) / len(recalls), 6),
            "deterministic": spread_ndcg == 0.0 and spread_recall == 0.0,
            "spread_ndcg10": spread_ndcg,
            "spread_recall100": spread_recall,
        }
    return agg


def run(args) -> dict:
    caveats = [
        f"embeddings model={args.model} dim={args.dim} (real API, pre-warmed + disk-cached)",
        "RRF fusion is the in-DB ai.hybrid_search_rrf function (product path), not an offline twin",
    ]
    base = {
        "dataset": args.dataset, "split": args.split, "model": args.model, "dim": args.dim,
        "k_rrf": args.k_rrf, "top": args.top, "runs": args.runs,
        "limit_queries": args.limit_queries, "timestamp": None,  # stamped by the caller/CI, not here
        "nproc": os.cpu_count(), "load_pre": _load(),
    }

    if not os.environ.get("OPENAI_API_KEY"):
        caveats.append("OPENAI_API_KEY absent — no embeddings fetched; run is UNBENCHMARKED")
        return {**base, "status": "UNBENCHMARKED", "per_retriever": None,
                "bm25_status": "SKIPPED: not run (unbenchmarked)", "caveats": caveats}

    # 1. Real BEIR dataset (downloads + caches on first use; offline thereafter).
    dataset = load_beir_dataset(args.dataset, args.cache_dir, split=args.split,
                                limit_queries=args.limit_queries)
    if args.limit_queries is not None:
        caveats.append(f"SMOKE: limited to {len(dataset.queries)} queries (corpus NOT subsampled)")

    # 2. Pre-warm embeddings for the whole corpus + queries, then get a pure offline embed_fn.
    embedder = CachedOpenAIEmbedder(model=args.model, dim=args.dim, cache_dir=args.cache_dir)
    warm_start = time.perf_counter()
    embedder.warm(list(dataset.corpus.values()) + list(dataset.queries.values()))
    warm_s = round(time.perf_counter() - warm_start, 2)
    embed_fn = embedder.as_embed_fn()

    # 3. DB boundary.
    db = VectorDB(_dsn()).connect()
    db.ensure_extension()

    include_bm25 = False
    bm25_status = "SKIPPED: not requested (--include-bm25 not set)"
    if args.include_bm25:
        if db.pg_textsearch_available():
            include_bm25 = True
            bm25_status = "INCLUDED"
        else:
            bm25_status = "SKIPPED: pg_textsearch unavailable (run against packaging/Dockerfile.bm25)"
            caveats.append(bm25_status)

    names = list(_RETRIEVERS) + (["bm25"] if include_bm25 else [])

    # 4. Repeat the retriever pass --runs times to prove determinism (same data + index ⇒ same ranks).
    run_results = []
    loads = []
    for _ in range(args.runs):
        loads.append(_load())
        run_results.append(run_three_retrievers(
            db, dataset, embed_fn, args.dim, table="m53_beir_eval",
            k_rrf=args.k_rrf, top=args.top, include_bm25=include_bm25,
        ))
    db.close()

    per_retriever = _aggregate_runs(run_results, names)
    non_det = [n for n, m in per_retriever.items() if not m["deterministic"]]
    if non_det:
        caveats.append(f"NON-DETERMINISTIC retrievers across runs: {non_det} — investigate before claiming")

    return {
        **base,
        "status": "OK",
        "n_docs": len(dataset.corpus),
        "n_queries": len(dataset.queries),
        "warm_s": warm_s,
        "load_per_run": loads,
        "load_post": _load(),
        "per_retriever": per_retriever,
        "bm25_status": bm25_status,
        "caveats": caveats,
    }


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--dataset", default="scifact")
    ap.add_argument("--split", default="test")
    ap.add_argument("--model", default="text-embedding-3-small")
    ap.add_argument("--dim", type=int, default=1536)
    ap.add_argument("--top", type=int, default=100)
    ap.add_argument("--k-rrf", type=int, default=60)
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--limit-queries", type=int, default=None,
                    help="smoke only: score just the first N queries (corpus never subsampled)")
    ap.add_argument("--cache-dir", default="benchmarks/.cache")
    ap.add_argument("--out", default="docs/benchmarks/m53-hybrid-beir.json")
    ap.add_argument("--include-bm25", action="store_true")
    args = ap.parse_args()

    data = run(args)

    os.makedirs(os.path.dirname(args.out) or ".", exist_ok=True)
    with open(args.out, "w") as fh:
        json.dump(data, fh, indent=2)

    print(f"wrote {args.out}  status={data['status']}  dataset={data['dataset']}")
    if data["status"] == "UNBENCHMARKED":
        print("  UNBENCHMARKED: set OPENAI_API_KEY and re-run to produce numbers.")
        return
    print(f"  n_docs={data['n_docs']} n_queries={data['n_queries']} "
          f"model={data['model']} dim={data['dim']} bm25={data['bm25_status']}")
    for name, m in data["per_retriever"].items():
        det = "det" if m["deterministic"] else f"VAR spread={m['spread_ndcg10']}/{m['spread_recall100']}"
        print(f"  {name:>7}: nDCG@10={m['ndcg10']:.4f}  Recall@100={m['recall100']:.4f}  ({det})")


if __name__ == "__main__":
    sys.exit(main())
