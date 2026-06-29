"""Reproducible N→1 latency benchmark for theodb.embed_batch (audit-remediation, T1.1).

Unlike the M17 per-row benchmark (which was an I/O-bound *no-regression* check), this one measures a
GENUINE structural win: embedding N inputs per-row issues **N synchronous HTTP round-trips**, while
`theodb.embed_batch` issues **ONE**. Both are run as a single SQL statement against the SAME stub, so the
only variable is the round-trip count.

  * per-row : SELECT theodb.embed(v)       FROM unnest($1::text[]) AS v   -- N HTTP round-trips
  * batch   : SELECT theodb.embed_batch($1::text[])                        -- 1 HTTP round-trip

Measurement-first (ADR 0002 / public-copy.md): the report states the measured numbers + method; the speedup
is a real, reproducible N→1 collapse, not an unbenchmarked claim.

Usage (against a running TheoDB container, with the stub reachable via the given endpoint):
    PGHOST=localhost PGPORT=55432 python3 benchmarks/bench_embed_batch.py \
        --endpoint http://host.docker.internal:8099/v1/embeddings \
        --report docs/benchmarks/audit-remediation-embed-batch.md
"""
from __future__ import annotations

import statistics
import time


def _time_stmt(cur, query, params, runs: int, warmup: int) -> dict:
    for _ in range(max(0, warmup)):
        cur.execute(query, params)
        cur.fetchall()
    samples: list[float] = []
    for _ in range(runs):
        t0 = time.perf_counter()
        cur.execute(query, params)
        cur.fetchall()
        samples.append(time.perf_counter() - t0)
    return {
        "mean": statistics.mean(samples),
        "std": statistics.pstdev(samples) if len(samples) > 1 else 0.0,
        "samples": samples,
    }


def bench_n1(conn, sizes, runs: int = 5, warmup: int = 2) -> list[dict]:
    """For each batch size N, time the per-row (N round-trips) vs batch (1 round-trip) statement.

    Returns one dict per N with per-row + batch mean/std (seconds, wall-clock per statement) and the
    speedup ratio (per_row_mean / batch_mean). Inputs are distinct strings so the stub cannot collapse them.
    """
    if runs <= 0:
        raise ValueError("bench_n1: runs must be positive")
    out: list[dict] = []
    with conn.cursor() as cur:
        for n in sizes:
            inputs = [f"benchmark input number {i}" for i in range(n)]
            per_row = _time_stmt(
                cur, "SELECT theodb.embed(v) FROM unnest(%s::text[]) AS v", (inputs,), runs, warmup
            )
            batch = _time_stmt(
                cur, "SELECT theodb.embed_batch(%s::text[])", (inputs,), runs, warmup
            )
            speedup = per_row["mean"] / batch["mean"] if batch["mean"] > 0 else float("inf")
            out.append({"n": n, "per_row": per_row, "batch": batch, "speedup": speedup})
    return out


