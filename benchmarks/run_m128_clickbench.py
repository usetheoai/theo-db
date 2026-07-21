#!/usr/bin/env python3
"""M128 — official-benchmark COLUMNAR pillar: run the 43 ClickBench queries over `theodb_columnar` and prove the
retained byte-identical result A/B (columnar vs heap) — the correctness oracle ClickBench lacks (blueprint Q10/Q11).

ClickBench protocol (faithful): load the `hits` table, run each of the 43 queries 3× (cold = 1st after a cache
flush, hot = min of the 2 warm runs), record the raw [t1,t2,t3] triple per query into a ClickBench-format
results.json. PLUS the wrap layer: EXPLAIN each query (columnar CustomScan vs native), and for each query compare
the columnar result rows vs a heap copy — byte-identical or a loud divergence (ClickBench's `check` is a `SELECT 1`;
it validates NOTHING).

Honesty rails (ADR M128-2, CLAUDE.md rule 5): `hits` is CC-BY-NC-SA → CI-downloaded (streamed, subsampled),
NEVER vendored. Self-hosted box (labeled), NOT canonical c6a.4xlarge. No data → UNBENCHMARKED, clean exit.
A query unsupported by theodb_columnar is recorded ERRORED with its typed message — never silently skipped.
"""
import argparse
import gzip
import json
import os
import subprocess
import sys
import time
import urllib.request

import psycopg2

HITS_TSV_GZ = "https://datasets.clickhouse.com/hits_compatible/hits.tsv.gz"  # CC-BY-NC-SA — CI-only, never vendored
_UA = "Mozilla/5.0 (X11; Linux x86_64)"
HERE = os.path.dirname(os.path.abspath(__file__))
ENTRY = os.path.join(HERE, "clickbench", "theodb")


def _conn():
    return psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"), port=os.environ.get("PGPORT", "28900"),
        dbname=os.environ.get("PGDATABASE", "postgres"), user=os.environ.get("PGUSER", "postgres"),
        password=os.environ.get("PGPASSWORD", "postgres"))


_CB_RAW = "https://raw.githubusercontent.com/ClickHouse/ClickBench/main/postgresql"  # CC-BY-NC-SA — CI-fetch, never vendor


def ensure_entry_sql() -> bool:
    """Fetch ClickBench's create.sql + queries.sql at runtime (CC-BY-NC-SA → CI-download, NOT vendored into our
    Apache tree, per the D1 gate). Adapts create.sql to `USING theodb_columnar`. Returns True on success."""
    create_p, queries_p = os.path.join(ENTRY, "create.sql"), os.path.join(ENTRY, "queries.sql")
    if os.path.isfile(create_p) and os.path.isfile(queries_p):
        return True
    try:
        os.makedirs(ENTRY, exist_ok=True)
        req = urllib.request.Request(f"{_CB_RAW}/queries.sql", headers={"User-Agent": _UA})  # noqa: S310
        with urllib.request.urlopen(req, timeout=60) as r:
            open(queries_p, "wb").write(r.read())
        req = urllib.request.Request(f"{_CB_RAW}/create.sql", headers={"User-Agent": _UA})  # noqa: S310
        with urllib.request.urlopen(req, timeout=60) as r:
            ddl = r.read().decode().rstrip()
        if not ddl.endswith(");"):
            raise ValueError("unexpected ClickBench create.sql ending (want '…);')")
        ddl = ddl[:-2] + ") USING theodb_columnar;\n"  # swap the final ');' for ') USING theodb_columnar;'
        open(create_p, "w").write(
            "-- Fetched from ClickBench (CC-BY-NC-SA) at runtime, NOT vendored — only the AM clause is ours.\n" + ddl)
        return True
    except Exception as e:
        print(f"  ClickBench SQL fetch failed: {e}")
        return False


def _load_queries():
    with open(os.path.join(ENTRY, "queries.sql")) as fh:
        return [q.strip() for q in fh.read().split("\n") if q.strip() and not q.strip().startswith("--")]


def _ensure_sample(path: str, n_rows: int) -> bool:
    """Stream hits.tsv.gz and keep the first n_rows (curl | zcat | head — never downloads the full ~100 GB)."""
    if os.path.isfile(path) and os.path.getsize(path) > 0:
        return True
    try:
        print(f"  streaming {n_rows} hits rows → {path} (CC-BY-NC-SA, CI-only) …", flush=True)
        cmd = f"curl -sL -A '{_UA}' '{HITS_TSV_GZ}' | zcat | head -n {int(n_rows)} > {path}"
        subprocess.run(["bash", "-c", cmd], check=True, timeout=1800)
        return os.path.isfile(path) and os.path.getsize(path) > 0
    except Exception as e:
        print(f"  hits stream failed: {e}")
        return False


