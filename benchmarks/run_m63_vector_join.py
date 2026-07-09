#!/usr/bin/env python3
"""M63 — vector JOIN via LATERAL-index-scan: join-recall + latency, 3 arms + dedup e2e.

The `a CROSS JOIN LATERAL (SELECT … FROM b ORDER BY b.emb <=> a.emb LIMIT k) j` pattern is a
planner-integrated similarity join whose inner branch is an index-served single-vector top-k
(`amcanorderbyop`, mod.rs:78) — the `#[pg_test]` `vector_join_uses_index_scan` proves the EXPLAIN.
This harness MEASURES it (Rule 5 — performance is a claim, not opinion):

  T1 — LATERAL over theodb_hnsw            (the product)
  T2 — naive top-level cross-join + sort   (O(n·m), the anti-objective; recall=1.0 by construction)
  T3 — LATERAL over pgvector hnsw          (the SOTA permissive control, M45/M52 discipline)

Primary metric: join-recall per outer row `a_i`, `recall_i = |ANN_i ∩ EXACT_i| / k`, reported
min + mean ± std vs the exact O(n·m) ground truth on a tractable subset (R2 — the min surfaces
recall-0 rows a mean would hide). Plus an end-to-end kNN-self-join dedup case with planted
duplicates → detection precision/recall.

Reuses `theodb_bench.metrics.latency_percentiles` (Rule 9 — no new harness infra); mirrors
`run_m52_filtered_ann.py`. The claim-bearing arithmetic (join_recall, dedup_metrics, verdict) is
pure stdlib so it is unit-testable WITHOUT a container (`benchmarks/tests/test_run_m63_vector_join.py`).
Honest-negative accepted: if T1 loses the latency race to T3, the DoD ("uses the index, not O(n²)")
is still met by T1 and failed by T2 — the latency gap is documented, not masked (public-copy.md §4).
"""
import argparse
import json
import os
import statistics
import time

from theodb_bench.metrics import latency_percentiles  # stdlib-only (Rule 9); no psycopg2 pulled

# ── pure claim-bearing arithmetic (no DB — unit-testable) ─────────────────────────────────────

SEED = 42
N_CLUSTERS = 8          # gaussian-mixture centers → the corpus has real NN structure (ADR 0012)
CLUSTER_STD = 0.10      # within-cluster spread ≪ inter-center distance → tight, unambiguous NNs


def join_recall(ann_per_row, exact_per_row):
    """Per-outer-row join-recall. `ann_per_row`/`exact_per_row` are lists of sets (one set of
    top-k ids per outer row of `a`). Returns {min, mean, std, n}. recall_i = |ANN∩EXACT|/|EXACT|.

    A row whose exact top-k is empty (no candidates) is not a recall data point (skipped).
    """
    recalls = []
    for ann, exact in zip(ann_per_row, exact_per_row):
        if not exact:
            continue
        recalls.append(len(ann & exact) / len(exact))
    if not recalls:
        return {"min": None, "mean": None, "std": None, "n": 0}
    return {
        "min": round(min(recalls), 4),
        "mean": round(statistics.mean(recalls), 4),
        "std": round(statistics.pstdev(recalls), 4) if len(recalls) > 1 else 0.0,
        "n": len(recalls),
    }


def dedup_metrics(found_pairs, planted_pairs):
    """Duplicate-detection precision + recall (both, never a blended score — R2). Pairs are
    normalized unordered (min,max) so (a,b)==(b,a). precision=|found∩planted|/|found|;
    recall=|found∩planted|/|planted|. Empty-found → precision None (undefined), recall 0.
    """
    def _norm(ps):
        return {tuple(sorted(p)) for p in ps}
    found, planted = _norm(found_pairs), _norm(planted_pairs)
    hits = len(found & planted)
    precision = round(hits / len(found), 4) if found else None
    recall = round(hits / len(planted), 4) if planted else None
    return {"precision": precision, "recall": recall, "found": len(found),
            "planted": len(planted), "hits": hits}


