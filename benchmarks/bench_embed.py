"""Reproducible latency micro-benchmark for theodb.embed (M17 — Rust vs plpython3u).

Measurement-first (ADR 0002 / `.claude/rules/public-copy.md`): this harness reports the measured
numbers ONLY — it makes no performance claim. `theodb.embed` is **I/O-bound** (the embedding endpoint
dominates wall-clock), so the honest expected result is "no latency regression" from the plpython3u→Rust
rewrite, NOT a speedup. The same deterministic stub (`benchmarks/servers/embedding_server.py`) is used for both
implementations so the comparison is apples-to-apples.

Usage (against a running TheoDB container with theodb.embedding_endpoint reachable):
    PGHOST=localhost PGPORT=5432 python3 benchmarks/bench_embed.py --endpoint http://host.docker.internal:PORT/v1/embeddings
"""
from __future__ import annotations

import statistics
import time


def bench(
    conn,
    n: int = 200,
    runs: int = 3,
    text: str = "benchmark text",
    warmup: int = 5,
    func: str = "theodb.embed",
) -> dict:
    """Measure per-call latency of ``func`` (an embedding function) over ``runs`` runs of ``n`` serial calls.

    ``func`` defaults to ``theodb.embed`` (the Rust impl) but can target a plpython3u baseline (e.g.
    ``theodb.embed_py``) for an apples-to-apples Rust-vs-plpython3u comparison in the SAME container.
    Returns a dict with mean/std (seconds per call), n, runs, and the raw per-run samples.
    Calls are issued serially by design — this measures per-call latency, not concurrency.
    """
    if n <= 0 or runs <= 0:
        raise ValueError("bench: n and runs must be positive")
    # `func` is a trusted caller-supplied identifier (benchmark harness, not user input).
    query = f"SELECT {func}(%s)"
    with conn.cursor() as cur:
        for _ in range(max(0, warmup)):
            cur.execute(query, (text,))
        samples: list[float] = []
        for _ in range(runs):
            t0 = time.perf_counter()
            for _ in range(n):
                cur.execute(query, (text,))
            samples.append((time.perf_counter() - t0) / n)
    return {
        "mean": statistics.mean(samples),
        "std": statistics.pstdev(samples) if len(samples) > 1 else 0.0,
        "n": n,
        "runs": runs,
        "per_call_samples": samples,
    }


def _main() -> None:
    import argparse
    import os

    import psycopg2

    ap = argparse.ArgumentParser(description="theodb.embed latency benchmark")
    ap.add_argument("--endpoint", required=True, help="theodb.embedding_endpoint to SET for the run")
    ap.add_argument("--n", type=int, default=200)
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--label", default="impl", help="label for the measured implementation")
    args = ap.parse_args()

    conn = psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"),
        port=os.environ.get("PGPORT", "5432"),
        dbname=os.environ.get("PGDATABASE", "postgres"),
        user=os.environ.get("PGUSER", "postgres"),
        password=os.environ.get("PGPASSWORD", "postgres"),
    )
    try:
        with conn.cursor() as cur:
            cur.execute("SET theodb.embedding_endpoint = %s", (args.endpoint,))
        r = bench(conn, n=args.n, runs=args.runs)
        print(
            f"{args.label}: mean={r['mean'] * 1000:.3f} ms/call  std={r['std'] * 1000:.3f} ms  "
            f"(n={r['n']}, runs={r['runs']}, samples_ms={[round(s * 1000, 3) for s in r['per_call_samples']]})"
        )
    finally:
        conn.close()


if __name__ == "__main__":
    _main()
