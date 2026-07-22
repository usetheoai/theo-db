#!/usr/bin/env python3
"""M50 — SOTA ruler: recall@10 × QPS of theodb_hnsw vs pgvector hnsw vs pgvectorscale diskann, COSINE metric.

The honest apples-to-apples SOTA-in-Postgres comparison (same process, same buffer manager, same SQL floor).
Uses cosine (requires M49). GT is exact seqscan. Load is recorded per run (M46 lesson — external contention
invalidates sub-ms latency; the artifact caveats it explicitly). Reuses theodb_bench.metrics (Rule 9).

DELIBERATELY SMALLER than the ROADMAP's 1M×1536d target (user decision 2026-07-06 — the dev box has ~11
concurrent containers, so a 1M×3-build run is infeasible here; the 1M streaming build is M55+). This run
CHARACTERIZES the relative recall×QPS position at a tractable scale, with the scale + box-load caveats stated.
"""
import argparse
import json
import os
import statistics
import threading
import time

import psycopg2

from theodb_bench.metrics import latency_percentiles

PGHOST = os.environ.get("PGHOST", "localhost")
PGPORT = os.environ.get("PGPORT", "55490")
PGUSER = os.environ.get("PGUSER", "postgres")
PGPASSWORD = os.environ.get("PGPASSWORD", "postgres")
SEED = 42

