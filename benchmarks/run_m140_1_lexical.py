#!/usr/bin/env python3
"""M140.1 runner — own BM25 (Tantivy) vs baseline ts_rank_cd, two axes (T2.1).

Axis 1 (graded quality): BEIR nDCG@10 with human qrels — reproduces/extends M138.
Axis 2 (lexical trace-like): public LogHub corpus, known-item MRR@10/Success@1 — the
proxy the owner chose (ADR D1), with the domain-fidelity caveat carried into the report.

Rigor (theodb-evolution Phase-2 measurement invariant): >=3 seeds on the log axis,
mean +/- std reported, paired permutation test, no cherry-pick. PG unavailable -> the
ts_rank axis is `skipped` with a reason, never a fabricated number.

Usage:
  python3 run_m140_1_lexical.py --smoke --out /tmp/m140_smoke.json
  python3 run_m140_1_lexical.py --dataset hdfs --seeds 3 --n 2000 --out benchmarks/artifacts/m140-1-data/logproxy.json
  python3 run_m140_1_lexical.py --beir scifact,nfcorpus --out benchmarks/artifacts/m140-1-data/beir.json
"""
from __future__ import annotations

import argparse
import json
import os
import random
import statistics
from pathlib import Path

from theodb_bench import beir, knownitem, lexical_engines
from theodb_bench.metrics import ndcg_at_k
from theodb_bench.significance import paired_significance

K = 10
DEFAULT_DSN = os.environ.get(
    "M140_DSN", "host=127.0.0.1 port=55432 dbname=postgres user=postgres password=postgres"
)


def _verdict(bm25: list[float], base: list[float]) -> dict:
    """Paired significance + a derived flip (base -> bm25 is worth it iff p<0.05 and bm25 higher)."""
    if len(bm25) < 2 or len(bm25) != len(base):
        return {"skipped": True, "reason": f"insufficient paired samples ({len(bm25)} vs {len(base)})"}
    sig = paired_significance(bm25, base)
    flip = bool(sig["p_permutation"] < 0.05 and sig["mean_diff"] > 0)
    return {
        "mean_bm25": statistics.fmean(bm25),
        "mean_base": statistics.fmean(base),
        "mean_diff": sig["mean_diff"],
        "p_permutation": sig["p_permutation"],
        "wins": sig["wins"],
        "losses": sig["losses"],
        "ties": sig["ties"],
        "cohens_dz": sig["cohens_dz"],
        "flip": flip,
        "n_queries": len(bm25),
    }


def _connect_pg(dsn: str):
    """Return a connected PgTsRank or None (with the reason printed) — never fabricate."""
    try:
        return lexical_engines.PgTsRank(dsn).connect()
    except Exception as e:
        print(f"[m140.1] ts_rank axis SKIPPED — no PostgreSQL at {dsn}: {e}")
        return None


def run_beir_axis(datasets: list[str], cache_dir: str, dsn: str, limit_q: int | None) -> list[dict]:
    results = []
    for name in datasets:
        ds = (
            beir.synthetic_dataset()
            if name == "_synthetic"
            else beir.load_beir_dataset(name, cache_dir=cache_dir, limit_queries=limit_q)
        )
        tv = lexical_engines.TantivyBM25()
        tv.index(ds.corpus)
        pg = _connect_pg(dsn)
        if pg is not None:
            pg.index(ds.corpus)
        bm25_ndcg, base_ndcg = [], []
        for qid, qtext in ds.queries.items():
            qrels = ds.qrels.get(qid, {})
            if not qrels:
                continue
            bm25_ndcg.append(ndcg_at_k(tv.search(qtext, K), qrels, K))
            if pg is not None:
                base_ndcg.append(ndcg_at_k(pg.search(qtext, K), qrels, K))
        entry = {
            "axis": "beir",
            "dataset": name,
            "n_docs": len(ds.corpus),
            "n_queries": len(bm25_ndcg),
            "mean_ndcg_bm25": statistics.fmean(bm25_ndcg) if bm25_ndcg else 0.0,
        }
        if pg is not None:
            entry["mean_ndcg_base"] = statistics.fmean(base_ndcg) if base_ndcg else 0.0
            entry["verdict"] = _verdict(bm25_ndcg, base_ndcg)
            pg.close()
        else:
            entry["verdict"] = {"skipped": True, "reason": "no PostgreSQL for ts_rank baseline"}
        results.append(entry)
    return results


