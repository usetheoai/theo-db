"""Three plugable lexical retrievers for M140.1 (T1.1/T1.2).

The head-to-head the milestone decides (ADR D3): own BM25 (Tantivy — the same engine as
the M139 spike) vs the shipped baseline `ts_rank_cd` (PostgreSQL core). `pg_textsearch`
BM25 is a reference (T1.2): measured when the extension is present, else the report cites
the already-measured M138 numbers — never a fabricated number.

Ranking is storage-independent (D3): the BM25 score Tantivy produces is identical in-PG or
standalone, so retrieval QUALITY (nDCG/MRR) is measured off-PG here; storage cost
(index size / ingest / MVCC) is the T3 axis, where the M139 heap evidence already applies.

`Retriever` is a structural protocol (DIP, architecture.md §2) so the runner is engine-agnostic.
"""
from __future__ import annotations

import re
import shutil
import time
from pathlib import Path
from typing import Protocol, runtime_checkable

import tantivy


@runtime_checkable
class Retriever(Protocol):
    def index(self, docs: dict[str, str]) -> None: ...
    def search(self, query: str, k: int) -> list[str]: ...


class TantivyBM25:
    """Own-engine BM25 via tantivy-py (MIT). Schema mirrors the M139 spike: `body`
    TEXT (default tokenizer) + stored `id`. Optionally persists to `path` so T3 can
    measure the on-disk index size (the payload of the heap buffer-then-flush store).
    """

    def __init__(self, path: str | None = None):
        sb = tantivy.SchemaBuilder()
        sb.add_text_field("id", stored=True)
        sb.add_text_field("body", stored=True, tokenizer_name="default")
        self._schema = sb.build()
        self._path = path
        self._index: tantivy.Index | None = None
        self.ingest_ms: float = 0.0

    def index(self, docs: dict[str, str]) -> None:
        if self._path is not None:
            # Clean the path first: tantivy.Index(path=...) OPENS an existing index and
            # would APPEND, producing duplicate segments and a doubled index_bytes on a
            # stale dir (M140.1 review H1). A fresh dir guarantees a single-segment,
            # reproducible on-disk size.
            shutil.rmtree(self._path, ignore_errors=True)
            Path(self._path).mkdir(parents=True, exist_ok=True)
            idx = tantivy.Index(self._schema, path=self._path)
        else:
            idx = tantivy.Index(self._schema)  # in-RAM
        writer = idx.writer(heap_size=15_000_000)
        t0 = time.perf_counter()
        for doc_id, body in docs.items():
            writer.add_document(tantivy.Document(id=doc_id, body=body or ""))
        writer.commit()
        self.ingest_ms = (time.perf_counter() - t0) * 1000.0
        idx.reload()
        self._index = idx

    @staticmethod
    def _sanitize(query: str) -> str:
        """Free text -> bag of word tokens. Tantivy's query parser treats +, -, (), ", :
        as operators; BEIR natural-language queries contain them, so we reduce the query
        to alphanumeric/underscore tokens (the same lexical matching both engines do)."""
        return " ".join(re.findall(r"[A-Za-z0-9_]+", query.lower()))

    def search(self, query: str, k: int) -> list[str]:
        if self._index is None:
            raise RuntimeError("index() must be called before search()")
        clean = self._sanitize(query)
        if not clean:
            return []
        q = self._index.parse_query(clean, ["body"])
        searcher = self._index.searcher()
        hits = searcher.search(q, k).hits
        out: list[str] = []
        for _score, addr in hits:
            doc = searcher.doc(addr)
            vals = doc.get_all("id")
            if vals:
                out.append(vals[0])
        return out

    def index_bytes(self) -> int:
        """On-disk index size in bytes (requires path=...). 0 for in-RAM."""
        if self._path is None:
            return 0
        return sum(f.stat().st_size for f in Path(self._path).rglob("*") if f.is_file())


