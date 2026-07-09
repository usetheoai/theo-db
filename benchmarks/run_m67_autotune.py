#!/usr/bin/env python3
"""M67 — ef_search recommender convergence measured: does theodb.recommend_ef reach the recall target?

The recommender running is NOT the milestone — the milestone is whether the recommended ef ACTUALLY reaches
the recall target (measured against an exact seqscan GT), and how tightly it converges. The DoD asks for a
"medida de convergência": for each target R* we call theodb.recommend_ef, then MEASURE the real recall of the
recommended ef, and report MAE |recall − R*|, RQUT (% queries below target — the tail, not just the mean),
and the ef ladder (iterations). Compared against a fixed-ef baseline.

Synthetic gaussian-mixture corpus (a real theodb_hnsw index — the ef↔recall curve is what we measure, no
embedding model needed, so this runs fast and deterministic). The claim-bearing arithmetic (recall_from_sets,
mae_vs_target, rqut, converged) is pure stdlib → unit-testable WITHOUT a container. Mirrors run_m65/m66.
"""
import argparse
import json
import os
import statistics


# ── Pure arithmetic (unit-testable WITHOUT a container) ─────────────────────────────────────────

def recall_from_sets(ann, gt):
    """recall@k for one query = |ANN ∩ GT| / |GT|. Empty GT → None (not a data point)."""
    if not gt:
        return None
    return len(set(ann) & set(gt)) / len(gt)


def mae_vs_target(recalls, target):
    """Mean absolute error of the achieved recall vs the target. Lower = tighter convergence."""
    vals = [r for r in recalls if r is not None]
    if not vals:
        raise ValueError("no recall data points")
    return sum(abs(r - target) for r in vals) / len(vals)


def rqut(recalls, target):
    """Recall-Quality-Under-Target: fraction of queries whose recall is BELOW the target (the tail safety
    metric — a good mean can hide undershoot). Lower = safer."""
    vals = [r for r in recalls if r is not None]
    if not vals:
        raise ValueError("no recall data points")
    return sum(1 for r in vals if r < target) / len(vals)


def converged(mean_recall, target, band=0.02):
    """Converged = the mean achieved recall is within `band` above the target (reached it without gross
    over-search) OR at/above it. Below target − band → NOT converged (undershoot)."""
    return mean_recall >= target - band


def autotune_verdict(per_target):
    """Per-target convergence verdict. STRATEGY... err, CONVERGED if every target's mean recall reached it
    (within band); else HONEST_NEGATIVE with the failing targets (e.g. 0.99 unreachable within ef_max)."""
    failed = [t for t, m in per_target.items() if not converged(m["mean_recall"], float(t))]
    return {
        "axis": "recall_convergence_per_target",
        "targets": {t: {"ef": m["ef"], "mean_recall": round(m["mean_recall"], 4),
                        "mae": round(m["mae"], 4), "rqut": round(m["rqut"], 4)} for t, m in per_target.items()},
        "failed_targets": failed,
        "status": "CONVERGED" if not failed else "HONEST_NEGATIVE",
        "note": "converged = mean achieved recall within band of the target. HONEST_NEGATIVE lists targets the "
                "recommender could not reach within ef_max (e.g. 0.99 needs super-linear ef — diminishing returns).",
    }


# ── DB integration (needs a container) ──────────────────────────────────────────────────────────

PGHOST = os.environ.get("PGHOST", "localhost")
PGPORT = os.environ.get("PGPORT", "28817")
PGUSER = os.environ.get("PGUSER", "theo")
PGDATABASE = os.environ.get("PGDATABASE", "postgres")


def _conn():
    import psycopg2
    c = psycopg2.connect(host=PGHOST, port=PGPORT, user=PGUSER, dbname=PGDATABASE, connect_timeout=15)
    c.autocommit = True
    return c


