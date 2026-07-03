#!/usr/bin/env python3
"""M19 benchmark — ai.nl_to_sql Rust (theodb_rs) vs the retired plpython3u baseline, head-to-head.

Measurement-first (CLAUDE.md TheoDB rule 5: no perf claim without a reproducible benchmark). Both the Rust
ai.nl_to_sql and the reconstructed plpython3u baseline call the SAME Rust ai._chat -> the SAME deterministic
chat stub, so the LLM round-trip is held constant: the measured delta isolates the VALIDATION glue (Rust
stdlib token-scan + Spi EXPLAIN vs plpython3u `re` + plpy EXPLAIN). This is the honest comparison — nl_to_sql
latency is dominated by the model call (I/O-bound), exactly like per-row embed; the win, if any, is in the
validation path, and the bar is NO-REGRESSION.

Reproduce:
    docker run -d --name theodb-bench --add-host=host.docker.internal:host-gateway \
      -e POSTGRES_PASSWORD=postgres -e POSTGRES_HOST_AUTH_METHOD=trust -p 55432:5432 theo-db:m19
    PGHOST=localhost PGPORT=55432 PGUSER=postgres PGPASSWORD=postgres \
      python3 benchmarks/bench_nl.py [--iters 20] [--runs 5] [--write-doc]

The plpython3u baseline below is the EXACT shipped body from git 6c1dddb:sql/60-theodb-nl.sql (post the
parser-grade-EXPLAIN BLOCKER fix), renamed to ai.nl_to_sql_plpy so both can coexist in the bench DB. It is
study/measurement code only — NOT shipped (the extension is 100% plpython3u-free since M19).
"""
import argparse
import os
import socket
import statistics
import subprocess
import sys
import time
import urllib.request

import psycopg2

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# The retired plpython3u nl_to_sql (git 6c1dddb:sql/60), renamed for the head-to-head. Calls the SAME Rust
# ai._chat as the Rust nl_to_sql, so the only difference vs the Rust path is the validation implementation.
PLPY_BASELINE = r"""
CREATE OR REPLACE FUNCTION ai.nl_to_sql_plpy(question text, allowed_relations text[], model text DEFAULT NULL)
RETURNS text
LANGUAGE plpython3u
AS $$
import re, json

if question is None or not question.strip():
    plpy.error("ai.nl_to_sql: question must not be empty", sqlstate="22023")
if not allowed_relations:
    plpy.error("ai.nl_to_sql: allowed_relations must be a non-empty list", sqlstate="22023")

allowed = set()
for r in allowed_relations:
    if r and r.strip():
        allowed.add(r.strip().lower())
if not allowed:
    plpy.error("ai.nl_to_sql: allowed_relations must be a non-empty list", sqlstate="22023")

system = (
    "You translate a question into exactly ONE read-only PostgreSQL SELECT query. "
    "You may reference ONLY these relations: " + ", ".join(sorted(allowed)) + ". "
    "Output ONLY the SQL - no prose, no markdown, no trailing semicolon. "
    "Use SELECT or WITH only. Never modify data."
)
raw = plpy.execute(
    plpy.prepare("SELECT ai._chat($1, $2, $3) AS v", ["text", "text", "text"]),
    [question, system, model],
)[0]["v"] or ""

sql = raw.strip()
if sql.startswith("```"):
    sql = re.sub(r"^```[a-zA-Z]*\n?", "", sql)
    sql = re.sub(r"\n?```$", "", sql).strip()

nocom = re.sub(r"--[^\n]*", " ", sql)
nocom = re.sub(r"/\*.*?\*/", " ", nocom, flags=re.S)
low = nocom.lower().strip()

if ";" in low.rstrip().rstrip(";"):
    plpy.error("ai.nl_to_sql: multiple statements are not allowed", sqlstate="22023")
if not re.match(r"^\s*(select|with)\b", low):
    plpy.error("ai.nl_to_sql: only SELECT/WITH queries are allowed (got: %s)" % sql[:60], sqlstate="22023")
BANNED = re.compile(
    r"\b(drop|insert|update|delete|alter|truncate|grant|revoke|create|copy|merge|reindex|vacuum|"
    r"pg_read_file|pg_read_binary_file|pg_stat_file|pg_ls_dir|pg_ls_waldir|pg_ls_logdir|pg_ls_tmpdir|"
    r"pg_ls_archive_statusdir|pg_ls_|lo_import|lo_export|lo_get|lo_put|dblink|pg_sleep|set_config|"
    r"current_setting|pg_terminate_backend|pg_cancel_backend|pg_read_server_files)\b", re.I)
m = BANNED.search(low)
if m:
    plpy.error("ai.nl_to_sql: banned token '%s' in generated SQL" % m.group(1), sqlstate="22023")
if re.search(r"\bdo\b\s*\$\$|\bcall\b", low):
    plpy.error("ai.nl_to_sql: procedural blocks are not allowed", sqlstate="22023")

try:
    plan_rows = plpy.execute("EXPLAIN (FORMAT JSON, VERBOSE false) " + sql)
except Exception as e:  # noqa: BLE001
    plpy.error("ai.nl_to_sql: query did not plan (rejected): %s" % str(e)[:120], sqlstate="22023")

plan_json = plan_rows[0][list(plan_rows[0].keys())[0]]
if isinstance(plan_json, str):
    plan_json = json.loads(plan_json)

def _collect_relations(node, acc):
    if isinstance(node, dict):
        name = node.get("Relation Name")
        if name:
            schema = (node.get("Schema") or "public").lower()
            acc.add((schema, name.lower()))
        for v in node.values():
            _collect_relations(v, acc)
    elif isinstance(node, list):
        for v in node:
            _collect_relations(v, acc)

rels = set()
_collect_relations(plan_json, rels)
for schema, name in rels:
    qualified = schema + "." + name
    if qualified not in allowed and not (name in allowed and schema == "public"):
        plpy.error("ai.nl_to_sql: relation '%s' is not in the allowlist" % qualified, sqlstate="22023")

return sql
$$;
"""


