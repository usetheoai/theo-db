"""M13 theodb_ml model registry tests — split from test_ai_sql.py (file-size budget).

Same fixtures (conn + deterministic chat stub) as test_ai_sql.py; the registry bridges to the unchanged
ai._chat via session GUCs (apply_model). REAL test is opt-in (THEODB_LLM_ENDPOINT + OPENAI_API_KEY).
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
        [sys.executable, os.path.join(_REPO, "tools", "chat_server.py"),
         "--host", "0.0.0.0", "--port", str(port)],
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
            raise RuntimeError("chat stub server did not become healthy")
        # the container reaches the host via host.docker.internal (host-gateway)
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


def _set_endpoint(cur, endpoint: str) -> None:
    cur.execute("SET theodb.llm_endpoint = %s", (endpoint,))
    cur.execute("SET theodb.llm_model = 'stub-chat'")


# --- M13: theodb_ml model registry (create_model / apply_model bridges to the unchanged ai._chat) ---------

def test_theodb_ml_create_apply_drives_generate(conn, chat_server):
    # register a model pointing at the stub, apply it (SETs the session GUCs ai._chat reads), then ai.generate
    # works using the applied endpoint — proves the registry -> ai._chat bridge with NO change to ai._chat.
    with conn.cursor() as cur:
        cur.execute("SELECT theodb_ml.create_model('stubm', %s, 'stub-chat')", (chat_server,))
        cur.execute("SELECT theodb_ml.apply_model('stubm')")
        cur.execute("SELECT current_setting('theodb.llm_endpoint', true)")
        assert cur.fetchone()[0] == chat_server  # apply_model set the GUC
        cur.execute("SET theodb.llm_api_key = 'x'")  # key is set separately (never persisted)
        cur.execute("SELECT ai.generate('describe theodb')")
        out = cur.fetchone()[0]
    assert isinstance(out, str) and len(out) > 0


def test_theodb_ml_registry_has_no_api_key_column(conn):
    # SECURITY (ADR D2): keys must NEVER be persisted in the registry table.
    with conn.cursor() as cur:
        cur.execute(
            "SELECT count(*) FROM information_schema.columns "
            "WHERE table_schema='theodb_ml' AND table_name='models' AND column_name ILIKE '%key%'"
        )
        assert cur.fetchone()[0] == 0


def test_theodb_ml_create_model_rejects_non_http_endpoint(conn):
    with conn.cursor() as cur:
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023 — SSRF scheme guard
            cur.execute("SELECT theodb_ml.create_model('bad', 'file:///etc/passwd', 'm')")


def test_theodb_ml_apply_unknown_model_raises(conn):
    with conn.cursor() as cur:
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023
            cur.execute("SELECT theodb_ml.apply_model('does_not_exist')")


def test_theodb_ml_drop_and_list(conn, chat_server):
    with conn.cursor() as cur:
        cur.execute("SELECT theodb_ml.create_model('tmpm', %s, 'stub-chat')", (chat_server,))
        cur.execute("SELECT count(*) FROM theodb_ml.list_models() WHERE model_id='tmpm'")
        assert cur.fetchone()[0] == 1
        cur.execute("SELECT theodb_ml.drop_model('tmpm')")
        cur.execute("SELECT count(*) FROM theodb_ml.list_models() WHERE model_id='tmpm'")
        assert cur.fetchone()[0] == 0
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023 — drop unknown
            cur.execute("SELECT theodb_ml.drop_model('tmpm')")


def test_theodb_ml_functions_revoked_from_public(conn):
    with conn.cursor() as cur:
        for fn in ("theodb_ml.create_model(text,text,text)", "theodb_ml.drop_model(text)",
                   "theodb_ml.list_models()", "theodb_ml.apply_model(text)", "ai.hybrid_search(jsonb)"):
            cur.execute("SELECT has_function_privilege('public', %s, 'execute')", (fn,))
            assert cur.fetchone()[0] is False, f"{fn} must not be PUBLIC-executable"


@pytest.mark.skipif(
    not (os.environ.get("THEODB_LLM_ENDPOINT") and os.environ.get("OPENAI_API_KEY")),
    reason="real-OpenAI test: set THEODB_LLM_ENDPOINT + OPENAI_API_KEY to enable",
)
def test_real_openai_theodb_ml_apply_model(conn):
    # M13 real evidence: a registered real model applied via apply_model drives a real ai.generate call.
    with conn.cursor() as cur:
        cur.execute(
            "SELECT theodb_ml.create_model('openai', %s, %s)",
            (os.environ["THEODB_LLM_ENDPOINT"], os.environ.get("THEODB_LLM_MODEL", "gpt-4o-mini")),
        )
        cur.execute("SELECT theodb_ml.apply_model('openai')")
        cur.execute("SET theodb.llm_api_key = %s", (os.environ["OPENAI_API_KEY"],))
        cur.execute("SELECT ai.generate('Reply with the single word: ok')")
        out = cur.fetchone()[0]
    assert isinstance(out, str) and len(out) > 0  # real model via the registry-applied endpoint
