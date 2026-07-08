#!/usr/bin/env python3
"""M57 (P0) — RAM-pressure QPS: does SBQ-inline give ≥2× QPS over f32 when the working set exceeds cache?

The D3 crux. In-RAM the SBQ codes buy nothing (measured: SBQ ≤ f32 — everything is cached anyway). The thesis is
that under memory pressure the SMALL SBQ codes stay cacheable while the LARGE f32 vectors spill to disk, so SBQ wins
the QPS race. This driver measures exactly that, in two externally-orchestrated phases so RAM can be constrained
BETWEEN build and measure (a full HNSW build needs maintenance_work_mem the constrained state cannot give):

  phase=build   : (re)build the 3 indexes on an existing m51bench table, persist queries+GT to --state, exit.
  phase=measure : load queries+GT from --state, measure QPS per index under WHATEVER RAM the container now has.

Orchestration (external): build at full RAM  →  `docker update --memory=<tight> pgm57`  →  drop OS caches  →
measure. Reuses the m51 harness (SPECS, _conn, _measure, _ground_truth, _make_dataset) — Rule 9, no reimplementation.
"""
import argparse
import json
import os
import time

import run_m51_sbq_inline as h  # reuse SPECS/_conn/_measure/_ground_truth/_make_dataset (Rule 9 — no dup logic)


def phase_build(n, dim, nq, k, state_path, make):
    conn = h._conn()
    cur = conn.cursor()
    cur.execute("CREATE EXTENSION IF NOT EXISTS theodb CASCADE")
    table = "m51bench"
    if make:
        h._make_dataset(cur, table, n, dim, h.SEED)
    queries = h._queries(dim, nq, h.SEED)
    gt = h._ground_truth(cur, table, queries, k)
    build_s = {}
    for name, spec in h.SPECS.items():
        cur.execute(spec["drop"])
        try:
            t0 = time.perf_counter()
            cur.execute(spec["ddl"].format(t=table))
            build_s[name] = round(time.perf_counter() - t0, 2)
        except Exception as e:  # noqa: BLE001 — record honestly
            build_s[name] = None
            print(f"BUILD ERROR {name}: {str(e)[:160]}")
    conn.close()
    json.dump({"n": n, "dim": dim, "k": k, "queries": queries, "gt": [sorted(g) for g in gt],
               "build_s": build_s}, open(state_path, "w"))
    print(f"BUILD done: {build_s}; state -> {state_path}")


def phase_measure(state_path, out_path, mem_note):
    st = json.load(open(state_path))
    queries = st["queries"]
    gt = [set(g) for g in st["gt"]]
    k = st["k"]
    conn = h._conn()
    cur = conn.cursor()
    table = "m51bench"
    results = {}
    for name, spec in h.SPECS.items():
        if st["build_s"].get(name) is None:
            results[name] = {"error": "build failed"}
            continue
        pts = [{"knob": v, "build_s": st["build_s"][name], **h._measure(cur, table, spec, v, queries, gt, k)}
               for v in spec["sweep"]]
        best = max(pts, key=lambda p: p["recall"])
        results[name] = {"best": best, "sweep": pts}
    conn.close()
    # the pressure verdict: SBQ best-recall QPS vs f32 best-recall QPS, at recall ≥ 0.99
    def qps_at_gate(name):
        pts = results.get(name, {}).get("sweep", [])
        ok = [p for p in pts if p["recall"] >= 0.99]
        return max((p["qps_1client"] for p in ok), default=None)
    sbq_q, f32_q = qps_at_gate("theodb_hnsw_sbq"), qps_at_gate("theodb_hnsw_f32")
    ratio = round(sbq_q / f32_q, 2) if (sbq_q and f32_q) else None
    out = {"mem_note": mem_note, "n": st["n"], "dim": st["dim"], "k": k, "load": h._load(),
           "sbq_qps_at_recall99": sbq_q, "f32_qps_at_recall99": f32_q, "sbq_over_f32_ratio": ratio,
           "thesis_2x_met": (ratio is not None and ratio >= 2.0), "per_spec": results}
    json.dump(out, open(out_path, "w"), indent=2)
    print(f"MEASURE ({mem_note}): SBQ={sbq_q} f32={f32_q} qps@recall0.99 → ratio={ratio} (≥2× met: {out['thesis_2x_met']})")
    for name, r in results.items():
        if "best" in r:
            b = r["best"]
            print(f"  {name}: best recall {b['recall']} p50 {b['p50_ms']}ms qps {b['qps_1client']}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--phase", choices=["build", "measure"], required=True)
    ap.add_argument("--n", type=int, default=100000)
    ap.add_argument("--dim", type=int, default=768)
    ap.add_argument("--nq", type=int, default=50)
    ap.add_argument("--k", type=int, default=10)
    ap.add_argument("--make", action="store_true", help="(build only) regenerate the corpus")
    ap.add_argument("--state", default="/root/m57_pressure_state.json")
    ap.add_argument("--out", default="/root/m57_pressure.json")
    ap.add_argument("--mem-note", default="unconstrained")
    args = ap.parse_args()
    if args.phase == "build":
        phase_build(args.n, args.dim, args.nq, args.k, args.state, args.make)
    else:
        phase_measure(args.state, args.out, args.mem_note)


if __name__ == "__main__":
    main()
