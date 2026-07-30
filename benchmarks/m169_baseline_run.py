"""M169 T1.2 — the 100M ClickBench baseline runner: how many of the 43 queries COMPLETE.

This is a completion measurement, not a performance one. The acceptance criterion is "the query terminates",
not "the query is fast" — so each query runs ONCE. Running it three times (as `m162_timing.py` does, for a
timing pass) would triple a multi-hour run for a number T1.2 does not ask for, and would make the T4.1 delta
compare two different protocols.

Prior art: `benchmarks/m162_timing.py` — "query the ALREADY-LOADED columnar `hits` (100M), no reload, no A/B".
Its shape is reused (fresh connection per query, session GUCs, statement_timeout, JSONL flushed per line); three
of its properties are deliberately NOT inherited:

1. hard-coded `dbname="clickbench"` / `password="x"` (`m162_timing.py:14`) — wrong for this box, and credentials
   in source are wrong anywhere. Everything comes from the environment.
2. `top_node` (`m162_timing.py:25-36`) keys on the BROAD `Custom Scan (theodb_columnar` — the M164 B2 guard
   exists because that signal is ~always true and hides whether the AGG path routed. We use
   `run_m128_clickbench.plan_shows_agg_pushdown`, the agg-specific one.
3. three runs per query — see above.

What this runner must NEVER do: call `run_m128_clickbench.run()`. That function drops `hits` and `hits_heap`
unconditionally (`:324-325`) and reloads 69.7 GB. The tables are already loaded; destroying them to measure them
is the failure this whole file exists to avoid.
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import psycopg2  # noqa: E402

import m169_baseline_summarize as summ  # noqa: E402
from run_m128_clickbench import _load_queries, plan_shows_agg_pushdown  # noqa: E402

# Matched to the M162 run (`docs/benchmarks/m162-100m-gap-verdict.md:10`). The "19/43" this milestone measures
# against is only comparable under the SAME ceiling — the harness default of 60 s would mark nearly everything
# `timeout` and produce a number that means nothing.
DEFAULT_TIMEOUT_S = 300
DEFAULT_WORK_MEM = "256MB"


def _conn():
    c = psycopg2.connect(host=os.environ.get("PGHOST", "127.0.0.1"), port=os.environ.get("PGPORT", "5432"),
                         dbname=os.environ.get("PGDATABASE", "postgres"),
                         user=os.environ.get("PGUSER", "postgres"),
                         password=os.environ.get("PGPASSWORD") or None)
    c.autocommit = True
    return c


def session_gucs(timeout_s: int, work_mem: str, agg: bool, stream: bool) -> list[str]:
    """ONE place the session is configured, parameterised. T1.2 and T4.1 must differ only by the streaming GUC;
    if each copies its own SET list, one forgotten line makes T4.1 measure the new binary with the new path OFF
    and conclude 'nothing changed'. The effective list is written into the artifact."""
    return [
        f"SET theodb.enable_columnar_agg = {'on' if agg else 'off'}",
        f"SET theodb.enable_columnar_agg_stream = {'on' if stream else 'off'}",
        "SET theodb.enable_columnar_late_mat = on",
        f"SET work_mem = '{work_mem}'",
        "SET max_parallel_workers_per_gather = 0",
        f"SET statement_timeout = '{timeout_s}s'",
    ]


def verify_gucs_took_effect(cur, gucs: list[str]) -> dict:
    """Read back every GUC that was SET and report which ones the server actually KNOWS.

    MEASURED 2026-07-30, and it contradicts the obvious assumption: `SET theodb.<name-that-does-not-exist> = off`
    returns `SET` — PostgreSQL accepts an unknown parameter inside an extension's prefix as a PLACEHOLDER. The
    statement succeeds, the value is remembered, and NOTHING happens.

    Why that is the dangerous direction: at T4.1 a typo in the streaming GUC would SET cleanly, the new code path
    would stay off, and the run would conclude 'the fix changed nothing' — a false negative on the milestone's own
    deliverable, produced by a green command.

    The discriminator is `pg_settings`: a real GUC is listed there, a placeholder is not."""
    effective = {}
    for g in gucs:
        name = g.split("SET ", 1)[1].split("=")[0].strip()
        cur.execute("SELECT setting FROM pg_settings WHERE name = %s", (name,))
        row = cur.fetchone()
        effective[name] = row[0] if row else "PLACEHOLDER — o servidor não conhece esta GUC; o SET não teve efeito"
    return effective


def _kernel_killed(pid: int) -> bool:
    """True only when the kernel log names THIS backend's pid. Without that line the verdict stays `crash` —
    a vanished backend might have been OOM-killed, segfaulted, or been terminated, and picking one because it
    fits the story is a diagnosis, not a measurement."""
    for cmd in (f"dmesg -T 2>/dev/null | grep -i 'killed process {pid} '",
                f"journalctl -k --no-pager 2>/dev/null | grep -i 'killed process {pid} '"):
        try:
            if subprocess.run(cmd, shell=True, capture_output=True, text=True, timeout=30).stdout.strip():
                return True
        except (subprocess.SubprocessError, OSError):
            continue
    return False


def run_query(i: int, sql: str, gucs: list[str]) -> dict:
    """One query, one fresh connection — so a backend that dies does not poison the remaining 42."""
    rec: dict = {"q": i, "verdict": None, "elapsed_s": None, "agg_routed": None, "ab_identical": None}
    backend_pid = None
    c = None
    t0 = time.time()
    try:
        c = _conn()
        cur = c.cursor()
        for g in gucs:
            cur.execute(g)
        cur.execute("SELECT pg_backend_pid()")
        backend_pid = cur.fetchone()[0]
        try:
            cur.execute("EXPLAIN (COSTS OFF) " + sql)
            rec["agg_routed"] = plan_shows_agg_pushdown("\n".join(r[0] for r in cur.fetchall()))
        except psycopg2.Error as e:
            rec["explain_error"] = str(e).splitlines()[0][:120]
        t0 = time.time()
        cur.execute(sql)
        cur.fetchall()
        rec["verdict"] = summ.OK
    except psycopg2.Error as e:
        pid = getattr(e, "pgcode", None)
        oom = (not pid) and backend_pid is not None and _kernel_killed(backend_pid)
        rec["verdict"] = summ.classify_failure(pid, str(e).splitlines()[0], oom_evidence=oom)
        rec["error"] = str(e).splitlines()[0][:160]
        rec["backend_pid"] = backend_pid
    finally:
        rec["elapsed_s"] = round(time.time() - t0, 3)
        if c is not None:
            try:
                c.close()
            except psycopg2.Error:
                pass
    return rec


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--out", required=True, help="JSONL, flushed per line — a run killed at hour 3 keeps its 30 verdicts")
    ap.add_argument("--queries", default=None)
    ap.add_argument("--timeout-s", type=int, default=DEFAULT_TIMEOUT_S)
    ap.add_argument("--work-mem", default=DEFAULT_WORK_MEM)
    ap.add_argument("--agg", action="store_true", help="enable the columnar-agg pushdown (q20's overflow needs it)")
    ap.add_argument("--stream", action="store_true", help="enable the M169 aggregate streaming (OFF at baseline)")
    ap.add_argument("--label", required=True, help="goes in the artifact; distinguishes baseline from the T4.1 re-run")
    args = ap.parse_args()

    queries = _load_queries() if args.queries is None else \
        [q.strip() for q in open(args.queries).read().split("\n") if q.strip() and not q.strip().startswith("--")]
    gucs = session_gucs(args.timeout_s, args.work_mem, args.agg, args.stream)

    # Prove the session config took effect BEFORE spending hours measuring under it.
    probe = _conn()
    probe_cur = probe.cursor()
    for guc in gucs:
        probe_cur.execute(guc)
    effective = verify_gucs_took_effect(probe_cur, gucs)
    probe.close()

    header = {"label": args.label, "n_queries": len(queries), "gucs": gucs,
              "gucs_effective": effective,
              "timeout_s": args.timeout_s, "work_mem": args.work_mem}
    placeholders = [k for k, v in effective.items() if str(v).startswith("PLACEHOLDER")]
    if placeholders:
        print(f"  AVISO: GUCs que o servidor NÃO conhece (SET sucedeu sem efeito): {placeholders}", flush=True)
    print(json.dumps({"header": header}), flush=True)

    with open(args.out, "w") as fh:
        fh.write(json.dumps({"header": header}) + "\n")
        fh.flush()
        for i, sql in enumerate(queries):
            rec = run_query(i, sql, gucs)
            fh.write(json.dumps(rec) + "\n")
            fh.flush()
            print(f"  q{i:02d} {rec['verdict']:>16} {rec['elapsed_s']:>9}s agg={rec['agg_routed']}", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
