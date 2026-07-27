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
    "f4":  {"pg": "float4",     "edges": [0.0, "'-0.0'", 1.5, "'NaN'", "'Infinity'", "'-Infinity'"]},  # M154 IEEE
    "f8":  {"pg": "float8",     "edges": [0.0, "'-0.0'", 1.5, "'NaN'", "'Infinity'", "'-Infinity'"]},
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
    if "'-0.0'" not in catalog["f8"]["edges"] or "'NaN'" not in catalog["f8"]["edges"]:
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
def plan_routes(plan_lines: list[str], node: str | None = None) -> bool:
    """Pure (unit-testable, no DB): does the ON arm route to the columnar CustomScan? When `node` is given, require THAT
    specific columnar node (e.g. 'theodb_columnar_agg') — not just any columnar substring — so a non-load-bearing
    columnar child under a native Agg cannot masquerade as 'the tested path ran in columnar' (council-benchmark HIGH)."""
    needle = f"Custom Scan ({node}" if node else "Custom Scan (theodb_columnar"
    return any(needle in ln for ln in plan_lines)


def _sub_table(sql: str, table: str) -> str:
    """Substitute the base table `hits` → `table` (word-boundary; `hits_heap` is not matched by \\bhits\\b)."""
    import re
    return re.sub(r"\bhits\b", table, sql)


def _off_sql(sql: str) -> str:
    return _sub_table(sql, "hits_heap")


def _symmetric_except(cur, on_sql: str, off_sql: str) -> int:
    """The oracle: symmetric EXCEPT ALL (multiset byte-identity — a duplicated row on one side is caught, unlike plain
    EXCEPT/DISTINCT). Returns the diverged row count; 0 ⇒ byte-identical result multisets."""
    ex = (f"WITH a AS ({on_sql}), b AS ({off_sql}) "
          f"SELECT (SELECT count(*) FROM ((SELECT * FROM a EXCEPT ALL SELECT * FROM b) "
          f"UNION ALL (SELECT * FROM b EXCEPT ALL SELECT * FROM a)) d)")
    cur.execute(ex)
    return cur.fetchone()[0]


def ab_check(cur, sql: str, off_table: str = "hits_heap", expect_node: str | None = None) -> dict:
    """EXPLAIN the ON arm; if it routes (to `expect_node` when given) → symmetric-EXCEPT-ALL vs `off_table` → diverged;
    else → 'declined'. Returns {status, routed, diverged}. status ∈ {ok, declined, diverged, error}."""
    try:
        cur.execute("EXPLAIN (COSTS OFF) " + sql)
        plan = [r[0] for r in cur.fetchall()]
    except Exception as e:  # noqa: BLE001
        return {"status": "error", "routed": None, "diverged": None, "err": str(e).splitlines()[0][:80]}
    routed = plan_routes(plan, expect_node)
    if not routed:
        return {"status": "declined", "routed": False, "diverged": None}
    off = _sub_table(sql, off_table)
    try:
        diverged = _symmetric_except(cur, sql, off)
    except Exception as e:  # noqa: BLE001
        return {"status": "error", "routed": True, "diverged": None, "err": str(e).splitlines()[0][:80]}
    return {"status": "ok" if diverged == 0 else "diverged", "routed": True, "diverged": diverged}


