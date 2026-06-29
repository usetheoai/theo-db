"""Audit-remediation tests for the ai.hybrid_search_rrf fail-fast seam guard (T2.1, audit #3/#8).

theodb.embed is a late-bound cross-extension call inside ai.hybrid_search_rrf with no pg_depend edge, so
dropping theodb_rs would surface as a cryptic 42883 mid-query. The guard turns it into a clear typed 0A000.

  * test_absent_raises_0A000     — with theodb_rs (and thus theodb.embed) removed, a query_text call raises
                                   feature_not_supported (0A000) with the actionable message. Simulated via
                                   DROP EXTENSION inside a transaction that is ROLLED BACK (non-destructive).
  * test_present_unchanged       — with theodb.embed present, the normal RRF path returns rows (no behavior
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
        [sys.executable, os.path.join(_REPO, "tools", "embedding_server.py"),
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


def test_absent_raises_0A000():
    # Simulate theodb.embed absent by dropping theodb_rs inside a transaction; the guard must raise a
    # clear feature_not_supported (0A000), not a cryptic 42883. ROLLBACK restores the extension.
    conn = _connect()
    conn.autocommit = False
    try:
        with conn.cursor() as cur:
            cur.execute("DROP EXTENSION theodb_rs")  # removes theodb.embed (its member); no hard dependents
            with pytest.raises(psycopg2.errors.FeatureNotSupported) as exc:
                cur.execute(
                    "SELECT * FROM ai.hybrid_search_rrf("
                    "  'pg_class'::regclass, 'relname', 'relname', 'relname', query_text := 'anything')"
                )
            msg = str(exc.value)
            assert "theodb.embed is unavailable" in msg
            assert "theodb_rs" in msg
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
