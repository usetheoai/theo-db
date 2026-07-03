#!/usr/bin/env python3
"""M39 PQ vs SBQ benchmark — TheoDB's own Product Quantization (`theodb.pq_knn`) vs SBQ (`theodb.sbq_knn`),
recall@K + QPS on a deterministic corpus with an exact brute-force ground truth.

Measurement-first (blueprint D3, anti-sunk-cost): PQ is only worth merging if it BEATS SBQ at fixed recall
(higher QPS) OR delivers higher recall at comparable bytes/vector — effect > run-to-run variance (>=3 runs,
mean +/- std, analysis-golden-rule A1). M38 falsified SBQ (recall<1.0 on SIFT); this measures whether PQ+ADC
closes that gap. No performance claim without this benchmark (public-copy.md).

Usage:
  PGPORT=<port> python3 benchmarks/run_m39_pq.py --n 2000 --dim 64 --m 8 --bits 4 --runs 3 --write-doc
"""
import argparse
import json
import os
import statistics
import time
from pathlib import Path

import numpy as np
import psycopg2

from theodb_bench.recall import brute_force_ground_truth, recall_at_k

K = 10
_REPO = Path(__file__).resolve().parent.parent


def conn():
    return psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"), port=os.environ.get("PGPORT", "5432"),
        user=os.environ.get("PGUSER", "postgres"), password=os.environ.get("PGPASSWORD", "postgres"),
        dbname=os.environ.get("PGDATABASE", "postgres"),
    )


def vec_lit(v):
    return "[" + ",".join(repr(float(x)) for x in v) + "]"


def setup(cur, corpus, dim):
    cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
    cur.execute("CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE")
    cur.execute("DROP TABLE IF EXISTS m39_bench")
    cur.execute(f"CREATE TABLE m39_bench (id integer PRIMARY KEY, embedding vector({dim}))")
    cur.executemany("INSERT INTO m39_bench VALUES (%s, %s::vector)",
                    [(i, vec_lit(v)) for i, v in enumerate(corpus)])


def _measure(cur, fn, queries, true_d, extra):
    """One run of `theodb.{fn}` over all queries → (recall@K, qps). Times the whole batch (wall-clock)."""
    qlits = [vec_lit(q) for q in queries]
    t0 = time.perf_counter()
    cur.execute(
        f"SELECT query_idx, distance FROM theodb.{fn}('m39_bench'::regclass, 'embedding', %s::vector[], {extra}) "
        "ORDER BY query_idx, distance",
        (qlits,),
    )
    per = [[] for _ in range(len(queries))]
    for qi, d in cur.fetchall():
        per[qi].append(float(d))
    elapsed = time.perf_counter() - t0
    qps = len(queries) / elapsed if elapsed > 0 else 0.0
    return recall_at_k(true_d, per, K), qps


def mean_std(xs):
    return (statistics.mean(xs), statistics.pstdev(xs) if len(xs) > 1 else 0.0)


