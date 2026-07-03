"""Tests for ai.hybrid_search_rrf availability under theodb_rs removal (T2.1, audit #3/#8 evolved by M19).

M19 ported ai.hybrid_search_rrf to the Rust theodb_rs extension — so it now CO-RESIDES with theodb.embed
(both theodb_rs members). The former cross-extension seam (hybrid in theodb, embed in theodb_rs → a 0A000
guard) no longer applies: dropping theodb_rs removes BOTH, so calling hybrid raises a clean 42883
(undefined_function) on the function itself — which is itself the actionable error (re-create theodb_rs).
The defensive 0A000 guard remains in the Rust code for the (hard-to-reach) individually-dropped-embed case.

  * test_absent_raises_undefined — with theodb_rs removed, ai.hybrid_search_rrf is cleanly absent (42883).
                                   Simulated via DROP EXTENSION inside a transaction that is ROLLED BACK.
  * test_present_unchanged       — with theodb_rs present, the normal RRF path returns rows (no behavior
                                   change when the common case holds).

Run against a rebuilt container started with `--add-host=host.docker.internal:host-gateway`, PG* env set.
"""
import os
import socket
import subprocess
import sys
import time
import urllib.request

import psycopg2
import pytest

pytestmark = pytest.mark.integration

_REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def _free_port() -> int:
    s = socket.socket()
    s.bind(("", 0))
    port = s.getsockname()[1]
    s.close()
    return port


def _connect():
    return psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"),
        port=os.environ.get("PGPORT", "5432"),
        dbname=os.environ.get("PGDATABASE", "postgres"),
        user=os.environ.get("PGUSER", "postgres"),
        password=os.environ.get("PGPASSWORD", "postgres"),
    )


@pytest.fixture(scope="module")
def embed_server():
    port = _free_port()
    proc = subprocess.Popen(
        [sys.executable, os.path.join(_REPO, "benchmarks", "servers", "embedding_server.py"),
         "--host", "0.0.0.0", "--port", str(port), "--model", "BAAI/bge-small-en-v1.5"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    try:
        for _ in range(120):
            try:
                with urllib.request.urlopen(f"http://127.0.0.1:{port}/health", timeout=2) as r:
                    if r.status == 200:
                        break
            except OSError:
                time.sleep(1)
        else:
            raise RuntimeError("embedding server did not become healthy")
        yield f"http://host.docker.internal:{port}/v1/embeddings"
    finally:
        proc.terminate()
        proc.wait(timeout=10)


def test_absent_raises_undefined():
    # M19: ai.hybrid_search_rrf is now a theodb_rs member (Rust port), co-resident with theodb.embed.
    # Dropping theodb_rs removes the function itself, so a call raises a clean undefined_function (42883)
    # naming the function — the actionable error is "re-create theodb_rs". ROLLBACK restores the extension.
    conn = _connect()
    conn.autocommit = False
    try:
        with conn.cursor() as cur:
            cur.execute("DROP EXTENSION theodb_rs")  # removes hybrid_search_rrf AND theodb.embed (both members)
            with pytest.raises(psycopg2.errors.UndefinedFunction) as exc:
                cur.execute(
                    "SELECT * FROM ai.hybrid_search_rrf("
                    "  'pg_class'::regclass, 'relname', 'relname', 'relname', query_text := 'anything')"
                )
            msg = str(exc.value)
            assert "hybrid_search_rrf" in msg  # the function itself is cleanly gone (not a cryptic mid-query error)
    finally:
        conn.rollback()  # undo the DROP — non-destructive
        conn.close()


def test_present_unchanged(embed_server):
    # With theodb.embed present, the normal RRF path runs and returns rows (no behavior change).
    conn = _connect()
    conn.autocommit = True
    try:
        with conn.cursor() as cur:
            cur.execute("SET theodb.embedding_endpoint = %s", (embed_server,))
            cur.execute(
                "CREATE TEMP TABLE guard_docs("
                "  id text, content text,"
                "  content_tsv tsvector GENERATED ALWAYS AS (to_tsvector('english', content)) STORED,"
                "  embedding vector(384))"
            )
            cur.execute(
                "INSERT INTO guard_docs(id, content, embedding) VALUES "
                "('1', 'the quick brown fox', theodb.embed('the quick brown fox')),"
                "('2', 'a slow green turtle', theodb.embed('a slow green turtle'))"
            )
            cur.execute(
                "SELECT id, score FROM ai.hybrid_search_rrf("
                "  'guard_docs'::regclass, 'id', 'content_tsv', 'embedding', query_text := 'quick fox')"
            )
            rows = cur.fetchall()
            cur.execute("DROP TABLE guard_docs")
        assert len(rows) >= 1
        assert all(isinstance(r[0], str) for r in rows)
    finally:
        conn.close()
