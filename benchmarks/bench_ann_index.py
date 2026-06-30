#!/usr/bin/env python3
"""M21 ANN parity benchmark — TheoDB's own HNSW/IVFFlat (Rust) vs pgvector, recall@k + latency.

Measurement-first (ROADMAP-v2 M21 DoD): builds a deterministic corpus, computes an exact brute-force ground
truth, then measures recall@k for BOTH (a) pgvector's `hnsw`/`ivfflat` indexes and (b) TheoDB's own
`theodb.hnsw_knn`/`theodb.ivfflat_knn` across an ef_search/probes sweep over >= 3 runs (mean +/- std), and
emits a parity verdict (own >= pgvector - tolerance). Reuses `theodb_bench.recall` (no harness rebuild).

Honest anti-sunk-cost (ADR D3): if parity is NOT reached, the verdict is RETAIN_PGVECTOR and the doc records
the gap — the milestone delivers the measurement, not a regression.

Usage:
    PGHOST=... PGPORT=... python3 benchmarks/bench_ann_index.py --write-doc
    python3 benchmarks/bench_ann_index.py --n 2000 --dim 64 --nq 100 --runs 3 --tolerance 0.05 --write-doc
"""
import argparse
import os
import statistics
import time

import numpy as np
import psycopg2

from theodb_bench.recall import brute_force_ground_truth, recall_at_k

K = 10


def conn():
    return psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"), port=os.environ.get("PGPORT", "5432"),
        user=os.environ.get("PGUSER", "postgres"), password=os.environ.get("PGPASSWORD", "postgres"),
        dbname=os.environ.get("PGDATABASE", "postgres"),
    )


def vec_lit(v):
    return "[" + ",".join(repr(float(x)) for x in v) + "]"


def setup(cur, corpus, dim):
    cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
    cur.execute("CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE")
    cur.execute("DROP TABLE IF EXISTS m21_bench")
    cur.execute(f"CREATE TABLE m21_bench (id integer PRIMARY KEY, embedding vector({dim}))")
    cur.executemany("INSERT INTO m21_bench VALUES (%s, %s::vector)",
                    [(i, vec_lit(v)) for i, v in enumerate(corpus)])


def pgvector_hnsw_recall(cur, queries, true_d, ef):
    cur.execute("DROP INDEX IF EXISTS m21_bench_hnsw")
    cur.execute("CREATE INDEX m21_bench_hnsw ON m21_bench USING hnsw (embedding vector_l2_ops) WITH (m=16, ef_construction=64)")
    cur.execute(f"SET hnsw.ef_search = {ef}")
    cur.execute("SET enable_seqscan = off")
    run = []
    for q in queries:
        cur.execute("SELECT embedding <-> %s::vector FROM m21_bench ORDER BY embedding <-> %s::vector LIMIT %s",
                    (vec_lit(q), vec_lit(q), K))
        run.append([float(r[0]) for r in cur.fetchall()])
    cur.execute("SET enable_seqscan = on")
    cur.execute("DROP INDEX IF EXISTS m21_bench_hnsw")
    return recall_at_k(true_d, run, K)


def pgvector_ivf_recall(cur, queries, true_d, probes, lists):
    cur.execute("DROP INDEX IF EXISTS m21_bench_ivf")
    cur.execute(f"CREATE INDEX m21_bench_ivf ON m21_bench USING ivfflat (embedding vector_l2_ops) WITH (lists={lists})")
    cur.execute(f"SET ivfflat.probes = {probes}")
    cur.execute("SET enable_seqscan = off")
    run = []
    for q in queries:
        cur.execute("SELECT embedding <-> %s::vector FROM m21_bench ORDER BY embedding <-> %s::vector LIMIT %s",
                    (vec_lit(q), vec_lit(q), K))
        run.append([float(r[0]) for r in cur.fetchall()])
    cur.execute("SET enable_seqscan = on")
    cur.execute("DROP INDEX IF EXISTS m21_bench_ivf")
    return recall_at_k(true_d, run, K)


