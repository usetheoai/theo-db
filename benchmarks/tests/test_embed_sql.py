"""Integration test for theodb.embed() — the M2 DoD-3 SQL embedding function.

Real, no mock: a local embedding server (fastembed BAAI/bge-small-en-v1.5, 384-dim) is the configurable
model endpoint; the DB calls it via plpython3u exactly as it would call a cloud provider. The container
(PG* env vars) must be built from the image that ships plpython3u + theodb.embed AND started with
`--add-host=host.docker.internal:host-gateway` so the in-container function can reach this host server.
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
def embed_server():
    port = _free_port()
    proc = subprocess.Popen(
        [sys.executable, os.path.join(_REPO, "tools", "embedding_server.py"),
         "--host", "0.0.0.0", "--port", str(port), "--model", "BAAI/bge-small-en-v1.5"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    try:
        # wait until the model is loaded and /health responds
        for _ in range(120):
            try:
                with urllib.request.urlopen(f"http://127.0.0.1:{port}/health", timeout=2) as r:
                    if r.status == 200:
                        break
            except OSError:
                time.sleep(1)
        else:
            raise RuntimeError("embedding server did not become healthy")
        # the container reaches the host via host.docker.internal (host-gateway)
        yield f"http://host.docker.internal:{port}/v1/embeddings"
    finally:
        proc.terminate()
        proc.wait(timeout=10)


@pytest.fixture
def conn():
    c = psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"),
        port=os.environ.get("PGPORT", "5432"),
        dbname=os.environ.get("PGDATABASE", "postgres"),
        user=os.environ.get("PGUSER", "postgres"),
        password=os.environ.get("PGPASSWORD", "postgres"),
    )
    yield c
    c.close()


def _embed(conn, text, endpoint):
    with conn.cursor() as cur:
        cur.execute("SET theodb.embedding_endpoint = %s", (endpoint,))
        cur.execute("SELECT theodb.embed(%s)::text", (text,))
        raw = cur.fetchone()[0]
    return [float(x) for x in raw.strip("[]").split(",")]


def test_embed_returns_real_384d_vector(conn, embed_server):
    vec = _embed(conn, "hello world", embed_server)
    assert len(vec) == 384  # bge-small-en-v1.5 dimensionality
    assert any(abs(x) > 1e-6 for x in vec)  # not all-zeros


def test_embed_is_semantically_meaningful(conn, embed_server):
    # a REAL model: paraphrases are closer than unrelated text (proves it is not a mock/hash)
    with conn.cursor() as cur:
        cur.execute("SET theodb.embedding_endpoint = %s", (embed_server,))
        cur.execute(
            "SELECT theodb.embed('the cat sat on the mat') <=> theodb.embed('a feline rested on the rug'), "
            "       theodb.embed('the cat sat on the mat') <=> theodb.embed('quarterly financial report')"
        )
        sim_related, sim_unrelated = cur.fetchone()
    # cosine DISTANCE: smaller = more similar
    assert sim_related < sim_unrelated


def test_embed_is_deterministic(conn, embed_server):
    a = _embed(conn, "reproducible text", embed_server)
    b = _embed(conn, "reproducible text", embed_server)
    assert a == b


def test_embed_without_endpoint_raises_typed_error(conn):
    # negative case: unset endpoint fails fast with a clear, typed error (not a silent NULL)
    with conn.cursor() as cur:
        cur.execute("RESET theodb.embedding_endpoint")
        with pytest.raises(psycopg2.errors.InvalidParameterValue) as exc:
            cur.execute("SELECT theodb.embed('x')")
    assert "embedding_endpoint is not set" in str(exc.value)
