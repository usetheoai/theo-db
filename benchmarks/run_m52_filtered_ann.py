#!/usr/bin/env python3
"""M52 — filtered ANN: recall@10 sob `WHERE` seletivo, theodb_hnsw (iterative scan) vs pgvector 0.8 (iterative).

O gate do DoD: recall sob filtro seletivo ≥ paridade pgvector 0.8. HNSW ingênuo (≤ ef_search tuplas) colapsa sob
filtro; o iterative scan re-busca com ef crescente até recuperar k tuplas que passam. Mede 3 seletividades
(1% / 10% / 50%) via uma coluna `cat`. GT = seqscan exato + filtro. Reusa theodb_bench.metrics (Rule 9); espelha
o harness M50/M51. Escala tratável (decisão do usuário 2026-07-06 — box contendida).
"""
import argparse
import json
import os
import statistics
import time

import psycopg2

from theodb_bench.metrics import latency_percentiles

PGHOST = os.environ.get("PGHOST", "localhost")
PGPORT = os.environ.get("PGPORT", "55492")
PGUSER = os.environ.get("PGUSER", "postgres")
PGPASSWORD = os.environ.get("PGPASSWORD", "postgres")
SEED = 42
NCAT = 100  # cat in 0..NCAT → `cat = X` ≈ 1% ; `cat < 10` ≈ 10% ; `cat < 50` ≈ 50%

# selectivity label -> WHERE predicate
FILTERS = {"1pct": "cat = 7", "10pct": "cat < 10", "50pct": "cat < 50"}

SPECS = {
    "theodb_hnsw": {
        "ddl": "CREATE INDEX bench_th ON {t} USING theodb_hnsw (v theodb_hnsw_cosine_ops)",
        "drop": "DROP INDEX IF EXISTS bench_th",
        # iterative scan on (max_scan_tuples default) + a moderate ef floor
        "setup": ["SET theodb_hnsw.ef_search = 100", "SET theodb_hnsw.max_scan_tuples = 20000"],
    },
    "pgvector_hnsw": {
        "ddl": "CREATE INDEX bench_pgv ON {t} USING hnsw (v vector_cosine_ops) WITH (m=16, ef_construction=64)",
        "drop": "DROP INDEX IF EXISTS bench_pgv",
        # pgvector 0.8 iterative index scans (relaxed) — the SOTA permissive baseline the DoD anchors on
        "setup": ["SET hnsw.ef_search = 100", "SET hnsw.iterative_scan = relaxed_order", "SET hnsw.max_scan_tuples = 20000"],
    },
}


def _conn():
    c = psycopg2.connect(host=PGHOST, port=PGPORT, user=PGUSER, password=PGPASSWORD, dbname="postgres",
                         connect_timeout=15)
    c.autocommit = True
    return c


def _load():
    return round(os.getloadavg()[0], 2)


def _make_dataset(cur, table, n, dim, seed):
    import random
    rnd = random.Random(seed)
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    cur.execute(f"CREATE TABLE {table} (id int, cat int, v vector({dim}))")
    rows = []
    for i in range(n):
        vec = "[" + ",".join(f"{rnd.gauss(0, 1):.4f}" for _ in range(dim)) + "]"
        rows.append((i, i % NCAT, vec))
        if len(rows) >= 1000:
            cur.executemany(f"INSERT INTO {table} VALUES (%s,%s,%s)", rows)
            rows = []
    if rows:
        cur.executemany(f"INSERT INTO {table} VALUES (%s,%s,%s)", rows)
    cur.execute(f"ANALYZE {table}")


def _queries(dim, nq, seed):
    import random
    rnd = random.Random(seed + 1)
    return ["[" + ",".join(f"{rnd.gauss(0, 1):.4f}" for _ in range(dim)) + "]" for _ in range(nq)]


def _ground_truth(cur, table, queries, predicate, k):
    cur.execute("SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on")
    gt = []
    for q in queries:
        cur.execute(f"SELECT id FROM {table} WHERE {predicate} ORDER BY v <=> %s LIMIT {k}", (q,))
        gt.append(set(r[0] for r in cur.fetchall()))
    cur.execute("RESET enable_indexscan; RESET enable_bitmapscan; RESET enable_seqscan")
    return gt


