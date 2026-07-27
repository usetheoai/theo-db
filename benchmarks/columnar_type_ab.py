#!/usr/bin/env python3
"""M163 — type-coverage A/B differential harness for the columnar routing paths.

Runs BEFORE /review. For each routed admit-path (IN-list / int±k / extract / agg) × each per-type EDGE value, it runs
the query against the columnar `hits` (ON) and the heap twin `hits_heap` (OFF) and asserts EITHER byte-identical
(symmetric-EXCEPT diverged=0) OR correct-decline (EXPLAIN shows no Custom Scan) — the M161 fail-closed contract, now over
the TYPE space the ClickBench A/B never exercises. Includes a POSITIVE CONTROL (a seeded-divergent pair the harness MUST
flag) as its self-test. Design: blueprint `m163-type-coverage-ab-blueprint.md` (ADR-1 bespoke pytest, reuse the shipped
symmetric-EXCEPT oracle, no new dep).

CLI: `python3 benchmarks/columnar_type_ab.py [--out docs/benchmarks/m163-type-coverage-verdict.md]` → exit 0 iff every
routed case is diverged=0 AND every expected-decline case is declined AND the positive control is caught.
"""
from __future__ import annotations

import argparse
import os
import sys

try:
    import psycopg2  # type: ignore
except Exception:  # pragma: no cover - import guard
    psycopg2 = None


# --- connection (reuses the benchmarks/m162_timing.py pattern) --------------------------------------------------------
def _conn():
    if psycopg2 is None:
        raise RuntimeError("psycopg2 unavailable")
    c = psycopg2.connect(
        host=os.environ.get("PGHOST", "127.0.0.1"),
        port=os.environ.get("PGPORT", "5432"),
        dbname=os.environ.get("PGDATABASE", "m163_type_ab"),
        user=os.environ.get("PGUSER", "postgres"),
        password=os.environ.get("PGPASSWORD", "x"),
    )
    c.autocommit = True
    return c


def session_setup(cur) -> None:
    cur.execute("SET theodb.enable_columnar_agg = on")
    cur.execute("SET theodb.enable_columnar_late_mat = on")
    cur.execute("SET enable_sort = off")  # force HASHED so a text group key routes (M161 false-green lesson)
    cur.execute("SET work_mem = '64MB'")
    cur.execute("SET max_parallel_workers_per_gather = 0")
    cur.execute("SET statement_timeout = '30s'")


# --- the per-type edge catalog (blueprint Corner 4) -------------------------------------------------------------------
# Column (name, PG type, edge literals). The edges are the boundary values where a type-class bug surfaces — the union
# of DuckDB's per-type dirs + the M151/M154/M157/M161 traps. `None` = SQL NULL.
EDGE_CATALOG: dict[str, dict] = {
    "c2":  {"pg": "int2",       "edges": [-32768, -1, 0, 1, 32767]},                    # M161 BLOCKER: c2+5 @ 32767 -> int4
    "c4":  {"pg": "int4",       "edges": [-2147483647, -1, 0, 1, 2147483647]},           # int4±int8 widening (fail-closed)
    "c8":  {"pg": "int8",       "edges": [-9223372036854775807, 0, 9223372036854775807]},
    "f4":  {"pg": "float4",     "edges": [0.0, "-0.0", 1.5, "'NaN'", "'Infinity'", "'-Infinity'"]},  # M154 IEEE
    "f8":  {"pg": "float8",     "edges": [0.0, "-0.0", 1.5, "'NaN'", "'Infinity'", "'-Infinity'"]},
    "ts":  {"pg": "timestamp",  "edges": ["'2000-01-01 00:00:00'", "'2013-07-15 10:37:30.123456'"]},  # M157 epoch
    "tz":  {"pg": "timestamptz","edges": ["'2013-07-15 10:37:30+00'"]},                 # M157 must DECLINE
    "d":   {"pg": "date",       "edges": ["'2000-01-01'", "'2013-07-15'"]},              # M161 temporal gate leak
    "t":   {"pg": "text",       "edges": ["'a'", "'A'", "'b'", "''", None]},             # M153/M158 collation
    "b":   {"pg": "bool",       "edges": [True, False, None]},
}
ROUTED_TYPES = {"int2", "int4", "int8", "float4", "float8", "timestamp", "date", "timestamptz", "text", "bool"}


