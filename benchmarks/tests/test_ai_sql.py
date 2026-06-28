"""Contract tests for the M7-S3 generative-AI SQL functions (ai.generate/if/analyze_sentiment/summarize/rank).

Two layers (plan ADR D3):
 - OFFLINE (default, CI): a deterministic OpenAI-compatible stub (tools/chat_server.py) is the configurable
   endpoint, so each function's SQL->HTTP->parse contract is exercised with zero external calls / cost.
 - REAL (opt-in, `-k real`): runs against OpenAI only when THEODB_LLM_ENDPOINT + OPENAI_API_KEY are set
   (key from the gitignored .env); asserts shape/polarity, never exact text (LLM non-determinism). Skips
   cleanly otherwise (no silent green).

The container (PG* env vars) must ship plpython3u + sql/50 AND be started with
`--add-host=host.docker.internal:host-gateway` so the in-container function reaches the host stub.
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


# --- offline contract tests (deterministic stub) -------------------------------------------------

def test_generate_returns_text(conn, chat_server):
    with conn.cursor() as cur:
        _set_endpoint(cur, chat_server)
        cur.execute("SELECT ai.generate('describe theodb')")
        out = cur.fetchone()[0]
    # generate sends NO system prompt -> stub returns the raw canned sentence (asserts wrapper routing)
    assert isinstance(out, str) and len(out) > 0
    assert not out.startswith("A concise summary"), "generate must not route through the summarize system prompt"


def test_if_parses_boolean_true_and_false(conn, chat_server):
    with conn.cursor() as cur:
        _set_endpoint(cur, chat_server)
        cur.execute("SELECT ai.if('is the sky blue?')")
        assert cur.fetchone()[0] is True
        cur.execute("SELECT ai.if('is this not true?')")  # stub returns 'no' on 'not'
        assert cur.fetchone()[0] is False


def test_analyze_sentiment_in_label_set(conn, chat_server):
    with conn.cursor() as cur:
        _set_endpoint(cur, chat_server)
        cur.execute("SELECT ai.analyze_sentiment('this movie is great')")
        pos = cur.fetchone()[0]
        cur.execute("SELECT ai.analyze_sentiment('this movie is terrible and boring')")
        neg = cur.fetchone()[0]
    assert pos in ("positive", "negative", "neutral")
    assert neg in ("positive", "negative", "neutral")
    assert pos == "positive" and neg == "negative"


def test_summarize_returns_text(conn, chat_server):
    with conn.cursor() as cur:
        _set_endpoint(cur, chat_server)
        cur.execute("SELECT ai.summarize('a long piece of text to condense into a summary')")
        out = cur.fetchone()[0]
    # summarize sends the summarize system prompt -> stub prefixes "A concise summary" (asserts routing)
    assert out.startswith("A concise summary"), f"summarize wrapper did not route the summarize system prompt: {out!r}"


def test_rank_parses_float(conn, chat_server):
    with conn.cursor() as cur:
        _set_endpoint(cur, chat_server)
        cur.execute("SELECT ai.rank('score this review from 0 to 1')")
        out = cur.fetchone()[0]
    assert isinstance(out, float)
    assert 0.0 <= out <= 1.0


# --- negative / failure-scenario contract tests --------------------------------------------------

def test_if_malformed_output_raises_typed(conn, chat_server):
    with conn.cursor() as cur:
        _set_endpoint(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023 — D4 fail-fast
            cur.execute("SELECT ai.if('__MALFORMED__ is this true?')")


def test_rank_malformed_output_raises_typed(conn, chat_server):
    with conn.cursor() as cur:
        _set_endpoint(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023 — no number in prose
            cur.execute("SELECT ai.rank('__MALFORMED__ score this')")


def test_sentiment_malformed_output_raises_typed(conn, chat_server):
    with conn.cursor() as cur:
        _set_endpoint(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023 — out-of-set label
            cur.execute("SELECT ai.analyze_sentiment('__MALFORMED__ how do you feel')")


def test_endpoint_unset_raises_typed(conn):
    with conn.cursor() as cur:
        cur.execute("RESET theodb.llm_endpoint")
        with pytest.raises(psycopg2.errors.InvalidParameterValue) as exc:  # 22023
            cur.execute("SELECT ai.generate('x')")
    assert "llm_endpoint is not set" in str(exc.value)


def test_non_http_scheme_rejected_ssrf(conn):
    with conn.cursor() as cur:
        cur.execute("SET theodb.llm_endpoint = 'file:///etc/passwd'")
        with pytest.raises(psycopg2.errors.InvalidParameterValue) as exc:  # 22023 — SSRF guard
            cur.execute("SELECT ai.generate('x')")
    assert "http(s)" in str(exc.value)


def test_connection_refused_raises_typed(conn):
    # endpoint points at a closed port -> URLError -> 38000 (fail-fast, not silent NULL)
    with conn.cursor() as cur:
        cur.execute("SET theodb.llm_endpoint = 'http://127.0.0.1:1/v1/chat/completions'")
        with pytest.raises(psycopg2.errors.ExternalRoutineException):  # 38000
            cur.execute("SELECT ai.generate('x')")


def test_ai_functions_not_executable_by_public(conn):
    # least-privilege: outbound-HTTP functions must NOT be granted to PUBLIC
    with conn.cursor() as cur:
        for fn in ("ai.generate(text,text)", "ai.if(text,text)", "ai.analyze_sentiment(text,text)",
                   "ai.summarize(text,text)", "ai.rank(text,text)", "ai._chat(text,text,text)"):
            cur.execute("SELECT has_function_privilege('public', %s, 'execute')", (fn,))
            assert cur.fetchone()[0] is False, f"{fn} must not be PUBLIC-executable"


# --- opt-in real-OpenAI test (skips unless configured) -------------------------------------------

@pytest.mark.skipif(
    not (os.environ.get("THEODB_LLM_ENDPOINT") and os.environ.get("OPENAI_API_KEY")),
    reason="real-OpenAI test: set THEODB_LLM_ENDPOINT + OPENAI_API_KEY to enable",
)
def test_real_openai_sentiment_polarity(conn):
    with conn.cursor() as cur:
        cur.execute("SET theodb.llm_endpoint = %s", (os.environ["THEODB_LLM_ENDPOINT"],))
        cur.execute("SET theodb.llm_model = %s", (os.environ.get("THEODB_LLM_MODEL", "gpt-4o-mini"),))
        cur.execute("SET theodb.llm_api_key = %s", (os.environ["OPENAI_API_KEY"],))
        cur.execute("SELECT ai.analyze_sentiment('I absolutely loved this, it was wonderful')")
        pos = cur.fetchone()[0]
        cur.execute("SELECT ai.analyze_sentiment('This was awful, I hated every minute')")
        neg = cur.fetchone()[0]
    # assert SHAPE/polarity, never exact text (LLM non-determinism)
    assert pos in ("positive", "negative", "neutral")
    assert neg in ("positive", "negative", "neutral")
    assert pos == "positive" and neg == "negative"


# --- added in review: untested fail-fast branches + neutral label + message assertions ------------

def test_null_prompt_raises_typed(conn):
    # ai._chat NULL-prompt guard (22023) — runs before the endpoint check, no stub needed.
    with conn.cursor() as cur:
        with pytest.raises(psycopg2.errors.InvalidParameterValue) as exc:
            cur.execute("SELECT ai.generate(NULL)")
    assert "must not be NULL" in str(exc.value)


def test_empty_completion_raises_typed(conn, chat_server):
    # ai._chat empty-completion guard (38000) — stub returns "" on __EMPTY__.
    with conn.cursor() as cur:
        _set_endpoint(cur, chat_server)
        with pytest.raises(psycopg2.errors.ExternalRoutineException) as exc:
            cur.execute("SELECT ai.generate('__EMPTY__ please')")
    assert "empty completion" in str(exc.value)


def test_bad_response_shape_raises_typed(conn, chat_server):
    # ai._chat response-shape guard (38000) — stub returns {choices: []} on __BADSHAPE__.
    with conn.cursor() as cur:
        _set_endpoint(cur, chat_server)
        with pytest.raises(psycopg2.errors.ExternalRoutineException) as exc:
            cur.execute("SELECT ai.generate('__BADSHAPE__ please')")
    assert "unexpected chat response shape" in str(exc.value)


def test_sentiment_neutral_label(conn, chat_server):
    # the neutral-label path (stub returns 'neutral' on __NEUTRAL__) — previously untested.
    with conn.cursor() as cur:
        _set_endpoint(cur, chat_server)
        cur.execute("SELECT ai.analyze_sentiment('__NEUTRAL__ the story is okay')")
        assert cur.fetchone()[0] == "neutral"


def test_if_false_via_explicit_no(conn, chat_server):
    # ai.if false branch decoupled from the English-'not' heuristic (stub __NO__ -> 'no').
    with conn.cursor() as cur:
        _set_endpoint(cur, chat_server)
        cur.execute("SELECT ai.if('__NO__ is this true')")
        assert cur.fetchone()[0] is False


def test_connection_refused_message(conn):
    # 38000 connection-refused carries a distinct message (vs empty/bad-shape which also are 38000).
    with conn.cursor() as cur:
        cur.execute("SET theodb.llm_endpoint = 'http://127.0.0.1:1/v1/chat/completions'")
        with pytest.raises(psycopg2.errors.ExternalRoutineException) as exc:
            cur.execute("SELECT ai.generate('x')")
    assert "call failed" in str(exc.value)