def _free_port() -> int:
    s = socket.socket()
    s.bind(("", 0))
    port = s.getsockname()[1]
    s.close()
    return port


def _start_chat_stub() -> tuple[subprocess.Popen, str]:
    port = _free_port()
    proc = subprocess.Popen(
        [sys.executable, os.path.join(REPO, "benchmarks", "servers", "chat_server.py"),
         "--host", "0.0.0.0", "--port", str(port)],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    for _ in range(120):
        try:
            with urllib.request.urlopen(f"http://127.0.0.1:{port}/health", timeout=2) as r:
                if r.status == 200:
                    break
        except OSError:
            time.sleep(0.5)
    else:
        proc.terminate()
        raise RuntimeError("chat stub did not become healthy")
    return proc, f"http://host.docker.internal:{port}/v1/chat/completions"


def _admin_conn():
    return psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"), port=os.environ.get("PGPORT", "5432"),
        user=os.environ.get("PGUSER", "postgres"), password=os.environ.get("PGPASSWORD", "postgres"),
        dbname="postgres",
    )


def _bench_db(name: str):
    admin = _admin_conn()
    admin.autocommit = True
    with admin.cursor() as cur:
        cur.execute(f"DROP DATABASE IF EXISTS {name} WITH (FORCE)")
        cur.execute(f"CREATE DATABASE {name}")
    admin.close()
    c = psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"), port=os.environ.get("PGPORT", "5432"),
        user=os.environ.get("PGUSER", "postgres"), password=os.environ.get("PGPASSWORD", "postgres"),
        dbname=name,
    )
    c.autocommit = True
    return c