def catalog_covers_routed_types(catalog: dict = EDGE_CATALOG) -> bool:
    """T1.1 assertion: every routed PG type has ≥1 edge; the M161/M154 traps are present."""
    present = {v["pg"] for v in catalog.values()}
    if not ROUTED_TYPES.issubset(present):
        return False
    if 32767 not in catalog["c2"]["edges"]:
        return False
    if "-0.0" not in catalog["f8"]["edges"] or "'NaN'" not in catalog["f8"]["edges"]:
        return False
    return True


def _lit(v) -> str:
    if v is None:
        return "NULL"
    if isinstance(v, bool):
        return "true" if v else "false"
    return str(v)


def setup_tables(cur) -> int:
    """CREATE the columnar `hits` + heap twin `hits_heap`, load the catalog cross-product (one row per (col-index)).
    Returns the loaded row count (equal in both). Fails fast if theodb_columnar is unavailable (no silent heap fallback)."""
    cur.execute("SELECT 1 FROM pg_am WHERE amname = 'theodb_columnar'")
    if cur.fetchone() is None:
        raise RuntimeError("theodb_columnar AM unavailable — CREATE EXTENSION theodb_rs missing (false-green guard)")
    cols = ", ".join(f"{name} {spec['pg']}" for name, spec in EDGE_CATALOG.items())
    for tbl, am in (("hits", " USING theodb_columnar"), ("hits_heap", "")):
        cur.execute(f"DROP TABLE IF EXISTS {tbl}")
        cur.execute(f"CREATE TABLE {tbl} ({cols}){am}")
    # Build rows: cycle each column's edges. Pad to ~2000 rows so the cost-based planner treats the table as non-trivial
    # and the columnar-agg swap fires (the Unresolved Question in the plan). Every edge value is still present (cycled).
    nrows = max(2000, max(len(s["edges"]) for s in EDGE_CATALOG.values()))
    rows = []
    for i in range(nrows):
        vals = [_lit(spec["edges"][i % len(spec["edges"])]) for spec in EDGE_CATALOG.values()]
        rows.append("(" + ", ".join(vals) + ")")
    values = ", ".join(rows)
    # Heap first (direct VALUES — fast); then the columnar table via INSERT-SELECT (the proven M162 columnar-writer path;
    # direct COPY into the wide columnar TAM row is the one that hangs — VALUES-per-row into columnar is fine but
    # INSERT-SELECT matches the shipped bulk path).
    cur.execute(f"INSERT INTO hits_heap VALUES {values}")
    cur.execute("INSERT INTO hits SELECT * FROM hits_heap")
    cur.execute("SELECT count(*) FROM hits")
    n_col = cur.fetchone()[0]
    cur.execute("SELECT count(*) FROM hits_heap")
    n_heap = cur.fetchone()[0]
    if n_col != n_heap or n_col == 0:
        raise RuntimeError(f"row-count mismatch hits={n_col} hits_heap={n_heap}")
    return n_col


# --- the differential oracle -------------------------------------------------------------------------------------------
def plan_routes(plan_lines: list[str]) -> bool:
    """Pure: given EXPLAIN output lines, does the ON arm route to the columnar CustomScan? (unit-testable, no DB)."""
    return any("Custom Scan (theodb_columnar" in ln for ln in plan_lines)


def _off_sql(sql: str) -> str:
    """The heap arm: same query with `hits` → `hits_heap` (word-boundary, not `hits_heap` itself)."""
    import re
    return re.sub(r"\bhits\b", "hits_heap", sql)


