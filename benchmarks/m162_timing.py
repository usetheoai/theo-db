#!/usr/bin/env python3
# M162 lean 100M timing pass: query the ALREADY-LOADED columnar `hits` (100M), no reload, no A/B.
# Per query: cold (1st run) + hot (min of 2 more), median-agnostic (hot=min per ClickBench convention).
# Bounded: work_mem modest + statement_timeout so a runaway non-covered query records ERRORED (the measurement),
# never OOMs the box. Emits JSONL {q, node, cold_s, hot_s, status}.
import json, os, sys, time
import psycopg2

QUERIES = sys.argv[1] if len(sys.argv) > 1 else "/root/theo-db/benchmarks/clickbench/theodb/queries.sql"
OUT = sys.argv[2] if len(sys.argv) > 2 else "/root/m162_theodb_100m.jsonl"
TIMEOUT_S = int(os.environ.get("QTIMEOUT", "300"))

def conn():
    c = psycopg2.connect(host="127.0.0.1", port=5432, dbname="clickbench", user="postgres", password="x")
    c.autocommit = True
    return c

def session_setup(cur):
    cur.execute("SET theodb.enable_columnar_agg = on")
    cur.execute("SET theodb.enable_columnar_late_mat = on")
    cur.execute("SET work_mem = '256MB'")           # bound the DataFusion pool + PG sorts
    cur.execute("SET max_parallel_workers_per_gather = 0")
    cur.execute(f"SET statement_timeout = '{TIMEOUT_S}s'")

def top_node(cur, sql):
    try:
        cur.execute("EXPLAIN (COSTS OFF) " + sql)
        rows = cur.fetchall()
        for r in rows:
            t = r[0].strip()
            if "Custom Scan (theodb_columnar" in t: return "PUSHDOWN:" + t
            if any(k in t for k in ("Aggregate", "GroupAggregate", "HashAggregate", "Limit", "Sort", "Gather")):
                return "native:" + t
        return rows[0][0].strip() if rows else "?"
    except Exception as e:
        return "explain-err:" + str(e)[:40]

def run_once(sql):
    # fresh connection per run so a backend crash (OOM) on one query doesn't poison the rest
    c = conn(); cur = c.cursor(); session_setup(cur)
    t0 = time.time()
    try:
        cur.execute(sql)
        cur.fetchall()
        dt = time.time() - t0
        c.close()
        return dt, "ok"
    except Exception as e:
        c.close()
        return time.time() - t0, "ERR:" + str(e).splitlines()[0][:60]

def main():
    with open(QUERIES) as f:
        qs = [ln.strip() for ln in f if ln.strip()]
    ce = conn(); cur = ce.cursor(); session_setup(cur)
    out = open(OUT, "w")
    for i, q in enumerate(qs):
        node = top_node(cur, q)
        # cold
        cold, st = run_once(q)
        hot = None
        if st == "ok":
            h = []
            for _ in range(2):
                d, s2 = run_once(q)
                if s2 == "ok": h.append(d)
                else: st = s2; break
            hot = min(h) if h else None
        rec = {"q": i, "node": node, "cold_s": round(cold, 4),
               "hot_s": round(hot, 4) if hot is not None else None, "status": st}
        out.write(json.dumps(rec) + "\n"); out.flush()
        print(f"q{i:2d} {node[:34]:34s} cold={cold:8.3f} hot={hot if hot is None else round(hot,3)} {st}", flush=True)
    out.close(); ce.close()
    print("M162_TIMING_DONE")

if __name__ == "__main__":
    main()
