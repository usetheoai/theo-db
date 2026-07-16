#!/usr/bin/env python3
"""M107 Phase-0 spike driver — native CSR+BFS vs recursive-CTE baseline, mean±std over trials.

Per (scale, trial-seed): run the native Rust bin (build/traverse/oracle + CSV), load the SAME
graph into the local docker PostgreSQL 17, run the recursive-CTE baseline (theo-rag semantics),
assert the reachable-set oracle matches (count + checksum), record timings. Emits the benchmark
artifact. ZERO fabricated numbers — every value comes from a real run.
"""
import json, subprocess, statistics, sys, re, pathlib, datetime

BIN = "./target/release/m107_graph_spike"
CONTAINER = "theo-workspace-pg-cloud-1"
DB = "m107_spike"
SCALES = [(100_000, 20_000), (1_000_000, 200_000)]  # (edges, nodes)
HOPS = 3
N_SEEDS = 5
TRIALS = [1, 2, 3, 4]
CTE_TIMEOUT = "120s"

def sh(cmd, inp=None):
    return subprocess.run(cmd, shell=True, capture_output=True, text=True, input=inp)

def native(edges, nodes, seed, csv):
    r = sh(f"{BIN} {edges} {nodes} {seed} {HOPS} {N_SEEDS} {csv}")
    if r.returncode != 0:
        sys.exit(f"native bin failed: {r.stderr}")
    return json.loads(r.stdout.strip())

def cte(seeds_csv):
    seeds = seeds_csv.replace(" ", ",")
    sql = f"""DROP TABLE IF EXISTS edges; CREATE TABLE edges(src bigint,dst bigint,weight int);
\\copy edges FROM '/tmp/edges.csv' WITH (FORMAT csv)
CREATE INDEX ON edges(src); CREATE INDEX ON edges(dst); ANALYZE edges;
SET statement_timeout='{CTE_TIMEOUT}';
\\timing on
WITH RECURSIVE reach(node,hop) AS (
  SELECT unnest(ARRAY[{seeds}]::bigint[]),0
  UNION ALL
  SELECT CASE WHEN e.src=r.node THEN e.dst ELSE e.src END, r.hop+1
  FROM reach r JOIN edges e ON (e.src=r.node OR e.dst=r.node) WHERE r.hop<{HOPS})
SELECT count(DISTINCT node) reached, sum(DISTINCT node) checksum FROM reach;"""
    pathlib.Path("/tmp/m107_q.sql").write_text(sql)
    sh(f"docker cp /tmp/m107_q.sql {CONTAINER}:/tmp/q.sql")
    r = sh(f"docker exec -i {CONTAINER} psql -U theo -d {DB} -f /tmp/q.sql")
    out = r.stdout + r.stderr
    if "statement timeout" in out.lower() or "canceling statement" in out.lower():
        return {"timed_out": True, "ms": None, "reached": None, "checksum": None}
    m_time = re.findall(r"Time:\s*([\d.]+)\s*ms", out)
    m_row = re.search(r"^\s*(\d+)\s*\|\s*(\d+)\s*$", out, re.M)
    if not m_time or not m_row:
        sys.exit(f"could not parse CTE output:\n{out}")
    # the LAST Time: is the recursive query (the DDL timings precede it)
    return {"timed_out": False, "ms": float(m_time[-1]), "reached": int(m_row.group(1)), "checksum": int(m_row.group(2))}

rows = []
for edges, nodes in SCALES:
    for t in TRIALS:
        nat = native(edges, nodes, t, "/tmp/edges.csv")
        sh(f"docker cp /tmp/edges.csv {CONTAINER}:/tmp/edges.csv")
        b = cte(nat["seeds_csv"])
        # correctness oracle: native reachable-set == CTE reachable-set (unless CTE timed out)
        oracle = "N/A (CTE timeout)" if b["timed_out"] else (
            "PASS" if (b["reached"] == nat["reached_count"] and b["checksum"] == nat["reached_checksum"]) else "FAIL")
        if oracle == "FAIL":
            sys.exit(f"ORACLE FAIL scale={edges} trial={t}: native({nat['reached_count']},{nat['reached_checksum']}) != cte({b['reached']},{b['checksum']})")
        rows.append({"edges": edges, "nodes": nodes, "trial": t, "hops": HOPS,
                     "native_build_ms": nat["build_ms"], "native_traverse_ms": nat["traverse_ms"],
                     "native_total_ms": nat["build_ms"] + nat["traverse_ms"],
                     "cte_ms": b["ms"], "cte_timed_out": b["timed_out"],
                     "reached": nat["reached_count"], "oracle": oracle})
        cte_str = "TIMEOUT" if b["timed_out"] else f"{b['ms']:.1f}ms"
        nb, nt, rc = nat["build_ms"], nat["traverse_ms"], nat["reached_count"]
        print(f"scale={edges:>9} trial={t}: native build={nb:.2f} traverse={nt:.3f}ms  cte={cte_str}  reached={rc}  oracle={oracle}")

def agg(scale_edges):
    r = [x for x in rows if x["edges"] == scale_edges]
    def ms(k): return [x[k] for x in r]
    completed = [x for x in r if not x["cte_timed_out"]]
    speedups_tr = [x["cte_ms"] / x["native_traverse_ms"] for x in completed]
    speedups_tot = [x["cte_ms"] / x["native_total_ms"] for x in completed]
    def stat(v): return {"mean": round(statistics.mean(v), 3), "std": round(statistics.pstdev(v), 3)} if v else None
    return {"edges": scale_edges, "trials": len(r),
            "native_build_ms": stat(ms("native_build_ms")), "native_traverse_ms": stat(ms("native_traverse_ms")),
            "native_total_ms": stat(ms("native_total_ms")),
            "cte_ms": stat([x["cte_ms"] for x in completed]) if completed else None,
            "cte_timeouts": sum(1 for x in r if x["cte_timed_out"]),
            "speedup_traverse_x": stat(speedups_tr), "speedup_total_x": stat(speedups_tot)}

summary = [agg(e) for e, _ in SCALES]
out = {"date": datetime.date.today().isoformat(), "hops": HOPS, "n_seeds": N_SEEDS, "trials_per_scale": len(TRIALS),
       "host": "local docker postgres:17 (PG 17.10) + native Rust release", "rows": rows, "summary": summary}
docs = pathlib.Path("../../docs/benchmarks")
(docs / "m107-graph-spike.json").write_text(json.dumps(out, indent=2))
print("\n=== SUMMARY ===")
for s in summary:
    print(json.dumps(s))
print("\nwrote docs/benchmarks/m107-graph-spike.json")
