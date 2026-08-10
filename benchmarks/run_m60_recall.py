#!/usr/bin/env python3
"""M60 — recall@10 gate for the OWN theodb_hnsw (f32) at scale, after the build-descent beam fix.

The M57 measurement (`benchmarks/artifacts/m57-raw/m57p_ef1000.json`) showed theodb_hnsw_f32 saturating recall@10 at
~0.974 even at ef_search=1000 (an ef-INSENSITIVE plateau) at 500k×768d, while pgvector reached ≥0.99 on the same
gaussian-mixture data. M60's discover (blueprint m60-hnsw-recall-quality) traced the ceiling to the BUILD-path
upper-layer descent being a naive hill-climb (`greedy_descend`) instead of a width-1 beam (Malkov-Yashunin INSERT
Alg.1 / pgvector `HnswSearchLayer`). This driver re-runs the exact regime with the fix and reports the honest
verdict: does theodb_hnsw_f32 best recall@10 cross the 0.99 gate?

Recall is deterministic given (graph, queries, ef); QPS is measured `--qps-runs` times for mean±std (measurement
rigor). Reuses run_m51_sbq_inline SPECS/_helpers (Rule 9 — no duplicated recall logic). pgvector spec is skipped
(removed in M70; not needed — the gate is theodb vs EXACT brute-force GT).

  python3 run_m60_recall.py --n 500000 --dim 768 --nq 50 --qps-runs 3 --out /root/m60_recall.json
"""
import argparse
import json
import statistics
import time

import run_m51_sbq_inline as h

# The M57 plateau (0.974) was measured AT ef_search=1000 — the max the theodb_hnsw.ef_search GUC allows (1..1000).
# Re-measuring the SAME ef points with the beam-descent fix is the direct apples-to-apples RED→GREEN comparison.
F32_EF_SWEEP = [200, 400, 800, 1000]
SBQ_OVER_FETCH = [4, 8, 16, 32]  # D3 re-check: SBQ still < f32 at recall≥0.99?


def _index_name(name):
    # SPECS ddl is "CREATE INDEX <name> ON ..." — extract the index relname so we can reuse an existing build.
    return h.SPECS[name]["ddl"].split("CREATE INDEX", 1)[1].split(" ON ", 1)[0].strip()


def _build(cur, table, name):
    spec = h.SPECS[name]
    cur.execute("SELECT 1 FROM pg_class WHERE relname = %s", (_index_name(name),))
    if cur.fetchone():  # reuse an index already built in a prior (crashed) run — avoids a costly rebuild
        return "reused"
    cur.execute(spec["drop"])
    t0 = time.perf_counter()
    cur.execute(spec["ddl"].format(t=table))
    return round(time.perf_counter() - t0, 2)


def _sweep(cur, table, name, sweep, queries, gt, k, qps_runs):
    spec = h.SPECS[name]
    pts = []
    for v in sweep:
        recall = None
        qps_samples = []
        for _ in range(qps_runs):
            m = h._measure(cur, table, spec, v, queries, gt, k)
            recall = m["recall"]  # deterministic across runs
            qps_samples.append(m["qps_1client"])
        pts.append({
            "knob": v, "recall": recall,
            "qps_mean": round(statistics.mean(qps_samples), 1),
            "qps_std": round(statistics.pstdev(qps_samples), 2) if len(qps_samples) > 1 else 0.0,
            "qps_runs": qps_samples,
        })
    return pts


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--n", type=int, default=500000)
    ap.add_argument("--dim", type=int, default=768)
    ap.add_argument("--nq", type=int, default=50)
    ap.add_argument("--k", type=int, default=10)
    ap.add_argument("--qps-runs", type=int, default=3)
    ap.add_argument("--out", default="/root/m60_recall.json")
    a = ap.parse_args()

    conn = h._conn()
    cur = conn.cursor()
    # theodb_rs is the base (post-M70 flip): provides the `vector` type + `theodb_hnsw` AM + cosine opclass +
    # `theodb_hnsw.ef_search` GUC — everything the recall gate needs. The umbrella `theodb` (AI/embed) is not required.
    cur.execute("CREATE EXTENSION IF NOT EXISTS theodb_rs")
    table = "m60bench"
    # The corpus is deterministic (seed h.SEED). Skip regeneration if the table is already fully populated — this
    # lets a re-run reuse an index built in a prior (crashed) run instead of dropping+rebuilding it.
    cur.execute("SELECT to_regclass(%s)", (table,))
    exists = cur.fetchone()[0] is not None
    n_rows = 0
    if exists:
        cur.execute(f"SELECT count(*) FROM {table}")
        n_rows = cur.fetchone()[0]
    if n_rows != a.n:
        h._make_dataset(cur, table, a.n, a.dim, h.SEED)
    queries = h._queries(a.dim, a.nq, h.SEED)
    gt = h._ground_truth(cur, table, queries, a.k)  # EXACT brute-force top-k (no pgvector needed)

    out = {"n": a.n, "dim": a.dim, "k": a.k, "nq": a.nq, "qps_runs": a.qps_runs,
           "load": h._load(), "metric": "cosine", "specs": {}}

    # Primary gate — theodb_hnsw_f32.
    bs = _build(cur, table, "theodb_hnsw_f32")
    f32 = _sweep(cur, table, "theodb_hnsw_f32", F32_EF_SWEEP, queries, gt, a.k, a.qps_runs)
    out["specs"]["theodb_hnsw_f32"] = {"build_s": bs, "sweep": f32}
    best_f32 = max(f32, key=lambda p: p["recall"])
    out["f32_best_recall"] = best_f32["recall"]
    out["f32_best_ef"] = best_f32["knob"]
    out["m60_gate_met"] = best_f32["recall"] >= 0.99

    # (pgvector control runs standalone in a separate DB — see run_m60_pgvector_control.py — because pgvector's
    # `public.vector` collides with theodb_rs's own `public.vector`, so the two cannot coexist in one database.)

    # D3 re-check — SBQ still below f32 at production recall.
    try:
        bs_sbq = _build(cur, table, "theodb_hnsw_sbq")
        sbq = _sweep(cur, table, "theodb_hnsw_sbq", SBQ_OVER_FETCH, queries, gt, a.k, a.qps_runs)
        out["specs"]["theodb_hnsw_sbq"] = {"build_s": bs_sbq, "sweep": sbq}
        out["sbq_best_recall"] = max(sbq, key=lambda p: p["recall"])["recall"]
    except Exception as e:  # noqa: BLE001 — record honestly
        out["specs"]["theodb_hnsw_sbq"] = {"error": str(e)[:200]}

    conn.close()
    json.dump(out, open(a.out, "w"), indent=2)
    verdict = "GREEN (recall≥0.99)" if out["m60_gate_met"] else "HONEST-NEGATIVE (recall<0.99)"
    print(f"M60 f32 best recall@10 = {out['f32_best_recall']} @ef={out['f32_best_ef']} → {verdict}")
    for p in f32:
        print(f"  ef={p['knob']:>4}  recall={p['recall']:.4f}  qps={p['qps_mean']}±{p['qps_std']}")
    print(f"artifact -> {a.out}")


if __name__ == "__main__":
    main()