# (name, ddl_template, ef/knob session-set template, sweep values). Cosine opclasses.
SPECS = {
    "theodb_hnsw": {
        "ddl": "CREATE INDEX bench_theodb ON {t} USING theodb_hnsw (v theodb_hnsw_cosine_ops)",
        "drop": "DROP INDEX IF EXISTS bench_theodb",
        "knob": lambda x: [f"SET theodb_hnsw.ef_search = {x}"],
        "sweep": [40, 100, 200, 400],
    },
    "pgvector_hnsw": {
        "ddl": "CREATE INDEX bench_pgv ON {t} USING hnsw (v vector_cosine_ops) WITH (m=16, ef_construction=64)",
        "drop": "DROP INDEX IF EXISTS bench_pgv",
        "knob": lambda x: [f"SET hnsw.ef_search = {x}"],
        "sweep": [40, 100, 200, 400],
    },
    "diskann": {
        "ddl": "CREATE INDEX bench_diskann ON {t} USING diskann (v vector_cosine_ops)",
        "drop": "DROP INDEX IF EXISTS bench_diskann",
        "knob": lambda x: [f"SET diskann.query_search_list_size = {x}", f"SET diskann.query_rescore = {min(x, 1000)}"],
        "sweep": [100, 500, 1000, 2000],
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
    cur.execute(f"CREATE TABLE {table} (id int, v vector({dim}))")
    # Batched insert of gaussian-ish vectors (realistic embedding distribution shape, not clustered).
    rows = []
    for i in range(n):
        vec = "[" + ",".join(f"{rnd.gauss(0, 1):.4f}" for _ in range(dim)) + "]"
        rows.append((i, vec))
        if len(rows) >= 1000:
            cur.executemany(f"INSERT INTO {table} VALUES (%s,%s)", rows)
            rows = []
    if rows:
        cur.executemany(f"INSERT INTO {table} VALUES (%s,%s)", rows)
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
            "p95_ms": round(perc["p95"], 3), "qps_1client": round(1000.0 / perc["p50"], 1)}


def _multiclient_qps(table, spec, knob_val, queries, k, conns, seconds):
    """DB-throughput QPS at C concurrent clients (item 3 of M50 DoD — 'QPS a 1 cliente é 1/latência,
    não throughput de banco'). Each client owns a connection; all issue the query set in a loop for
    `seconds` wall-clock; aggregate throughput = total completed / elapsed. Returns qps + p95 across
    all clients' per-query latencies."""
    stop = time.perf_counter() + seconds
    results = {"done": [0] * conns, "lat": [[] for _ in range(conns)]}

    def worker(cid):
        c = _conn()
        cur = c.cursor()
        cur.execute("SET enable_seqscan = off")
        for s in spec["knob"](knob_val):
            cur.execute(s)
        qi = 0
        while time.perf_counter() < stop:
            q = queries[qi % len(queries)]
            qi += 1
            t0 = time.perf_counter()
            cur.execute(f"SELECT id FROM {table} ORDER BY v <=> %s LIMIT {k}", (q,))
            cur.fetchall()
            results["lat"][cid].append((time.perf_counter() - t0) * 1000.0)
            results["done"][cid] += 1
        c.close()

    t_start = time.perf_counter()
    threads = [threading.Thread(target=worker, args=(i,)) for i in range(conns)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    elapsed = time.perf_counter() - t_start
    total = sum(results["done"])
    all_lat = [x for sub in results["lat"] for x in sub]
    perc = latency_percentiles(all_lat) if all_lat else {"p95": 0.0}
    return {"conns": conns, "qps": round(total / elapsed, 1), "queries": total,
            "elapsed_s": round(elapsed, 2), "p95_ms": round(perc["p95"], 3)}


def run(n, dim, nq, k, runs):
    conn = _conn()
    cur = conn.cursor()
    cur.execute("CREATE EXTENSION IF NOT EXISTS theodb CASCADE")
    table = "m50bench"
    load_pre = _load()
    # Load-guard (M46 lesson): record honestly; warn (do NOT hard-abort — user chose to run on the
    # contended box 2026-07-06). load_guard_tripped is surfaced in the artifact as an explicit caveat.
    load_guard_tripped = load_pre > (os.cpu_count() or 12) * 0.75
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
            except Exception as e:  # noqa: BLE001 — diskann may be absent; record honestly
                results[name].append({"error": str(e)[:120]})
                continue
            pts = [{"knob": v, "build_s": build_s, **_measure(cur, table, spec, v, queries, gt, k)}
                   for v in spec["sweep"]]
            results[name].append(pts)
            cur.execute(spec["drop"])

    # Multi-client DB-throughput pass (item 3): theodb vs pgvector at their top-recall knob, 8 & 16 conns.
    multiclient = {}
    for name in ("theodb_hnsw", "pgvector_hnsw"):
        spec = SPECS[name]
        top_knob = spec["sweep"][-1]  # highest-recall knob
        cur.execute(spec["drop"])
        cur.execute(spec["ddl"].format(t=table))
        multiclient[name] = {"knob": top_knob,
                             "sweeps": [_multiclient_qps(table, spec, top_knob, queries, k, c, 5)
                                        for c in (8, 16)]}
        cur.execute(spec["drop"])
    load_post = _load()
    conn.close()

    # aggregate: best recall point per spec (highest recall) + its p50, mean over runs
    agg = {}
    for name in SPECS:
        run_pts = [r for r in results[name] if isinstance(r, list)]
        if not run_pts:
            agg[name] = {"error": results[name][0].get("error", "no data") if results[name] else "no data"}
            continue
        # per knob index, mean recall/p50 across runs
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
    return {"n": n, "dim": dim, "queries": nq, "k": k, "runs": runs, "metric": "cosine",
            "load_pre": load_pre, "load_per_run": loads, "load_post": load_post,
            "load_guard_tripped": load_guard_tripped, "nproc": os.cpu_count(),
            "per_spec": agg, "multiclient": multiclient, "raw": results}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--n", type=int, default=int(os.environ.get("M50_N", "50000")))
    ap.add_argument("--dim", type=int, default=int(os.environ.get("M50_DIM", "128")))
    ap.add_argument("--nq", type=int, default=int(os.environ.get("M50_NQ", "50")))
    ap.add_argument("--k", type=int, default=10)
    ap.add_argument("--runs", type=int, default=int(os.environ.get("M50_RUNS", "3")))
    ap.add_argument("--out", default="/tmp/m50.json")
    args = ap.parse_args()
    data = run(args.n, args.dim, args.nq, args.k, args.runs)
    json.dump(data, open(args.out, "w"), indent=2)
    print(f"wrote {args.out} (n={args.n} dim={args.dim} runs={args.runs}); load_pre={data['load_pre']}")
    for name, a in data["per_spec"].items():
        if "curve" in a:
            top = max(a["curve"], key=lambda c: c["recall_mean"])
            print(f"  {name}: best recall {top['recall_mean']} @ p50 {top['p50_ms_mean']}ms (knob {top['knob']}, build {top['build_s']}s)")
        else:
            print(f"  {name}: {a['error']}")


if __name__ == "__main__":
    main()
