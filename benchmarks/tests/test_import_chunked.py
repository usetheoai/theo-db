"""Audit-remediation tests for theodb.import_vectors_chunked (T5.1, audit #6/#7).

The PROCEDURE ingests a Pinecone export in chunk_size batches with a COMMIT per batch (bounds memory/WAL).
Verified against the rebuilt image:

  * test_import_chunked_all_inserted   — 2500 records, chunk_size 1000 -> all 2500 rows present.
  * test_import_chunked_commits_batches — a bad record at index 1500 raises mid-chunk-2, but the first
    COMMITted chunk (1000 rows) survives (NOT all-or-nothing — the documented chunking semantic).
  * test_chunk_size_guard               — chunk_size 0 -> typed 22023, no ingest.

CALLed in autocommit (the procedure issues COMMITs; an explicit transaction block would forbid them).
Run against a rebuilt container started with `--add-host=host.docker.internal:host-gateway`, PG* env set.
"""
import json
import os

import psycopg2
import pytest

pytestmark = pytest.mark.integration


@pytest.fixture
def conn():
    c = psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"),
        port=os.environ.get("PGPORT", "5432"),
        dbname=os.environ.get("PGDATABASE", "postgres"),
        user=os.environ.get("PGUSER", "postgres"),
        password=os.environ.get("PGPASSWORD", "postgres"),
    )
    c.autocommit = True  # the PROCEDURE COMMITs per chunk — must not be inside an explicit transaction
    yield c
    c.close()


def _make_table(cur, name):
    cur.execute(f"DROP TABLE IF EXISTS {name}")
    cur.execute(f"CREATE TABLE {name} (id text PRIMARY KEY, embedding vector(3), metadata jsonb)")


def _export(n, bad_at=None):
    out = []
    for g in range(n):
        if bad_at is not None and g == bad_at:
            out.append({"id": str(g)})  # missing "values" -> invalid record
        else:
            out.append({"id": str(g), "values": [0.1, 0.2, 0.3], "metadata": {"k": g}})
    return json.dumps(out)


def test_import_chunked_all_inserted(conn):
    with conn.cursor() as cur:
        _make_table(cur, "imp_all")
        cur.execute(
            "CALL theodb.import_vectors_chunked('imp_all'::regclass, %s::jsonb, 1000)",
            (_export(2500),),
        )
        cur.execute("SELECT count(*) FROM imp_all")
        assert cur.fetchone()[0] == 2500
        cur.execute("SELECT vector_dims(embedding) FROM imp_all LIMIT 1")
        assert cur.fetchone()[0] == 3
        cur.execute("DROP TABLE imp_all")


def test_import_chunked_commits_batches(conn):
    with conn.cursor() as cur:
        _make_table(cur, "imp_partial")
        # A bad record at index 1500 fails mid-chunk-2; chunk-1 (records 0..999) was already COMMITted.
        with pytest.raises(psycopg2.errors.InvalidParameterValue):
            cur.execute(
                "CALL theodb.import_vectors_chunked('imp_partial'::regclass, %s::jsonb, 1000)",
                (_export(2500, bad_at=1500),),
            )
        cur.execute("SELECT count(*) FROM imp_partial")
        # The first committed chunk survives the abort (chunking is NOT all-or-nothing by design).
        assert cur.fetchone()[0] == 1000
        cur.execute("DROP TABLE imp_partial")


def test_chunk_size_guard(conn):
    with conn.cursor() as cur:
        _make_table(cur, "imp_guard")
        with pytest.raises(psycopg2.errors.InvalidParameterValue) as exc:
            cur.execute(
                "CALL theodb.import_vectors_chunked('imp_guard'::regclass, %s::jsonb, 0)",
                (_export(10),),
            )
        assert "chunk_size must be > 0" in str(exc.value)
        cur.execute("SELECT count(*) FROM imp_guard")
        assert cur.fetchone()[0] == 0
        cur.execute("DROP TABLE imp_guard")