# Known-item query lengths swept for transparency. m=1 is the FAIREST ranking test
# (a single distinctive term — the OR-vs-AND query-parser difference is inert for one
# term, so the gap is pure ranking + tokenization). Larger m increasingly stresses the
# Tantivy-default(OR) vs websearch_to_tsquery(AND) semantics difference, NOT ranking —
# so m=5 inflates the gap as an ARTIFACT (disclosed in the report, never the headline).
QUERY_LENGTHS = (1, 2, 3, 5)
PRIMARY_M = 2  # realistic "find the trace with these couple of distinctive terms"


def run_logproxy_axis(dataset: str, seeds: list[int], n: int, cache_dir: str, dsn: str) -> dict:
    from theodb_bench.logcorpus import load_logcorpus

    pg = _connect_pg(dsn)
    storage: dict = {}
    # sweep[m] = {"seed_bm25": [[per-query MRR for seed0], [seed1], ...], "seed_base": [...]}
    # Per-seed paired arrays are kept SEPARATE (M140.1 review M1): the 3 seeds share the
    # same corpus, so pooling their per-query obs into one paired test is pseudo-replication.
    sweep = {m: {"seed_bm25": [], "seed_base": []} for m in QUERY_LENGTHS}

    for si, seed in enumerate(seeds):
        docs = load_logcorpus(dataset=dataset, n=n, cache_dir=cache_dir, seed=seed)
        ids = list(docs.keys())
        rng = random.Random(seed)
        targets = ids if len(ids) <= 300 else rng.sample(ids, 300)

        tv = lexical_engines.TantivyBM25(path=str(Path(cache_dir) / f"tvidx-{dataset}-{seed}") if si == 0 else None)
        tv.index(docs)
        if pg is not None:
            pg.index(docs)
        if si == 0:
            storage = {"tantivy_index_bytes": tv.index_bytes(), "tantivy_ingest_ms": tv.ingest_ms}
            if pg is not None:
                total = pg.index_bytes()
                tsv = pg.tsv_column_bytes()
                storage["pg_total_relation_bytes"] = total  # faithful baseline (theo-lens materialises tsv)
                storage["pg_gin_index_bytes"] = pg.gin_index_bytes()  # apples-to-apples peer of tantivy index
                storage["pg_tsv_column_bytes"] = tsv
                storage["pg_lean_footprint_bytes"] = total - tsv  # heap+gin+pkey+toast, no redundant tsv col

        for m in QUERY_LENGTHS:
            s_bm25, s_base = [], []
            for tid in targets:
                q = knownitem.make_known_item_query(docs[tid], random.Random(hash((seed, tid, m)) & 0xFFFF), m=m)
                s_bm25.append(knownitem.mrr_at_k(tv.search(q, K), tid, K))
                if pg is not None:
                    s_base.append(knownitem.mrr_at_k(pg.search(q, K), tid, K))
            sweep[m]["seed_bm25"].append(s_bm25)
            if pg is not None:
                sweep[m]["seed_base"].append(s_base)

    def _agg(rec: dict) -> dict:
        per_seed_bm25_mean = [statistics.fmean(s) for s in rec["seed_bm25"]]
        e = {
            "mrr_bm25_mean": statistics.fmean(per_seed_bm25_mean),
            "mrr_bm25_std": statistics.pstdev(per_seed_bm25_mean) if len(per_seed_bm25_mean) > 1 else 0.0,
        }
        if pg is None or not rec["seed_base"]:
            e["verdict"] = {"skipped": True, "reason": "no PostgreSQL for ts_rank baseline"}
            return e
        per_seed_base_mean = [statistics.fmean(s) for s in rec["seed_base"]]
        e["mrr_base_mean"] = statistics.fmean(per_seed_base_mean)
        e["mrr_base_std"] = statistics.pstdev(per_seed_base_mean) if len(per_seed_base_mean) > 1 else 0.0
        # One VALID paired permutation test per seed (300 obs each) — no cross-seed pooling.
        per_seed = [_verdict(rec["seed_bm25"][i], rec["seed_base"][i]) for i in range(len(rec["seed_bm25"]))]
        valid = [v for v in per_seed if not v.get("skipped")]
        e["per_seed_verdict"] = per_seed
        if not valid:
            e["verdict"] = {"skipped": True, "reason": "no valid per-seed pairs"}
            return e
        max_p = max(v["p_permutation"] for v in valid)   # worst-case p across seeds
        min_diff = min(v["mean_diff"] for v in valid)    # worst-case effect across seeds
        e["verdict"] = {
            "flip": bool(max_p < 0.05 and min_diff > 0),  # holds iff EVERY seed flips
            "p_permutation": max_p,
            "mean_diff": min_diff,
            "wins": sum(v["wins"] for v in valid),
            "losses": sum(v["losses"] for v in valid),
            "ties": sum(v["ties"] for v in valid),
            "n_queries_per_seed": len(rec["seed_bm25"][0]),
            "n_seeds": len(valid),
            "note": "per-seed paired permutation (same corpus, distinct target samples); flip=all seeds, p=worst-case — no pooled pseudo-replication (review M1)",
        }
        return e

    m_sweep = {str(m): _agg(sweep[m]) for m in QUERY_LENGTHS}
    out = {
        "axis": "logproxy",
        "dataset": dataset,
        "seeds": seeds,
        "caveat": "PUBLIC LOG PROXY (LogHub HDFS_2k) — NOT theo-lens production traffic; real-trace validation is M140.4/M141 (ADR D1)",
        "methodology_note": (
            "Known-item MRR@10. m = query length (distinctive terms). m=1 is the fairest RANKING test "
            "(single term neutralises the Tantivy-default-OR vs websearch-AND parser difference). Large m "
            "inflates the gap via query-semantics/tokenisation, NOT ranking — an ARTIFACT, not a headline."
        ),
        "primary_m": PRIMARY_M,
        "primary": m_sweep[str(PRIMARY_M)],
        "m_sweep": m_sweep,
        "storage": storage,
    }
    out["verdict"] = m_sweep[str(PRIMARY_M)]["verdict"]  # top-level alias (gate/smoke compat)
    if pg is not None:
        pg.close()
    return out


