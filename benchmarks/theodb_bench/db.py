"""PostgreSQL adapter for the benchmark harness — the ONLY I/O boundary (DIP).

Encapsulates psycopg2 so the pure logic (recall/metrics/dataset) never imports a driver.
Swapping psycopg2 for pg8000 (BSD) would be a change confined to this file.

Errors are typed: connect/ping/operational failures raise DBUnavailableError; a query that
does not use the index raises IndexNotUsedError; bad metric raises ValueError. No magic return
values (Rule 8) — every failure is loud and typed.
"""
from __future__ import annotations

import time
from contextlib import contextmanager

import psycopg2
from psycopg2.extras import execute_values

_OPS = {"l2": "<->", "cosine": "<=>"}


class DBUnavailableError(RuntimeError):
    """Raised when the database cannot be reached or an operation fails at the boundary."""


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

    @contextmanager
    def _cursor(self):
        """Yield a cursor, translating any psycopg2 operational error into DBUnavailableError."""
        if self._conn is None:
            raise DBUnavailableError("not connected (call connect() first)")
        try:
            with self._conn.cursor() as cur:
                yield cur
        except psycopg2.Error as e:
            raise DBUnavailableError(f"db operation failed: {e}") from e

    # --- connection -------------------------------------------------------
    def connect(self) -> "VectorDB":
        try:
            self._conn = psycopg2.connect(self.dsn)
            self._conn.autocommit = True
        except psycopg2.Error as e:
            raise DBUnavailableError(f"cannot connect ({self.dsn}): {e}") from e
        return self

    def ping(self) -> None:
        with self._cursor() as cur:
            cur.execute("SELECT 1")
            cur.fetchone()

    def close(self) -> None:
        if self._conn is not None:
            self._conn.close()
            self._conn = None

    # --- schema + load ----------------------------------------------------
    def ensure_extension(self) -> None:
        with self._cursor() as cur:
            cur.execute("CREATE EXTENSION IF NOT EXISTS vector")

    def create_table(self, table: str, dim: int, embed_col: str = "embedding") -> None:
        with self._cursor() as cur:
            cur.execute(f"DROP TABLE IF EXISTS {table}")
            cur.execute(
                f"CREATE TABLE {table} (id INTEGER PRIMARY KEY, {embed_col} vector({int(dim)}))"
            )

    def load_vectors(self, table, vectors, embed_col: str = "embedding") -> None:
        rows = [(i, "[" + ",".join(repr(float(x)) for x in v) + "]") for i, v in enumerate(vectors)]
        with self._cursor() as cur:
            execute_values(
                cur, f"INSERT INTO {table} (id, {embed_col}) VALUES %s", rows, page_size=1000
            )

    # --- index + query ----------------------------------------------------
    def build_index(self, ddl: str) -> float:
        start = time.perf_counter()
        with self._cursor() as cur:
            cur.execute(ddl)
        return time.perf_counter() - start

    def set_session(self, statement: str) -> None:
        with self._cursor() as cur:
            cur.execute(statement)

    def query_topk(self, table, qvec, k, metric="l2", embed_col="embedding"):
        sql = self._topk_sql(table, embed_col, k, metric)
        vec = "[" + ",".join(repr(float(x)) for x in qvec) + "]"
        with self._cursor() as cur:
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
        with self._cursor() as cur:
            cur.execute(sql, (vec, vec))
            plan = "\n".join(r[0] for r in cur.fetchall())
        if "Index Scan" not in plan and "Index Only Scan" not in plan:
            raise IndexNotUsedError(f"planner did not use the index:\n{plan}")

    def index_size_bytes(self, index_name: str) -> int:
        with self._cursor() as cur:
            cur.execute("SELECT pg_relation_size(%s::regclass)", (index_name,))
            return int(cur.fetchone()[0])

    # --- M7-S1: documents table (text + tsvector + vector) for the hybrid eval -------------------
    def create_documents_table(self, table: str, dim: int) -> None:
        with self._cursor() as cur:
            cur.execute(f"DROP TABLE IF EXISTS {table}")
            cur.execute(
                f"CREATE TABLE {table} ("
                f"  doc_id text PRIMARY KEY,"
                f"  content text,"
                f"  text_tsv tsvector GENERATED ALWAYS AS (to_tsvector('english', coalesce(content,''))) STORED,"
                f"  embedding vector({int(dim)}))"
            )
            cur.execute(f"CREATE INDEX {table}_gin ON {table} USING gin (text_tsv)")

    def load_documents(self, table: str, docs: dict) -> None:
        """docs: {doc_id: (content, vector_list)}."""
        rows = [
            (doc_id, content, "[" + ",".join(repr(float(x)) for x in vec) + "]")
            for doc_id, (content, vec) in docs.items()
        ]
        with self._cursor() as cur:
            execute_values(
                cur, f"INSERT INTO {table} (doc_id, content, embedding) VALUES %s", rows, page_size=1000
            )

    def vector_query_docs(self, table: str, qvec, n: int) -> list:
        vec = "[" + ",".join(repr(float(x)) for x in qvec) + "]"
        with self._cursor() as cur:
            cur.execute(
                f"SELECT doc_id FROM {table} WHERE embedding IS NOT NULL "
                f"ORDER BY embedding <=> %s::vector LIMIT {int(n)}",
                (vec,),
            )
            return [r[0] for r in cur.fetchall()]

    def fts_query(self, table: str, query_text: str, n: int) -> list:
        with self._cursor() as cur:
            cur.execute(
                f"SELECT doc_id FROM {table} WHERE text_tsv @@ plainto_tsquery('english', %s) "
                f"ORDER BY ts_rank_cd(text_tsv, plainto_tsquery('english', %s)) DESC LIMIT {int(n)}",
                (query_text, query_text),
            )
            return [r[0] for r in cur.fetchall()]

    def hybrid_rrf_docs(self, table: str, query_text: str, qvec, k: int, n: int) -> list:
        """Call the in-DB ai.hybrid_search_rrf with a precomputed query vector; return ranked doc ids."""
        vec = "[" + ",".join(repr(float(x)) for x in qvec) + "]"
        with self._cursor() as cur:
            cur.execute(
                "SELECT id FROM ai.hybrid_search_rrf("
                "  tbl => %s::regclass, id_col => 'doc_id', content_tsv_col => 'text_tsv',"
                "  vector_col => 'embedding', query_text => %s, query_vector => %s::vector,"
                "  k => %s, per_leg_limit => %s, result_limit => %s)",
                (table, query_text, vec, int(k), int(n), int(n)),
            )
            return [r[0] for r in cur.fetchall()]

    # --- M7-S2: pg_textsearch BM25 leg (throwaway image only; requires shared_preload_libraries) ----
    def pg_textsearch_available(self) -> bool:
        """True iff the pg_textsearch control file is installed AND the lib is in shared_preload_libraries.

        Both are required: the extension cannot be created without the control file (make install) NOR
        without the preload (it RAISEs 'library not loaded'). We check both so the skip gate is honest —
        checking only pg_available_extensions would pass on an image that installed the files but was
        started without `-c shared_preload_libraries=pg_textsearch`.
        """
        with self._cursor() as cur:
            cur.execute("SELECT count(*) FROM pg_available_extensions WHERE name = 'pg_textsearch'")
            installed = int(cur.fetchone()[0]) > 0
            cur.execute("SELECT current_setting('shared_preload_libraries', true)")
            preload = cur.fetchone()[0] or ""
        return installed and ("pg_textsearch" in preload)

    def ensure_bm25_extension(self) -> None:
        with self._cursor() as cur:
            cur.execute("CREATE EXTENSION IF NOT EXISTS pg_textsearch")

    def create_bm25_index(self, table: str, text_col: str = "content", text_config: str = "english") -> None:
        with self._cursor() as cur:
            cur.execute(
                f"CREATE INDEX {table}_bm25 ON {table} "
                f"USING bm25 ({text_col}) WITH (text_config='{text_config}')"
            )

    def bm25_query(self, table: str, query_text: str, n: int, text_col: str = "content") -> list:
        # pg_textsearch: `col <@> 'query'` returns the negative BM25 score (lower = better match),
        # so ASC ordering yields best-first. LIMIT enables its top-k (Block-Max WAND) optimization.
        with self._cursor() as cur:
            cur.execute(
                f"SELECT doc_id FROM {table} WHERE {text_col} IS NOT NULL "
                f"ORDER BY {text_col} <@> %s LIMIT {int(n)}",
                (query_text,),
            )
            return [r[0] for r in cur.fetchall()]

    # --- M6: pg_mooncake columnar/HTAP (throwaway measurement substrate; requires preload + wal_level) ----
    def pg_mooncake_available(self) -> bool:
        """True iff the pg_mooncake control file is installed AND it is in shared_preload_libraries.

        Both are required: pg_mooncake needs the preload (it relies on pg_duckdb being preloaded) — checking
        only pg_available_extensions would pass on an image that installed the files but was started without
        `-c shared_preload_libraries=pg_duckdb,pg_mooncake` (honesty parity with pg_textsearch_available)."""
        with self._cursor() as cur:
            cur.execute("SELECT count(*) FROM pg_available_extensions WHERE name = 'pg_mooncake'")
            installed = int(cur.fetchone()[0]) > 0
            cur.execute("SELECT current_setting('shared_preload_libraries', true)")
            preload = cur.fetchone()[0] or ""
        return installed and ("pg_mooncake" in preload)

    def ensure_mooncake_extension(self) -> None:
        with self._cursor() as cur:
            cur.execute("CREATE EXTENSION IF NOT EXISTS pg_mooncake CASCADE")

    def create_columnstore_mirror(self, mirror: str, base: str) -> None:
        with self._cursor() as cur:
            cur.execute("CALL mooncake.create_table(%s, %s)", (mirror, base))

    def explain_plan(self, sql: str) -> str:
        with self._cursor() as cur:
            cur.execute("EXPLAIN " + sql)
            return "\n".join(r[0] for r in cur.fetchall())

    def timed_query(self, sql: str):
        """Return (rows, elapsed_ms) for a read query."""
        with self._cursor() as cur:
            start = time.perf_counter()
            cur.execute(sql)
            rows = cur.fetchall()
            ms = (time.perf_counter() - start) * 1000.0
        return rows, ms
