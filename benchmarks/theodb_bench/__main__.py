"""CLI entrypoint: `python -m theodb_bench --seed 42 --n 5000 --dim 128 --k 10 --metric l2`."""
from __future__ import annotations

import argparse
import os
import sys

from .db import VectorDB
from .harness import run_benchmark

_OPCLASS = {"l2": "vector_l2_ops", "cosine": "vector_cosine_ops"}


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="theodb_bench", description="TheoDB vector recall@k benchmark harness")
    p.add_argument("--seed", type=int, default=42)
    p.add_argument("--n", type=int, default=5000)
    p.add_argument("--dim", type=int, default=128)
    p.add_argument("--n-queries", type=int, default=100)
    p.add_argument("--k", type=int, default=10)
    p.add_argument("--metric", choices=["l2", "cosine"], default="l2")
    p.add_argument("--runs", type=int, default=3)
    p.add_argument(
        "--index",
        choices=["hnsw", "diskann", "both"],
        default="hnsw",
        help="which index(es) to benchmark (diskann requires the vectorscale extension)",
    )
    p.add_argument("--dsn", default=None, help="libpq DSN (else built from PG* env vars)")
    p.add_argument("--out", default="docs/benchmarks")
    return p


def _dsn_from_env() -> str:
    return (
        f"host={os.environ.get('PGHOST', 'localhost')} "
        f"port={os.environ.get('PGPORT', '5432')} "
        f"dbname={os.environ.get('PGDATABASE', 'postgres')} "
        f"user={os.environ.get('PGUSER', 'postgres')} "
        f"password={os.environ.get('PGPASSWORD', 'postgres')}"
    )


def _hnsw_spec(table: str, opclass: str) -> dict:
    # Force the index on so we measure the index, not the planner's seqscan choice on small/medium N
    # — this is the pgvector recall-test methodology (blueprint §Integration).
    return {
        "name": "hnsw",
        "index_name": "bench_hnsw",
        "ddl": f"CREATE INDEX bench_hnsw ON {table} USING hnsw (embedding {opclass})",
        "sweep": [
            {"label": "ef_search=40", "session": ["SET enable_seqscan = off", "SET hnsw.ef_search = 40"]},
            {"label": "ef_search=100", "session": ["SET enable_seqscan = off", "SET hnsw.ef_search = 100"]},
        ],
    }


def _diskann_spec(table: str, opclass: str) -> dict:
    # pgvectorscale StreamingDiskANN; query_search_list_size (+ query_rescore) trade recall for speed.
    # The sweep spans a wide range because SBQ quantization needs a larger candidate list than HNSW's
    # ef_search to reach equivalent recall on non-clustered data (measured: synthetic gaussian needs
    # sls up to ~1000-2000; real embedding distributions reach high recall at far lower sls).
    def _sw(sls: int) -> dict:
        return {
            "label": f"sls={sls}",
            "session": [
                "SET enable_seqscan = off",
                f"SET diskann.query_search_list_size = {sls}",
                f"SET diskann.query_rescore = {min(sls, 500)}",
            ],
        }

    return {
        "name": "diskann",
        "index_name": "bench_diskann",
        "ddl": f"CREATE INDEX bench_diskann ON {table} USING diskann (embedding {opclass})",
        "sweep": [_sw(100), _sw(500), _sw(1000)],
    }


def build_config(args: argparse.Namespace) -> dict:
    opclass = _OPCLASS[args.metric]
    table = "bench_vectors"
    specs = []
    if args.index in ("hnsw", "both"):
        specs.append(_hnsw_spec(table, opclass))
    if args.index in ("diskann", "both"):
        specs.append(_diskann_spec(table, opclass))
    return {
        "seed": args.seed,
        "n": args.n,
        "dim": args.dim,
        "n_queries": args.n_queries,
        "k": args.k,
        "metric": args.metric,
        "runs": args.runs,
        "table": table,
        "index_specs": specs,
    }


def main(argv=None) -> int:
    args = build_parser().parse_args(argv)
    dsn = args.dsn or _dsn_from_env()
    db = VectorDB(dsn).connect()
    db.ping()
    try:
        report = run_benchmark(build_config(args), db, args.out)
    finally:
        db.close()
    for r in report["results"]:
        print(
            f"{r['index']:6} {r['params']:14} recall@{report['k']}={r['recall_at_k']:.4f} "
            f"qps={r['qps']:.1f} p95={r['p95']:.3f}ms build={r['build_ms']:.0f}ms size={r['index_bytes']}B"
        )
    print(f"report -> {args.out}/{report['date']}-pgvector-{report['metric']}.json")
    return 0


if __name__ == "__main__":
    sys.exit(main())
