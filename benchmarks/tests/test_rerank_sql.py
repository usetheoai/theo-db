"""Contract tests for the M65 `ai.rerank` SQL surface against a REAL cross-encoder stub.

Integration layer (opt-in, needs a container/pgrx instance + sentence-transformers): spins up
`benchmarks/servers/rerank_server.py` (a real BGE cross-encoder) as the configurable endpoint and exercises
the `ai.rerank` SQL→HTTP→parse contract end-to-end. Asserts index-alignment + relevance ordering — never a
mock (the score is a real model output), but the ASSERTIONS are on structure (idx in range, sorted DESC,
the clearly-relevant doc outranks the clearly-irrelevant one), not exact float values (model-version drift).

Requires PG* env (PGHOST/PGPORT/PGUSER) pointing at an instance with `CREATE EXTENSION theodb_rs` and the
stub reachable at 127.0.0.1 (co-located pgrx-run instance).
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


@pytest.fixture(scope="module")
def rerank_endpoint():
    port = _free_port()
    proc = subprocess.Popen(
        [sys.executable, os.path.join(_REPO, "benchmarks", "servers", "rerank_server.py"),
         "--host", "0.0.0.0", "--port", str(port)],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    try:
        for _ in range(120):  # model load can take a while on first run
            try:
                with urllib.request.urlopen(f"http://127.0.0.1:{port}/health", timeout=2) as r:
                    if r.status == 200:
                        break
            except OSError:
                time.sleep(1.0)
        else:
            raise RuntimeError("rerank stub server did not become healthy")
        yield f"http://127.0.0.1:{port}/rerank"
    finally:
        proc.terminate()
        proc.wait(timeout=10)


@pytest.fixture(scope="module")
def conn():
    c = psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"),
        port=os.environ.get("PGPORT", "28817"),
        user=os.environ.get("PGUSER", "theo"),
        dbname=os.environ.get("PGDATABASE", "postgres"),
        connect_timeout=15,
    )
    c.autocommit = True
    yield c
    c.close()


def _rerank(conn, endpoint, query, docs, top_n=None):
    cur = conn.cursor()
    cur.execute("SET theodb.rerank_endpoint = %s", (endpoint,))
    cur.execute(
        "SELECT idx, score FROM ai.rerank(%s, %s::text[], NULL, %s) ORDER BY score DESC",
        (query, docs, top_n),
    )
    return cur.fetchall()


def test_rerank_orders_relevant_doc_first(conn, rerank_endpoint):
    # A clearly-relevant doc must outrank a clearly-irrelevant one (structural, not exact-score).
    docs = ["The capital of France is Paris.", "Bananas are a yellow fruit rich in potassium."]
    rows = _rerank(conn, rerank_endpoint, "What is the capital of France?", docs)
    assert len(rows) == 2
    # idx are 0-based into the input; the top row (highest score) must be the France doc (idx 0).
    assert rows[0][0] == 0, f"expected the relevant doc (idx 0) first, got {rows}"


def test_rerank_scores_index_aligned_and_in_range(conn, rerank_endpoint):
    docs = ["alpha doc", "beta doc", "gamma doc"]
    rows = _rerank(conn, rerank_endpoint, "beta", docs)
    idxs = sorted(r[0] for r in rows)
    assert idxs == [0, 1, 2], f"every input index must appear exactly once, got {idxs}"


def test_rerank_top_n_truncates(conn, rerank_endpoint):
    docs = ["a", "b", "c", "d"]
    rows = _rerank(conn, rerank_endpoint, "b", docs, top_n=2)
    assert len(rows) == 2, "top_n=2 must return exactly 2 rows"


def test_rerank_empty_docs_returns_no_rows(conn, rerank_endpoint):
    cur = conn.cursor()
    cur.execute("SET theodb.rerank_endpoint = %s", (rerank_endpoint,))
    cur.execute("SELECT count(*) FROM ai.rerank('q', ARRAY[]::text[], NULL, NULL)")
    assert cur.fetchone()[0] == 0