def run(port, n, dim, m, bits, nq, runs, seed, lists, probes, over_fetch):
    if dim % m != 0:
        raise SystemExit(f"FATAL: dim {dim} not divisible by m {m} (PQ subspaces must be equal-sized)")
    rng = np.random.default_rng(seed)
    corpus = rng.standard_normal((n, dim)).astype(np.float32)
    queries = rng.standard_normal((nq, dim)).astype(np.float32)
    _, true_d = brute_force_ground_truth(corpus, queries, K, metric="l2")

    os.environ.setdefault("PGPORT", str(port))
    c = conn()
    c.autocommit = True
    with c.cursor() as cur:
        setup(cur, corpus, dim)
        pq_extra = f"k=>{K}, m=>{m}, lists=>{lists}, probes=>{probes}, over_fetch=>{over_fetch}, metric=>'l2'"
        sbq_extra = f"k=>{K}, bits=>{bits}, lists=>{lists}, probes=>{probes}, over_fetch=>{over_fetch}, metric=>'l2'"
        # warmup (discard) then >=`runs` timed runs each.
        _measure(cur, "pq_knn", queries, true_d, pq_extra)
        _measure(cur, "sbq_knn", queries, true_d, sbq_extra)
        pq = [_measure(cur, "pq_knn", queries, true_d, pq_extra) for _ in range(runs)]
        sbq = [_measure(cur, "sbq_knn", queries, true_d, sbq_extra) for _ in range(runs)]
        cur.execute("SELECT theodb.sbq_bytes_per_vector(%s, %s)", (dim, bits))
        sbq_bytes = int(cur.fetchone()[0])
        cur.execute("DROP TABLE IF EXISTS m39_bench")

    pq_recall_m, pq_recall_s = mean_std([r for r, _ in pq])
    pq_qps_m, pq_qps_s = mean_std([q for _, q in pq])
    sbq_recall_m, sbq_recall_s = mean_std([r for r, _ in sbq])
    sbq_qps_m, sbq_qps_s = mean_std([q for _, q in sbq])
    pq_bytes = m  # PQ code = m bytes/vector (k*=256 → 1 byte/subspace)
    f32_bytes = dim * 4

    # D3 gate (honest, effect > variance AND > a meaningful floor): PQ wins for the P0 vector-superiority goal
    # ONLY if — at parity recall — it has meaningfully HIGHER QPS (the P0 is latency/QPS), OR it has MEANINGFULLY
    # higher recall (not noise) at <= bytes. A sub-0.01 recall gap is parity, NOT a win (M38 lesson: a noise-level
    # delta is not evidence). A 4x memory saving at 4x QPS loss is NOT a QPS win — it is a memory/latency trade.
    RECALL_FLOOR = 0.01  # <1 recall point is within noise on this scale — treat as parity
    recall_gap = pq_recall_m - sbq_recall_m
    recall_margin = max(pq_recall_s + sbq_recall_s, RECALL_FLOOR)
    qps_gap = pq_qps_m - sbq_qps_m
    qps_margin = pq_qps_s + sbq_qps_s
    parity_recall = abs(recall_gap) <= recall_margin
    pq_meaningfully_higher_recall = recall_gap > recall_margin and pq_bytes <= sbq_bytes
    pq_higher_qps_at_parity = parity_recall and qps_gap > qps_margin
    verdict = "PQ_BEATS_SBQ" if (pq_meaningfully_higher_recall or pq_higher_qps_at_parity) else "SBQ_RETAINED"
    # Honest secondary note: whether PQ trades memory for latency (parity recall, fewer bytes, lower QPS).
    memory_latency_tradeoff = parity_recall and pq_bytes < sbq_bytes and qps_gap < 0

    return {
        "params": {"n": n, "dim": dim, "m": m, "bits": bits, "nq": nq, "runs": runs, "seed": seed,
                   "lists": lists, "probes": probes, "over_fetch": over_fetch, "K": K},
        "pq": {"recall_mean": round(pq_recall_m, 4), "recall_std": round(pq_recall_s, 4),
               "qps_mean": round(pq_qps_m, 1), "qps_std": round(pq_qps_s, 1), "bytes_per_vector": pq_bytes},
        "sbq": {"recall_mean": round(sbq_recall_m, 4), "recall_std": round(sbq_recall_s, 4),
                "qps_mean": round(sbq_qps_m, 1), "qps_std": round(sbq_qps_s, 1), "bytes_per_vector": sbq_bytes},
        "f32_bytes_per_vector": f32_bytes,
        "verdict": verdict,
        "memory_latency_tradeoff": memory_latency_tradeoff,
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--n", type=int, default=2000)
    ap.add_argument("--dim", type=int, default=64)
    ap.add_argument("--m", type=int, default=8, help="PQ subspaces (dim must be divisible by m)")
    ap.add_argument("--bits", type=int, default=4, help="SBQ bits/dim (M38 best-recall config)")
    ap.add_argument("--nq", type=int, default=100)
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--seed", type=int, default=2026)
    ap.add_argument("--lists", type=int, default=0, help="0 → sqrt(n)")
    ap.add_argument("--probes", type=int, default=16)
    ap.add_argument("--over-fetch", type=int, default=16)
    ap.add_argument("--port", type=int, default=int(os.environ.get("PGPORT", "5432")))
    ap.add_argument("--write-doc", action="store_true")
    args = ap.parse_args()
    lists = args.lists if args.lists > 0 else max(8, int(args.n ** 0.5))

    res = run(args.port, args.n, args.dim, args.m, args.bits, args.nq, args.runs, args.seed,
              lists, args.probes, args.over_fetch)

    p = res["pq"]
    s = res["sbq"]
    print(f"\n=== M39 PQ vs SBQ (n={args.n} dim={args.dim} m={args.m} bits={args.bits} "
          f"nq={args.nq} runs={args.runs}) ===")
    print(f"PQ  recall@{K}={p['recall_mean']}+/-{p['recall_std']} qps={p['qps_mean']}+/-{p['qps_std']} "
          f"bytes/vec={p['bytes_per_vector']}")
    print(f"SBQ recall@{K}={s['recall_mean']}+/-{s['recall_std']} qps={s['qps_mean']}+/-{s['qps_std']} "
          f"bytes/vec={s['bytes_per_vector']}")
    print(f"f32 bytes/vec={res['f32_bytes_per_vector']}")
    print(f"\nD3 VERDICT: {res['verdict']}")

    if args.write_doc:
        out = _REPO / "docs" / "benchmarks"
        out.mkdir(parents=True, exist_ok=True)
        (out / "m39-pq.json").write_text(json.dumps(res, indent=2))
        print(f"wrote {out / 'm39-pq.json'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
