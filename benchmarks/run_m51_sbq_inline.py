#!/usr/bin/env python3
"""M51 — SBQ-inline recall×QPS: theodb_hnsw SBQ (v2) vs theodb_hnsw f32 (v1) vs pgvector hnsw, cosine.

The gate this measures (M51 DoD): does the inline-SBQ read path (Hamming walk + exact f32 rerank) PRESERVE
recall@10 ≥ 0.99 while cheapening the scan? Sweeps `over_fetch` for the SBQ spec (the recall-recovery knob, M40).
GT is exact seqscan. Load recorded per run (M46 lesson). Reuses theodb_bench.metrics (Rule 9); mirrors the M50
harness. DELIBERATELY SMALLER than 1M (user decision 2026-07-06 — the ≥2× QPS claim at memory-pressure scale is a
tracked follow-up; this run validates the RECALL gate + the QPS direction at a tractable scale).
"""
import argparse
import json
import os
import statistics
import time

import psycopg2

from theodb_bench.metrics import latency_percentiles

PGHOST = os.environ.get("PGHOST", "localhost")
PGPORT = os.environ.get("PGPORT", "55491")
PGUSER = os.environ.get("PGUSER", "postgres")
PGPASSWORD = os.environ.get("PGPASSWORD", "postgres")
SEED = 42

