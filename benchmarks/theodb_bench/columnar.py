"""M6 columnar/HTAP measurement: an analytical query on a pg_mooncake columnstore mirror vs the row-store.

Proves the capability (mirror result == row result), measures both timings (DoD-1), and captures the
row-vs-columnar plan choice (DuckDBScan on the mirror vs Seq Scan on the row table — DoD-2). The columnar layer
is a DuckDB+Iceberg lakehouse (on disk), NOT AlloyDB's in-memory columnar (D2 — documented honestly).
"""
from __future__ import annotations

import time

_AGG = ("SELECT category, count(*) AS c, round(avg(amount)::numeric, 4) AS a "
        "FROM {t} GROUP BY category ORDER BY category")


def seed_metrics(db, table: str, n: int) -> None:
    with db._cursor() as cur:
        cur.execute(f"DROP TABLE IF EXISTS {table} CASCADE")
        cur.execute(f"CREATE TABLE {table} (id bigint PRIMARY KEY, category text, amount double precision)")
        # deterministic synthetic data: 5 categories, n rows
        cur.execute(
            f"INSERT INTO {table} "
            f"SELECT g, 'cat' || (g %% 5), (g %% 1000) * 1.5 FROM generate_series(1, %s) g",
            (n,),
        )


def _wait_mirror_synced(db, mirror: str, base: str, tries: int = 30) -> int:
    """Fail-fast sync barrier: block until the columnstore mirror row count equals the base (the mirror may
    backfill/sync asynchronously). Returns the synced count; raises if it never converges (clear message,
    never a confusing aggregate mismatch downstream)."""
    with db._cursor() as cur:
        cur.execute(f"SELECT count(*) FROM {base}")
        want = int(cur.fetchone()[0])
    for _ in range(tries):
        with db._cursor() as cur:
            cur.execute(f"SELECT count(*) FROM {mirror}")
            have = int(cur.fetchone()[0])
        if have == want:
            return have
        time.sleep(0.5)
    raise RuntimeError(f"columnstore mirror {mirror} did not sync ({have}/{want} rows) — sync barrier timed out")


def run_columnar_vs_row(db, n: int = 100000, table: str = "metrics") -> dict:
    """Seed a row table, create a columnstore mirror, run the analytical aggregate on both. Returns
    {"row": {...}, "columnar": {...}} each with result/ms/plan, plus n, rows_synced + match (count + per-group
    avg within epsilon — cross-engine PG-vs-DuckDB summation can differ at the last decimal)."""
    db.ensure_mooncake_extension()
    mirror = table + "_cs"
    with db._cursor() as cur:  # idempotent: a columnstore mirror is NOT dropped by the base's CASCADE
        cur.execute(f"DROP TABLE IF EXISTS {mirror}")
    seed_metrics(db, table, n)
    db.create_columnstore_mirror(mirror, table)
    rows_synced = _wait_mirror_synced(db, mirror, table)  # barrier: mirror reflects the base before we compare

    row_rows, row_ms = db.timed_query(_AGG.format(t=table))
    col_rows, col_ms = db.timed_query(_AGG.format(t=mirror))
    return {
        "n": n,
        "rows_synced": rows_synced,
        "match": _results_match(row_rows, col_rows),
        "row": {"result": row_rows, "ms": row_ms, "plan": db.explain_plan(_AGG.format(t=table))},
        "columnar": {"result": col_rows, "ms": col_ms, "plan": db.explain_plan(_AGG.format(t=mirror))},
    }


def _results_match(row_rows, col_rows, eps: float = 1e-3) -> bool:
    """Same groups + exact counts + per-group avg within eps (cross-engine numeric tolerance)."""
    if len(row_rows) != len(col_rows) or len(row_rows) == 0:
        return False
    for (rc, rn, ra), (cc, cn, ca) in zip(row_rows, col_rows):
        if rc != cc or int(rn) != int(cn):
            return False
        if abs(float(ra) - float(ca)) > eps:
            return False
    return True