def own_recall(cur, fn, queries, true_d, extra):
    qlits = [vec_lit(q) for q in queries]
    cur.execute(
        f"SELECT query_idx, distance FROM theodb.{fn}('m21_bench'::regclass, 'embedding', %s::vector[], {extra}) "
        "ORDER BY query_idx, distance",
        (qlits,),
    )
    per = [[] for _ in range(len(queries))]
    for qi, d in cur.fetchall():
        per[qi].append(float(d))
    return recall_at_k(true_d, per, K)


def mean_std(xs):
    return (statistics.mean(xs), statistics.pstdev(xs) if len(xs) > 1 else 0.0)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--n", type=int, default=2000)
    ap.add_argument("--dim", type=int, default=64)
    ap.add_argument("--nq", type=int, default=100)
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--tolerance", type=float, default=0.05)
    ap.add_argument("--seed", type=int, default=2026)
    ap.add_argument("--write-doc", action="store_true")
    args = ap.parse_args()

    rng = np.random.default_rng(args.seed)
    corpus = rng.standard_normal((args.n, args.dim)).astype(np.float32)
    queries = rng.standard_normal((args.nq, args.dim)).astype(np.float32)
    _, true_d = brute_force_ground_truth(corpus, queries, K, metric="l2")

    c = conn()
    c.autocommit = True
    with c.cursor() as cur:
        setup(cur, corpus, args.dim)

        hnsw_sweep = [10, 40, 100, 200]
        ivf_sweep = [1, 8, 16, 32]
        lists = max(8, int(args.n ** 0.5))

        rows = []  # (algo, knob, pg_mean, pg_std, own_mean, own_std, pass)
        for ef in hnsw_sweep:
            pg = [pgvector_hnsw_recall(cur, queries, true_d, ef) for _ in range(args.runs)]
            own = [own_recall(cur, "hnsw_knn", queries, true_d,
                              f"k=>10, m=>16, ef_construction=>64, ef_search=>{ef}, metric=>'l2'") for _ in range(args.runs)]
            pgm, pgs = mean_std(pg); om, os_ = mean_std(own)
            rows.append(("hnsw", f"ef_search={ef}", pgm, pgs, om, os_, om >= pgm - args.tolerance))
        for pr in ivf_sweep:
            pg = [pgvector_ivf_recall(cur, queries, true_d, pr, lists) for _ in range(args.runs)]
            own = [own_recall(cur, "ivfflat_knn", queries, true_d,
                              f"k=>10, lists=>{lists}, probes=>{pr}, metric=>'l2'") for _ in range(args.runs)]
            pgm, pgs = mean_std(pg); om, os_ = mean_std(own)
            rows.append(("ivfflat", f"probes={pr}", pgm, pgs, om, os_, om >= pgm - args.tolerance))

        # perf: build+query latency at a representative point
        t0 = time.perf_counter()
        own_recall(cur, "hnsw_knn", queries, true_d, "k=>10, m=>16, ef_construction=>64, ef_search=>100, metric=>'l2'")
        own_hnsw_wall = time.perf_counter() - t0

        cur.execute("DROP TABLE IF EXISTS m21_bench")

    # A milestone PASS = at least one strong operating point per algorithm reaches parity (the gate the test asserts).
    hnsw_pass = any(r[6] for r in rows if r[0] == "hnsw")
    ivf_pass = any(r[6] for r in rows if r[0] == "ivfflat")
    verdict = "PARITY_REACHED" if (hnsw_pass and ivf_pass) else "RETAIN_PGVECTOR"

    print(f"\n=== M21 ANN parity (n={args.n} dim={args.dim} nq={args.nq} runs={args.runs} tol={args.tolerance}) ===")
    print(f"{'algo':8} {'knob':14} {'pgvector':18} {'theodb':18} {'parity':6}")
    for algo, knob, pgm, pgs, om, os_, ok in rows:
        print(f"{algo:8} {knob:14} {pgm:.4f}+/-{pgs:.4f}   {om:.4f}+/-{os_:.4f}   {'PASS' if ok else 'fail'}")
    print(f"\nVERDICT: {verdict}  (own HNSW batch wall {own_hnsw_wall*1000:.1f} ms)")

    if args.write_doc:
        write_doc(args, rows, verdict, own_hnsw_wall, lists)
    return 0 if verdict == "PARITY_REACHED" else 0  # measurement always succeeds (anti-sunk-cost); test gates parity


