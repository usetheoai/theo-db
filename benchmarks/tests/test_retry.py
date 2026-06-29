"""Audit-remediation tests for recoverable-class retry (T3.1, audit #5).

Both clients (Rust embed via theodb.embed + plpython3u ai._chat via ai.generate) must:
  * recover a transient 503-then-200 (test_transient_recovers),
  * NOT retry a 4xx / input error — fail-fast, exactly one endpoint hit (test_no_retry_on_4xx),
  * stay bounded on a permanently-down endpoint (≤ 3 attempts) AND never leak the api_key in the final
    exhausted error (test_retry_respects_bounds_and_no_key_leak — EC-2).

A scripted endpoint with per-route hit counters serves the flaky / 4xx / always-503 behaviors. It binds
0.0.0.0 so the container reaches it via host.docker.internal; the test host reads counters via 127.0.0.1.
Run against a rebuilt container started with `--add-host=host.docker.internal:host-gateway`, PG* env set.
"""
import json as _json
import os
import socket
import threading
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import psycopg2
import pytest

pytestmark = pytest.mark.integration


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
def scripted():
    """A scripted embeddings+chat endpoint with per-route POST hit counters.

    Yields (container_base, host_base): SET the GUC to container_base/<route>; read counters via host_base.
    """

    class H(BaseHTTPRequestHandler):
        counts: dict = {}

        def log_message(self, *_a):
            pass

        def _send(self, code, body: bytes):
            self.send_response(code)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_GET(self):
            route = self.path.rstrip("/")
            if route == "/reset":
                H.counts.clear()
                self._send(200, b"{}")
            elif route == "/count":
                self._send(200, _json.dumps(H.counts).encode())
            else:
                self._send(404, b"{}")

        def do_POST(self):
            length = int(self.headers.get("Content-Length", 0))
            self.rfile.read(length)
            route = self.path.rstrip("/")
            H.counts[route] = H.counts.get(route, 0) + 1
            c = H.counts[route]
            if route == "/embed/flaky":
                if c == 1:
                    self._send(503, b'{"error":"temporarily unavailable"}')
                else:
                    self._send(200, _json.dumps(
                        {"data": [{"object": "embedding", "index": 0, "embedding": [0.1, 0.2, 0.3]}]}
                    ).encode())
            elif route == "/embed/badreq":
                self._send(400, b'{"error":"bad request"}')
            elif route == "/embed/down":
                self._send(503, b'{"error":"down"}')
            elif route == "/chat/flaky":
                if c == 1:
                    self._send(503, b'{"error":"temporarily unavailable"}')
                else:
                    self._send(200, _json.dumps(
                        {"choices": [{"message": {"content": "ok"}}]}
                    ).encode())
            else:
                self._send(404, b"{}")

    port = _free_port()
    srv = ThreadingHTTPServer(("0.0.0.0", port), H)
    t = threading.Thread(target=srv.serve_forever, daemon=True)
    t.start()
    try:
        yield (f"http://host.docker.internal:{port}", f"http://127.0.0.1:{port}")
    finally:
        srv.shutdown()
        srv.server_close()


def _reset(host_base):
    urllib.request.urlopen(host_base + "/reset", timeout=5).read()


def _count(host_base, route) -> int:
    data = _json.loads(urllib.request.urlopen(host_base + "/count", timeout=5).read())
    return data.get(route, 0)


def test_transient_recovers(conn, scripted):
    container, host = scripted
    # embed: 503-once-then-200 -> success after 1 retry (2 endpoint hits total).
    _reset(host)
    with conn.cursor() as cur:
        cur.execute("SET theodb.embedding_endpoint = %s", (container + "/embed/flaky",))
        cur.execute("SELECT theodb.embed('x')::text")
        vec = cur.fetchone()[0]
    assert vec.startswith("[")
    assert _count(host, "/embed/flaky") == 2

    # chat (ai.generate -> ai._chat): same transient recovery, all ai.* inherit it (DRY).
    _reset(host)
    with conn.cursor() as cur:
        cur.execute("SET theodb.llm_endpoint = %s", (container + "/chat/flaky",))
        cur.execute("SELECT ai.generate('hi')")
        assert cur.fetchone()[0] == "ok"
    assert _count(host, "/chat/flaky") == 2


def test_no_retry_on_4xx(conn, scripted):
    container, host = scripted
    _reset(host)
    with conn.cursor() as cur:
        cur.execute("SET theodb.embedding_endpoint = %s", (container + "/embed/badreq",))
        with pytest.raises(psycopg2.errors.ExternalRoutineException) as exc:
            cur.execute("SELECT theodb.embed('x')")
    # A 4xx is irrecoverable — exactly ONE hit, no retry.
    assert _count(host, "/embed/badreq") == 1
    assert "call failed" in str(exc.value)


def test_retry_respects_bounds_and_no_key_leak(conn, scripted):
    container, host = scripted
    _reset(host)
    with conn.cursor() as cur:
        cur.execute("SET theodb.embedding_endpoint = %s", (container + "/embed/down",))
        cur.execute("SET theodb.embedding_api_key = %s", ("SENTINEL_KEY_12345",))
        with pytest.raises(psycopg2.errors.ExternalRoutineException) as exc:
            cur.execute("SELECT theodb.embed('x')")
    hits = _count(host, "/embed/down")
    # Bounded: exactly 3 attempts total (1 + MAX_RETRIES=2) — a down endpoint can't hang unbounded.
    assert hits == 3
    # EC-2: the api_key must NEVER appear in the exhausted-retry error message.
    assert "SENTINEL_KEY_12345" not in str(exc.value)
