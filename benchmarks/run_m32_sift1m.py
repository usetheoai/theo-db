#!/usr/bin/env python3
"""M32 operator artifact — full SIFT1M (1M×128) 4-way head-to-head: theodb_ivfflat / theodb_hnsw vs
pgvector ivfflat / hnsw. Writes wiki/benchmarks/m32-scale-sift1m.md e benchmarks/artifacts/m32-scale-sift1m.json.

This is the >=1M evidence run (ADR-4: operator-invoked, not per-commit CI). It reuses the mature
`theodb_bench` harness via the `--index 4way --full-train` path (neighbors-GT, no 10^10 brute force).

Repro:
  # 1. cache the dataset (once):
  curl -fsSL -o benchmarks/.datasets/sift-128-euclidean.hdf5 http://ann-benchmarks.com/sift-128-euclidean.hdf5
  # 2. run against a container that has the theodb AMs (theo-db:m31b or later):
  PGPORT=<port> python3 benchmarks/run_m32_sift1m.py --n-queries 1000 --runs 3
"""
from __future__ import annotations

import argparse
import os
from pathlib import Path

from theodb_bench.__main__ import build_config, build_parser
from theodb_bench.db import VectorDB
from theodb_bench.harness import artifact_stem, run_benchmark

_REPO = Path(__file__).resolve().parents[1]
_DEFAULT_HDF5 = str(_REPO / "benchmarks" / ".datasets" / "sift-128-euclidean.hdf5")


def main() -> int:
    ap = argparse.ArgumentParser(description="M32 SIFT1M 4-way scale benchmark (operator artifact)")
    ap.add_argument("--n-queries", type=int, default=1000, help="query subsample from SIFT test (<=10000)")
    ap.add_argument("--runs", type=int, default=3, help="timed runs for best-of-N QPS + mean±std (>=3)")
    ap.add_argument("--theodb-hnsw-query-cap", type=int, default=50,
                    help="cap theodb_hnsw's query sample (O(N)-per-query scan ~4s at 1M); 0/None = uncapped")
    ap.add_argument("--hdf5", default=_DEFAULT_HDF5)
    ap.add_argument("--out", default=str(_REPO / "docs" / "benchmarks"))
    ap.add_argument("--maintenance-work-mem", default="2GB",
                    help="generous build budget applied to BOTH engines fairly (theodb builds in-proc "
                    "and ignores it; pgvector's hnsw/ivfflat build uses it)")
    args = ap.parse_args()

    if not Path(args.hdf5).is_file():
        raise SystemExit(f"SIFT1M not found at {args.hdf5} — see the repro header to download it")

    cli = ["--index", "4way", "--metric", "l2", "--full-train", "--hdf5", args.hdf5,
           "--n-queries", str(args.n_queries), "--runs", str(args.runs)]
    if args.theodb_hnsw_query_cap:
        cli += ["--theodb-hnsw-query-cap", str(args.theodb_hnsw_query_cap)]
    cfg = build_config(build_parser().parse_args(cli))
    cfg["dataset_label"] = "m32-scale-sift1m"

    dsn = (
        f"host={os.environ.get('PGHOST', 'localhost')} port={os.environ.get('PGPORT', '5432')} "
        f"dbname={os.environ.get('PGDATABASE', 'postgres')} user={os.environ.get('PGUSER', 'postgres')} "
        f"password={os.environ.get('PGPASSWORD', 'postgres')}"
    )
    db = VectorDB(dsn).connect()
    db.set_session(f"SET maintenance_work_mem = '{args.maintenance_work_mem}'")
    # Single-threaded builds for BOTH engines: theodb's M26 build is single-thread scalar (no parallel path),
    # so forcing pgvector's build single-threaded too makes the build-TIME axis a fair apples-to-apples
    # comparison (else pgvector gets 4 cores, theodb 1). It also avoids Docker's tiny default /dev/shm (64 MB):
    # pgvector parallel build requests a multi-GB dynamic-shared-memory segment that overflows /dev/shm.
    db.set_session("SET max_parallel_maintenance_workers = 0")
    try:
        report = run_benchmark(cfg, db, args.out)
    finally:
        db.close()

    # run_benchmark writes {date}-m32-scale-sift1m.{json,md}; normalize to the stable m32 artifact name.
    stem = artifact_stem(report)
    out = Path(args.out)
    for ext in ("json", "md"):
        src = out / f"{stem}.{ext}"
        dst = out / f"m32-scale-sift1m.{ext}"
        if src.exists() and src != dst:
            src.replace(dst)

    print(f"\n=== M32 SIFT1M 4-way (n={report['n']} dim={report['dim']} k={report['k']} "
          f"queries={report['n_queries']} runs={report['runs']}) ===")
    for r in report["results"]:
        print(f"{r['index']:16} {r['params']:14} recall@{report['k']}={r['recall_at_k']:.4f} "
              f"qps={r['qps']:.1f} p50={r['p50']:.2f} p95={r['p95']:.2f} p99={r['p99']:.2f} "
              f"build={r['build_ms']:.0f}ms size={r['index_bytes']}B")
    print(f"artifact -> {out}/m32-scale-sift1m.json")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