def write_doc(args, rows, verdict, own_hnsw_wall, lists):
    lines = [
        "# M21 — Own ANN index (HNSW + IVFFlat) recall@k parity vs pgvector",
        "",
        f"**Verdict:** `{verdict}`  ",
        "**Method:** measurement-first (ROADMAP-v2 M21). TheoDB's own Rust HNSW/IVFFlat (`theodb.hnsw_knn` / "
        "`theodb.ivfflat_knn`, theodb_rs M21) vs pgvector's `hnsw`/`ivfflat` indexes, recall@10 on an exact "
        "brute-force ground truth (`theodb_bench.recall`), distance-thresholded (ADR D2 tolerance band).  ",
        f"**Config:** n={args.n}, dim={args.dim}, queries={args.nq}, runs={args.runs} (mean ± std), "
        f"tolerance={args.tolerance}, ivf lists={lists}, metric=l2, seed={args.seed}.  ",
        "**Hardware/build:** the `theo-db` container image (postgres:17 + pgvector + theodb_rs).  ",
        "",
        "## Recall@10 (mean ± std over runs)",
        "",
        "| algorithm | knob | pgvector | theodb (own) | parity (own ≥ pgvector − tol) |",
        "|---|---|---|---|---|",
    ]
    for algo, knob, pgm, pgs, om, os_, ok in rows:
        lines.append(f"| {algo} | {knob} | {pgm:.4f} ± {pgs:.4f} | {om:.4f} ± {os_:.4f} | {'✅ PASS' if ok else '❌ fail'} |")
    lines += [
        "",
        "## Performance (honest)",
        "",
        f"- Own HNSW batch (build + {args.nq} queries) wall time: **{own_hnsw_wall*1000:.1f} ms** at ef_search=100. "
        "The SQL-callable form rebuilds the in-memory graph per call (measurement-first scope, ADR D1/D3); a "
        "persisted on-disk access method (`CREATE INDEX … USING`) is the deferred M21b follow-up.",
        "- pgvector queries use its persisted on-disk index; the comparison here is **recall**, not build/query "
        "latency (the latency profiles are not comparable until M21b persists the own index).",
        "",
        "## Reproduce",
        "",
        "```bash",
        "# against a running theo-db container (PG* env pointing at it):",
        f"python3 benchmarks/bench_ann_index.py --n {args.n} --dim {args.dim} --nq {args.nq} "
        f"--runs {args.runs} --tolerance {args.tolerance} --write-doc",
        "# gate as a test:",
        "pytest benchmarks/tests/test_ann_index.py::test_recall_parity_gate -v",
        "```",
        "",
        "## Migration decision (coexistence, measurement-first)",
        "",
        "Per blueprint ADR D1 + plan ADR D1/D3: the own algorithms **coexist** with pgvector — they read "
        "`embedding::real[]` and never touch pgvector's type, operators, or HNSW/IVFFlat indexes; "
        "`theodb.embed/hybrid/import` are unaffected. "
        + ("Parity is reached at the operating points marked PASS above, so the own algorithms are a viable "
           "opt-in path; the on-disk planner-integrated access method is M21b. "
           if verdict == "PARITY_REACHED" else
           "Parity is NOT reached at the measured points → **retain pgvector** (anti-sunk-cost); this milestone "
           "delivers the measurement + the gap, not a regression. ")
        + "No pgvector index is replaced by M21.",
    ]
    out = os.path.join(os.path.dirname(__file__), "..", "docs", "benchmarks", "m21-ann-index-parity.md")
    out = os.path.normpath(out)
    os.makedirs(os.path.dirname(out), exist_ok=True)
    with open(out, "w") as f:
        f.write("\n".join(lines) + "\n")
    print(f"wrote {out}")


if __name__ == "__main__":
    raise SystemExit(main())
