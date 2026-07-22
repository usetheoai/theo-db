#!/usr/bin/env python3
"""M140.3 — benchmark da engine BM25 de produção in-PG (T3.1).

Dois eixos, medidos contra um PG com a extensão `theodb_rs` (feature spike-lexical) instalada:

  1. LATÊNCIA (o Goal): `bm25_search` (com cache) vs `lexical_spike_search` (reload-por-query do
     spike M139) no MESMO corpus. Gate: latência com cache < 50% do reload.
  2. nDCG@10 (DoD-2): BEIR scifact indexado via `bm25_build`, queries via `bm25_search`, nDCG com
     `theodb_bench.metrics` — confirma a qualidade da engine na FORMA FINAL (in-PG). Compara com a
     baseline `pg_textsearch` do M138 (referência) e o ts_rank (o baseline shipado do theo-lens).

Uso: python3 run_m140_3_engine.py --dsn "host=127.0.0.1 port=PORT ..." --out result.json
     python3 run_m140_3_engine.py --smoke --out /tmp/s.json   (só valida o shape, N pequeno, sem BEIR)
"""
from __future__ import annotations

import argparse
import json
import statistics
import time

import psycopg2


def _latency_axis(conn, n_docs: int, k_queries: int) -> dict:
    """Indexa n_docs (produção + spike) e mede a latência de K buscas: cache vs reload-por-query."""
    cur = conn.cursor()
    cur.execute("DROP TABLE IF EXISTS m140_3_bench")
    cur.execute("CREATE TABLE m140_3_bench (id bigint, body text)")
    # corpus determinístico: cada doc tem um termo distintivo (docNNN) + ruído comum
    cur.executemany(
        "INSERT INTO m140_3_bench VALUES (%s, %s)",
        [(i, f"doc{i} lazy quick fox jumping sleeping dogs vixens number {i % 97}") for i in range(n_docs)],
    )
    conn.commit()

    # engine de produção (id+body) + índice do spike (body only, bulk determinístico)
    cur.execute("SELECT bm25_build(700, 'm140_3_bench', 'id', 'body')")
    cur.execute("SELECT lexical_spike_bulk_index(701, %s)", (n_docs,))
    conn.commit()

    def _time(sql: str, args, runs: int) -> list[float]:
        samples = []
        for _ in range(runs):
            t0 = time.perf_counter()
            cur.execute(sql, args)
            cur.fetchall()
            samples.append((time.perf_counter() - t0) * 1000.0)
        return samples

    # aquece a primeira chamada (cache miss / primeiro load) fora da medição de ambos
    cur.execute("SELECT count(*) FROM bm25_search(700, 'lazy', 10)")
    cur.execute("SELECT lexical_spike_search(701, 'lazy')")
    conn.commit()

    cache = _time("SELECT count(*) FROM bm25_search(700, 'lazy', 10)", None, k_queries)
    reload = _time("SELECT lexical_spike_search(701, 'lazy')", None, k_queries)

    mean_cache = statistics.fmean(cache)
    mean_reload = statistics.fmean(reload)
    return {
        "n_docs": n_docs,
        "k_queries": k_queries,
        "latency_cache_ms_mean": mean_cache,
        "latency_cache_ms_p50": statistics.median(cache),
        "latency_reload_ms_mean": mean_reload,
        "latency_reload_ms_p50": statistics.median(reload),
        "cache_over_reload_ratio": (mean_cache / mean_reload) if mean_reload > 0 else None,
        "gate_cache_under_half_reload": bool(mean_cache < 0.5 * mean_reload),
    }


def _ndcg_axis(conn, dataset: str, cache_dir: str) -> dict:
    """Indexa um corpus BEIR via bm25_build, roda as queries via bm25_search, computa nDCG@10 in-PG."""
    from theodb_bench.beir import load_beir_dataset
    from theodb_bench.metrics import ndcg_at_k

    ds = load_beir_dataset(dataset, cache_dir=cache_dir)
    cur = conn.cursor()
    cur.execute("DROP TABLE IF EXISTS m140_3_beir")
    cur.execute("CREATE TABLE m140_3_beir (id bigint, body text)")
    # doc_id BEIR é string; mapeia para bigint estável por ordem
    doc_ids = list(ds.corpus.keys())
    id_map = {doc_id: i for i, doc_id in enumerate(doc_ids)}
    cur.executemany(
        "INSERT INTO m140_3_beir VALUES (%s, %s)",
        [(id_map[d], ds.corpus[d]) for d in doc_ids],
    )
    conn.commit()
    cur.execute("SELECT bm25_build(800, 'm140_3_beir', 'id', 'body')")
    conn.commit()

    rev = {v: k for k, v in id_map.items()}
    ndcgs = []
    for qid, qtext in ds.queries.items():
        qrels = ds.qrels.get(qid, {})
        if not qrels:
            continue
        cur.execute(
            "SELECT id FROM bm25_search(800, %s, 10) ORDER BY score DESC LIMIT 10", (qtext,)
        )
        ranked = [rev[r[0]] for r in cur.fetchall()]
        ndcgs.append(ndcg_at_k(ranked, qrels, 10))
    return {
        "dataset": dataset,
        "n_docs": len(ds.corpus),
        "n_queries": len(ndcgs),
        "ndcg_at_10_mean": statistics.fmean(ndcgs) if ndcgs else 0.0,
    }


def main(argv=None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dsn", default="host=127.0.0.1 port=28951 dbname=postgres user=postgres")
    ap.add_argument("--smoke", action="store_true")
    ap.add_argument("--beir", default="scifact")
    ap.add_argument("--n-sweep", default="2000,10000,50000", dest="n_sweep")
    ap.add_argument("--cache-dir", default=".cache140")
    ap.add_argument("--out", required=True)
    args = ap.parse_args(argv)

    conn = psycopg2.connect(args.dsn)
    conn.autocommit = False
    report: dict = {"schema_version": "m140.3-v1", "axes": []}

    if args.smoke:
        report["axes"].append(_latency_axis(conn, n_docs=200, k_queries=5))
    else:
        # Sweep de N: o custo de reload (load do heap + rebuild) escala com o tamanho do índice,
        # enquanto a busca com cache é ~flat. O ganho do cache cresce com N (M140.3 achado honesto).
        for n in (int(x) for x in args.n_sweep.split(",")):
            report["axes"].append(_latency_axis(conn, n_docs=n, k_queries=30))
        for ds in (d for d in args.beir.split(",") if d.strip()):
            report["axes"].append(_ndcg_axis(conn, ds, args.cache_dir))

    conn.close()
    import os

    os.makedirs(os.path.dirname(args.out) or ".", exist_ok=True)
    with open(args.out, "w") as f:
        json.dump(report, f, indent=2)
    print(f"[m140.3] wrote {args.out} ({len(report['axes'])} axes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
