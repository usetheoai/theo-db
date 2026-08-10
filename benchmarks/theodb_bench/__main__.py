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
        choices=["hnsw", "ivfflat", "diskann", "both", "all", "theodb_ivfflat", "theodb_hnsw", "4way"],
        default="hnsw",
        help="which index(es) to benchmark: hnsw | ivfflat | diskann | both (hnsw+diskann) | "
        "all (hnsw+ivfflat — excludes diskann; use 'both'/'diskann' for that) | "
        "theodb_ivfflat | theodb_hnsw (M26 persisted AMs, l2-only) | "
        "4way (pgvector hnsw+ivfflat vs theodb hnsw+ivfflat — the M32 head-to-head; l2 only). "
        "diskann requires the vectorscale extension",
    )
    p.add_argument(
        "--hdf5",
        default=None,
        help="path to an ANN-Benchmarks HDF5 (train/test) — real dataset instead of synthetic gaussian; "
        "dim is inferred from the file",
    )
    p.add_argument(
        "--full-train",
        action="store_true",
        help="load the FULL HDF5 train (no subsample) + derive exact GT from the file's `neighbors` dataset "
        "(M32 >=1M scale path); requires --hdf5 with a `neighbors` dataset",
    )
    p.add_argument(
        "--theodb-hnsw-query-cap",
        type=int,
        default=None,
        help="cap the query sample for theodb_hnsw only (its scan is O(N)-per-query — whole-blob deserialize, "
        "~4 s/query at 1M); keeps a scale run tractable. Recorded in the result label.",
    )
    p.add_argument("--dsn", default=None, help="libpq DSN (else built from PG* env vars)")
    p.add_argument("--out", default="benchmarks/artifacts")
    return p


def _dsn_from_env() -> str:
    return (
        f"host={os.environ.get('PGHOST', 'localhost')} "
        f"port={os.environ.get('PGPORT', '5432')} "
        f"dbname={os.environ.get('PGDATABASE', 'postgres')} "
        f"user={os.environ.get('PGUSER', 'postgres')} "
        f"password={os.environ.get('PGPASSWORD', 'postgres')}"
    )


def _theodb_spec(am: str, table: str, query_cap: int | None = None) -> dict:
    # theodb's M26/M31 persisted AMs (theodb_ivfflat / theodb_hnsw) have NO query-time knob — SCAN_PROBES /
    # SCAN_EF are fixed Rust constants (M32 ADR-3). So a single fixed operating point (no ef/probes sweep):
    # just force the index on. l2-only (theodb exposes {am}_l2_ops; cosine deferred — ADR 0010 / M32 ADR-2).
    # query_cap: theodb_hnsw's scan is O(N)-per-query (whole-blob deserialize; M31's structured partial-read is
    # ivfflat-only) — ~4 s/query at 1M. Cap its query sample so a scale run stays tractable (recorded in label).
    spec = {
        "name": am,
        "index_name": f"bench_{am}",
        "ddl": f"CREATE INDEX bench_{am} ON {table} USING {am} (embedding {am}_l2_ops)",
        "sweep": [{"label": "fixed", "session": ["SET enable_seqscan = off"]}],
    }
    if query_cap is not None:
        spec["query_cap"] = query_cap
    return spec


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


