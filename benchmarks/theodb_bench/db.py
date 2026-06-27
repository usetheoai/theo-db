"""PostgreSQL adapter for the benchmark harness — the ONLY I/O boundary (DIP).

Encapsulates psycopg2 so the pure logic (recall/metrics/dataset) never imports a driver.
Swapping psycopg2 for pg8000 (BSD) would be a change confined to this file.

Errors are typed (DBUnavailableError / IndexNotUsedError), never magic return values (Rule 8).
"""
from __future__ import annotations

import time

import psycopg2
from psycopg2.extras import execute_values

_OPS = {"l2": "<->", "cosine": "<=>", "ip": "<#>"}


class DBUnavailableError(RuntimeError):
    """Raised when the database cannot be reached (fail-fast at the boundary)."""


class IndexNotUsedError(RuntimeError):
    """Raised when an ANN query did not actually use the index (planner chose seqscan)."""


class VectorDB:
    def __init__(self, dsn: str):
        self.dsn = dsn
        self._conn = None

    # --- helpers (pure, unit-testable without a connection) ---------------
    @staticmethod
    def _op_for_metric(metric: str) -> str:
        try:
            return _OPS[metric]
        except KeyError:
            raise ValueError(f"unknown metric {metric!r} (expected {list(_OPS)})") from None

    @staticmethod
    def _topk_sql(table: str, embed_col: str, k: int, metric: str) -> str:
        op = VectorDB._op_for_metric(metric)
        return (
            f"SELECT id, {embed_col} {op} %s::vector AS distance "
            f"FROM {table} ORDER BY {embed_col} {op} %s::vector LIMIT {int(k)}"
        )

    # --- connection -------------------------------------------------------
    def connect(self) -> "VectorDB":
        try:
            self._conn = psycopg2.connect(self.dsn)
            self._conn.autocommit = True
        except psycopg2.Error as e:
            raise DBUnavailableError(f"cannot connect ({self.dsn}): {e}") from e
        return self

    def ping(self) -> None:
        try:
            with self._conn.cursor() as cur:
                cur.execute("SELECT 1")
                cur.fetchone()
        except (psycopg2.Error, AttributeError) as e:
            raise DBUnavailableError(f"ping failed: {e}") from e

    def close(self) -> None:
        if self._conn is not None:
            self._conn.close()
            self._conn = None

    # --- schema + load ----------------------------------------------------
    def ensure_extension(self) -> None:
        with self._conn.cursor() as cur:
            cur.execute("CREATE EXTENSION IF NOT EXISTS vector")

    def create_table(self, table: str, dim: int, embed_col: str = "embedding") -> None:
        with self._conn.cursor() as cur:
            cur.execute(f"DROP TABLE IF EXISTS {table}")
            cur.execute(
                f"CREATE TABLE {table} (id INTEGER PRIMARY KEY, {embed_col} vector({int(dim)}))"
            )

    def load_vectors(self, table, vectors, embed_col: str = "embedding") -> None:
        rows = [(i, "[" + ",".join(repr(float(x)) for x in v) + "]") for i, v in enumerate(vectors)]
        with self._conn.cursor() as cur:
            execute_values(
                cur, f"INSERT INTO {table} (id, {embed_col}) VALUES %s", rows, page_size=1000
            )

    # --- index + query ----------------------------------------------------
    def build_index(self, ddl: str) -> float:
        start = time.perf_counter()
        with self._conn.cursor() as cur:
            cur.execute(ddl)
        return time.perf_counter() - start

    def set_session(self, statement: str) -> None:
        with self._conn.cursor() as cur:
            cur.execute(statement)

    def query_topk(self, table, qvec, k, metric="l2", embed_col="embedding"):
        sql = self._topk_sql(table, embed_col, k, metric)
        vec = "[" + ",".join(repr(float(x)) for x in qvec) + "]"
        with self._conn.cursor() as cur:
            start = time.perf_counter()
            cur.execute(sql, (vec, vec))
            rows = cur.fetchall()
            latency = time.perf_counter() - start
        ids = [r[0] for r in rows]
        dists = [float(r[1]) for r in rows]
        return ids, dists, latency

    def assert_index_used(self, table, qvec, k, metric="l2", embed_col="embedding") -> None:
        sql = "EXPLAIN (FORMAT TEXT) " + self._topk_sql(table, embed_col, k, metric)
        vec = "[" + ",".join(repr(float(x)) for x in qvec) + "]"
        with self._conn.cursor() as cur:
            cur.execute(sql, (vec, vec))
            plan = "\n".join(r[0] for r in cur.fetchall())
        if "Index Scan" not in plan and "Index Only Scan" not in plan:
            raise IndexNotUsedError(f"planner did not use the index:\n{plan}")

    def index_size_bytes(self, index_name: str) -> int:
        with self._conn.cursor() as cur:
            cur.execute("SELECT pg_relation_size(%s::regclass)", (index_name,))
            return int(cur.fetchone()[0])
