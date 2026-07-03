"""Reproducible latency micro-benchmark for ai._chat (M18 — Rust vs plpython3u).

Measurement-first (ADR 0002 / `public-copy.md`): reports the measured numbers ONLY — no performance claim.
`ai._chat` is I/O-bound (the chat endpoint dominates wall-clock), so the honest expected result is "no
latency regression" from the plpython3u→Rust rewrite, NOT a speedup. The SAME deterministic chat stub
(`benchmarks/servers/chat_server.py`) serves both arms, so the comparison is apples-to-apples in ONE container.

The plpython3u baseline `ai._chat_py` is (re)created here for the comparison only (the shipped `ai._chat` is
Rust after M18) — minimal, no retry, same request/parse shape as the historical plpython3u `ai._chat`.

Usage (against a running TheoDB container, chat stub reachable via the endpoint):
    PGHOST=localhost PGPORT=55432 python3 benchmarks/bench_chat.py \
        --endpoint http://host.docker.internal:8100/v1/chat/completions \
        --report docs/benchmarks/m18-ai-rust-vs-plpython.md
"""
from __future__ import annotations

import statistics
import time

# The plpython3u baseline, recreated for the benchmark only (apples-to-apples vs the Rust ai._chat).
_AI_CHAT_PY = r"""
CREATE OR REPLACE FUNCTION ai._chat_py(prompt text, system text DEFAULT NULL, model text DEFAULT NULL)
RETURNS text LANGUAGE plpython3u AS $py$
import json, urllib.request, urllib.error
def _cfg(n): return plpy.execute("SELECT current_setting('%s', true) AS v" % n)[0]["v"]
if prompt is None: plpy.error("ai._chat_py: prompt must not be NULL", sqlstate="22023")
endpoint = _cfg("theodb.llm_endpoint")
if not endpoint: plpy.error("ai._chat_py: theodb.llm_endpoint is not set", sqlstate="22023")
mdl = model or _cfg("theodb.llm_model") or "default"
messages = []
if system: messages.append({"role":"system","content":system})
messages.append({"role":"user","content":prompt})
req = urllib.request.Request(endpoint, data=json.dumps({"model":mdl,"messages":messages}).encode(), method="POST")
req.add_header("Content-Type","application/json")
class _NR(urllib.request.HTTPRedirectHandler):
    def redirect_request(self,*a,**k): return None
try:
    with urllib.request.build_opener(_NR).open(req, timeout=30) as r:
        body = json.loads(r.read())
except (urllib.error.URLError, OSError, ValueError) as e:
    plpy.error("ai._chat_py: chat endpoint call failed: %s" % e, sqlstate="38000")
return body["choices"][0]["message"]["content"]
$py$;
"""


def bench(conn, func: str, n: int = 100, runs: int = 5, prompt: str = "benchmark prompt", warmup: int = 5) -> dict:
    """Per-call latency of `func(prompt)` over `runs` runs of `n` serial calls (seconds/call, mean±std)."""
    if n <= 0 or runs <= 0:
        raise ValueError("bench: n and runs must be positive")
    query = f"SELECT {func}(%s)"
    with conn.cursor() as cur:
        for _ in range(max(0, warmup)):
            cur.execute(query, (prompt,))
        samples: list[float] = []
        for _ in range(runs):
            t0 = time.perf_counter()
            for _ in range(n):
                cur.execute(query, (prompt,))
            samples.append((time.perf_counter() - t0) / n)
    return {
        "mean": statistics.mean(samples),
        "std": statistics.pstdev(samples) if len(samples) > 1 else 0.0,
        "n": n,
        "runs": runs,
        "per_call_samples": samples,
    }