# --- the case matrix (admit-path × type edges) ------------------------------------------------------------------------
# Each case: (name, sql, expect) — expect ∈ {"route" (must be ok, diverged=0), "decline" (must be declined)}.
def build_cases() -> list[tuple[str, str, str, str | None]]:
    """(name, sql, expect ∈ {route, decline}, expect_node). For `route`, expect_node is the SPECIFIC columnar node that
    must appear in EXPLAIN (so the tested admit-path — not a non-load-bearing columnar child — computed the result)."""
    AGG = "theodb_columnar_agg"
    return [
        # agg pushdown
        ("agg_count", "SELECT count(*) FROM hits", "route", AGG),
        ("agg_sum_i4", "SELECT sum(c4) FROM hits", "route", AGG),
        ("agg_sum_i8", "SELECT sum(c8) FROM hits", "route", AGG),                          # int8 routed (was uncovered)
        # IN-list integer (M161)
        ("inlist_i4", "SELECT count(*) FROM hits WHERE c4 IN (0, 1, -1)", "route", AGG),
        ("inlist_i2", "SELECT count(*) FROM hits WHERE c2 IN (0, 32767)", "route", AGG),
        ("inlist_null", "SELECT count(*) FROM hits WHERE c4 IN (NULL, 1)", "decline", None),  # 3-valued
        # int±k group-expr (M161 BLOCKER): int2+int4 -> int4 result MUST route + byte-identical
        ("intpk_i2", "SELECT c2+5, count(*) FROM hits GROUP BY c2+5", "route", AGG),
        ("intpk_i4", "SELECT c4-1, count(*) FROM hits GROUP BY c4-1", "route", AGG),
        ("intpk_i8_result", "SELECT c8+5, count(*) FROM hits GROUP BY c8+5", "decline", None),   # int8 result
        ("intpk_i4_wide", "SELECT c4+3000000000, count(*) FROM hits GROUP BY c4+3000000000", "decline", None),  # ->int8
        # temporal (M161 HIGH gate leak): date±int and multi-elem timestamp IN must DECLINE
        ("date_plus", "SELECT d+1, count(*) FROM hits GROUP BY d+1", "decline", None),
        ("ts_inlist", "SELECT count(*) FROM hits WHERE ts IN (TIMESTAMP '2000-01-01 00:00:00', TIMESTAMP '2013-07-15 10:37:30.123456')", "decline", None),
        # timestamptz grouped by EXACT equality routes byte-identical: the storage-vs-Arrow epoch offset is a bijection
        # under equality (the M157 divergence was date_trunc *calendar bucketing*, not bare equality). expect=route so a
        # future output-epoch regression surfaces as diverged>0 instead of being masked by a pessimistic decline.
        ("tz_group", "SELECT tz, count(*) FROM hits GROUP BY tz", "route", AGG),
        # extract epoch-invariant (M157): minute routes; day declines
        ("extract_minute", "SELECT extract(minute FROM ts) m, count(*) FROM hits GROUP BY m", "route", AGG),
        ("extract_day", "SELECT extract(day FROM ts) dd, count(*) FROM hits GROUP BY dd", "decline", None),
        # float group key: M163 found the columnar path diverges (IEEE −0.0/+0.0 vs float8eq) -> MUST decline
        ("group_f8", "SELECT f8, count(*) FROM hits GROUP BY f8", "decline", None),
        ("group_f4", "SELECT f4, count(*) FROM hits GROUP BY f4", "decline", None),
        # bare group keys (routed)
        ("group_i2", "SELECT c2, count(*) FROM hits GROUP BY c2", "route", AGG),
        ("group_bool", "SELECT b, count(*) FROM hits GROUP BY b", "route", AGG),
        ("group_text", "SELECT t, count(*) FROM hits GROUP BY t", "route", AGG),             # text key (M153/M158)
        # SCOPE: this harness covers the AGG/group/in-list/zone-pred/expr admit-paths — where every historical type-class
        # bug lived (M151/M154/M157/M161/M163). The late-materialization *projection* path (M158) is query-shape, not
        # type-class, and is cost-gated on table size + enable_sort — it belongs to M158's own A/B, not here.
    ]


# The POSITIVE CONTROL (ADR-2): seed a KNOWN divergence and run it THROUGH `ab_check` (not a hand-written EXCEPT) so a
# green control genuinely proves the exact oracle path the real cases use detects a wrong result (council-benchmark HIGH).
# We build a twin table `hits_bad` = hits_heap with one c4 bumped, then `ab_check(SELECT sum(c4) …, off_table='hits_bad')`
# routes (agg) and compares `hits` vs `hits_bad` → the sums differ → status must be 'diverged'.
def positive_control(cur) -> dict:
    cur.execute("DROP TABLE IF EXISTS hits_bad")
    cur.execute("CREATE TABLE hits_bad AS SELECT * FROM hits_heap")
    cur.execute("UPDATE hits_bad SET c4 = c4 + 1 WHERE ctid = (SELECT min(ctid) FROM hits_bad)")
    r = ab_check(cur, "SELECT sum(c4) FROM hits", off_table="hits_bad", expect_node="theodb_columnar_agg")
    cur.execute("DROP TABLE IF EXISTS hits_bad")
    return r


def run(out_path: str | None = None) -> int:
    c = _conn()
    cur = c.cursor()
    session_setup(cur)
    n = setup_tables(cur)
    results = []
    failed = 0
    # positive control first (THROUGH ab_check) — if the exact oracle path doesn't flag a seeded divergence it is broken.
    pc = positive_control(cur)
    if pc["status"] != "diverged":
        print(f"POSITIVE CONTROL FAILED: seeded divergence not flagged by ab_check (got {pc}) — oracle broken", flush=True)
        return 2
    print(f"positive control OK: ab_check flagged the seeded divergence (diverged={pc['diverged']})", flush=True)
    for name, sql, expect, node in build_cases():
        r = ab_check(cur, sql, expect_node=node)
        ok = (expect == "route" and r["status"] == "ok") or (expect == "decline" and r["status"] == "declined")
        if not ok:
            failed += 1
        results.append((name, expect, r["status"], r.get("diverged"), ok))
        print(f"{'PASS' if ok else 'FAIL'} {name:18s} expect={expect:8s} got={r['status']:9s} "
              f"diverged={r.get('diverged')}", flush=True)
    if out_path:
        _write_verdict(out_path, n, pc["diverged"], results, failed)
    print(f"\n=== M163 type-coverage A/B: {len(results)-failed}/{len(results)} cases as-expected; "
          f"rows={n}; positive_control diverged={pc['diverged']} ===", flush=True)
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
