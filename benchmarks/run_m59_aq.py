#!/usr/bin/env python3
"""M59 (P1) — anisotropic-PQ + Asymmetric-Hashing recall×QPS: theodb_hnsw AQ (v3) vs SBQ (v2) vs f32 (v1) vs
pgvector hnsw, cosine. The D3 gate: does the AH-scored walk + exact-f32 rerank PRESERVE recall@10 while cheapening
the per-candidate score to an in-register LUT lookup — and does that close/reduce the ~25× QPS gap vs ScaNN (M33)?

Reuses the M57 harness (`_make_dataset` gaussian-mixture, `_queries`, `_ground_truth`, `_measure`, `_conn` with
keepalives) — Rule 9, no reimplementation. Adds ONLY the `theodb_hnsw_aq` spec (meta v3, `WITH (pq_subspaces=M)`).
The scan knob is `over_fetch` (reused from SBQ — the recall-recovery carrier). `dim % pq_subspaces == 0` required
(768 % 8 == 0 → sub_dim 96). Honest-negative is a valid outcome (ADR-0019).
"""
import argparse
import json
import os
import statistics
import time

import run_m51_sbq_inline as h  # reuse dataset/queries/GT/measure/conn (Rule 9)

# M59: the AQ index + the three baselines. pq_subspaces must divide dim (768 → 8/16/32/48/64/96/128/192/256/384).
PQ_SUBSPACES = int(os.environ.get("PQ_SUBSPACES", "8"))
# The reloption `aq_threshold` is MILLI-SCALED (η × 1000, valid 1000..1_000_000) so a single int carries the float
# η. ScaNN's operating point T=0.2 maps to η≈4.1 (paper Theorem 3.4) → 4100. Default here = 4100.
AQ_THRESHOLD_MILLI = os.environ.get("AQ_THRESHOLD_MILLI", "4100")
EF_AQ = os.environ.get("EF_AQ", "800")

SPECS = {
    "theodb_hnsw_aq": {
        "ddl": ("CREATE INDEX bench_aq ON {t} USING theodb_hnsw (v theodb_hnsw_cosine_ops) "
                f"WITH (pq_subspaces = {PQ_SUBSPACES}, pq_bits = 4, aq_threshold = {AQ_THRESHOLD_MILLI})"),
        "drop": "DROP INDEX IF EXISTS bench_aq",
        # AH-scored walk (approx) + exact-f32 rerank of top k·over_fetch; over_fetch is the recall-recovery knob.
        "knob": lambda x: [f"SET theodb_hnsw.ef_search = {EF_AQ}", f"SET theodb_hnsw.over_fetch = {x}"],
        "sweep": [4, 8, 16, 32],
    },
    "theodb_hnsw_sbq": h.SPECS["theodb_hnsw_sbq"],
    "theodb_hnsw_f32": h.SPECS["theodb_hnsw_f32"],
    "pgvector_hnsw": h.SPECS["pgvector_hnsw"],
}


def run(n, dim, nq, k, runs):
    conn = h._conn()
    cur = conn.cursor()
    cur.execute("CREATE EXTENSION IF NOT EXISTS theodb CASCADE")
    table = "m59bench"
    load_pre = h._load()
    h._make_dataset(cur, table, n, dim, h.SEED)
    queries = h._queries(dim, nq, h.SEED)
    gt = h._ground_truth(cur, table, queries, k)

    results = {name: [] for name in SPECS}
    build_s = {}
    for name, spec in SPECS.items():
        cur.execute(spec["drop"])
        try:
            t0 = time.perf_counter()
            cur.execute(spec["ddl"].format(t=table))
            build_s[name] = round(time.perf_counter() - t0, 2)
        except Exception as e:  # noqa: BLE001 — record honestly, never fabricate
            build_s[name] = None
            results[name].append({"error": str(e)[:200]})
    loads = []
    for _ in range(runs):
        loads.append(h._load())
        for name, spec in SPECS.items():
            if build_s.get(name) is None:
                continue
            pts = [{"knob": v, "build_s": build_s[name], **h._measure(cur, table, spec, v, queries, gt, k)}
                   for v in spec["sweep"]]
            results[name].append(pts)
    for name, spec in SPECS.items():
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

    # D3 verdict inputs: best-recall QPS at recall≥0.99 for AQ vs SBQ vs f32 (matched recall).
    def best_qps_at(name, gate):
        pts = [c for c in agg.get(name, {}).get("curve", []) if c["recall_mean"] >= gate]
        return max((c["qps_1client_mean"] for c in pts), default=None)
    aq99, sbq99, f3299, pgv99 = (best_qps_at(x, 0.99) for x in
                                 ("theodb_hnsw_aq", "theodb_hnsw_sbq", "theodb_hnsw_f32", "pgvector_hnsw"))
    aq_best = max((c["recall_mean"] for c in agg.get("theodb_hnsw_aq", {}).get("curve", [])), default=None)
    return {"n": n, "dim": dim, "queries": nq, "k": k, "runs": runs, "metric": "cosine",
            "pq_subspaces": PQ_SUBSPACES, "aq_threshold_milli": AQ_THRESHOLD_MILLI, "load_pre": load_pre, "load_per_run": loads,
            "nproc": os.cpu_count(), "aq_best_recall_at_10": aq_best,
            "qps_at_recall_0_99": {"aq": aq99, "sbq": sbq99, "f32": f3299, "pgvector": pgv99},
            "aq_over_f32_ratio": (round(aq99 / f3299, 2) if (aq99 and f3299) else None),
            "aq_over_pgvector_ratio": (round(aq99 / pgv99, 2) if (aq99 and pgv99) else None),
            "per_spec": agg, "raw": results}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--n", type=int, default=100000)
    ap.add_argument("--dim", type=int, default=768)
    ap.add_argument("--nq", type=int, default=50)
    ap.add_argument("--k", type=int, default=10)
    ap.add_argument("--runs", type=int, default=3)  # ≥3 iterações: mean±std (regra measurement-first)
    ap.add_argument("--out", default="/root/m59_aq.json")
    args = ap.parse_args()
    if args.dim % PQ_SUBSPACES != 0:
        raise SystemExit(f"dim {args.dim} not divisible by pq_subspaces {PQ_SUBSPACES}")
    data = run(args.n, args.dim, args.nq, args.k, args.runs)
    json.dump(data, open(args.out, "w"), indent=2)
    print(f"wrote {args.out} (n={args.n} dim={args.dim} pq_subspaces={PQ_SUBSPACES} runs={args.runs})")
    print(f"AQ best recall@10 = {data['aq_best_recall_at_10']}")
    print(f"QPS @recall≥0.99: {data['qps_at_recall_0_99']}")
    print(f"AQ/f32 = {data['aq_over_f32_ratio']}  AQ/pgvector = {data['aq_over_pgvector_ratio']}")
    for name, a in data["per_spec"].items():
        if "curve" in a:
            top = max(a["curve"], key=lambda c: c["recall_mean"])
            print(f"  {name}: best recall {top['recall_mean']} @ p50 {top['p50_ms_mean']}ms "
                  f"qps {top['qps_1client_mean']} (knob {top['knob']}, build {top['build_s']}s)")
        else:
            print(f"  {name}: {a['error']}")


if __name__ == "__main__":
    main()
