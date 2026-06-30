#!/usr/bin/env python3
"""M22 SBQ parity benchmark — TheoDB's own SBQ quantizer (Rust) vs pgvectorscale SBQ (diskann), recall + memory.

Measurement-first (ROADMAP-v2 M22 DoD): builds a deterministic corpus, computes an exact brute-force ground
truth, then measures recall@k for BOTH (a) pgvectorscale's `diskann` SBQ index (storage_layout=memory_optimized)
and (b) TheoDB's own `theodb.sbq_knn` across a bits/over_fetch/probes sweep over >= 3 runs (mean +/- std), plus
the memory profile (bytes/vector). Emits a verdict (own recall >= pgvectorscale - tol AND own bytes/vector <=
pgvectorscale). Reuses `theodb_bench.recall` (no harness rebuild).

Memory honesty (EC-1): own bytes/vector == pgvectorscale's at the same bits (identical formula) → memory is
PARITY with pgvectorscale + ~Nx reduction vs f32; recall with rerank is the substantive differentiator.

Anti-sunk-cost (ADR D3): if parity is NOT reached, the verdict is RETAIN_PGVECTORSCALE and the doc records the gap.

Usage:
    PGHOST=... python3 benchmarks/bench_sbq_index.py --write-doc
"""
import argparse
import os
import statistics

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
    # precondition (EC-6): pgvectorscale diskann must exist for the baseline arm.
    cur.execute("SELECT count(*) FROM pg_am WHERE amname = 'diskann'")
    if cur.fetchone()[0] == 0:
        raise SystemExit("FATAL: pgvectorscale 'diskann' AM not present in this image — cannot run the M22 baseline arm.")
    cur.execute("DROP TABLE IF EXISTS m22_bench")
    cur.execute(f"CREATE TABLE m22_bench (id integer PRIMARY KEY, embedding vector({dim}))")
    cur.executemany("INSERT INTO m22_bench VALUES (%s, %s::vector)",
                    [(i, vec_lit(v)) for i, v in enumerate(corpus)])


def pgvs_recall(cur, queries, true_d):
    cur.execute("DROP INDEX IF EXISTS m22_bench_pgvs")
    cur.execute("CREATE INDEX m22_bench_pgvs ON m22_bench USING diskann (embedding vector_l2_ops) "
                "WITH (storage_layout = memory_optimized)")
    cur.execute("SET enable_seqscan = off")
    run = []
    for q in queries:
        cur.execute("SELECT embedding <-> %s::vector FROM m22_bench ORDER BY embedding <-> %s::vector LIMIT %s",
                    (vec_lit(q), vec_lit(q), K))
        run.append([float(r[0]) for r in cur.fetchall()])
    cur.execute("SET enable_seqscan = on")
    cur.execute("DROP INDEX IF EXISTS m22_bench_pgvs")
    return recall_at_k(true_d, run, K)


