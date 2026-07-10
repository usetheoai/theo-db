#!/usr/bin/env python3
"""M60 DECISIVE CONTROL — pgvector recall@10 on the IDENTICAL gaussian-mixture corpus that theodb_hnsw plateaus on.

theodb_hnsw_f32 plateaus at recall@10 ≈0.974 (ef-insensitive) at 500k×768d on this corpus (M57 → M60). The M60
premise is that pgvector reaches ≥0.99 on the SAME data — i.e. that the ~2pt gap is a theodb graph defect. That
premise was inferred from DIFFERENT scales in M57 and was never verified head-to-head at 500k×768d on the exact
same corpus. This script settles it: same seed (h.SEED) → byte-identical corpus + queries + exact GT, but pgvector's
own HNSW. If pgvector ALSO plateaus ~0.974, the ceiling is a property of the DATA (256 tight gaussian clusters →
many near-equidistant neighbors), NOT a theodb defect — an honest, milestone-reshaping finding (Rule 3).

Runs in a SEPARATE db (pgvector's public.vector collides with theodb_rs's own public.vector — M70). Connects to
PGDATABASE (default `pgvctl`), which must exist and NOT have theodb_rs installed.

  createdb -p 28817 pgvctl
  PGDATABASE=pgvctl python3 run_m60_pgvector_control.py --n 500000 --dim 768 --nq 50 --out /root/m60_pgvector.json
"""
import argparse
import json
import os
import time

import psycopg2

import run_m51_sbq_inline as h

EF_SWEEP = [40, 100, 200, 400, 1000]  # pgvector hnsw.ef_search valid up to very large; sweep to 1000 like theodb.


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--n", type=int, default=500000)
    ap.add_argument("--dim", type=int, default=768)
    ap.add_argument("--nq", type=int, default=50)
    ap.add_argument("--k", type=int, default=10)
    ap.add_argument("--out", default="/root/m60_pgvector.json")
    a = ap.parse_args()

    conn = psycopg2.connect(host=os.environ.get("PGHOST", "localhost"), port=os.environ.get("PGPORT", "28817"),
                            user=os.environ.get("PGUSER", "theo"), password=os.environ.get("PGPASSWORD", ""),
                            dbname=os.environ.get("PGDATABASE", "pgvctl"), connect_timeout=15)
    conn.autocommit = True
    cur = conn.cursor()
    cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
    table = "m60ctl"
    h._make_dataset(cur, table, a.n, a.dim, h.SEED)  # `v vector(dim)` == pgvector's vector here (same numeric data)
    queries = h._queries(a.dim, a.nq, h.SEED)
    gt = h._ground_truth(cur, table, queries, a.k)  # exact brute-force top-k over pgvector <=>

    cur.execute("DROP INDEX IF EXISTS bench_pgv")
    t0 = time.perf_counter()
    cur.execute(f"CREATE INDEX bench_pgv ON {table} USING hnsw (v vector_cosine_ops) WITH (m=16, ef_construction=64)")
    build_s = round(time.perf_counter() - t0, 2)

    pts = []
    for ef in EF_SWEEP:
        cur.execute("SET enable_seqscan = off")
        cur.execute(f"SET hnsw.ef_search = {ef}")
        lat, hit = [], 0
        for q, truth in zip(queries, gt):
            t0 = time.perf_counter()
            cur.execute(f"SELECT id FROM {table} ORDER BY v <=> %s LIMIT {a.k}", (q,))
            got = set(r[0] for r in cur.fetchall())
            lat.append((time.perf_counter() - t0) * 1000.0)
            hit += len(got & truth)
        lat.sort()
        pts.append({"knob": ef, "recall": round(hit / (a.nq * a.k), 4),
                    "p50_ms": round(lat[len(lat) // 2], 3)})

    best = max(pts, key=lambda p: p["recall"])
    out = {"n": a.n, "dim": a.dim, "k": a.k, "nq": a.nq, "metric": "cosine", "engine": "pgvector_hnsw",
           "build_s": build_s, "sweep": pts, "best_recall": best["recall"], "best_ef": best["knob"],
           "reaches_099": best["recall"] >= 0.99}
    json.dump(out, open(a.out, "w"), indent=2)
    print(f"pgvector control: best recall@10 = {best['recall']} @ef={best['knob']} → reaches 0.99: {out['reaches_099']}")
    for p in pts:
        print(f"  ef={p['knob']:>4}  recall={p['recall']:.4f}  p50={p['p50_ms']}ms")
    print(f"artifact -> {a.out}")


if __name__ == "__main__":
    main()