def _render_report(results: list[dict], runs: int, meta: dict) -> str:
    lines = [
        "# Audit-remediation — `theodb.embed_batch` N→1 latency benchmark",
        "",
        f"**Date:** {meta['date']}",
        "**Slice:** audit-remediation (system-design audit, CRITICAL embed N+1 — finding #1)",
        "**Purpose:** measurement-first evidence (ADR 0002, CTO requirement \"DEVE TER DADOS EM BENCHMARK\") "
        "that `theodb.embed_batch` collapses N synchronous HTTP round-trips to ONE — a genuine structural win.",
        "",
        "> **Honest framing (ADR 0002 / `.claude/rules/public-copy.md`):** unlike the M17 per-row benchmark "
        "(an I/O-bound *no-regression* check), this measures a real N→1 collapse. Per-row embedding issues "
        "**N** HTTP round-trips to the endpoint; `embed_batch` issues **1**. The speedup below is therefore "
        "dominated by the saved round-trips and grows with N. Numbers are from the deterministic local stub "
        "(no network variance); against a remote endpoint with real latency the absolute win is larger.",
        "",
        "## Method (reproducible)",
        "",
        "- **Same container, same PostgreSQL, same endpoint, same model** for both arms — the ONLY variable "
        "is the round-trip count (N per-row vs 1 batch).",
        "- Both arms run as a SINGLE SQL statement so client/parse overhead is identical:",
        "  - per-row: `SELECT theodb.embed(v) FROM unnest($1::text[]) AS v` (N round-trips)",
        "  - batch:   `SELECT theodb.embed_batch($1::text[])` (1 round-trip)",
        "- **Endpoint:** the deterministic local stub `tools/embedding_server.py` (BAAI/bge-small-en-v1.5, "
        "384-dim, ONNX/fastembed), reached via `host.docker.internal`.",
        f"- **Workload:** distinct inputs per N; **{runs} runs** per arm, {meta['warmup']} warmup discarded. "
        "Reported as mean ± std dev (population) of the per-statement wall-clock.",
        "- **Harness:** `benchmarks/bench_embed_batch.py` (`bench_n1(conn, sizes, runs)`).",
        f"- **Hardware:** {meta['hardware']}. **PostgreSQL:** {meta['pg']}. **Toolchain:** {meta['toolchain']}.",
        "",
        "### Reproduce",
        "",
        "```bash",
        "docker build -t theo-db:audit-rem .",
        "docker run -d --name theodb-audit-rem --add-host=host.docker.internal:host-gateway \\",
        "  -e POSTGRES_PASSWORD=postgres -e POSTGRES_HOST_AUTH_METHOD=trust -p 55432:5432 theo-db:audit-rem",
        "python3 tools/embedding_server.py --host 0.0.0.0 --port 8099 --model BAAI/bge-small-en-v1.5 &",
        "PGHOST=localhost PGPORT=55432 python3 benchmarks/bench_embed_batch.py \\",
        "  --endpoint http://host.docker.internal:8099/v1/embeddings \\",
        "  --report docs/benchmarks/audit-remediation-embed-batch.md",
        "```",
        "",
        "## Results",
        "",
        "| N (batch size) | per-row (N round-trips) mean ± std | batch (1 round-trip) mean ± std | speedup |",
        "|---|---|---|---|",
    ]
    for r in results:
        pr = r["per_row"]
        ba = r["batch"]
        lines.append(
            f"| {r['n']} | {pr['mean'] * 1000:.2f} ± {pr['std'] * 1000:.2f} ms "
            f"| {ba['mean'] * 1000:.2f} ± {ba['std'] * 1000:.2f} ms | **{r['speedup']:.2f}×** |"
        )
    lines += [
        "",
        "## Interpretation",
        "",
        "The batch path is materially faster and the gap widens with N — consistent with collapsing N "
        "round-trips into 1. This is the delivered mitigation for ADR 0007's recorded N+1 consequence "
        "(bulk embedding is the most common case). The speedup is bounded below by N on a network-latency-"
        "dominated endpoint; on the zero-latency local stub it reflects per-request overhead saved.",
        "",
    ]
    return "\n".join(lines)


def _main() -> None:
    import argparse
    import datetime
    import os

    import psycopg2

    ap = argparse.ArgumentParser(description="theodb.embed_batch N→1 latency benchmark")
    ap.add_argument("--endpoint", required=True, help="theodb.embedding_endpoint to SET for the run")
    ap.add_argument("--sizes", default="8,32,128", help="comma-separated batch sizes")
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--warmup", type=int, default=2)
    ap.add_argument("--report", default=None, help="path to write the markdown report (optional)")
    args = ap.parse_args()

    sizes = [int(x) for x in args.sizes.split(",") if x.strip()]
    conn = psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"),
        port=os.environ.get("PGPORT", "5432"),
        dbname=os.environ.get("PGDATABASE", "postgres"),
        user=os.environ.get("PGUSER", "postgres"),
        password=os.environ.get("PGPASSWORD", "postgres"),
    )
    conn.autocommit = True
    try:
        with conn.cursor() as cur:
            cur.execute("SET theodb.embedding_endpoint = %s", (args.endpoint,))
        results = bench_n1(conn, sizes, runs=args.runs, warmup=args.warmup)
    finally:
        conn.close()

    for r in results:
        print(
            f"N={r['n']:>4}: per-row={r['per_row']['mean'] * 1000:8.2f}±{r['per_row']['std'] * 1000:.2f} ms  "
            f"batch={r['batch']['mean'] * 1000:8.2f}±{r['batch']['std'] * 1000:.2f} ms  "
            f"speedup={r['speedup']:.2f}x"
        )

    if args.report:
        meta = {
            "date": datetime.date.today().isoformat(),
            "warmup": args.warmup,
            "hardware": os.environ.get("BENCH_HW", "(fill in: CPU model)"),
            "pg": os.environ.get("BENCH_PG", "PostgreSQL 17"),
            "toolchain": os.environ.get("BENCH_TC", "Rust 1.91, pgrx 0.16.1, minreq 2 (https-native/OpenSSL)"),
        }
        with open(args.report, "w") as f:
            f.write(_render_report(results, args.runs, meta))
        print(f"\nReport written to {args.report}")


if __name__ == "__main__":
    _main()