def _seed(cur, n, dim, seed):
    import random
    rnd = random.Random(seed)
    n_clusters = 20
    centers = [[rnd.gauss(0, 1) for _ in range(dim)] for _ in range(n_clusters)]
    cur.execute("DROP TABLE IF EXISTS m67_auto")
    cur.execute(f"CREATE TABLE m67_auto (id int PRIMARY KEY, e vector({dim}))")
    for i in range(n):
        c = centers[i % n_clusters]
        vec = "[" + ",".join(f"{c[j] + 0.15 * rnd.gauss(0, 1):.4f}" for j in range(dim)) + "]"
        cur.execute("INSERT INTO m67_auto VALUES (%s, %s)", (i, vec))
    cur.execute("CREATE INDEX m67_auto_idx ON m67_auto USING theodb_hnsw (e theodb_hnsw_cosine_ops)")
    cur.execute("ANALYZE m67_auto")


def _probes(cur, m):
    cur.execute(f"SELECT e::text FROM m67_auto ORDER BY id LIMIT {m}")
    return [r[0] for r in cur.fetchall()]


def _measured_recall(cur, probes, ef, k):
    """Real recall@k at `ef` vs exact seqscan GT, per query."""
    recalls = []
    for q in probes:
        cur.execute("SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on")
        cur.execute(f"SELECT ctid::text FROM m67_auto ORDER BY e <=> %s LIMIT {k}", (q,))
        gt = [r[0] for r in cur.fetchall()]
        cur.execute("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on")
        cur.execute(f"SET theodb_hnsw.ef_search = {ef}")
        cur.execute(f"SELECT ctid::text FROM m67_auto ORDER BY e <=> %s LIMIT {k}", (q,))
        ann = [r[0] for r in cur.fetchall()]
        recalls.append(recall_from_sets(ann, gt))
    return recalls


def run(n, dim, m_queries, k, targets, seed=42):
    conn = _conn()
    cur = conn.cursor()
    _seed(cur, n, dim, seed)
    probes = _probes(cur, m_queries)
    # Sample for the recommender = the first half; the measurement set = the second half (no leakage).
    sample = probes[: m_queries // 2]
    measure = probes[m_queries // 2:]
    per_target = {}
    for t in targets:
        cur.execute(
            "SELECT theodb.recommend_ef('m67_auto'::regclass, 'e', %s::text[], %s, %s)",
            (sample, t, k),
        )
        ef = cur.fetchone()[0]
        recalls = _measured_recall(cur, measure, ef, k)
        vals = [r for r in recalls if r is not None]
        per_target[str(t)] = {
            "ef": ef, "mean_recall": statistics.mean(vals) if vals else 0.0,
            "mae": mae_vs_target(recalls, t), "rqut": rqut(recalls, t), "n": len(vals),
        }
    # Baseline: fixed ef=64 (the default), measured recall for context.
    base = _measured_recall(cur, measure, 64, k)
    base_vals = [r for r in base if r is not None]
    cur.close()
    conn.close()
    return {
        "params": {"n": n, "dim": dim, "m_queries": m_queries, "k": k, "targets": targets, "seed": seed},
        "per_target": per_target,
        "baseline_ef64_recall": round(statistics.mean(base_vals), 4) if base_vals else 0.0,
        "verdict": autotune_verdict(per_target),
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--n", type=int, default=10000)
    ap.add_argument("--dim", type=int, default=128)
    ap.add_argument("--m-queries", type=int, default=100)
    ap.add_argument("--k", type=int, default=10)
    ap.add_argument("--targets", default="0.90,0.95,0.99")
    ap.add_argument("--out", default="docs/benchmarks/m67-autotune.json")
    a = ap.parse_args()
    targets = [float(x) for x in a.targets.split(",")]
    res = run(a.n, a.dim, a.m_queries, a.k, targets)
    os.makedirs(os.path.dirname(a.out), exist_ok=True)
    with open(a.out, "w") as f:
        json.dump(res, f, indent=2)
    print(json.dumps(res["verdict"], indent=2))
    print(f"\nwrote {a.out}")


if __name__ == "__main__":
    main()