def ab_check(cur, sql: str) -> dict:
    """EXPLAIN the ON arm; if it routes → symmetric-EXCEPT columnar-vs-heap → diverged; else → 'declined'.
    Returns {status, routed, diverged}. status ∈ {ok, declined, diverged, error}."""
    try:
        cur.execute("EXPLAIN (COSTS OFF) " + sql)
        plan = [r[0] for r in cur.fetchall()]
    except Exception as e:  # noqa: BLE001
        return {"status": "error", "routed": None, "diverged": None, "err": str(e).splitlines()[0][:80]}
    routed = plan_routes(plan)
    if not routed:
        return {"status": "declined", "routed": False, "diverged": None}
    off = _off_sql(sql)
    ex = (f"SELECT count(*) FROM (({sql}) EXCEPT ALL ({off}) "
          f"UNION ALL ({off}) EXCEPT ALL ({sql})) _d")
    # symmetric EXCEPT ALL: columnar EXCEPT heap, plus heap EXCEPT columnar
    ex = (f"WITH a AS ({sql}), b AS ({off}) "
          f"SELECT (SELECT count(*) FROM ((SELECT * FROM a EXCEPT SELECT * FROM b) "
          f"UNION ALL (SELECT * FROM b EXCEPT SELECT * FROM a)) d)")
    try:
        cur.execute(ex)
        diverged = cur.fetchone()[0]
    except Exception as e:  # noqa: BLE001
        return {"status": "error", "routed": True, "diverged": None, "err": str(e).splitlines()[0][:80]}
    return {"status": "ok" if diverged == 0 else "diverged", "routed": True, "diverged": diverged}


# --- the case matrix (admit-path × type edges) ------------------------------------------------------------------------
# Each case: (name, sql, expect) — expect ∈ {"route" (must be ok, diverged=0), "decline" (must be declined)}.
def build_cases() -> list[tuple[str, str, str]]:
    return [
        # agg pushdown over each type
        ("agg_count", "SELECT count(*) FROM hits", "route"),
        ("agg_sum_i4", "SELECT sum(c4) FROM hits", "route"),
        # IN-list integer (M161)
        ("inlist_i4", "SELECT count(*) FROM hits WHERE c4 IN (0, 1, -1)", "route"),
        ("inlist_i2", "SELECT count(*) FROM hits WHERE c2 IN (0, 32767)", "route"),
        ("inlist_null", "SELECT count(*) FROM hits WHERE c4 IN (NULL, 1)", "decline"),  # 3-valued -> decline
        # int±k group-expr (M161 BLOCKER): int2+int4 -> int4 result MUST route + byte-identical
        ("intpk_i2", "SELECT c2+5, count(*) FROM hits GROUP BY c2+5", "route"),
        ("intpk_i4", "SELECT c4-1, count(*) FROM hits GROUP BY c4-1", "route"),
        ("intpk_i8_result", "SELECT c8+5, count(*) FROM hits GROUP BY c8+5", "decline"),   # int8 result -> decline
        ("intpk_i4_wide", "SELECT c4+3000000000, count(*) FROM hits GROUP BY c4+3000000000", "decline"),  # -> int8
        # temporal (M161 HIGH gate leak): date±int and timestamp IN must DECLINE
        ("date_plus", "SELECT d+1, count(*) FROM hits GROUP BY d+1", "decline"),
        # 2 elements → a real ScalarArrayOpExpr IN-list (a single-element IN folds to `ts = const`, a zone pred that
        # DOES route). The integer-only IN-list gate (M161 HIGH fix) makes a temporal IN-list decline.
        ("ts_inlist", "SELECT count(*) FROM hits WHERE ts IN (TIMESTAMP '2000-01-01 00:00:00', TIMESTAMP '2013-07-15 10:37:30.123456')", "decline"),
        # extract epoch-invariant (M157): minute routes; day declines
        ("extract_minute", "SELECT extract(minute FROM ts) m, count(*) FROM hits GROUP BY m", "route"),
        ("extract_day", "SELECT extract(day FROM ts) dd, count(*) FROM hits GROUP BY dd", "decline"),
        # float group key (M154): -0.0/NaN must be byte-identical if it routes (else decline is fine)
        ("group_f8", "SELECT f8, count(*) FROM hits GROUP BY f8", "route"),
        # bare group keys
        ("group_i2", "SELECT c2, count(*) FROM hits GROUP BY c2", "route"),
        ("group_bool", "SELECT b, count(*) FROM hits GROUP BY b", "route"),
    ]