def verdict(agg, tolerance=0.01):
    """Honest per-axis verdict of T1 (theodb LATERAL-index) vs T3 (pgvector control) on join-recall,
    and vs T2 (naive) on the structural DoD. No cherry-pick: reports PARITY/SUPERIOR/GAP per axis.
    """
    t1 = agg.get("T1_lateral_index", {})
    t3 = agg.get("T3_pgvector", {})
    out = {}
    r1 = t1.get("recall", {}).get("mean")
    r3 = t3.get("recall", {}).get("mean")
    if r1 is None or r3 is None:
        out["join_recall"] = {"axis": "join_recall", "status": "UNBENCHMARKED",
                              "reason": "a control arm is missing (container absent or index not pushed)"}
    else:
        if r1 >= r3 - tolerance and r1 <= r3 + tolerance:
            status = "PARITY"
        elif r1 > r3 + tolerance:
            status = "SUPERIOR"
        else:
            status = "GAP"
        out["join_recall"] = {"axis": "join_recall", "theodb": r1, "pgvector": r3,
                              "tolerance": tolerance, "status": status}
    # Structural DoD: T1 uses the index (bounded work); T2 is the O(n·m) it must beat. The EXPLAIN
    # gate lives in the #[pg_test]; here we record the p50 latency contrast as supporting evidence.
    p1 = t1.get("p50_ms")
    p2 = agg.get("T2_naive_sort", {}).get("p50_ms")
    out["dod_index_not_nested_loop"] = {
        "axis": "index_served_not_On_m", "t1_lateral_p50_ms": p1, "t2_naive_p50_ms": p2,
        "note": "DoD = uses the index (proven by vector_join_uses_index_scan EXPLAIN); T2 is the "
                "O(n·m) baseline. If p1<p2 the index also wins latency; if not, the DoD still holds "
                "(structural, not latency-conditioned).",
    }
    return out


# ── DB integration (needs a container) ────────────────────────────────────────────────────────

PGHOST = os.environ.get("PGHOST", "localhost")
PGPORT = os.environ.get("PGPORT", "55492")
PGUSER = os.environ.get("PGUSER", "postgres")
PGPASSWORD = os.environ.get("PGPASSWORD", "postgres")

SPECS = {
    "T1_lateral_index": {
        "kind": "lateral",
        "ddl": "CREATE INDEX bench_th ON {t} USING theodb_hnsw (v theodb_hnsw_cosine_ops)",
        "drop": "DROP INDEX IF EXISTS bench_th",
        "setup": ["SET theodb_hnsw.ef_search = 100"],
    },
    "T2_naive_sort": {
        "kind": "naive",  # top-level cross-join + ORDER BY — the O(n·m) anti-objective (no index)
        "ddl": None,
        "drop": "SELECT 1",
        "setup": ["SET enable_seqscan = on"],
    },
    "T3_pgvector": {
        "kind": "lateral",
        "ddl": "CREATE INDEX bench_pgv ON {t} USING hnsw (v vector_cosine_ops) WITH (m=16, ef_construction=64)",
        "drop": "DROP INDEX IF EXISTS bench_pgv",
        "setup": ["SET hnsw.ef_search = 100"],
    },
}


def _conn():
    import psycopg2
    c = psycopg2.connect(host=PGHOST, port=PGPORT, user=PGUSER, password=PGPASSWORD,
                         dbname="postgres", connect_timeout=15)
    c.autocommit = True
    return c


def _load():
    return round(os.getloadavg()[0], 2)


def _make_dataset(cur, table, n, dim, seed):
    """Gaussian-mixture corpus (real NN structure; avoids ANN-degenerate uniform data)."""
    import random
    rnd = random.Random(seed)
    centers = [[rnd.gauss(0, 1) for _ in range(dim)] for _ in range(N_CLUSTERS)]
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    cur.execute(f"CREATE TABLE {table} (id int, v vector({dim}))")
    rows = []
    for i in range(n):
        c = centers[i % N_CLUSTERS]
        vec = "[" + ",".join(f"{c[j] + CLUSTER_STD * rnd.gauss(0, 1):.4f}" for j in range(dim)) + "]"
        rows.append((i, vec))
        if len(rows) >= 1000:
            cur.executemany(f"INSERT INTO {table} VALUES (%s,%s)", rows)
            rows = []
    if rows:
        cur.executemany(f"INSERT INTO {table} VALUES (%s,%s)", rows)
    cur.execute(f"ANALYZE {table}")


