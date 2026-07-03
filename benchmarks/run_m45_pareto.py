#!/usr/bin/env python3
"""M45 — rigorous mean±std recall×QPS Pareto: theodb_hnsw vs pgvector hnsw on SIFT1M.

Builds BOTH HNSW index-AMs on the SAME corpus with MATCHED build params (m=16, ef_construction=64,
single-thread build), sweeps a SHARED ef_search grid on each (session GUC — no rebuild), runs >=3 timed
passes per operating point → **mean ± std** QPS + recall vs exact GT, then computes the honest
matched-recall margin via Pareto interpolation (`m45_pareto.pareto_margin_verdict`). This is the rigor the
M42 single-run signal (`docs/benchmarks/sift1m-carrier-verdict.md`) lacked (public-copy.md §4 half 1).

Reuses `theodb_bench.{dataset,db,recall}` by import (Rule 9 — no harness rebuild); does NOT touch
`theodb_bench/harness.py` (which reports best-of-N, the very statistic we must NOT use here).

Usage:
  python3 benchmarks/run_m45_pareto.py --hdf5 benchmarks/.datasets/sift-128-euclidean.hdf5 \
      --nq 500 --runs 3 --port 5474 --write-doc
"""
from __future__ import annotations

import argparse
import json
import os
import statistics
import time
from pathlib import Path

from m45_pareto import pareto_margin_verdict
from m45_report import render_md
from theodb_bench.dataset import load_hdf5_full, load_hdf5_subsample, make_dataset
from theodb_bench.db import DBUnavailableError, VectorDB
from theodb_bench.recall import brute_force_ground_truth, recall_at_k

_REPO = Path(__file__).resolve().parent.parent

# The two index-AMs under test (ADR D1 — the planner query path, not the SQL-function path).
_DDL = {
    "theodb_hnsw": "CREATE INDEX bench_theodb_hnsw ON {t} USING theodb_hnsw (embedding theodb_hnsw_l2_ops)",
    # pgvector build params matched to theodb's fixed HNSW_M / HNSW_EF_CONSTRUCTION (am/build.rs) — ADR D4.
    "pgvector_hnsw": "CREATE INDEX bench_pgv_hnsw ON {t} USING hnsw (embedding vector_l2_ops) WITH (m = 16, ef_construction = 64)",
}
_IDXNAME = {"theodb_hnsw": "bench_theodb_hnsw", "pgvector_hnsw": "bench_pgv_hnsw"}
_EF_GUC = {"theodb_hnsw": "theodb_hnsw.ef_search", "pgvector_hnsw": "hnsw.ef_search"}
_ORDER = ["theodb_hnsw", "pgvector_hnsw"]


def qps_from_latency(mean_latency_s: float) -> float:
    """QPS from a mean per-query latency, guarded against a 0 (clock-granularity) latency (EC-3)."""
    return 1.0 / max(mean_latency_s, 1e-9)


def _dsn(port: int) -> str:
    return f"host=localhost port={port} dbname=postgres user=postgres password=postgres"


def _connect_with_retry(port: int, attempts: int = 20, delay_s: float = 2.0) -> VectorDB:
    """Wait-for-ready guard (failure scenario): the container may still be running initdb. Retry, then
    fail LOUD — never return a half-open connection that would yield a silent empty artifact."""
    last = None
    for _ in range(attempts):
        try:
            db = VectorDB(_dsn(port)).connect()
            db.ping()
            return db
        except DBUnavailableError as e:  # not ready yet
            last = e
            time.sleep(delay_s)
    raise DBUnavailableError(f"container on port {port} never became ready after {attempts} attempts: {last}")


def _measure_frontier(db, index, table, ef_grid, queries, gt_dist, k, runs, metric, warmups=2):
    """One frontier: for each ef, `warmups` untimed passes (page cache warm — the page-native theodb scan
    needs more than one pass to stabilize) then `runs` timed passes → per-run QPS → mean±std, recall vs GT.
    Per-run QPS is recorded (`qps_runs`) so the variance is fully transparent in the artifact."""
    guc = _EF_GUC[index]
    pts = []
    for ef in ef_grid:
        db.set_session(f"SET {guc} = {ef}")
        for _ in range(warmups):  # untimed warmups — warm the cache consistently before the timed runs
            for q in queries:
                db.query_topk(table, q, k, metric)
        run_qps, last_dists = [], None
        for _ in range(runs):
            lat, dists = [], []
            for q in queries:
                _ids, d, t = db.query_topk(table, q, k, metric)
                lat.append(t)
                dists.append(d)
            run_qps.append(qps_from_latency(sum(lat) / len(lat)))
            last_dists = dists
        recall = recall_at_k(gt_dist, last_dists, k)
        pts.append({
            "ef": ef,
            "recall": round(recall, 4),
            "qps_mean": round(statistics.mean(run_qps), 1),
            "qps_std": round(statistics.pstdev(run_qps), 1),
            "qps_runs": [round(x, 1) for x in run_qps],
            "nq": len(queries),
        })
    return pts


def _load_dataset(hdf5, n, dim, nq, seed, k, metric, full_train):
    if hdf5:
        if full_train:
            corpus, queries, gt_dist = load_hdf5_full(hdf5, nq, seed, k, metric)
        else:
            corpus, queries = load_hdf5_subsample(hdf5, n, nq, seed)
            _, gt_dist = brute_force_ground_truth(corpus, queries, k, metric)
    else:
        corpus, queries = make_dataset(n, dim, nq, seed)
        _, gt_dist = brute_force_ground_truth(corpus, queries, k, metric)
    return corpus, queries, gt_dist