# The POSITIVE CONTROL (ADR-2): a deliberately-divergent pair the oracle MUST flag. `hits` selects c4, `hits_heap` (via
# the _off substitution) would too — so to seed a divergence we compare a routed columnar query against a DIFFERENT heap
# expression by hand. Returns diverged>0 iff the oracle is working.
def positive_control(cur) -> int:
    # Disjoint filters (no arithmetic → no overflow): the positive rows of `hits` vs the negative rows of `hits_heap`
    # are disjoint sets → symmetric-EXCEPT MUST be > 0. If the oracle ever reports 0 here, it is broken.
    seeded = ("WITH a AS (SELECT c4 FROM hits WHERE c4 > 0), b AS (SELECT c4 FROM hits_heap WHERE c4 < 0) "
              "SELECT (SELECT count(*) FROM ((SELECT * FROM a EXCEPT SELECT * FROM b) "
              "UNION ALL (SELECT * FROM b EXCEPT SELECT * FROM a)) d)")
    cur.execute(seeded)
    return cur.fetchone()[0]


def run(out_path: str | None = None) -> int:
    c = _conn()
    cur = c.cursor()
    session_setup(cur)
    n = setup_tables(cur)
    results = []
    failed = 0
    # positive control first — if this doesn't catch a seeded divergence, the oracle is broken (abort)
    pc = positive_control(cur)
    if pc <= 0:
        print(f"POSITIVE CONTROL FAILED: seeded divergence not detected (diverged={pc}) — oracle broken", flush=True)
        return 2
    print(f"positive control OK: seeded divergence detected (diverged={pc})", flush=True)
    for name, sql, expect in build_cases():
        r = ab_check(cur, sql)
        ok = (expect == "route" and r["status"] == "ok") or (expect == "decline" and r["status"] == "declined")
        if not ok:
            failed += 1
        results.append((name, expect, r["status"], r.get("diverged"), ok))
        print(f"{'PASS' if ok else 'FAIL'} {name:18s} expect={expect:8s} got={r['status']:9s} "
              f"diverged={r.get('diverged')}", flush=True)
    if out_path:
        _write_verdict(out_path, n, pc, results, failed)
    print(f"\n=== M163 type-coverage A/B: {len(results)-failed}/{len(results)} cases as-expected; "
          f"rows={n}; positive_control={pc} ===", flush=True)
    return 0 if failed == 0 else 1


def _write_verdict(path, nrows, pc, results, failed) -> None:
    lines = [
        "# M163 — type-coverage A/B run", "",
        f"**Rows loaded:** {nrows} (equal in `hits` columnar + `hits_heap`).  ",
        f"**Positive control:** seeded divergence detected (diverged={pc}) — the oracle catches a wrong result.  ",
        f"**Result:** {len(results)-failed}/{len(results)} cases as-expected.", "",
        "| case | expect | got | diverged | ok |", "|---|---|---|---|---|",
    ]
    for name, expect, status, diverged, ok in results:
        lines.append(f"| {name} | {expect} | {status} | {diverged} | {'✅' if ok else '❌'} |")
    lines += ["", "Each `route` case is EXPLAIN=Custom Scan + symmetric-EXCEPT diverged=0; each `decline` case is native "
              "(no Custom Scan), the M161 fail-closed contract, over the type-edge catalog the ClickBench A/B misses.", ""]
    with open(path, "w") as f:
        f.write("\n".join(lines))


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=None)
    a = ap.parse_args()
    return run(a.out)


if __name__ == "__main__":
    sys.exit(main())