def _ivfflat_spec(table: str, opclass: str, n: int) -> dict:
    # pgvector IVFFlat: vectors are partitioned into `lists` clusters; a query scans the `probes`
    # nearest clusters. `lists` is a BUILD param; `probes` is the query-time recall/speed knob.
    # pgvector guidance: lists ~= rows/1000 for <= 1M rows. probes is swept (recall x QPS curve).
    # At probes==lists every cluster is scanned, so IVFFlat (flat/full-precision storage) becomes
    # exact-among-indexed. Force the index on (pgvector recall-test methodology) so we measure the
    # index, not a seqscan.
    #
    # Measurement honesty: we CLAMP each probe to `lists` BEFORE de-duplicating, so every sweep
    # point's LABEL equals the value actually executed (probes > lists is a no-op in pgvector — an
    # unclamped label like "probes=10" on a lists=5 index would silently run probes=5 and report a
    # duplicate operating point under a wrong label).
    lists = max(1, n // 1000)
    probes = sorted({min(p, lists) for p in (1, 10, lists)})
    return {
        "name": "ivfflat",
        "index_name": "bench_ivfflat",
        "ddl": f"CREATE INDEX bench_ivfflat ON {table} USING ivfflat (embedding {opclass}) WITH (lists = {lists})",
        "sweep": [
            {
                "label": f"probes={p}",
                "session": ["SET enable_seqscan = off", f"SET ivfflat.probes = {p}"],
            }
            for p in probes
        ],
    }


def _diskann_spec(table: str, opclass: str) -> dict:
    # pgvectorscale StreamingDiskANN. Two knobs trade recall for speed:
    #   - query_search_list_size (sls): SBQ-approximate candidate list size
    #   - query_rescore: how many candidates get re-ranked with exact full-precision vectors
    # We scale rescore WITH sls (rescore == min(sls, engine-max 1000)) so the recall×QPS Pareto curve
    # is honest: freezing rescore at an arbitrary 500 below sls (an earlier bug) capped recall while QPS
    # still fell, manufacturing a fake plateau. pgvectorscale's hard ceiling for query_rescore is 1000;
    # tying rescore to sls up to that ceiling is the symmetric sweep — every point measures a real
    # (recall, QPS) pair, including the high-recall top of the curve (sls=2000 reaches rescore=1000).
    # SBQ needs a larger candidate list than HNSW's ef_search on non-clustered data (synthetic gaussian:
    # sls up to ~2000; real embedding distributions reach high recall at far lower sls).
    _RESCORE_MAX = 1000  # pgvectorscale diskann.query_rescore valid range is 0..1000

    def _sw(sls: int) -> dict:
        rescore = min(sls, _RESCORE_MAX)
        return {
            "label": f"sls={sls},rescore={rescore}",
            "session": [
                "SET enable_seqscan = off",
                f"SET diskann.query_search_list_size = {sls}",
                f"SET diskann.query_rescore = {rescore}",
            ],
        }

    return {
        "name": "diskann",
        "index_name": "bench_diskann",
        "ddl": f"CREATE INDEX bench_diskann ON {table} USING diskann (embedding {opclass})",
        "sweep": [_sw(100), _sw(500), _sw(1000), _sw(2000)],
    }


def _train_size(args: argparse.Namespace) -> int:
    """The corpus size that pgvector's ivfflat `lists` must be derived from. Under --full-train the true N is the
    HDF5 train size, NOT the --n default — deriving `lists` from the default (5000 -> lists=5) would build a
    crippled, unfair pgvector ivfflat on a 1M corpus (measurement-integrity defect). Read the real size."""
    if getattr(args, "full_train", False) and args.hdf5:
        import h5py

        with h5py.File(args.hdf5, "r") as f:
            return int(f["train"].shape[0])
    return args.n


def build_config(args: argparse.Namespace) -> dict:
    if getattr(args, "theodb_hnsw_query_cap", None) is not None and args.theodb_hnsw_query_cap < 1:
        raise ValueError(f"--theodb-hnsw-query-cap must be >= 1, got {args.theodb_hnsw_query_cap}")
    opclass = _OPCLASS[args.metric]
    table = "bench_vectors"
    ivfflat_n = _train_size(args)
    specs = []
    if args.index in ("hnsw", "both", "all", "4way"):
        specs.append(_hnsw_spec(table, opclass))
    if args.index in ("ivfflat", "all", "4way"):
        specs.append(_ivfflat_spec(table, opclass, ivfflat_n))
    if args.index in ("diskann", "both"):
        specs.append(_diskann_spec(table, opclass))
    # theodb's persisted AMs are l2-only (M32 ADR-2 / ADR 0010). On cosine we DO NOT emit a fabricated cosine
    # opclass — skip them with a clear notice rather than a wrong DDL.
    want_theodb_ivf = args.index in ("theodb_ivfflat", "4way")
    want_theodb_hnsw = args.index in ("theodb_hnsw", "4way")
    if (want_theodb_ivf or want_theodb_hnsw) and args.metric != "l2":
        print(
            f"NOTE: theodb AMs are l2-only; skipping theodb specs for --metric {args.metric} "
            "(cosine opclass deferred — ADR 0010).",
            file=sys.stderr,
        )
    else:
        if want_theodb_ivf:
            specs.append(_theodb_spec("theodb_ivfflat", table))
        if want_theodb_hnsw:
            specs.append(_theodb_spec("theodb_hnsw", table, query_cap=getattr(args, "theodb_hnsw_query_cap", None)))
    if not specs:
        raise ValueError(
            f"no index specs for --index {args.index} --metric {args.metric} "
            "(theodb AMs are l2-only; use --metric l2 or a pgvector index)"
        )
    config = {
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
    if args.hdf5:
        config["hdf5_path"] = args.hdf5
        # label = filename stem (e.g. glove-25-angular.hdf5 -> glove-25-angular)
        config["dataset_label"] = os.path.splitext(os.path.basename(args.hdf5))[0]
    if getattr(args, "full_train", False):
        config["full_train"] = True
    return config


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
    from .harness import artifact_stem

    print(f"report -> {args.out}/{artifact_stem(report)}.json")
    return 0


if __name__ == "__main__":
    sys.exit(main())