def _measure(cur, fn: str, question: str, allowed: list, iters: int) -> list:
    """Return per-call latencies (ms) for `iters` sequential calls of ai.<fn>(question, allowed)."""
    lat = []
    for _ in range(iters):
        t0 = time.perf_counter()
        cur.execute(f"SELECT ai.{fn}(%s, %s)", (question, allowed))
        cur.fetchone()
        lat.append((time.perf_counter() - t0) * 1000.0)
    return lat


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--iters", type=int, default=20, help="calls per run")
    ap.add_argument("--runs", type=int, default=5, help="number of runs (>=3 for mean+/-std)")
    ap.add_argument("--write-doc", action="store_true", help="write docs/benchmarks/m19-nl-rust-vs-plpython.md")
    ap.add_argument("--db", default="m19_bench_nl")
    args = ap.parse_args()
    if args.runs < 3:
        print("ERROR: --runs must be >= 3 (mean +/- std needs >= 3 runs)", file=sys.stderr)
        return 2

    proc, endpoint = _start_chat_stub()
    conn = None
    try:
        conn = _bench_db(args.db)
        with conn.cursor() as cur:
            # Rust nl_to_sql (+ ai._chat) comes with theodb_rs; plpython3u for the baseline; both call ai._chat.
            cur.execute("CREATE EXTENSION theodb_rs CASCADE")   # requires theodb -> vector; provides ai.nl_to_sql + ai._chat
            cur.execute("CREATE EXTENSION IF NOT EXISTS plpython3u")
            cur.execute(PLPY_BASELINE)
            cur.execute("SET theodb.llm_endpoint = %s", (endpoint,))
            cur.execute("SET theodb.llm_model = 'stub-chat'")
            # Allowlisted relation the stub's generated SELECT references (stub returns a benign SELECT).
            cur.execute("CREATE TABLE documents (id int PRIMARY KEY, content text)")
            cur.execute("INSERT INTO documents VALUES (1, 'hello'), (2, 'world')")

            question = "how many documents are there"
            allowed = ["documents"]

            # Warmup (JIT plan caches, connection warm, stub warm) — excluded from measurement.
            _measure(cur, "nl_to_sql", question, allowed, 5)
            _measure(cur, "nl_to_sql_plpy", question, allowed, 5)

            rust_run_means, plpy_run_means = [], []
            rust_all, plpy_all = [], []
            for _ in range(args.runs):
                r = _measure(cur, "nl_to_sql", question, allowed, args.iters)
                p = _measure(cur, "nl_to_sql_plpy", question, allowed, args.iters)
                rust_run_means.append(statistics.mean(r))
                plpy_run_means.append(statistics.mean(p))
                rust_all.extend(r)
                plpy_all.extend(p)

        rust_mean = statistics.mean(rust_run_means)
        rust_std = statistics.pstdev(rust_run_means)
        plpy_mean = statistics.mean(plpy_run_means)
        plpy_std = statistics.pstdev(plpy_run_means)
        rust_p95 = sorted(rust_all)[int(0.95 * len(rust_all)) - 1]
        plpy_p95 = sorted(plpy_all)[int(0.95 * len(plpy_all)) - 1]
        ratio = rust_mean / plpy_mean if plpy_mean else float("inf")

        print(f"samples: {args.runs} runs x {args.iters} calls = {args.runs * args.iters} per impl")
        print(f"Rust  ai.nl_to_sql       : mean {rust_mean:7.3f} ms  +/- {rust_std:.3f} (run means)  p95 {rust_p95:7.3f} ms")
        print(f"plpy  ai.nl_to_sql_plpy  : mean {plpy_mean:7.3f} ms  +/- {plpy_std:.3f} (run means)  p95 {plpy_p95:7.3f} ms")
        print(f"ratio rust/plpy          : {ratio:.3f}  ({'NO-REGRESSION' if ratio <= 1.20 else 'REGRESSION'} @ 1.20 bar)")

        if args.write_doc:
            _write_doc(args, rust_mean, rust_std, rust_p95, plpy_mean, plpy_std, plpy_p95, ratio)
            print("wrote docs/benchmarks/m19-nl-rust-vs-plpython.md")

        # No-regression gate: the Rust validation path must not be materially slower than plpython3u. Both are
        # LLM-RTT-dominated, so a 1.20 ceiling (20% headroom for stub jitter) is a generous, honest bar.
        if ratio > 1.20:
            print("FAIL: regression (rust/plpy > 1.20)", file=sys.stderr)
            return 1
        return 0
    finally:
        if conn is not None:
            conn.close()
        proc.terminate()
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()


