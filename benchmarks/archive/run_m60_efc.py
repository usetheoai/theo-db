#!/usr/bin/env python3
"""M60/M71 root-cause probe — is the theodb HNSW recall MONOTONIC in ef_construction?

A correct HNSW build has recall non-decreasing in ef_construction. M57 measured an INVERSION (efc 64→200 made
recall WORSE, 0.974→0.832) — a build-bug signal. This probe isolates it: sweep efc × {sequential, parallel} build
and read recall@10. If the SEQUENTIAL build (no overwrite lost-update) is monotonic-increasing in efc while the
PARALLEL build (with the `hnsw_parallel.rs` overwrite) inverts, the overwrite is the graph-navigability root cause
(the whole P0 pillar's blocker — `docs/benchmarks/p0-vector-superiority-root-blocker.md`).

The efc + build-mode are set via env on the POSTMASTER (THEODB_HNSW_EF_CONSTRUCTION / THEODB_HNSW_PARALLEL_THRESHOLD),
read at CREATE INDEX time — so the bash orchestrator restarts pg per config. This script does ONE config:
  --phase setup   : make the corpus + queries + exact GT once, persist to --state.
  --phase measure : DROP+CREATE bench_f32 (inherits the postmaster env), measure recall@10 at ef=1000, append result.
"""
import argparse
import json
import os
import time

import run_m51_sbq_inline as h


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--phase", choices=["setup", "measure"], required=True)
    ap.add_argument("--n", type=int, default=100000)
    ap.add_argument("--dim", type=int, default=768)
    ap.add_argument("--nq", type=int, default=50)
    ap.add_argument("--k", type=int, default=10)
    ap.add_argument("--ef", type=int, default=1000)
    ap.add_argument("--state", default="/home/theo/efc_state.json")
    ap.add_argument("--out", default="/home/theo/efc_results.json")
    a = ap.parse_args()

    conn = h._conn()
    cur = conn.cursor()
    cur.execute("CREATE EXTENSION IF NOT EXISTS theodb_rs")
    table = "efcbench"

    if a.phase == "setup":
        h._make_dataset(cur, table, a.n, a.dim, h.SEED)
        queries = h._queries(a.dim, a.nq, h.SEED)
        gt = h._ground_truth(cur, table, queries, a.k)
        json.dump({"queries": queries, "gt": [sorted(g) for g in gt], "n": a.n, "dim": a.dim, "k": a.k},
                  open(a.state, "w"))
        print(f"SETUP done: {a.n}×{a.dim}, GT for {a.nq} queries -> {a.state}")
        return

    st = json.load(open(a.state))
    queries, gt = st["queries"], [set(g) for g in st["gt"]]
    spec = h.SPECS["theodb_hnsw_f32"]
    cur.execute(spec["drop"])
    t0 = time.perf_counter()
    cur.execute(spec["ddl"].format(t=table))  # efc + mode come from the postmaster env (set by the orchestrator)
    build_s = round(time.perf_counter() - t0, 2)
    m = h._measure(cur, table, spec, a.ef, queries, gt, a.k)
    efc = os.environ.get("THEODB_HNSW_EF_CONSTRUCTION", "64")
    thr = os.environ.get("THEODB_HNSW_PARALLEL_THRESHOLD", "4096")
    mode = "sequential" if int(thr) > st["n"] else "parallel"
    row = {"efc": int(efc), "mode": mode, "ef_search": a.ef, "recall": m["recall"],
           "p50_ms": m["p50_ms"], "build_s": build_s}
    try:
        results = json.load(open(a.out))
    except Exception:  # noqa: BLE001
        results = []
    results.append(row)
    json.dump(results, open(a.out, "w"), indent=2)
    print(f"efc={efc:>3} mode={mode:<10} recall@{a.k}={m['recall']:.4f} p50={m['p50_ms']}ms build={build_s}s")


if __name__ == "__main__":
    main()