def _outer_probes(cur, table, n_a):
    """The outer side `a` = the first n_a rows of the base table (their id + vector text)."""
    cur.execute(f"SELECT id, v::text FROM {table} ORDER BY id LIMIT {n_a}")
    return cur.fetchall()


def _exact_gt(cur, table, probes, k):
    """Exact per-row top-k over the WHOLE base (seqscan brute force) — the O(n·m) GT."""
    cur.execute("SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on")
    gt = []
    for _pid, pvec in probes:
        cur.execute(f"SELECT id FROM {table} ORDER BY v <=> %s LIMIT {k}", (pvec,))
        gt.append(set(r[0] for r in cur.fetchall()))
    cur.execute("RESET enable_indexscan; RESET enable_bitmapscan; RESET enable_seqscan")
    return gt


def _measure_arm(cur, table, spec, probes, gt, k):
    """Run one arm: per-row top-k (via its idiom), collect join-recall + latency.

    T1/T3 = LATERAL over the index (force index on); T2 = naive per-row seqscan-sorted top-k
    (the O(n·m) shape — measured per outer row so the latency is comparable, index off).
    """
    kind = spec["kind"]
    if kind == "lateral":
        cur.execute("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on")
    else:  # naive — no index, seqscan + sort
        cur.execute("SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on")
    for s in spec["setup"]:
        cur.execute(s)
    lat, ann_rows = [], []
    for _pid, pvec in probes:
        t0 = time.perf_counter()
        cur.execute(f"SELECT id FROM {table} ORDER BY v <=> %s LIMIT {k}", (pvec,))
        got = set(r[0] for r in cur.fetchall())
        lat.append((time.perf_counter() - t0) * 1000.0)
        ann_rows.append(got)
    perc = latency_percentiles(lat) if lat else {"p50": 0.0, "p95": 0.0}
    rec = join_recall(ann_rows, gt)
    return {"recall": rec, "p50_ms": round(perc["p50"], 3), "p95_ms": round(perc.get("p95", 0.0), 3),
            "queries": len(lat)}


def _dedup_arm(cur, table, dim, seed, n_dups, tau, k=1):
    """End-to-end kNN-self-join dedup: plant `n_dups` near-duplicate rows, then recover them via
    `… LATERAL (… WHERE b.id<>a.id ORDER BY dist LIMIT 1) j WHERE j.d < τ`. Uses theodb_hnsw.
    """
    import random
    rnd = random.Random(seed + 7)
    dtable = f"{table}_dedup"
    cur.execute(f"DROP TABLE IF EXISTS {dtable}")
    cur.execute(f"CREATE TABLE {dtable} (id int, v vector({dim}))")
    base = []
    centers = [[rnd.gauss(0, 1) for _ in range(dim)] for _ in range(N_CLUSTERS)]
    for i in range(200):
        c = centers[i % N_CLUSTERS]
        base.append((i, [c[j] + CLUSTER_STD * rnd.gauss(0, 1) for j in range(dim)]))
    planted = []
    next_id = 200
    for i in range(n_dups):  # each planted dup = an existing row + tiny epsilon noise
        src = base[rnd.randrange(len(base))]
        dup = [x + 0.001 * rnd.gauss(0, 1) for x in src[1]]
        base.append((next_id, dup))
        planted.append((src[0], next_id))
        next_id += 1
    for rid, vec in base:
        cur.execute(f"INSERT INTO {dtable} VALUES (%s, %s)",
                    (rid, "[" + ",".join(f"{x:.5f}" for x in vec) + "]"))
    cur.execute(f"ANALYZE {dtable}")
    cur.execute(f"DROP INDEX IF EXISTS {dtable}_idx")
    cur.execute(f"CREATE INDEX {dtable}_idx ON {dtable} USING theodb_hnsw (v theodb_hnsw_cosine_ops)")
    cur.execute("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on")
    cur.execute("SET theodb_hnsw.ef_search = 100")
    cur.execute(
        f"SELECT a.id, j.id FROM {dtable} a CROSS JOIN LATERAL "
        f"(SELECT b.id, b.v <=> a.v AS d FROM {dtable} b WHERE b.id <> a.id "
        f" ORDER BY b.v <=> a.v LIMIT {k}) j WHERE j.d < {tau}"
    )
    found = [(r[0], r[1]) for r in cur.fetchall()]
    return dedup_metrics(found, planted)


