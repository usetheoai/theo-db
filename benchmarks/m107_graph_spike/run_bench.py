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

def load_edges():
    """Load /tmp/edges.csv into the throwaway PG (indexed + analyzed) — done once per trial."""
    sql = """DROP TABLE IF EXISTS edges; CREATE TABLE edges(src bigint,dst bigint,weight int);
\\copy edges FROM '/tmp/edges.csv' WITH (FORMAT csv)
CREATE INDEX ON edges(src); CREATE INDEX ON edges(dst); ANALYZE edges;"""
    pathlib.Path("/tmp/m107_load.sql").write_text(sql)
    sh(f"docker cp /tmp/m107_load.sql {CONTAINER}:/tmp/load.sql")
    sh(f"docker exec -i {CONTAINER} psql -U theo -d {DB} -f /tmp/load.sql")

def cte(seeds_csv, union_kind):
    """Run the recursive-CTE baseline. union_kind ∈ {'UNION ALL' (theo-rag), 'UNION' (dedup/visited-tracking)}.
    A set-hash oracle (bit_xor of a mixed id) is stronger than count+sum (not injective)."""
    seeds = seeds_csv.replace(" ", ",")
    sql = f"""SET statement_timeout='{CTE_TIMEOUT}';
\\timing on
WITH RECURSIVE reach(node,hop) AS (
  SELECT unnest(ARRAY[{seeds}]::bigint[]),0
  {union_kind}
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
        load_edges()
        ba = cte(nat["seeds_csv"], "UNION ALL")   # theo-rag baseline
        bu = cte(nat["seeds_csv"], "UNION")        # fairer: dedup / visited-tracking
        # correctness oracle: BOTH CTE variants' reachable-set == native's (count + checksum)
        def check(b, label):
            if b["timed_out"]:
                return "N/A (timeout)"
            ok = b["reached"] == nat["reached_count"] and b["checksum"] == nat["reached_checksum"]
            if not ok:
                sys.exit(f"ORACLE FAIL [{label}] scale={edges} trial={t}: native({nat['reached_count']},{nat['reached_checksum']}) != cte({b['reached']},{b['checksum']})")
            return "PASS"
        oa, ou = check(ba, "UNION ALL"), check(bu, "UNION")
        rows.append({"edges": edges, "nodes": nodes, "trial": t, "hops": HOPS,
                     "native_build_ms": nat["build_ms"], "native_traverse_ms": nat["traverse_ms"],
                     "native_total_ms": nat["build_ms"] + nat["traverse_ms"],
                     "cte_unionall_ms": ba["ms"], "cte_unionall_timed_out": ba["timed_out"],
                     "cte_union_ms": bu["ms"], "cte_union_timed_out": bu["timed_out"],
                     "reached": nat["reached_count"], "oracle_unionall": oa, "oracle_union": ou})
        aa = "TIMEOUT" if ba["timed_out"] else f"{ba['ms']:.1f}"
        uu = "TIMEOUT" if bu["timed_out"] else f"{bu['ms']:.1f}"
        nb, nt, rc = nat["build_ms"], nat["traverse_ms"], nat["reached_count"]
        print(f"scale={edges:>9} trial={t}: native build={nb:.2f} traverse={nt:.3f}ms  cte(all)={aa}ms cte(dedup)={uu}ms  reached={rc}  oracle={oa}/{ou}")

def agg(scale_edges):
    r = [x for x in rows if x["edges"] == scale_edges]
    def ms(k): return [x[k] for x in r]
    def stat(v): return {"mean": round(statistics.mean(v), 3), "std": round(statistics.pstdev(v), 3)} if v else None
    ca = [x for x in r if not x["cte_unionall_timed_out"]]
    cu = [x for x in r if not x["cte_union_timed_out"]]
    return {"edges": scale_edges, "trials": len(r),
            "native_build_ms": stat(ms("native_build_ms")), "native_traverse_ms": stat(ms("native_traverse_ms")),
            "native_total_ms": stat(ms("native_total_ms")),
            "cte_unionall_ms": stat([x["cte_unionall_ms"] for x in ca]) if ca else None,
            "cte_union_dedup_ms": stat([x["cte_union_ms"] for x in cu]) if cu else None,
            "speedup_traverse_vs_unionall_x": stat([x["cte_unionall_ms"]/x["native_traverse_ms"] for x in ca]),
            "speedup_total_vs_unionall_x": stat([x["cte_unionall_ms"]/x["native_total_ms"] for x in ca]),
            "speedup_traverse_vs_union_dedup_x": stat([x["cte_union_ms"]/x["native_traverse_ms"] for x in cu])}

summary = [agg(e) for e, _ in SCALES]
out = {"date": datetime.date.today().isoformat(), "hops": HOPS, "n_seeds": N_SEEDS, "trials_per_scale": len(TRIALS),
       "host": "local docker postgres:17 (PG 17.10) + native Rust release", "rows": rows, "summary": summary}
docs = pathlib.Path("../../docs/benchmarks")
(docs / "m107-graph-spike.json").write_text(json.dumps(out, indent=2))
print("\n=== SUMMARY ===")
for s in summary:
    print(json.dumps(s))
print("\nwrote docs/benchmarks/m107-graph-spike.json")