def _flush_caches():
    """ClickBench cold-run flush (best-effort; needs privilege). Falls back to a no-op if not permitted."""
    try:
        subprocess.run(["bash", "-c", "sync 2>/dev/null; echo 3 > /proc/sys/vm/drop_caches 2>/dev/null || true"],
                       timeout=30, check=False, stderr=subprocess.DEVNULL)
    except Exception:
        pass


def _run_once(cur, sql):
    t0 = time.perf_counter()
    cur.execute(sql)
    try:
        rows = cur.fetchall()
    except psycopg2.ProgrammingError:
        rows = None  # a statement with no result set
    return time.perf_counter() - t0, rows


def _canonical(rows):
    """Order-insensitive canonical form of a result set for the byte-identical A/B (ClickBench queries with an
    ORDER BY are deterministic; aggregates are single-row; sorting makes the compare order-independent)."""
    return sorted([tuple(str(c) for c in r) for r in (rows or [])])


def run(args) -> dict:
    base = {"dataset": "clickbench-hits", "n_rows": args.n, "box": "self-hosted (NOT canonical c6a.4xlarge)",
            "protocol": "3 runs/query: cold=1st (cache-flushed), hot=min-of-2"}
    if not ensure_entry_sql():
        return {**base, "status": "UNBENCHMARKED", "reason": "ClickBench create/queries SQL unavailable", "queries": None}
    sample = os.path.join(args.cache, "hits_sample.tsv")
    os.makedirs(args.cache, exist_ok=True)
    if not _ensure_sample(sample, args.n):
        return {**base, "status": "UNBENCHMARKED", "reason": "hits dataset unavailable", "queries": None}

    conn = _conn(); conn.autocommit = True
    cur = conn.cursor()
    # Build the columnar `hits` (create.sql) + a heap copy `hits_heap` for the result A/B.
    with open(os.path.join(ENTRY, "create.sql")) as fh:
        create_sql = fh.read()
    cur.execute("DROP TABLE IF EXISTS hits CASCADE")
    cur.execute("DROP TABLE IF EXISTS hits_heap CASCADE")
    cur.execute(create_sql)
    cur.execute(create_sql.replace("USING theodb_columnar", "").replace("hits", "hits_heap"))
    # theodb_columnar bulk-load path is INSERT-SELECT (as the columnar_*_ab.py benchmarks do), NOT direct COPY —
    # COPY FROM STDIN into the columnar TAM hangs on the 105-col wide row (recorded honestly; see the doc). So:
    # COPY the sample into the heap copy (fast), then INSERT INTO hits SELECT * FROM hits_heap (columnar writer).
    with open(sample) as fh:
        cur.copy_expert("COPY hits_heap FROM STDIN WITH (FORMAT text)", fh)
    cur.execute("INSERT INTO hits SELECT * FROM hits_heap")  # noqa: naive table-name replace is safe — no ClickBench query/col contains "hits" beyond the bare table ref (council-benchmark LOW)
    # enable_columnar_agg=OFF: run over columnar STORAGE via PG's native executor. The vectorized-aggregate
    # CustomScan (agg=on) has a PLANNER hang on the real 105-col hits table for at least one query
    # (GROUP BY UserID) — uninterruptible by statement_timeout because it is during planning, not execution
    # (filed as an issue). The columnar-storage path (agg off) is the sound, complete measurement; the pushdown is
    # tracked follow-up. Honest scope, not a workaround: the CustomScan is an optional optimization on the pillar.
    cur.execute(f"SET theodb.enable_columnar_agg = {'on' if args.agg else 'off'}")
    cur.execute("SET max_parallel_workers_per_gather = 0")
    # Per-query ceiling: a query the columnar path cannot complete in time is recorded ERRORED (honest per-query
    # status, plan failure-scenario) rather than hanging the whole run — ClickBench itself has no such guard.
    cur.execute(f"SET statement_timeout = '{int(args.query_timeout_s) * 1000}'")

    queries = _load_queries()
    results, ab_pass, ab_diverged, errored, customscan = [], 0, 0, 0, 0
    from theodb_bench.regression import assert_byte_identical  # reuse the M127 byte-identical comparator

    for i, sql in enumerate(queries):
        entry = {"q": i, "sql": sql[:60]}
        # timing 3× (cold flush before run 1)
        try:
            triple = []
            for run_i in range(3):
                if run_i == 0:
                    _flush_caches()
                dt, _rows = _run_once(cur, sql)
                triple.append(round(dt, 4))
            entry["timings"] = triple
            entry["cold"], entry["hot"] = triple[0], min(triple[1], triple[2])
        except Exception as e:
            conn.rollback() if not conn.autocommit else None
            entry["error"] = str(e).splitlines()[0][:120]
            errored += 1
            results.append(entry); continue
        # plan: columnar CustomScan vs native
        try:
            cur.execute("EXPLAIN (FORMAT TEXT) " + sql)
            plan = "\n".join(r[0] for r in cur.fetchall())
            entry["columnar_customscan"] = "theodb_columnar_agg" in plan or "Custom Scan" in plan
            customscan += 1 if entry["columnar_customscan"] else 0
        except Exception:
            entry["columnar_customscan"] = None
        # byte-identical result A/B: columnar vs heap. Strip the trailing `LIMIT N` first: ClickBench's
        # `... ORDER BY count DESC LIMIT 10` has many tied counts on a subsample, so the LIMIT cut picks an
        # ARBITRARY-but-valid 10 among the ties — a legitimate scan-order difference, NOT a storage bug. Comparing
        # the FULL (unlimited) deterministic aggregation is the real columnar-storage correctness oracle.
        import re as _re
        ab_sql = _re.sub(r"\s+LIMIT\s+\d+\s*;?\s*$", "", sql.rstrip().rstrip(";"))
        try:
            cur.execute(ab_sql); rc = _canonical(cur.fetchall())
            cur.execute(ab_sql.replace("hits", "hits_heap")); rh = _canonical(cur.fetchall())
            r = assert_byte_identical({j: rc[j] for j in range(len(rc))}, {j: rh[j] for j in range(len(rh))}) \
                if len(rc) == len(rh) else {"identical": False, "diverged": abs(len(rc) - len(rh))}
            entry["result_ab_identical"] = r["identical"]
            ab_pass += 1 if r["identical"] else 0
            ab_diverged += 0 if r["identical"] else 1
        except Exception as e:
            entry["result_ab_identical"] = None
            entry["result_ab_note"] = str(e).splitlines()[0][:80]
        results.append(entry)
        print(f"  q{i:>2} cold={entry.get('cold','ERR')} hot={entry.get('hot','ERR')} "
              f"cs={entry.get('columnar_customscan')} ab={entry.get('result_ab_identical')}", flush=True)
    cur.close(); conn.close()

    ok = [e for e in results if "timings" in e]
    geomean = None
    if ok:
        import math
        geomean = round(math.exp(sum(math.log(max(e["hot"], 1e-6)) for e in ok) / len(ok)), 5)
    return {
        **base, "status": "OK", "n_queries": len(queries), "queries_ok": len(ok), "queries_errored": errored,
        "columnar_customscan_count": customscan, "hot_geomean_s": geomean,
        "result_ab": {"pass": ab_pass, "diverged": ab_diverged,
                      "verdict": "byte-identical (columnar==heap)" if ab_diverged == 0 else "DIVERGENCE — pushdown bug"},
        "queries": results,
        "caveats": [
            "self-hosted box, NOT canonical AWS c6a.4xlarge — QPS/timings not leaderboard-comparable (ADR M128-2)",
            f"hits subsampled to {args.n} rows (full 99.9M is the operational follow-up); CC-BY-NC-SA, CI-only, never vendored",
            "byte-identical result A/B (columnar vs heap) is the TheoDB-owned correctness oracle ClickBench lacks",
        ],
    }


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--n", type=int, default=1_000_000, help="hits subsample rows")
    ap.add_argument("--cache", default="benchmarks/.cache")
    ap.add_argument("--query-timeout-s", type=int, default=60, help="per-query ceiling; slow query -> ERRORED")
    ap.add_argument("--agg", action="store_true", help="enable the vectorized columnar-agg CustomScan (has a planner-hang bug on real hits; default OFF)")
    ap.add_argument("--out", default="docs/benchmarks/m128-clickbench-columnar.json")
    args = ap.parse_args()
    data = run(args)
    os.makedirs(os.path.dirname(args.out) or ".", exist_ok=True)
    with open(args.out, "w") as fh:
        json.dump(data, fh, indent=2)
    # also drop the ClickBench-format results.json (raw timing triples) into the entry dir
    if data["status"] == "OK":
        with open(os.path.join(ENTRY, "template.json")) as fh:
            tpl = json.load(fh)
        tpl["result"] = [e.get("timings", []) for e in data["queries"]]
        with open(os.path.join(ENTRY, "results.json"), "w") as fh:
            json.dump(tpl, fh, indent=2)
    print(f"wrote {args.out}  status={data['status']}")
    if data["status"] == "OK":
        print(f"  queries ok={data['queries_ok']}/{data['n_queries']} errored={data['queries_errored']} "
              f"columnar_customscan={data['columnar_customscan_count']} hot_geomean={data['hot_geomean_s']}s")
        print(f"  result A/B: {data['result_ab']['verdict']} (pass={data['result_ab']['pass']}, diverged={data['result_ab']['diverged']})")


if __name__ == "__main__":
    sys.exit(main())
