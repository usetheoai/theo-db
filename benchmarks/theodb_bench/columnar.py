"""M6 columnar/HTAP measurement: an analytical query on a pg_mooncake columnstore mirror vs the row-store.

Proves the capability (mirror result == row result), measures both timings (DoD-1), and captures the
row-vs-columnar plan choice (DuckDBScan on the mirror vs Seq Scan on the row table — DoD-2). The columnar layer
is a DuckDB+Iceberg lakehouse (on disk), NOT AlloyDB's in-memory columnar (D2 — documented honestly).
"""
from __future__ import annotations

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


def run_columnar_vs_row(db, n: int = 100000, table: str = "metrics") -> dict:
    """Seed a row table, create a columnstore mirror, run the analytical aggregate on both. Returns
    {"row": {...}, "columnar": {...}} each with result/ms/plan, plus n + match."""
    db.ensure_mooncake_extension()
    mirror = table + "_cs"
    with db._cursor() as cur:  # idempotent: a columnstore mirror is NOT dropped by the base's CASCADE
        cur.execute(f"DROP TABLE IF EXISTS {mirror}")
    seed_metrics(db, table, n)
    db.create_columnstore_mirror(mirror, table)

    row_rows, row_ms = db.timed_query(_AGG.format(t=table))
    col_rows, col_ms = db.timed_query(_AGG.format(t=mirror))
    return {
        "n": n,
        "match": row_rows == col_rows,
        "row": {"result": row_rows, "ms": row_ms, "plan": db.explain_plan(_AGG.format(t=table))},
        "columnar": {"result": col_rows, "ms": col_ms, "plan": db.explain_plan(_AGG.format(t=mirror))},
    }
