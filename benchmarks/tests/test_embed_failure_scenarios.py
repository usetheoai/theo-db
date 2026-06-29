"""M17 failure-scenario tests for theodb.embed (Rust) — closes the coverage gaps the review flagged.

The frozen parity oracle `test_embed_sql.py` covers unset/NULL/scheme/unreachable/empty-body/malformed/
non-JSON. This file adds the rows the plan's `## Failure scenarios` table promised but the oracle did not
exercise, against the REAL Rust `theodb.embed` in the rebuilt image:

  * SSRF-via-redirect — a 3xx to an internal host (cloud metadata) is NOT followed (the plan's own HIGH risk).
  * HTTP 4xx — maps to SQLSTATE 38000 (external_routine_exception), parity with the plpython3u baseline
    (urllib raised HTTPError ⊂ URLError → the 38000 branch). NB: the plan v1.1 rows said 22023; the v1.3
    error-code correction + the baseline make HTTP failures 38000. This test pins the shipped contract.
  * empty-but-valid content `embed('')` — '' is a valid input (not NULL); it is passed through and yields a
    vector, exactly like the plpython3u baseline (which only rejects None).

Run like the oracle: against a theo-db:m17 container started with `--add-host=host.docker.internal:host-gateway`,
with PG* env vars pointing at it.
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
    yield c
    c.close()


@pytest.fixture(scope="module")
def redirect_4xx_server():
    """Serves a 302 (to an internal host) and a 400 so the SSRF-redirect and 4xx branches are exercised."""

    class H(BaseHTTPRequestHandler):
        def log_message(self, *_a):
            pass

        def do_POST(self):
            length = int(self.headers.get("Content-Length", 0))
            self.rfile.read(length)
            route = self.path.rstrip("/")
            if route == "/redirect":
                # 302 to the cloud-metadata address — must NOT be followed (SSRF).
                self.send_response(302)
                self.send_header("Location", "http://169.254.169.254/latest/meta-data/")
                self.end_headers()
                return
            # /badreq → 400 (4xx)
            payload = _json.dumps({"error": "bad request"}).encode()
            self.send_response(400)
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


@pytest.fixture(scope="module")
def embed_server():
    """The real deterministic model endpoint (same as the oracle) — for the empty-content parity case."""
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


def test_embed_redirect_to_internal_host_not_followed(conn, redirect_4xx_server):
    # SSRF: a 302 to 169.254.169.254 must NOT be followed → typed 38000, never reaches the internal host.
    with conn.cursor() as cur:
        cur.execute("SET theodb.embedding_endpoint = %s", (redirect_4xx_server + "/redirect",))
        with pytest.raises(psycopg2.errors.ExternalRoutineException) as exc:
            cur.execute("SELECT theodb.embed('x')")
    assert "call failed" in str(exc.value)


def test_embed_4xx_maps_to_external_routine_exception(conn, redirect_4xx_server):
    # HTTP 400 → SQLSTATE 38000 "call failed" (parity with the plpython3u baseline's URLError branch).
    with conn.cursor() as cur:
        cur.execute("SET theodb.embedding_endpoint = %s", (redirect_4xx_server + "/badreq",))
        with pytest.raises(psycopg2.errors.ExternalRoutineException) as exc:
            cur.execute("SELECT theodb.embed('x')")
    assert "call failed" in str(exc.value)


def test_embed_empty_content_is_valid_and_returns_vector(conn, embed_server):
    # '' is a valid (non-NULL) input — passed through to the endpoint, yields a vector (parity: the
    # baseline only rejects None, not empty string).
    with conn.cursor() as cur:
        cur.execute("SET theodb.embedding_endpoint = %s", (embed_server,))
        cur.execute("SELECT vector_dims(theodb.embed(''))")
        dims = cur.fetchone()[0]
    assert dims == 384
