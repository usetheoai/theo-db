"""Audit-remediation tests for theodb.embed_batch(text[]) — the CRITICAL embed N+1 fix (T1.1).

Proves the batch path against the REAL Rust extension in the rebuilt image:
  * parity     — embed_batch(['a','b']) == [embed('a'), embed('b')] element-wise (same stub, deterministic).
  * N-in/N-out — count mismatch -> 38000; NULL element -> 22023; empty array -> empty vector[] (no HTTP).
  * index map  — out-of-order endpoint response is still placed by `index`, NOT array position (EC-3);
                 a duplicate input ('a','b','a') yields [0]==[2], [1] differs (EC-3).

Run like the oracle: against a rebuilt container started with `--add-host=host.docker.internal:host-gateway`,
with PG* env vars pointing at it; the real model endpoint is the local fastembed stub.
"""
import json as _json
import os
import socket
import subprocess
import sys
import threading
import time
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

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


@pytest.fixture
def conn():
    c = psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"),
        port=os.environ.get("PGPORT", "5432"),
        dbname=os.environ.get("PGDATABASE", "postgres"),
        user=os.environ.get("PGUSER", "postgres"),
        password=os.environ.get("PGPASSWORD", "postgres"),
    )
    c.autocommit = True
    yield c
    c.close()


@pytest.fixture(scope="module")
def embed_server():
    """The real deterministic model endpoint (same fastembed stub the oracle uses)."""
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


@pytest.fixture(scope="module")
def scripted_server():
    """A controllable endpoint that crafts the embeddings response to exercise the N-in/N-out guards.

    Routes (selected by URL path):
      /short   -> returns N-1 data items for an N-input request (size mismatch -> 38000).
      /shuffle -> returns N items whose `index` is reversed and whose embedding is the 1-dim vector
                  [float(index)]; if the Rust maps by `index` (correct) result[i] == '[i]', if it mapped
                  by array position it would be reversed. Proves index-alignment (EC-3).
    """

    class H(BaseHTTPRequestHandler):
        def log_message(self, *_a):
            pass

        def do_POST(self):
            length = int(self.headers.get("Content-Length", 0))
            body = _json.loads(self.rfile.read(length) or b"{}")
            inp = body.get("input")
            texts = [inp] if isinstance(inp, str) else list(inp)
            n = len(texts)
            route = self.path.rstrip("/")
            if route == "/short":
                items = [{"object": "embedding", "index": i, "embedding": [float(i)]}
                         for i in range(max(n - 1, 0))]
            else:  # /shuffle — reversed index order, embedding encodes the index
                items = [{"object": "embedding", "index": i, "embedding": [float(i)]}
                         for i in reversed(range(n))]
            payload = _json.dumps({"object": "list", "data": items, "model": "scripted"}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

    port = _free_port()
    srv = ThreadingHTTPServer(("0.0.0.0", port), H)
    t = threading.Thread(target=srv.serve_forever, daemon=True)
    t.start()
    try:
        yield f"http://host.docker.internal:{port}"
    finally:
        srv.shutdown()
        srv.server_close()


def _set_endpoint(cur, endpoint):
    cur.execute("SET theodb.embedding_endpoint = %s", (endpoint,))


def test_embed_batch_parity(conn, embed_server):
    # embed_batch(['a','b']) must equal [embed('a'), embed('b')] element-wise (same deterministic stub).
    with conn.cursor() as cur:
        _set_endpoint(cur, embed_server)
        cur.execute(
            "SELECT theodb.embed_batch(ARRAY['hello world','quarterly report'])::text "
            "     = ARRAY[theodb.embed('hello world'), theodb.embed('quarterly report')]::vector[]::text"
        )
        assert cur.fetchone()[0] is True


def test_embed_batch_dim_and_count(conn, embed_server):
    with conn.cursor() as cur:
        _set_endpoint(cur, embed_server)
        cur.execute("SELECT theodb.embed_batch(ARRAY['a','b','c'])")
        cur.execute("SELECT cardinality(theodb.embed_batch(ARRAY['a','b','c']))")
        assert cur.fetchone()[0] == 3
        cur.execute("SELECT vector_dims((theodb.embed_batch(ARRAY['a','b','c']))[1])")
        assert cur.fetchone()[0] == 384


def test_embed_batch_empty_returns_empty_array(conn):
    # EC-1: empty input -> empty vector[] (NOT NULL), and NO HTTP call (endpoint unreachable proves it).
    with conn.cursor() as cur:
        _set_endpoint(cur, "http://127.0.0.1:1/v1/embeddings")
        cur.execute("SELECT theodb.embed_batch(ARRAY[]::text[]) IS NOT NULL")
        assert cur.fetchone()[0] is True
        cur.execute("SELECT cardinality(theodb.embed_batch(ARRAY[]::text[]))")
        assert cur.fetchone()[0] == 0


def test_embed_batch_rejects_null_element(conn):
    # NULL element breaks N-in/N-out alignment -> 22023, fail-fast before any HTTP.
    with conn.cursor() as cur:
        _set_endpoint(cur, "http://127.0.0.1:1/v1/embeddings")
        with pytest.raises(psycopg2.errors.InvalidParameterValue) as exc:
            cur.execute("SELECT theodb.embed_batch(ARRAY['x', NULL]::text[])")
    assert "must not be NULL" in str(exc.value)


def test_embed_batch_size_mismatch(conn, scripted_server):
    # Endpoint returns N-1 embeddings -> typed 38000 "batch size mismatch", never a silent misalignment.
    with conn.cursor() as cur:
        _set_endpoint(cur, scripted_server + "/short")
        with pytest.raises(psycopg2.errors.ExternalRoutineException) as exc:
            cur.execute("SELECT theodb.embed_batch(ARRAY['a','b','c'])")
    assert "batch size mismatch" in str(exc.value)


def test_embed_batch_out_of_order_is_index_aligned(conn, scripted_server):
    # EC-3: the scripted endpoint returns data in REVERSED index order with embedding [float(index)].
    # Correct (index-aligned) mapping => result[i] == '[i]'. Positional mapping would reverse it.
    with conn.cursor() as cur:
        _set_endpoint(cur, scripted_server + "/shuffle")
        cur.execute("SELECT (theodb.embed_batch(ARRAY['a','b','c','d']))[1]::text")
        first = cur.fetchone()[0]
        cur.execute("SELECT (theodb.embed_batch(ARRAY['a','b','c','d']))[4]::text")
        fourth = cur.fetchone()[0]
    assert first == "[0]"
    assert fourth == "[3]"


def test_embed_batch_order_and_dups(conn, embed_server):
    # EC-3: a duplicate input ('a','b','a') -> 3 vectors where [0]==[2] (same text, deterministic) and
    # [1] differs. Proves both order preservation and dup handling against the real model.
    with conn.cursor() as cur:
        _set_endpoint(cur, embed_server)
        cur.execute(
            "WITH v AS (SELECT theodb.embed_batch(ARRAY['alpha','beta','alpha']) AS e) "
            "SELECT (e)[1]::text = (e)[3]::text, (e)[1]::text = (e)[2]::text FROM v"
        )
        same_0_2, same_0_1 = cur.fetchone()
    assert same_0_2 is True
    assert same_0_1 is False