def run(port, hdf5, n=None, dim=128, nq=500, runs=3, ef_grid=(40, 64, 100, 200, 400),
        seed=2026, k=10, metric="l2", full_train=True, maint_work_mem="2GB"):
    """Build both index-AMs, sweep the shared ef grid, measure mean±std QPS + recall, verdict. Returns the
    report dict (frontier per index + matched-recall verdict + params)."""
    corpus, queries, gt_dist = _load_dataset(hdf5, n, dim, nq, seed, k, metric, full_train)
    n_actual, dim_actual = corpus.shape[0], corpus.shape[1]
    q_list = [list(map(float, q)) for q in queries]

    db = _connect_with_retry(port)
    db.ensure_extension()  # pgvector (the `vector` type + hnsw AM)
    db.set_session("CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE")  # theodb_hnsw AM
    table = "m45"
    db.create_table(table, dim_actual)
    db.load_vectors(table, corpus)
    db.set_session("SET max_parallel_maintenance_workers = 0")  # single-thread build on BOTH (fair — ADR D4)
    # Give pgvector adequate build memory (its docs recommend it for large HNSW builds): at the 64MB default
    # a 1M×128 graph (~640MB) spills to a far slower ON-DISK build — an artificial penalty. theodb ignores
    # this GUC (its Rust build uses process memory), so raising it is FAIR to pgvector. Claim is recall×QPS.
    db.set_session(f"SET maintenance_work_mem = '{maint_work_mem}'")
    db.set_session("SET enable_seqscan = off")

    frontier, build_s = {}, {}
    for index in _ORDER:
        for other in _ORDER:  # isolate: drop the OTHER index so the planner cannot cross-use it
            if other != index:
                db.set_session(f"DROP INDEX IF EXISTS {_IDXNAME[other]}")
        build_s[index] = round(db.build_index(_DDL[index].format(t=table)), 1)
        db.assert_index_used(table, q_list[0], k, metric)  # guard: the intended AM is actually used
        pts = _measure_frontier(db, index, table, list(ef_grid), q_list, gt_dist, k, runs, metric)
        for p in pts:
            p["build_s"] = build_s[index]
        frontier[index] = pts

    db.close()
    verdict = pareto_margin_verdict(frontier["theodb_hnsw"], frontier["pgvector_hnsw"])
    return {
        "params": {"n": int(n_actual), "dim": int(dim_actual), "nq": nq, "runs": runs,
                   "ef_grid": list(ef_grid), "seed": seed, "k": k, "metric": metric,
                   "dataset": Path(hdf5).name if hdf5 else f"synthetic-gaussian-{metric}",
                   "build_params": f"m=16, ef_construction=64, max_parallel_maintenance_workers=0 (matched), "
                                  f"maintenance_work_mem={maint_work_mem} (pgvector only; theodb builds in Rust memory)"},
        "build_s": build_s,
        "frontier": frontier,
        "verdict": verdict,
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--hdf5", default=None, help="ANN-Benchmarks HDF5 (SIFT1M). Omit for synthetic.")
    ap.add_argument("--n", type=int, default=50000, help="corpus size for synthetic / subsample")
    ap.add_argument("--dim", type=int, default=128)
    ap.add_argument("--nq", type=int, default=500)
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--seed", type=int, default=2026)
    ap.add_argument("--ef-grid", default="40,64,100,200,400")
    ap.add_argument("--port", type=int, default=int(os.environ.get("PORT", os.environ.get("PGPORT", "5474"))))
    ap.add_argument("--no-full-train", action="store_true", help="subsample instead of full-train GT")
    ap.add_argument("--maint-work-mem", default="2GB", help="pgvector build memory (theodb ignores it)")
    ap.add_argument("--write-doc", action="store_true")
    args = ap.parse_args()

    ef_grid = tuple(int(x) for x in args.ef_grid.split(","))
    res = run(args.port, args.hdf5, n=args.n, dim=args.dim, nq=args.nq, runs=args.runs,
              ef_grid=ef_grid, seed=args.seed, full_train=not args.no_full_train,
              maint_work_mem=args.maint_work_mem)

    print(f"\n=== M45 rigorous Pareto (n={res['params']['n']} nq={args.nq} runs={args.runs}) ===")
    for index in _ORDER:
        print(f"\n{index} (build {res['build_s'][index]}s):")
        for pt in res["frontier"][index]:
            print(f"  ef={pt['ef']:>4}  recall={pt['recall']:.4f}  qps={pt['qps_mean']}±{pt['qps_std']}")
    print(f"\nVERDICT: {res['verdict']['verdict']} — {res['verdict'].get('reason','')}")
    for m in res["verdict"].get("margins", []):
        print(f"  recall={m['recall']}  margin={m['margin']}×  (effect>variance={m['effect_gt_variance']})")

    if args.write_doc:
        out = _REPO / "docs" / "benchmarks"
        out.mkdir(parents=True, exist_ok=True)
        (out / "m45-pareto-sift1m.json").write_text(json.dumps(res, indent=2))
        (out / "m45-pareto-sift1m.md").write_text(render_md(res, _ORDER))
        print(f"\nwrote {out / 'm45-pareto-sift1m.json'} + .md")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