def main(argv=None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--smoke", action="store_true", help="tiny synthetic run, no network")
    ap.add_argument("--beir", default="", help="comma list of BEIR datasets (e.g. scifact,nfcorpus)")
    ap.add_argument("--dataset", default="", help="log dataset for the log-proxy axis (hdfs/bgl/_fixture)")
    ap.add_argument("--seeds", type=int, default=3)
    ap.add_argument("--n", type=int, default=2000)
    ap.add_argument("--cache-dir", default=".cache140")
    ap.add_argument("--dsn", default=DEFAULT_DSN)
    ap.add_argument("--out", required=True)
    args = ap.parse_args(argv)

    report: dict = {"schema_version": "m140.1-v1", "k": K, "axes": []}
    if args.smoke:
        report["axes"] += run_beir_axis(["_synthetic"], args.cache_dir, args.dsn, None)
        report["axes"].append(run_logproxy_axis("_fixture", [0], 20, args.cache_dir, args.dsn))
    else:
        if args.beir:
            report["axes"] += run_beir_axis(args.beir.split(","), args.cache_dir, args.dsn, None)
        if args.dataset:
            seeds = list(range(args.seeds))
            report["axes"].append(run_logproxy_axis(args.dataset, seeds, args.n, args.cache_dir, args.dsn))

    Path(args.out).parent.mkdir(parents=True, exist_ok=True)
    Path(args.out).write_text(json.dumps(report, indent=2))
    print(f"[m140.1] wrote {args.out} ({len(report['axes'])} axes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
