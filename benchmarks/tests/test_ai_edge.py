"""M18 edge-case tests for the Rust ai.* parsers — the behaviors the frozen oracle (test_ai_sql.py) does
not exercise because the stub never emitted those response shapes (review finding TESTS-M18-01). New stub
seams (`__bignum__`, `__jsonnull__`, `__nullcontent__`) drive the production parse branches deterministically.

  * test_rank_no_clamp                         — ai.rank parses a value > 1 verbatim (no [0,1] clamp).
  * test_generate_batch_json_null_element      — a JSON `null` array element -> SQL NULL (preserved).
  * test_generate_batch_whole_array_null_raises — ai.generate_batch(NULL) -> typed 22023.
  * test_null_content_is_empty_completion      — content: null -> 38000 "empty completion" (parity, DOM-05).

Run against the rebuilt container started with `--add-host=host.docker.internal:host-gateway`, PG* env set.
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
def chat_server():
    port = _free_port()
    proc = subprocess.Popen(
        [sys.executable, os.path.join(_REPO, "benchmarks", "servers", "chat_server.py"), "--host", "0.0.0.0", "--port", str(port)],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    try:
        for _ in range(60):
            try:
                with urllib.request.urlopen(f"http://127.0.0.1:{port}/health", timeout=2) as r:
                    if r.status == 200:
                        break
            except OSError:
                time.sleep(0.5)
        else:
            raise RuntimeError("chat stub did not become healthy")
        yield f"http://host.docker.internal:{port}/v1/chat/completions"
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
    c.autocommit = True
    yield c
    c.close()


def _set_llm(cur, endpoint):
    cur.execute("SET theodb.llm_endpoint = %s", (endpoint,))


def test_rank_no_clamp(conn, chat_server):
    # The model returns 7 (> 1); ai.rank parses the first number verbatim — NO clamp to [0,1] (parity).
    with conn.cursor() as cur:
        _set_llm(cur, chat_server)
        cur.execute("SELECT ai.rank('__bignum__ score this')")
        assert cur.fetchone()[0] == pytest.approx(7.0)


def test_generate_batch_json_null_element(conn, chat_server):
    # A JSON null element in the model's array -> a SQL NULL element (preserved, not coerced).
    with conn.cursor() as cur:
        _set_llm(cur, chat_server)
        cur.execute("SELECT ai.generate_batch(ARRAY['__jsonnull__ a', 'b'])")
        result = cur.fetchone()[0]
    assert len(result) == 2
    assert result[0] is not None
    assert result[1] is None  # JSON null -> SQL NULL


def test_generate_batch_whole_array_null_raises(conn):
    # ai.generate_batch(NULL) -> typed 22023 (the whole-array NULL guard at the pgrx boundary).
    with conn.cursor() as cur:
        with pytest.raises(psycopg2.errors.InvalidParameterValue) as exc:
            cur.execute("SELECT ai.generate_batch(NULL::text[])")
    assert "must not be NULL" in str(exc.value)


def test_null_content_is_empty_completion(conn, chat_server):
    # content: null in the chat response -> 38000 "empty completion" (distinct from "unexpected shape").
    with conn.cursor() as cur:
        _set_llm(cur, chat_server)
        with pytest.raises(psycopg2.errors.ExternalRoutineException) as exc:
            cur.execute("SELECT ai.generate('__nullcontent__ hi')")
    assert "empty completion" in str(exc.value)