# (name, build DDL, drop, knob-template, sweep). Cosine (requires M49). SBQ sweeps over_fetch; f32/pgvector sweep ef.
SPECS = {
    # sbq_bits=8 + ef=400: the config that reaches the recall≥0.99 gate (probe 2026-07-06: 0.997). The
    # Hamming-navigated walk needs adequate bits + carrier (ef·over_fetch) for the rerank to recover recall (M40);
    # 2-bit/ef=100 tops at ~0.52 (honest-negative, recorded in the artifact). over_fetch is the swept knob.
    "theodb_hnsw_sbq": {
        "ddl": "CREATE INDEX bench_sbq ON {t} USING theodb_hnsw (v theodb_hnsw_cosine_ops) WITH (sbq_bits = 8)",
        "drop": "DROP INDEX IF EXISTS bench_sbq",
        "knob": lambda x: ["SET theodb_hnsw.ef_search = 400", f"SET theodb_hnsw.over_fetch = {x}"],
        "sweep": [2, 4, 8, 16],
    },
    "theodb_hnsw_f32": {
        "ddl": "CREATE INDEX bench_f32 ON {t} USING theodb_hnsw (v theodb_hnsw_cosine_ops)",
        "drop": "DROP INDEX IF EXISTS bench_f32",
        "knob": lambda x: [f"SET theodb_hnsw.ef_search = {x}"],
        "sweep": [40, 100, 200, 400],
    },
    "pgvector_hnsw": {
        "ddl": "CREATE INDEX bench_pgv ON {t} USING hnsw (v vector_cosine_ops) WITH (m=16, ef_construction=64)",
        "drop": "DROP INDEX IF EXISTS bench_pgv",
        "knob": lambda x: [f"SET hnsw.ef_search = {x}"],
        "sweep": [40, 100, 200, 400],
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
    # M57: stream the gaussian corpus via COPY (O(1) client RAM, fast at 1M) instead of per-row INSERT — reuses
    # the M55 streaming loader (Rule 9). The `executemany` path is O(n) round-trips and unusable at 1M scale.
    import os
    import sys
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    from run_m55_vacuum_wall import _copy_vectors
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    cur.execute(f"CREATE TABLE {table} (id int, v vector({dim}))")
    _copy_vectors(cur, table, 0, n, dim, seed)
    cur.execute(f"ANALYZE {table}")


def _queries(dim, nq, seed):
    import random
    rnd = random.Random(seed + 1)
    return ["[" + ",".join(f"{rnd.gauss(0, 1):.4f}" for _ in range(dim)) + "]" for _ in range(nq)]


def _ground_truth(cur, table, queries, k):
    cur.execute("SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on")
    gt = []
    for q in queries:
        cur.execute(f"SELECT id FROM {table} ORDER BY v <=> %s LIMIT {k}", (q,))
        gt.append(set(r[0] for r in cur.fetchall()))
    cur.execute("RESET enable_indexscan; RESET enable_bitmapscan")
    return gt


def _measure(cur, table, spec, knob_val, queries, gt, k):
    cur.execute("SET enable_seqscan = off")
    for s in spec["knob"](knob_val):
        cur.execute(s)
    lat, hit = [], 0
    for q, truth in zip(queries, gt):
        t0 = time.perf_counter()
        cur.execute(f"SELECT id FROM {table} ORDER BY v <=> %s LIMIT {k}", (q,))
        got = set(r[0] for r in cur.fetchall())
        lat.append((time.perf_counter() - t0) * 1000.0)
        hit += len(got & truth)
    perc = latency_percentiles(lat)
    return {"recall": round(hit / (len(queries) * k), 4), "p50_ms": round(perc["p50"], 3),
            "qps_1client": round(1000.0 / perc["p50"], 1)}


def run(n, dim, nq, k, runs):
    conn = _conn()
    cur = conn.cursor()
    cur.execute("CREATE EXTENSION IF NOT EXISTS theodb CASCADE")
    table = "m51bench"
    load_pre = _load()
    _make_dataset(cur, table, n, dim, SEED)
    queries = _queries(dim, nq, SEED)
    gt = _ground_truth(cur, table, queries, k)

    results = {name: [] for name in SPECS}
    loads = []
    for _ in range(runs):
        loads.append(_load())
        for name, spec in SPECS.items():
            cur.execute(spec["drop"])
            try:
                t0 = time.perf_counter()
                cur.execute(spec["ddl"].format(t=table))
                build_s = round(time.perf_counter() - t0, 2)
            except Exception as e:  # noqa: BLE001 — record honestly, never fabricate
                results[name].append({"error": str(e)[:160]})
                continue
            pts = [{"knob": v, "build_s": build_s, **_measure(cur, table, spec, v, queries, gt, k)}
                   for v in spec["sweep"]]
            results[name].append(pts)
            cur.execute(spec["drop"])
    conn.close()

    agg = {}
    for name in SPECS:
        run_pts = [r for r in results[name] if isinstance(r, list)]
        if not run_pts:
            agg[name] = {"error": results[name][0].get("error", "no data") if results[name] else "no data"}
            continue
        knobs = SPECS[name]["sweep"]
        curve = []
        for i, kv in enumerate(knobs):
            recs = [rp[i]["recall"] for rp in run_pts]
            p50s = [rp[i]["p50_ms"] for rp in run_pts]
            qps = [rp[i]["qps_1client"] for rp in run_pts]
            curve.append({"knob": kv, "recall_mean": round(statistics.mean(recs), 4),
                          "recall_std": round(statistics.pstdev(recs), 4) if len(recs) > 1 else 0.0,
                          "p50_ms_mean": round(statistics.mean(p50s), 3),
                          "qps_1client_mean": round(statistics.mean(qps), 1),
                          "build_s": run_pts[0][i]["build_s"]})
        agg[name] = {"curve": curve}
    # the M51 recall gate: the best SBQ recall point (highest recall over the over_fetch sweep)
    sbq = agg.get("theodb_hnsw_sbq", {})
    gate = max((c["recall_mean"] for c in sbq.get("curve", [])), default=None)
    return {"n": n, "dim": dim, "queries": nq, "k": k, "runs": runs, "metric": "cosine",
            "load_pre": load_pre, "load_per_run": loads, "nproc": os.cpu_count(),
            "sbq_best_recall_at_10": gate, "recall_gate_0_99_met": (gate is not None and gate >= 0.99),
            "per_spec": agg, "raw": results}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--n", type=int, default=25000)
    ap.add_argument("--dim", type=int, default=128)
    ap.add_argument("--nq", type=int, default=50)
    ap.add_argument("--k", type=int, default=10)
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--out", default="/tmp/m51.json")
    args = ap.parse_args()
    data = run(args.n, args.dim, args.nq, args.k, args.runs)
    json.dump(data, open(args.out, "w"), indent=2)
    print(f"wrote {args.out} (n={args.n} dim={args.dim} runs={args.runs}); load_pre={data['load_pre']}")
    print(f"SBQ best recall@10 = {data['sbq_best_recall_at_10']} | gate>=0.99 met: {data['recall_gate_0_99_met']}")
    for name, a in data["per_spec"].items():
        if "curve" in a:
            top = max(a["curve"], key=lambda c: c["recall_mean"])
            print(f"  {name}: best recall {top['recall_mean']} @ p50 {top['p50_ms_mean']}ms qps {top['qps_1client_mean']} (knob {top['knob']}, build {top['build_s']}s)")
        else:
            print(f"  {name}: {a['error']}")


if __name__ == "__main__":
    main()