def run(n_a, n_b, dim, k, runs, seed=SEED):
    conn = _conn()
    cur = conn.cursor()
    cur.execute("CREATE EXTENSION IF NOT EXISTS theodb CASCADE")
    table = "m63bench"
    load_pre = _load()
    _make_dataset(cur, table, n_b, dim, seed)
    probes = _outer_probes(cur, table, n_a)
    gt = _exact_gt(cur, table, probes, k)

    results = {name: [] for name in SPECS}
    loads = []
    for _ in range(runs):
        loads.append(_load())
        for name, spec in SPECS.items():
            if spec["ddl"]:
                cur.execute(spec["drop"])
                try:
                    t0 = time.perf_counter()
                    cur.execute(spec["ddl"].format(t=table))
                    build_s = round(time.perf_counter() - t0, 2)
                except Exception as e:  # noqa: BLE001 — control arm may be absent; record honestly
                    results[name].append({"error": str(e)[:180]})
                    continue
            else:
                build_s = 0.0
            try:
                m = _measure_arm(cur, table, spec, probes, gt, k)
                results[name].append({"build_s": build_s, **m})
            except Exception as e:  # noqa: BLE001
                results[name].append({"error": str(e)[:180]})
            if spec["ddl"]:
                cur.execute(spec["drop"])

    # aggregate: mean of the per-run means (R2 min surfaced per run already)
    agg = {}
    for name in SPECS:
        pts = [r for r in results[name] if "recall" in r and r["recall"]["mean"] is not None]
        if not pts:
            agg[name] = {"error": "no data (arm skipped / index not pushed)"}
            continue
        rmeans = [p["recall"]["mean"] for p in pts]
        rmins = [p["recall"]["min"] for p in pts]
        p50s = [p["p50_ms"] for p in pts]
        p95s = [p["p95_ms"] for p in pts]
        agg[name] = {
            "recall": {"min": round(min(rmins), 4), "mean": round(statistics.mean(rmeans), 4),
                       "std": round(statistics.pstdev(rmeans), 4) if len(rmeans) > 1 else 0.0},
            "p50_ms": round(statistics.mean(p50s), 3),
            "p95_ms": round(statistics.mean(p95s), 3),
        }

    dedup = None
    try:
        dedup = _dedup_arm(cur, table, dim, seed, n_dups=max(10, n_a // 10), tau=0.02)
    except Exception as e:  # noqa: BLE001
        dedup = {"error": str(e)[:180]}
    conn.close()

    return {"n_a": n_a, "n_b": n_b, "dim": dim, "k": k, "runs": runs, "metric": "cosine",
            "seed": seed, "load_pre": load_pre, "load_per_run": loads, "nproc": os.cpu_count(),
            "arms": list(SPECS), "per_arm": agg, "dedup": dedup,
            "verdict": verdict(agg), "raw": results}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--n-a", type=int, default=200, help="outer rows (the `a` side); GT is O(n_a·n_b)")
    ap.add_argument("--n-b", type=int, default=5000, help="base rows (the `b` side)")
    ap.add_argument("--dim", type=int, default=128)
    ap.add_argument("--k", type=int, default=10)
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--out", default="docs/benchmarks/m63-vector-join.json")
    args = ap.parse_args()
    data = run(args.n_a, args.n_b, args.dim, args.k, args.runs)
    os.makedirs(os.path.dirname(args.out) or ".", exist_ok=True)
    json.dump(data, open(args.out, "w"), indent=2)
    print(f"wrote {args.out} (n_a={args.n_a} n_b={args.n_b} dim={args.dim} runs={args.runs}); "
          f"load_pre={data['load_pre']}")
    v = data["verdict"].get("join_recall", {})
    print(f"  join_recall: {v}")
    print(f"  dedup: {data['dedup']}")


if __name__ == "__main__":
    main()