def _write_doc(args, rust_mean, rust_std, rust_p95, plpy_mean, plpy_std, plpy_p95, ratio):
    import platform
    verdict = "NO-REGRESSION" if ratio <= 1.20 else "REGRESSION"
    doc = f"""# M19 benchmark — `ai.nl_to_sql` Rust vs plpython3u

**Verdict: {verdict}** (rust/plpy = {ratio:.3f}, bar 1.20).

## What this measures

`ai.nl_to_sql` was ported from `plpython3u` to Rust (theodb_rs, M19). This is a head-to-head of the Rust
implementation against the **exact retired plpython3u body** (git `6c1dddb:sql/60-theodb-nl.sql`, renamed
`ai.nl_to_sql_plpy`). Both call the **same** Rust `ai._chat` against the **same** deterministic chat stub
(`benchmarks/servers/chat_server.py`), so the LLM round-trip is held constant and the measured delta isolates the
**validation glue**: Rust stdlib token-scan + `Spi` EXPLAIN vs plpython3u `re` + `plpy` EXPLAIN.

`ai.nl_to_sql` end-to-end latency is dominated by the model call (I/O-bound) — like per-row `embed`, the
honest expectation is parity, and the gate is **no-regression**, not a speedup claim.

## Results

| Implementation | mean (ms) | ± std (run means) | p95 (ms) |
|---|---|---|---|
| Rust `ai.nl_to_sql` | {rust_mean:.3f} | {rust_std:.3f} | {rust_p95:.3f} |
| plpython3u `ai.nl_to_sql_plpy` | {plpy_mean:.3f} | {plpy_std:.3f} | {plpy_p95:.3f} |

- Samples: {args.runs} runs × {args.iters} calls = {args.runs * args.iters} per implementation (warmup excluded).
- Ratio Rust/plpython3u (mean of run means): **{ratio:.3f}**.
- Host: {platform.platform()}; Python {platform.python_version()}.

## Methodology

1. Build the M19 image (`docker build -t theo-db:m19 .`) and start it with `--add-host=host.docker.internal:host-gateway`.
2. A throwaway DB installs `theodb_rs` (Rust `ai.nl_to_sql` + `ai._chat`) + `plpython3u` + the baseline `ai.nl_to_sql_plpy`.
3. `theodb.llm_endpoint` points at the local `benchmarks/servers/chat_server.py` stub (deterministic benign SELECT).
4. Warmup 5 calls/impl (excluded); then {args.runs} runs × {args.iters} sequential calls/impl, timed client-side
   (`time.perf_counter` around `SELECT ai.<fn>(question, allowed)`), same question + allowlist for both.
5. Report mean ± population-std of per-run means + p95 over all samples.

Reproduce: `PGHOST=localhost PGPORT=55432 PGUSER=postgres PGPASSWORD=postgres python3 benchmarks/bench_nl.py --write-doc`.

## Honesty notes

- The stub removes real-network variance so the comparison is repeatable; against a real model both impls pay
  the same (much larger) RTT, so the validation-glue delta shrinks to noise in production.
- The plpython3u baseline is reconstructed from git for measurement only — it is **not** shipped (the extension
  is 100% plpython3u-free since M19).
"""
    out_dir = os.path.join(REPO, "docs", "benchmarks")
    os.makedirs(out_dir, exist_ok=True)
    with open(os.path.join(out_dir, "m19-nl-rust-vs-plpython.md"), "w") as f:
        f.write(doc)


if __name__ == "__main__":
    sys.exit(main())
