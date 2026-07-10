#!/usr/bin/env python3
"""M72 — QPS a 1M sob N clientes CONCORRENTES (o regime real de produção).

M32/M34 mediram p50 single-client. M72 mede o **throughput agregado sob concorrência**: N conexões martelando
queries em paralelo contra o MESMO índice, ao ponto de recall casado. Compara theodb_hnsw (multi-entry, M71) vs
pgvector no MESMO corpus/hardware. Honesto (council-benchmark): reporta QPS agregado + p50/p95/p99 a iso-recall.

Roda em um DB com theodb_rs (theodb) OU vector (pgvector), escolhido por --engine. O corpus é gerado uma vez
(gaussian-mixture determinístico, h.SEED); o índice é construído; então N workers (multiprocessing) rodam por
--secs segundos e o driver soma o throughput.

  python3 run_m72_multiclient.py --engine theodb --n 1000000 --dim 128 --clients 8 --ef 200 --secs 20 --out t.json
  python3 run_m72_multiclient.py --engine pgvector --n 1000000 --dim 128 --clients 8 --ef 100 --secs 20 --out p.json
"""
import argparse
import json
import os
import time
from multiprocessing import Process, Queue

import psycopg2

import run_m51_sbq_inline as h


def _conn(db):
    return psycopg2.connect(host=os.environ.get("PGHOST", "localhost"), port=os.environ.get("PGPORT", "28817"),
                            user=os.environ.get("PGUSER", "theo"), password=os.environ.get("PGPASSWORD", ""),
                            dbname=db, connect_timeout=15)


def _worker(db, table, engine, ef, queries, k, secs, q):
    """Run queries round-robin for `secs` seconds; report (count, latencies_ms)."""
    conn = _conn(db)
    cur = conn.cursor()
    cur.execute("SET enable_seqscan = off")
    cur.execute(f"SET {'theodb_hnsw' if engine == 'theodb' else 'hnsw'}.ef_search = {ef}")
    lat = []
    deadline = time.perf_counter() + secs
    i = 0
    while time.perf_counter() < deadline:
        qv = queries[i % len(queries)]
        i += 1
        t0 = time.perf_counter()
        cur.execute(f"SELECT id FROM {table} ORDER BY v <=> %s LIMIT {k}", (qv,))
        cur.fetchall()
        lat.append((time.perf_counter() - t0) * 1000.0)
    conn.close()
    q.put((len(lat), lat))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--engine", choices=["theodb", "pgvector"], required=True)
    ap.add_argument("--db", default=None)
    ap.add_argument("--n", type=int, default=1000000)
    ap.add_argument("--dim", type=int, default=128)
    ap.add_argument("--nq", type=int, default=200)
    ap.add_argument("--k", type=int, default=10)
    ap.add_argument("--clients", type=int, default=8)
    ap.add_argument("--ef", type=int, default=200)
    ap.add_argument("--secs", type=int, default=20)
    ap.add_argument("--out", default="/home/theo/m72.json")
    a = ap.parse_args()
    db = a.db or ("postgres" if a.engine == "theodb" else "pgvctl")
    table = "m72bench"

    conn = _conn(db)
    conn.autocommit = True
    cur = conn.cursor()
    cur.execute(f"CREATE EXTENSION IF NOT EXISTS {'theodb_rs' if a.engine == 'theodb' else 'vector'}")
    cur.execute("SELECT to_regclass(%s)", (table,))
    if cur.fetchone()[0] is None or (cur.execute(f"SELECT count(*) FROM {table}") or cur.fetchone()[0] != a.n):
        h._make_dataset(cur, table, a.n, a.dim, h.SEED)
    queries = h._queries(a.dim, a.nq, h.SEED)
    gt = h._ground_truth(cur, table, queries, a.k)

    # build index (idempotent)
    idx, opclass = ("m72_theodb", "USING theodb_hnsw (v theodb_hnsw_cosine_ops)") if a.engine == "theodb" \
        else ("m72_pgv", "USING hnsw (v vector_cosine_ops) WITH (m=16, ef_construction=64)")
    cur.execute(f"SELECT 1 FROM pg_class WHERE relname='{idx}'")
    if not cur.fetchone():
        t0 = time.perf_counter()
        cur.execute(f"CREATE INDEX {idx} ON {table} {opclass}")
        build_s = round(time.perf_counter() - t0, 1)
    else:
        build_s = "reused"

    # recall at this ef (single-client, for the iso-recall label)
    cur.execute("SET enable_seqscan = off")
    cur.execute(f"SET {'theodb_hnsw' if a.engine == 'theodb' else 'hnsw'}.ef_search = {a.ef}")
    hit = 0
    for qv, truth in zip(queries, gt):
        cur.execute(f"SELECT id FROM {table} ORDER BY v <=> %s LIMIT {a.k}", (qv,))
        hit += len(set(r[0] for r in cur.fetchall()) & truth)
    recall = round(hit / (a.nq * a.k), 4)
    conn.close()

    # concurrent load
    q = Queue()
    procs = [Process(target=_worker, args=(db, table, a.engine, a.ef, queries, a.k, a.secs, q))
             for _ in range(a.clients)]
    t0 = time.perf_counter()
    for p in procs:
        p.start()
    counts, all_lat = 0, []
    for _ in procs:
        c, lat = q.get()
        counts += c
        all_lat.extend(lat)
    for p in procs:
        p.join()
    wall = time.perf_counter() - t0
    all_lat.sort()

    def pct(p):
        return round(all_lat[min(len(all_lat) - 1, int(len(all_lat) * p))], 3) if all_lat else None
    out = {"engine": a.engine, "n": a.n, "dim": a.dim, "clients": a.clients, "ef": a.ef, "recall": recall,
           "build_s": build_s, "wall_s": round(wall, 2), "total_queries": counts,
           "qps_aggregate": round(counts / wall, 1), "p50_ms": pct(0.50), "p95_ms": pct(0.95), "p99_ms": pct(0.99)}
    json.dump(out, open(a.out, "w"), indent=2)
    print(f"M72 {a.engine}: {a.clients} clients, ef={a.ef}, recall={recall} → "
          f"QPS_agg={out['qps_aggregate']} p50={out['p50_ms']} p95={out['p95_ms']} p99={out['p99_ms']}ms")


if __name__ == "__main__":
    main()
