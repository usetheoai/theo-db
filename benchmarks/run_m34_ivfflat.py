#!/usr/bin/env python3
"""M34 operator artifact — theodb_ivfflat (WITH lists=N + tuned probes) vs pgvector ivfflat at SIFT1M.

Proves the M34 DoD: with the new `lists` reloption + `theodb_ivfflat.probes` GUC, theodb_ivfflat reaches p50
<= pgvector ivfflat at matched probes on 1M×128 (recall >= parity). Reuses the M32 harness (`--full-train`
neighbors-GT) with two ivfflat specs at the SAME lists + a shared probes sweep — a fair, tuned head-to-head.
Writes docs/benchmarks/m34-ivfflat-reloption.{md,json}.

Repro:
  PGPORT=<port> python3 benchmarks/run_m34_ivfflat.py --lists 1000 --n-queries 1000 --runs 3
"""
from __future__ import annotations

import argparse
import os
from pathlib import Path

from theodb_bench.db import VectorDB
from theodb_bench.harness import artifact_stem, run_benchmark

_REPO = Path(__file__).resolve().parents[1]
_DEFAULT_HDF5 = str(_REPO / "benchmarks" / ".datasets" / "sift-128-euclidean.hdf5")
_PROBES = (1, 10, 50, 100)
_TABLE = "bench_vectors"


def _theodb_ivfflat_spec(lists: int) -> dict:
    return {
        "name": "theodb_ivfflat",
        "index_name": "bench_theodb_ivfflat",
        "ddl": f"CREATE INDEX bench_theodb_ivfflat ON {_TABLE} USING theodb_ivfflat "
               f"(embedding theodb_ivfflat_l2_ops) WITH (lists = {lists})",
        "sweep": [
            {"label": f"probes={p}",
             "session": ["SET enable_seqscan = off", f"SET theodb_ivfflat.probes = {p}"]}
            for p in _PROBES
        ],
    }


def _pgvector_ivfflat_spec(lists: int) -> dict:
    return {
        "name": "ivfflat",
        "index_name": "bench_ivfflat",
        "ddl": f"CREATE INDEX bench_ivfflat ON {_TABLE} USING ivfflat "
               f"(embedding vector_l2_ops) WITH (lists = {lists})",
        "sweep": [
            {"label": f"probes={p}",
             "session": ["SET enable_seqscan = off", f"SET ivfflat.probes = {p}"]}
            for p in _PROBES
        ],
    }


def main() -> int:
    ap = argparse.ArgumentParser(description="M34 theodb_ivfflat vs pgvector ivfflat at SIFT1M (tuned lists/probes)")
    ap.add_argument("--lists", type=int, default=1000, help="lists for BOTH ivfflat builds (fair; pgvector guidance ~ rows/1000)")
    ap.add_argument("--n-queries", type=int, default=1000)
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--hdf5", default=_DEFAULT_HDF5)
    ap.add_argument("--out", default=str(_REPO / "docs" / "benchmarks"))
    args = ap.parse_args()
    if not Path(args.hdf5).is_file():
        raise SystemExit(f"SIFT1M not found at {args.hdf5} — download it first (see run_m32_sift1m.py header)")

    cfg = {
        "seed": 42, "n": 0, "dim": 128, "n_queries": args.n_queries, "k": 10, "metric": "l2",
        "runs": args.runs, "table": _TABLE, "full_train": True, "hdf5_path": args.hdf5,
        "dataset_label": "m34-ivfflat-reloption",
        "index_specs": [_theodb_ivfflat_spec(args.lists), _pgvector_ivfflat_spec(args.lists)],
    }
    dsn = (
        f"host={os.environ.get('PGHOST', 'localhost')} port={os.environ.get('PGPORT', '5432')} "
        f"dbname={os.environ.get('PGDATABASE', 'postgres')} user={os.environ.get('PGUSER', 'postgres')} "
        f"password={os.environ.get('PGPASSWORD', 'postgres')}"
    )
    db = VectorDB(dsn).connect()
    db.set_session("SET maintenance_work_mem = '2GB'")
    db.set_session("SET max_parallel_maintenance_workers = 0")  # single-thread both builds (fair build-time)
    try:
        report = run_benchmark(cfg, db, args.out)
    finally:
        db.close()

    stem = artifact_stem(report)
    out = Path(args.out)
    for ext in ("json", "md"):
        src = out / f"{stem}.{ext}"
        dst = out / f"m34-ivfflat-reloption.{ext}"
        if src.exists() and src != dst:
            src.replace(dst)

    print(f"\n=== M34 ivfflat SIFT1M (n={report['n']} lists={args.lists} k={report['k']} "
          f"queries={report['n_queries']} runs={report['runs']}) ===")
    for r in report["results"]:
        print(f"{r['index']:16} {r['params']:12} recall@{report['k']}={r['recall_at_k']:.4f} "
              f"qps={r['qps']:.1f} p50={r['p50']:.2f} p95={r['p95']:.2f} build={r['build_ms']:.0f}ms size={r['index_bytes']}B")
    print(f"artifact -> {out}/m34-ivfflat-reloption.json")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