def _measure(cur, table, spec, predicate, queries, gt, k):
    cur.execute("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on")
    for s in spec["setup"]:
        cur.execute(s)
    lat, hit, tot = [], 0, 0
    for q, truth in zip(queries, gt):
        if not truth:
            continue  # a query whose filter matched < 1 row is not a recall data point
        t0 = time.perf_counter()
        cur.execute(f"SELECT id FROM {table} WHERE {predicate} ORDER BY v <=> %s LIMIT {k}", (q,))
        got = set(r[0] for r in cur.fetchall())
        lat.append((time.perf_counter() - t0) * 1000.0)
        hit += len(got & truth)
        tot += len(truth)
    perc = latency_percentiles(lat) if lat else {"p50": 0.0}
    return {"recall": round(hit / tot, 4) if tot else None, "p50_ms": round(perc["p50"], 3),
            "qps_1client": round(1000.0 / perc["p50"], 1) if perc["p50"] > 0 else None, "queries_used": len(lat)}


def run(n, dim, nq, k, runs):
    conn = _conn()
    cur = conn.cursor()
    cur.execute("CREATE EXTENSION IF NOT EXISTS theodb CASCADE")
    table = "m52bench"
    load_pre = _load()
    _make_dataset(cur, table, n, dim, SEED)
    queries = _queries(dim, nq, SEED)
    gts = {sel: _ground_truth(cur, table, queries, pred, k) for sel, pred in FILTERS.items()}

    results = {name: {sel: [] for sel in FILTERS} for name in SPECS}
    loads = []
    for _ in range(runs):
        loads.append(_load())
        for name, spec in SPECS.items():
            cur.execute(spec["drop"])
            try:
                t0 = time.perf_counter()
                cur.execute(spec["ddl"].format(t=table))
                build_s = round(time.perf_counter() - t0, 2)
            except Exception as e:  # noqa: BLE001 — record honestly
                for sel in FILTERS:
                    results[name][sel].append({"error": str(e)[:160]})
                continue
            for sel, pred in FILTERS.items():
                results[name][sel].append({"build_s": build_s, **_measure(cur, table, spec, pred, queries, gts[sel], k)})
            cur.execute(spec["drop"])
    conn.close()

    agg = {}
    for name in SPECS:
        agg[name] = {}
        for sel in FILTERS:
            pts = [r for r in results[name][sel] if "recall" in r and r["recall"] is not None]
            if not pts:
                agg[name][sel] = {"error": "no recall data (filter matched too few rows?)"}
                continue
            recs = [p["recall"] for p in pts]
            p50s = [p["p50_ms"] for p in pts]
            agg[name][sel] = {
                "recall_mean": round(statistics.mean(recs), 4),
                "recall_std": round(statistics.pstdev(recs), 4) if len(recs) > 1 else 0.0,
                "p50_ms_mean": round(statistics.mean(p50s), 3),
                "qps_1client_mean": round(1000.0 / statistics.mean(p50s), 1) if statistics.mean(p50s) > 0 else None,
            }
    # DoD gate: recall theodb >= pgvector (parity) at every selectivity
    gate = {}
    for sel in FILTERS:
        th = agg.get("theodb_hnsw", {}).get(sel, {}).get("recall_mean")
        pv = agg.get("pgvector_hnsw", {}).get(sel, {}).get("recall_mean")
        # Explicit 0.01 tolerance (documented, not hidden): "parity" = theodb within 1pt of pgvector recall.
        gate[sel] = {"theodb": th, "pgvector": pv, "tolerance": 0.01,
                     "theodb_ge_pgvector_within_1pct": (th is not None and pv is not None and th >= pv - 0.01)}
    return {"n": n, "dim": dim, "queries": nq, "k": k, "runs": runs, "metric": "cosine", "ncat": NCAT,
            "load_pre": load_pre, "load_per_run": loads, "nproc": os.cpu_count(),
            "filters": FILTERS, "parity_gate": gate, "per_spec": agg, "raw": results}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--n", type=int, default=25000)
    ap.add_argument("--dim", type=int, default=128)
    ap.add_argument("--nq", type=int, default=50)
    ap.add_argument("--k", type=int, default=10)
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--out", default="/tmp/m52.json")
    args = ap.parse_args()
    data = run(args.n, args.dim, args.nq, args.k, args.runs)
    json.dump(data, open(args.out, "w"), indent=2)
    print(f"wrote {args.out} (n={args.n} dim={args.dim} runs={args.runs}); load_pre={data['load_pre']}")
    for sel, g in data["parity_gate"].items():
        print(f"  {sel}: theodb recall {g['theodb']} vs pgvector {g['pgvector']} → parity(±0.01) {g['theodb_ge_pgvector_within_1pct']}")


if __name__ == "__main__":
    main()
