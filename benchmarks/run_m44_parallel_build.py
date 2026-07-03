#!/usr/bin/env python3
"""M44 parallel HNSW build A/B — sequential (theo-db:m43) vs parallel (theo-db:m44) theodb_hnsw build over the same
SIFT subset. Measures CREATE INDEX wall-clock (>=3 samples mean±std) + recall@10 vs exact GT (parity gate).

Cross-image A/B (no GUC needed — parsimony): theo-db:m43 builds everything sequentially; theo-db:m44 builds
in parallel above PARALLEL_BUILD_THRESHOLD (4096). Point the two ports at the two containers.

Usage:
  python3 benchmarks/run_m44_parallel_build.py --n 50000 --dim 128 --runs 3 --seq-port 5461 --par-port 5464
"""
import argparse
import json
import os
import statistics
import time
from pathlib import Path

import numpy as np
import psycopg2
from psycopg2.extras import execute_values

_REPO = Path(__file__).resolve().parent.parent


def _vlit(v):
    return "[" + ",".join(repr(float(x)) for x in v) + "]"


def _load(port, corpus, dim):
    c = psycopg2.connect(host="localhost", port=port, dbname="postgres", user="postgres", password="postgres")
    c.autocommit = True
    cur = c.cursor()
    cur.execute("CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE")
    cur.execute("DROP TABLE IF EXISTS m44 CASCADE")
    cur.execute(f"CREATE TABLE m44 (id int primary key, embedding vector({dim}))")
    execute_values(cur, "INSERT INTO m44 (id, embedding) VALUES %s",
                   [(i, _vlit(v)) for i, v in enumerate(corpus)], page_size=2000)
    cur.execute("SET max_parallel_maintenance_workers = 0")  # neither uses PG parallel workers; fair
    return c, cur


def _build_once(cur):
    cur.execute("DROP INDEX IF EXISTS m44_h")
    t = time.perf_counter()
    cur.execute("CREATE INDEX m44_h ON m44 USING theodb_hnsw (embedding theodb_hnsw_l2_ops)")
    return time.perf_counter() - t


def _recall(cur, queries, gt):
    cur.execute("SET enable_seqscan = off")
    cur.execute("SET theodb_hnsw.ef_search = 100")
    hits = 0
    for i, q in enumerate(queries):
        cur.execute("SELECT id FROM m44 ORDER BY embedding <-> %s::vector LIMIT 10", (_vlit(q),))
        got = {r[0] for r in cur.fetchall()}
        hits += len(got & gt[i])
    return hits / (len(queries) * 10)


def run(seq_port, par_port, n, dim, nq, runs, seed):
    rng = np.random.default_rng(seed)
    corpus = rng.standard_normal((n, dim)).astype(np.float32)
    queries = rng.standard_normal((nq, dim)).astype(np.float32)
    gt = [set(np.argsort(((corpus - q) ** 2).sum(1))[:10].tolist()) for q in queries]

    out = {}
    for label, port in (("sequential", seq_port), ("parallel", par_port)):
        c, cur = _load(port, corpus, dim)
        times = [_build_once(cur) for _ in range(runs)]
        recall = _recall(cur, queries, gt)
        cur.execute("DROP TABLE IF EXISTS m44 CASCADE")
        c.close()
        out[label] = {
            "build_s_mean": round(statistics.mean(times), 1),
            "build_s_std": round(statistics.pstdev(times), 1),
            "recall_at_10": round(recall, 4),
        }
    seq, par = out["sequential"], out["parallel"]
    speedup = seq["build_s_mean"] / par["build_s_mean"] if par["build_s_mean"] > 0 else 0.0
    recall_delta = par["recall_at_10"] - seq["recall_at_10"]
    # D3 gate: parallel meaningfully faster (effect > combined std) AND recall parity (within ±0.03).
    faster = (seq["build_s_mean"] - par["build_s_mean"]) > (seq["build_s_std"] + par["build_s_std"])
    parity = abs(recall_delta) <= 0.03
    verdict = "PARALLEL_WINS" if (faster and parity) else ("RECALL_REGRESSION" if not parity else "NO_SPEEDUP")
    return {"params": {"n": n, "dim": dim, "nq": nq, "runs": runs, "seed": seed},
            "sequential": seq, "parallel": par, "build_speedup": round(speedup, 2),
            "recall_delta": round(recall_delta, 4), "verdict": verdict}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--n", type=int, default=50000)
    ap.add_argument("--dim", type=int, default=128)
    ap.add_argument("--nq", type=int, default=200)
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--seed", type=int, default=2026)
    ap.add_argument("--seq-port", type=int, default=int(os.environ.get("SEQ_PORT", "5461")))
    ap.add_argument("--par-port", type=int, default=int(os.environ.get("PAR_PORT", "5464")))
    ap.add_argument("--write-doc", action="store_true")
    args = ap.parse_args()

    res = run(args.seq_port, args.par_port, args.n, args.dim, args.nq, args.runs, args.seed)
    s, p = res["sequential"], res["parallel"]
    print(f"\n=== M44 parallel HNSW build A/B (n={args.n} dim={args.dim} runs={args.runs}) ===")
    print(f"sequential (m43): build={s['build_s_mean']}±{s['build_s_std']}s  recall@10={s['recall_at_10']}")
    print(f"parallel   (m44): build={p['build_s_mean']}±{p['build_s_std']}s  recall@10={p['recall_at_10']}")
    print(f"\nBUILD SPEEDUP: {res['build_speedup']}x   recall Δ={res['recall_delta']:+.4f}")
    print(f"D3 VERDICT: {res['verdict']}")

    if args.write_doc:
        out = _REPO / "docs" / "benchmarks"
        out.mkdir(parents=True, exist_ok=True)
        (out / "m44-parallel-build.json").write_text(json.dumps(res, indent=2))
        print(f"wrote {out / 'm44-parallel-build.json'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
