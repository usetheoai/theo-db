#!/usr/bin/env python3
"""M36 operator artifact — index-scan optimization (lazy min-heap top-K vs the old full sort).

The measurement-first gate of M36 (THEODB_SCAN_PROFILE) FALSIFIED the original "quantize the distance" premise:
distance is ~15% of scan cost; the bottlenecks are reads (~44-51%) and sort (~35-41%). Phase 1 replaces the
O(C·log C) full sort of ALL candidates with a lazy O(C)-heapify + O(k·log C) pops (the executor pulls ~k for a
LIMIT k). Top-K is byte-identical → recall UNCHANGED (proven by 61 coexistence tests: same kNN ids).

This driver measures the END-TO-END query latency at a realistic operating point (LIMIT k) on a given container,
so the M35 (sort) vs M36 (heap) speedup is measured, not supposed. recall is identical by construction.

Repro:
  # baseline (sort): a pre-M36 image (theo-db:m35) on one port
  # optimized (heap): theo-db:m36 on another port
  PGPORT=<port> python3 benchmarks/run_m36_scan.py --n 200000 --probes 50 --k 10 --runs 5 --label <sort|heap>
"""
from __future__ import annotations

import argparse
import io
import json
import os
import random
import time
from pathlib import Path

_REPO = Path(__file__).resolve().parents[1]


def _conn(port):
    import psycopg2
    c = psycopg2.connect(host=os.environ.get("PGHOST", "localhost"), port=port,
                         user="postgres", password="postgres", dbname="postgres")
    c.autocommit = True
    return c


def measure(port, n, dim, probes, k, runs, nq):
    cur = _conn(port).cursor()
    cur.execute("CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE")
    cur.execute("DROP TABLE IF EXISTS m36 CASCADE")
    cur.execute(f"CREATE TABLE m36 (id bigint, embedding vector({dim}))")
    rng = random.Random(42)
    buf = io.StringIO()
    for g in range(1, n + 1):
        buf.write(f"{g}\t[" + ",".join(f"{rng.random():.5f}" for _ in range(dim)) + "]\n")
    buf.seek(0)
    cur.copy_expert("COPY m36 (id, embedding) FROM STDIN", buf)
    lists = max(1, n // 1000)
    cur.execute(f"CREATE INDEX m36_ivf ON m36 USING theodb_ivfflat (embedding) WITH (lists={lists})")
    cur.execute("SET enable_seqscan=off")
    cur.execute("SET enable_indexscan=on")
    cur.execute(f"SET theodb_ivfflat.probes={probes}")
    qrng = random.Random(7)
    queries = ["[" + ",".join(f"{qrng.random():.5f}" for _ in range(dim)) + "]" for _ in range(nq)]
    # warmup
    for q in queries:
        cur.execute(f"SELECT id FROM m36 ORDER BY embedding <-> '{q}' LIMIT {k}")
        cur.fetchall()
    run_means = []
    for _ in range(runs):
        lat = []
        for q in queries:
            t = time.perf_counter()
            cur.execute(f"SELECT id FROM m36 ORDER BY embedding <-> '{q}' LIMIT {k}")
            cur.fetchall()
            lat.append(time.perf_counter() - t)
        run_means.append(sum(lat) / len(lat))
    best_mean_s = min(run_means)
    return {"n": n, "dim": dim, "probes": probes, "k": k, "runs": runs, "n_queries": nq,
            "qps": round(1.0 / best_mean_s, 1), "p50_ms": round(best_mean_s * 1000.0, 3)}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--n", type=int, default=200000)
    ap.add_argument("--dim", type=int, default=128)
    ap.add_argument("--probes", type=int, default=50)
    ap.add_argument("--k", type=int, default=10)
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--n-queries", type=int, default=200)
    ap.add_argument("--label", default="run")
    ap.add_argument("--out", default=str(_REPO / "docs" / "benchmarks"))
    args = ap.parse_args()
    port = os.environ.get("PGPORT", "5432")
    r = measure(port, args.n, args.dim, args.probes, args.k, args.runs, args.n_queries)
    r["label"] = args.label
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    (out / f".m36-{args.label}.json").write_text(json.dumps(r, indent=2) + "\n")
    print(f"[{args.label}] n={r['n']} probes={r['probes']} k={r['k']}: QPS={r['qps']} p50={r['p50_ms']}ms")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