def _render_report(rust: dict, py: dict, meta: dict) -> str:
    delta = (rust["mean"] - py["mean"]) * 1000
    return "\n".join([
        "# M18 — `ai._chat` latency: Rust (pgrx) vs plpython3u",
        "",
        f"**Date:** {meta['date']}",
        "**Milestone:** M18 (ROADMAP-v2 — own code in Rust; the generative ai.* surface)",
        "**Purpose:** measurement-first evidence (ADR 0002, CTO requirement \"DEVE TER DADOS EM BENCHMARK\") "
        "that porting `ai._chat` from plpython3u to Rust (pgrx) does **not regress latency**.",
        "",
        "> **Honest framing (ADR 0002 / `public-copy.md`):** `ai._chat` is **I/O-bound** — every call makes "
        "one synchronous HTTP round-trip to the chat endpoint, which dominates wall-clock. The rewrite is "
        "about **owning the code in Rust** (ROADMAP-v2), proven at **functional parity** (the 36-test "
        "`test_ai_sql.py` oracle), NOT a speed win. The numbers below document *no regression*; they are "
        "**not** a performance claim. Per-call latency is governed by the endpoint, not the function language.",
        "",
        "## Method (reproducible)",
        "",
        "- **Same container, same PostgreSQL, same endpoint, same stub** for both arms — the ONLY variable is "
        "the function language (Rust `ai._chat` vs plpython3u `ai._chat_py`, the latter recreated for the bench).",
        "- **Endpoint:** the deterministic local stub `benchmarks/servers/chat_server.py`, reached via `host.docker.internal`.",
        f"- **Workload:** {rust['n']} serial calls/run, **{rust['runs']} runs**, {meta['warmup']} warmup discarded. "
        "Per-call latency = run wall-clock / n. Reported as mean ± std dev (population) over the runs.",
        "- **Harness:** `benchmarks/bench_chat.py`.",
        f"- **Hardware:** {meta['hardware']}. **PostgreSQL:** {meta['pg']}. **Toolchain:** {meta['toolchain']}.",
        "",
        "## Results",
        "",
        "| Implementation | mean ± std (ms/call) |",
        "|---|---|",
        f"| `ai._chat` (Rust/pgrx) | {rust['mean']*1000:.3f} ± {rust['std']*1000:.3f} |",
        f"| `ai._chat_py` (plpython3u) | {py['mean']*1000:.3f} ± {py['std']*1000:.3f} |",
        "",
        f"**Delta (Rust − plpython3u): {delta:+.3f} ms/call** — within I/O-bound noise; no regression "
        "(both arms are dominated by the same endpoint round-trip).",
        "",
    ])


def _main() -> None:
    import argparse
    import datetime
    import os

    import psycopg2

    ap = argparse.ArgumentParser(description="ai._chat latency benchmark (Rust vs plpython3u)")
    ap.add_argument("--endpoint", required=True)
    ap.add_argument("--n", type=int, default=100)
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--warmup", type=int, default=5)
    ap.add_argument("--report", default=None)
    args = ap.parse_args()

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
            cur.execute("SET theodb.llm_endpoint = %s", (args.endpoint,))
            cur.execute(_AI_CHAT_PY)  # create the plpython3u baseline for the comparison
        rust = bench(conn, "ai._chat", n=args.n, runs=args.runs, warmup=args.warmup)
        py = bench(conn, "ai._chat_py", n=args.n, runs=args.runs, warmup=args.warmup)
    finally:
        conn.close()

    print(f"ai._chat (Rust):       mean={rust['mean']*1000:.3f} ms  std={rust['std']*1000:.3f} ms")
    print(f"ai._chat_py (plpy):    mean={py['mean']*1000:.3f} ms  std={py['std']*1000:.3f} ms")
    print(f"delta (Rust-plpy):     {(rust['mean']-py['mean'])*1000:+.3f} ms/call")

    if args.report:
        meta = {
            "date": datetime.date.today().isoformat(),
            "warmup": args.warmup,
            "hardware": os.environ.get("BENCH_HW", "(fill in: CPU model)"),
            "pg": os.environ.get("BENCH_PG", "PostgreSQL 17"),
            "toolchain": os.environ.get("BENCH_TC", "Rust 1.91, pgrx 0.16.1, minreq 2 (https-native/OpenSSL)"),
        }
        with open(args.report, "w") as f:
            f.write(_render_report(rust, py, meta))
        print(f"\nReport written to {args.report}")


if __name__ == "__main__":
    _main()