def own_recall(cur, queries, true_d, extra):
    qlits = [vec_lit(q) for q in queries]
    cur.execute(f"SELECT query_idx, distance FROM theodb.sbq_knn('m22_bench'::regclass, 'embedding', %s::vector[], {extra}) "
                "ORDER BY query_idx, distance", (qlits,))
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

    lists = max(8, int(args.n ** 0.5))
    c = conn()
    c.autocommit = True
    rows = []
    with c.cursor() as cur:
        setup(cur, corpus, args.dim)
        # pgvectorscale baseline (one config — diskann SBQ memory_optimized)
        pg = [pgvs_recall(cur, queries, true_d) for _ in range(args.runs)]
        pgm, pgs = mean_std(pg)
        cur.execute("SELECT theodb.sbq_bytes_per_vector(%s, 1)", (args.dim,))
        own_bytes = cur.fetchone()[0]
        pg_bytes = ((args.dim * 1 + 63) // 64) * 8   # pgvectorscale SBQ formula at 1 bit (parity by construction)
        f32_bytes = args.dim * 4
        # own sweep: over_fetch x probes at bits=1
        for of in [8, 16, 32]:
            for pr in [8, 16, 32]:
                own = [own_recall(cur, queries, true_d,
                                  f"k=>10, bits=>1, lists=>{lists}, probes=>{pr}, over_fetch=>{of}, metric=>'l2'")
                       for _ in range(args.runs)]
                om, osd = mean_std(own)
                rows.append((of, pr, om, osd, om >= pgm - args.tolerance))
        cur.execute("DROP TABLE IF EXISTS m22_bench")

    recall_pass = any(r[4] for r in rows)
    mem_pass = own_bytes <= pg_bytes
    verdict = "PARITY_REACHED" if (recall_pass and mem_pass) else "RETAIN_PGVECTORSCALE"

    print(f"\n=== M22 SBQ parity (n={args.n} dim={args.dim} nq={args.nq} runs={args.runs} tol={args.tolerance}) ===")
    print(f"pgvectorscale diskann SBQ recall@{K}: {pgm:.4f}+/-{pgs:.4f}")
    print(f"memory bytes/vector: own={own_bytes} pgvectorscale={pg_bytes} f32={f32_bytes} "
          f"(compression vs f32 = {f32_bytes/own_bytes:.1f}x)  mem_parity={'PASS' if mem_pass else 'fail'}")
    print(f"{'over_fetch':10} {'probes':6} {'own recall':18} {'parity':6}")
    for of, pr, om, osd, ok in rows:
        print(f"{of:<10} {pr:<6} {om:.4f}+/-{osd:.4f}   {'PASS' if ok else 'fail'}")
    print(f"\nVERDICT: {verdict}")

    if args.write_doc:
        write_doc(args, rows, verdict, pgm, pgs, own_bytes, pg_bytes, f32_bytes, lists, recall_pass, mem_pass)
    return 0


def write_doc(args, rows, verdict, pgm, pgs, own_bytes, pg_bytes, f32_bytes, lists, recall_pass, mem_pass):
    lines = [
        "# M22 — Own SBQ scalar quantization recall@k + memory parity vs pgvectorscale",
        "",
        f"**Verdict:** `{verdict}`  ",
        "**Method:** measurement-first (ROADMAP-v2 M22). TheoDB's own SBQ quantizer + quantized search "
        "(`theodb.sbq_knn`, theodb_rs M22 — Hamming over the M21 IVFFlat carrier + full-precision f32 rerank) vs "
        "pgvectorscale's SBQ (`diskann`, storage_layout=memory_optimized). recall@10 on an exact brute-force "
        "ground truth (`theodb_bench.recall`), distance-thresholded.  ",
        f"**Config:** n={args.n}, dim={args.dim}, queries={args.nq}, runs={args.runs} (mean ± std), "
        f"tolerance={args.tolerance}, bits=1, ivf lists={lists}, metric=l2, seed={args.seed}.  ",
        "**Build:** the `theo-db` container image (postgres:17 + pgvector + pgvectorscale + theodb_rs).  ",
        "",
        "## Memory (bytes/vector)",
        "",
        f"| representation | bytes/vector | vs f32 |",
        f"|---|---|---|",
        f"| f32 (baseline) | {f32_bytes} | 1× |",
        f"| **own SBQ (1-bit)** | **{own_bytes}** | **{f32_bytes/own_bytes:.1f}× smaller** |",
        f"| pgvectorscale SBQ (1-bit) | {pg_bytes} | {f32_bytes/pg_bytes:.1f}× smaller |",
        "",
        f"Memory is **parity with pgvectorscale** (`own {own_bytes} {'≤' if mem_pass else '>'} pgvectorscale "
        f"{pg_bytes}` — identical `ceil(dim·bits/64)·8` formula, EC-1) and a **{f32_bytes/own_bytes:.0f}× reduction "
        "vs f32**. Honest: this is memory PARITY with pgvectorscale, not a memory win over it — the recall below is "
        "the substantive comparison.",
        "",
        f"## Recall@10 (own, mean ± std) — pgvectorscale diskann SBQ baseline = **{pgm:.4f} ± {pgs:.4f}**",
        "",
        "| over_fetch | probes | own recall | parity (own ≥ pgvectorscale − tol) |",
        "|---|---|---|---|",
    ]
    for of, pr, om, osd, ok in rows:
        lines.append(f"| {of} | {pr} | {om:.4f} ± {osd:.4f} | {'✅ PASS' if ok else '❌ fail'} |")
    lines += [
        "",
        "## Reproduce",
        "",
        "```bash",
        f"python3 benchmarks/bench_sbq_index.py --n {args.n} --dim {args.dim} --nq {args.nq} "
        f"--runs {args.runs} --tolerance {args.tolerance} --write-doc",
        "pytest benchmarks/tests/test_sbq_index.py::test_recall_memory_parity_gate -v",
        "```",
        "",
        "## Migration decision (coexistence, measurement-first)",
        "",
        "Per blueprint ADR D1/D3 + plan ADR D1/D3: the own SBQ quantizer + search **coexist** with pgvectorscale "
        "— they read `embedding::real[]` and never touch pgvectorscale's `diskann`/SBQ or pgvector. RaBitQ "
        "(vectorchord) is **AGPL** and was NOT borrowed (D1); the own SBQ is permissive std-only code (zero new "
        "deps). "
        + ("Recall parity is reached at the operating points marked PASS (with rerank) at memory parity, so the "
           "own quantizer is a viable opt-in path; the on-disk planner-integrated AM is M22b. "
           if verdict == "PARITY_REACHED" else
           "Recall parity is NOT reached at the measured points → **retain pgvectorscale** (anti-sunk-cost); this "
           "milestone delivers the measurement + the gap, not a regression. ")
        + "No pgvectorscale index is replaced by M22.",
    ]
    out = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", "docs", "benchmarks", "m22-sbq-parity.md"))
    os.makedirs(os.path.dirname(out), exist_ok=True)
    with open(out, "w") as f:
        f.write("\n".join(lines) + "\n")
    print(f"wrote {out}")


if __name__ == "__main__":
    raise SystemExit(main())