class PgTsRank:
    """PostgreSQL core `ts_rank_cd` over `to_tsvector('english', body)` — the exact
    baseline theo-lens ships (trace-read-repository.ts:365). Needs a live PG (docker).
    """

    def __init__(self, dsn: str, table: str = "m140_lex_docs"):
        self.dsn = dsn
        self.table = table
        self._conn = None
        self.available = False

    def connect(self) -> "PgTsRank":
        import psycopg2

        self._conn = psycopg2.connect(self.dsn)
        self._conn.autocommit = True
        self.available = True
        return self

    def index(self, docs: dict[str, str]) -> None:
        from psycopg2.extras import execute_values

        with self._conn.cursor() as cur:
            cur.execute(f"DROP TABLE IF EXISTS {self.table}")
            cur.execute(
                f"CREATE TABLE {self.table} (id text PRIMARY KEY, body text, tsv tsvector)"
            )
            execute_values(
                cur,
                f"INSERT INTO {self.table} (id, body, tsv) VALUES %s",
                [(k, v or "", v or "") for k, v in docs.items()],
                template="(%s, %s, to_tsvector('english', %s))",
            )
            cur.execute(f"CREATE INDEX {self.table}_gin ON {self.table} USING gin(tsv)")
            cur.execute("ANALYZE " + self.table)

    def search(self, query: str, k: int) -> list[str]:
        if not query.strip():
            return []
        with self._conn.cursor() as cur:
            cur.execute(
                f"SELECT id FROM {self.table} "
                "WHERE tsv @@ websearch_to_tsquery('english', %s) "
                "ORDER BY ts_rank_cd(tsv, websearch_to_tsquery('english', %s)) DESC "
                "LIMIT %s",
                (query, query, k),
            )
            return [r[0] for r in cur.fetchall()]

    def index_bytes(self) -> int:
        """Total relation size (heap + all indexes + toast) of the docs table in bytes.

        NOTE (review H2): this is the FAITHFUL footprint of the baseline theo-lens ships
        (a materialised `search_tsv` column + GIN, schema.ts:84) — it is NOT an
        index-only figure. For apples-to-apples, also read gin_index_bytes() and
        tsv_column_bytes() and compute the lean footprint in the report.
        """
        with self._conn.cursor() as cur:
            cur.execute("SELECT pg_total_relation_size(%s)", (self.table,))
            return int(cur.fetchone()[0])

    def gin_index_bytes(self) -> int:
        """Size of the GIN search index alone (the apples-to-apples peer of the Tantivy index)."""
        with self._conn.cursor() as cur:
            cur.execute("SELECT pg_relation_size(%s)", (f"{self.table}_gin",))
            return int(cur.fetchone()[0])

    def tsv_column_bytes(self) -> int:
        """Bytes the materialised `tsv` column occupies (redundant with the GIN) — the
        subtrahend for a lean footprint (heap without the materialised tsv)."""
        with self._conn.cursor() as cur:
            cur.execute(f"SELECT coalesce(sum(pg_column_size(tsv)), 0) FROM {self.table}")
            return int(cur.fetchone()[0])

    def close(self) -> None:
        if self._conn is not None:
            self._conn.close()


class PgTextsearchBM25:
    """Reference (T1.2): `pg_textsearch` BM25 in-PG. `available` is False when the
    extension is absent — the runner then cites the M138 measured numbers instead of
    fabricating one (Q2). This class only measures when the extension is really present.
    """

    def __init__(self, dsn: str):
        self.dsn = dsn
        self._conn = None
        self.available = False

    def connect(self) -> "PgTextsearchBM25":
        import psycopg2

        try:
            self._conn = psycopg2.connect(self.dsn)
            self._conn.autocommit = True
            with self._conn.cursor() as cur:
                cur.execute("CREATE EXTENSION IF NOT EXISTS pg_textsearch")
            self.available = True
        except Exception:
            self.available = False
        return self
