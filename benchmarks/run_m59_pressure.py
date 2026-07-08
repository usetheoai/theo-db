#!/usr/bin/env python3
"""M59 (P1) — RAM-pressure QPS: does the anisotropic-PQ + AH index (4-byte codes) beat f32 (3072-byte vectors)
when the working set exceeds cache? The D3 crux for AQ, mirroring `run_m57_pressure.py` for SBQ.

In-RAM the AH LUT buys little (everything is cached; the HNSW walk dominates — measured: AQ ≈ f32 at 100k). The
thesis is that under memory pressure the TINY AQ codes (⌈m/2⌉ bytes/vector — 768× smaller than f32) stay cacheable
while the LARGE f32 index spills to disk, so AQ wins the QPS race. Two externally-orchestrated phases so RAM can be
constrained BETWEEN build and measure. Reuses run_m59_aq.SPECS (AQ + f32) + the m57 harness (Rule 9).
"""
import argparse
import json
import os
import time

import run_m51_sbq_inline as h
import run_m59_aq as m59

# AQ vs f32 only — the pressure discriminator (SBQ/pgvector already characterized in M57/M59 in-RAM).
SPECS = {"theodb_hnsw_aq": m59.SPECS["theodb_hnsw_aq"], "theodb_hnsw_f32": m59.SPECS["theodb_hnsw_f32"]}


def phase_build(n, dim, nq, k, state_path, make):
    conn = h._conn(); cur = conn.cursor()
    cur.execute("CREATE EXTENSION IF NOT EXISTS theodb CASCADE")
    table = "m59bench"
    if make:
        h._make_dataset(cur, table, n, dim, h.SEED)
    queries = h._queries(dim, nq, h.SEED)
    gt = h._ground_truth(cur, table, queries, k)
    build_s = {}
    for name, spec in SPECS.items():
        cur.execute(spec["drop"])
        try:
            t0 = time.perf_counter(); cur.execute(spec["ddl"].format(t=table))
            build_s[name] = round(time.perf_counter() - t0, 2)
        except Exception as e:  # noqa: BLE001
            build_s[name] = None; print(f"BUILD ERROR {name}: {str(e)[:180]}")
    conn.close()
    json.dump({"n": n, "dim": dim, "k": k, "queries": queries, "gt": [sorted(g) for g in gt],
               "build_s": build_s}, open(state_path, "w"))
    print(f"BUILD done: {build_s}; state -> {state_path}")


def phase_measure(state_path, out_path, mem_note):
    st = json.load(open(state_path))
    queries, gt, k = st["queries"], [set(g) for g in st["gt"]], st["k"]
    conn = h._conn(); cur = conn.cursor(); table = "m59bench"
    results = {}
    for name, spec in SPECS.items():
        if st["build_s"].get(name) is None:
            results[name] = {"error": "build failed"}; continue
        pts = [{"knob": v, "build_s": st["build_s"][name], **h._measure(cur, table, spec, v, queries, gt, k)}
               for v in spec["sweep"]]
        results[name] = {"best": max(pts, key=lambda p: p["recall"]), "sweep": pts}
    conn.close()

    def qps_at(name, gate):
        pts = results.get(name, {}).get("sweep", [])
        ok = [p for p in pts if p["recall"] >= gate]
        return max((p["qps_1client"] for p in ok), default=None)
    # matched-recall comparison: use the max recall both actually reach (graph ceiling ~0.978, M60), not 0.99.
    common = min((max((p["recall"] for p in results[n]["sweep"]), default=0) for n in SPECS if "sweep" in results[n]),
                 default=0)
    gate = round(common - 0.001, 3)
    aq_q, f32_q = qps_at("theodb_hnsw_aq", gate), qps_at("theodb_hnsw_f32", gate)
    ratio = round(aq_q / f32_q, 2) if (aq_q and f32_q) else None
    out = {"mem_note": mem_note, "n": st["n"], "dim": st["dim"], "k": k, "load": h._load(),
           "matched_recall_gate": gate, "aq_qps": aq_q, "f32_qps": f32_q, "aq_over_f32_ratio": ratio,
           "thesis_2x_met": (ratio is not None and ratio >= 2.0), "per_spec": results}
    json.dump(out, open(out_path, "w"), indent=2)
    print(f"MEASURE ({mem_note}): matched recall≥{gate} → AQ={aq_q} f32={f32_q} qps  ratio={ratio} (≥2×: {out['thesis_2x_met']})")
    for name, r in results.items():
        if "best" in r:
            b = r["best"]; print(f"  {name}: best recall {b['recall']} p50 {b['p50_ms']}ms qps {b['qps_1client']}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--phase", choices=["build", "measure"], required=True)
    ap.add_argument("--n", type=int, default=500000)
    ap.add_argument("--dim", type=int, default=768)
    ap.add_argument("--nq", type=int, default=50)
    ap.add_argument("--k", type=int, default=10)
    ap.add_argument("--make", action="store_true")
    ap.add_argument("--state", default="/root/m59p_state.json")
    ap.add_argument("--out", default="/root/m59_pressure.json")
    ap.add_argument("--mem-note", default="unconstrained")
    args = ap.parse_args()
    if args.phase == "build":
        phase_build(args.n, args.dim, args.nq, args.k, args.state, args.make)
    else:
        phase_measure(args.state, args.out, args.mem_note)


if __name__ == "__main__":
    main()
