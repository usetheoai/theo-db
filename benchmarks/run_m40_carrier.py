#!/usr/bin/env python3
"""M40 carrier head-to-head — theodb_hnsw (graph, M35) vs theodb_ivfflat (probes, M34): recall x QPS at matched QPS.

Measurement-first re-scope (see wiki/benchmarks/m40-ceiling-probe.md): the ceiling probe proved the vector-pillar
recall is limited by the CARRIER (candidate generation), not the quantizer — so the real vector-superiority
question is which OWN carrier wins the recall x QPS trade-off. This runs BOTH persisted AMs over the same corpus +
exact brute-force ground truth, sweeping each AM's query-time knob (theodb_hnsw.ef_search / theodb_ivfflat.probes),
and reports the recall x QPS curve per AM so they can be compared at matched QPS. Honest: synthetic corpus at a
moderate scale (SIFT1M HDF5 not present locally) — the curves, not an absolute number, are the deliverable.

Usage:
  PGPORT=<port> python3 benchmarks/run_m40_carrier.py --n 50000 --dim 64 --runs 3
"""
import argparse
import json
import os
from pathlib import Path

from theodb_bench.db import VectorDB
from theodb_bench.harness import run_benchmark

_REPO = Path(__file__).resolve().parent.parent
_TABLE = "m40_carrier"


def _hnsw_spec(ef_values):
    return {
        "name": "theodb_hnsw",
        "index_name": "bench_theodb_hnsw",
        "ddl": f"CREATE INDEX bench_theodb_hnsw ON {_TABLE} USING theodb_hnsw (embedding theodb_hnsw_l2_ops)",
        "sweep": [
            {"label": f"ef_search={ef}",
             "session": ["SET enable_seqscan = off", f"SET theodb_hnsw.ef_search = {ef}"]}
            for ef in ef_values
        ],
    }


def _ivfflat_spec(lists, probe_values):
    return {
        "name": "theodb_ivfflat",
        "index_name": "bench_theodb_ivfflat",
        "ddl": f"CREATE INDEX bench_theodb_ivfflat ON {_TABLE} USING theodb_ivfflat "
               f"(embedding theodb_ivfflat_l2_ops) WITH (lists = {lists})",
        "sweep": [
            {"label": f"probes={p}",
             "session": ["SET enable_seqscan = off", f"SET theodb_ivfflat.probes = {p}"]}
            for p in probe_values
        ],
    }


def _dsn():
    return (
        f"host={os.environ.get('PGHOST', 'localhost')} port={os.environ.get('PGPORT', '5432')} "
        f"dbname={os.environ.get('PGDATABASE', 'postgres')} user={os.environ.get('PGUSER', 'postgres')} "
        f"password={os.environ.get('PGPASSWORD', 'postgres')}"
    )


def matched_qps_verdict(results):
    """At each theodb_hnsw operating point, find the theodb_ivfflat point with the closest QPS and compare recall.
    Returns the fraction of matched points where hnsw recall >= ivfflat recall (+ per-point rows)."""
    hnsw = [r for r in results if r["index"] == "theodb_hnsw"]
    ivf = [r for r in results if r["index"] == "theodb_ivfflat"]
    rows = []
    hnsw_wins = 0
    for h in hnsw:
        # nearest ivfflat point by QPS
        m = min(ivf, key=lambda r: abs(r["qps"] - h["qps"])) if ivf else None
        if m is None:
            continue
        win = h["recall_at_k"] >= m["recall_at_k"]
        hnsw_wins += 1 if win else 0
        rows.append({"hnsw_params": h["params"], "hnsw_qps": h["qps"], "hnsw_recall": h["recall_at_k"],
                     "ivf_params": m["params"], "ivf_qps": m["qps"], "ivf_recall": m["recall_at_k"],
                     "hnsw_wins_at_matched_qps": win})
    frac = hnsw_wins / len(rows) if rows else 0.0
    verdict = "THEODB_HNSW_WINS" if frac > 0.5 else ("TIE" if frac == 0.5 else "THEODB_IVFFLAT_WINS")
    return verdict, frac, rows


def main() -> int:
    ap = argparse.ArgumentParser(description="M40 theodb_hnsw vs theodb_ivfflat carrier head-to-head (recall x QPS)")
    ap.add_argument("--n", type=int, default=50000)
    ap.add_argument("--dim", type=int, default=64)
    ap.add_argument("--n-queries", type=int, default=500)
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--seed", type=int, default=2026)
    ap.add_argument("--out", default=str(_REPO / "docs" / "benchmarks"))
    args = ap.parse_args()

    lists = max(8, int(args.n ** 0.5))
    cfg = {
        "seed": args.seed, "n": args.n, "dim": args.dim, "n_queries": args.n_queries, "k": 10, "metric": "l2",
        "runs": args.runs, "table": _TABLE, "dataset_label": "m40-carrier-headhead",
        "index_specs": [
            _hnsw_spec([10, 40, 100, 200]),
            _ivfflat_spec(lists, [1, 4, 16, 44, 100]),
        ],
    }
    db = VectorDB(_dsn()).connect()
    db.set_session("SET maintenance_work_mem = '2GB'")
    db.set_session("SET max_parallel_maintenance_workers = 0")
    try:
        report = run_benchmark(cfg, db, args.out)
    finally:
        db.close()

    verdict, frac, rows = matched_qps_verdict(report["results"])
    print(f"\n=== M40 carrier head-to-head (n={report['n']} dim={report.get('dim', args.dim)} "
          f"k={report['k']} queries={report['n_queries']} runs={report['runs']}) ===")
    for r in report["results"]:
        print(f"{r['index']:16} {r['params']:16} recall@{report['k']}={r['recall_at_k']:.4f} "
              f"qps={r['qps']:.1f} p50={r['p50']:.2f}")
    print("\nMatched-QPS comparison (theodb_hnsw point -> nearest theodb_ivfflat point by QPS):")
    for row in rows:
        print(f"  hnsw {row['hnsw_params']:14} qps={row['hnsw_qps']:.0f} r={row['hnsw_recall']:.3f}  vs  "
              f"ivf {row['ivf_params']:12} qps={row['ivf_qps']:.0f} r={row['ivf_recall']:.3f}  "
              f"-> {'HNSW' if row['hnsw_wins_at_matched_qps'] else 'IVF'}")
    print(f"\nM40 VERDICT: {verdict} (hnsw wins {frac:.0%} of matched-QPS points)")

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    (out / "m40-carrier.json").write_text(json.dumps(
        {"params": {"n": args.n, "dim": args.dim, "n_queries": args.n_queries, "runs": args.runs, "lists": lists},
         "results": report["results"], "matched_qps": rows, "verdict": verdict, "hnsw_win_fraction": frac},
        indent=2, default=str))
    print(f"artifact -> {out / 'm40-carrier.json'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
