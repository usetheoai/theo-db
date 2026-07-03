"""Contract tests for the M7-S3 generative-AI SQL functions (ai.generate/if/analyze_sentiment/summarize/rank).

Two layers (plan ADR D3):
 - OFFLINE (default, CI): a deterministic OpenAI-compatible stub (benchmarks/servers/chat_server.py) is the configurable
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
        [sys.executable, os.path.join(_REPO, "benchmarks", "servers", "chat_server.py"),
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


def test_agg_summarize_over_rows(conn, chat_server):
    # M10: the aggregate collapses many rows into one summary via ai._chat (summarize system prompt).
    with conn.cursor() as cur:
        _set_endpoint(cur, chat_server)
        cur.execute("DROP TABLE IF EXISTS it_agg")
        cur.execute("CREATE TABLE it_agg (id int, content text)")
        cur.execute("INSERT INTO it_agg VALUES (1,'first note'),(2,'second note'),(3,'third note')")
        cur.execute("SELECT ai.agg_summarize(content) FROM it_agg")
        out = cur.fetchone()[0]
    assert isinstance(out, str) and len(out) > 0
    assert out.startswith("A concise summary"), f"agg_summarize did not route the summarize system prompt: {out!r}"


def test_agg_summarize_empty_and_null_input_is_null(conn, chat_server):
    # empty group -> finalfunc gets NULL state -> NULL (no LLM call); all-NULL rows -> still NULL.
    with conn.cursor() as cur:
        _set_endpoint(cur, chat_server)
        cur.execute("SELECT ai.agg_summarize(c) FROM (SELECT NULL::text c WHERE false) z")
        assert cur.fetchone()[0] is None
        cur.execute("SELECT ai.agg_summarize(c) FROM (VALUES (NULL::text),(NULL::text)) v(c)")
        assert cur.fetchone()[0] is None


# --- M11: ai.generate_batch (accelerated — N prompts in ONE round-trip) ---------------------------

def _stub_count(chat_server: str) -> int:
    # the fixture yields the in-container URL (host.docker.internal); read /count from the host side.
    import json as _json
    url = chat_server.replace("host.docker.internal", "127.0.0.1").replace("/v1/chat/completions", "/count")
    with urllib.request.urlopen(url, timeout=5) as r:
        return _json.loads(r.read())["count"]


def test_generate_batch_one_roundtrip(conn, chat_server):
    # M11 core: a batch of N prompts is ONE HTTP round-trip and returns N answers IN ORDER.
    before = _stub_count(chat_server)
    with conn.cursor() as cur:
        _set_endpoint(cur, chat_server)
        cur.execute("SELECT ai.generate_batch(ARRAY['first','second','third'])")
        res = cur.fetchone()[0]
    after = _stub_count(chat_server)
    assert after - before == 1, f"batch must be ONE round-trip, got {after - before}"
    assert isinstance(res, list) and len(res) == 3
    assert res == ["answer to item 1", "answer to item 2", "answer to item 3"]  # order preserved


def test_scalar_generate_is_n_roundtrips(conn, chat_server):
    # contrast: N scalar ai.generate calls are N round-trips (this is what batch accelerates).
    before = _stub_count(chat_server)
    with conn.cursor() as cur:
        _set_endpoint(cur, chat_server)
        for p in ("a", "b", "c"):
            cur.execute("SELECT ai.generate(%s)", (p,))
    after = _stub_count(chat_server)
    assert after - before == 3, f"3 scalar calls must be 3 round-trips, got {after - before}"


def test_generate_batch_empty_makes_no_call(conn, chat_server):
    before = _stub_count(chat_server)
    with conn.cursor() as cur:
        _set_endpoint(cur, chat_server)
        cur.execute("SELECT ai.generate_batch(ARRAY[]::text[])")
        res = cur.fetchone()[0]
    after = _stub_count(chat_server)
    assert res == []  # empty in -> empty out
    assert after - before == 0  # NO LLM call


def test_generate_batch_null_element_raises_typed(conn, chat_server):
    with conn.cursor() as cur:
        _set_endpoint(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023 — alignment contract
            cur.execute("SELECT ai.generate_batch(ARRAY['a', NULL])")


def test_generate_batch_wrong_length_raises_typed(conn, chat_server):
    # stub seam __wronglen__ returns N-1 items -> the len!=N guard fails fast (no silent misalignment).
    with conn.cursor() as cur:
        _set_endpoint(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023
            cur.execute("SELECT ai.generate_batch(ARRAY['__wronglen__ a','b','c'])")


def test_generate_batch_invalid_json_raises_typed(conn, chat_server):
    # stub seam __malformed__ returns prose (not JSON) -> the JSON-parse guard fails fast (22023).
    with conn.cursor() as cur:
        _set_endpoint(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue) as exc:
            cur.execute("SELECT ai.generate_batch(ARRAY['__malformed__ a','b'])")
    assert "valid JSON" in str(exc.value)


def test_generate_batch_non_string_element_raises_typed(conn, chat_server):
    # stub seam __nonstr__ returns a JSON array of NUMBERS -> the function must fail fast (22023),
    # never silently coerce 4 -> "4" / {"a":1} -> "{'a': 1}" into a plausible-but-wrong cell.
    with conn.cursor() as cur:
        _set_endpoint(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue) as exc:
            cur.execute("SELECT ai.generate_batch(ARRAY['__nonstr__ a','b'])")
    assert "non-string" in str(exc.value)


def test_generate_batch_strips_json_fence(conn, chat_server):
    # stub seam __fenced__ wraps the array in a ```json fence -> the function must strip it and parse N items.
    with conn.cursor() as cur:
        _set_endpoint(cur, chat_server)
        cur.execute("SELECT ai.generate_batch(ARRAY['__fenced__ a','b'])")
        res = cur.fetchone()[0]
    assert isinstance(res, list) and len(res) == 2


def test_generate_batch_embedded_numbered_line_still_one_batch(conn, chat_server):
    # a prompt that itself contains a "2. ..." line must NOT inflate N (stub sizes from the declared N).
    with conn.cursor() as cur:
        _set_endpoint(cur, chat_server)
        cur.execute("SELECT ai.generate_batch(ARRAY[E'list:\n1. foo\n2. bar', 'second'])")
        res = cur.fetchone()[0]
    assert isinstance(res, list) and len(res) == 2  # 2 prompts -> 2 answers, embedded numbering ignored


def test_stub_counter_is_threadsafe(chat_server):
    # concurrent reliability test: K parallel chat requests must bump the lock-guarded counter EXACTLY K
    # times (no lost updates), so the round-trip measurement above is trustworthy. The Lock guards the
    # read-modify-write of `_count["n"]` (a += is several bytecodes; the GIL does not make it atomic).
    import concurrent.futures
    import json as _json
    chat_url = chat_server.replace("host.docker.internal", "127.0.0.1")
    before = _stub_count(chat_server)
    K = 300

    def _hit(_):
        data = _json.dumps({"model": "stub", "messages": [{"role": "user", "content": "ping"}]}).encode()
        req = urllib.request.Request(chat_url, data=data, headers={"Content-Type": "application/json"})
        with urllib.request.urlopen(req, timeout=10) as r:
            return r.status

    with concurrent.futures.ThreadPoolExecutor(max_workers=16) as ex:
        statuses = list(ex.map(_hit, range(K)))
    after = _stub_count(chat_server)
    assert all(s == 200 for s in statuses)
    assert after - before == K, f"counter lost updates under concurrency: {after - before} != {K}"


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
                   "ai.summarize(text,text)", "ai.rank(text,text)", "ai._chat(text,text,text)",
                   # M10 aggregate + its support functions (same outbound-HTTP least-privilege posture)
                   "ai.agg_summarize(text)", "ai._agg_summ_accum(text,text)", "ai._agg_summ_final(text)"):
            cur.execute("SELECT has_function_privilege('public', %s, 'execute')", (fn,))
            assert cur.fetchone()[0] is False, f"{fn} must not be PUBLIC-executable"


def test_agg_summarize_finalfunc_is_volatile(conn):
    # M10 review: PostgreSQL gives EVERY aggregate provolatile='i' (string_agg/array_agg/sum are all 'i';
    # no aggregate can be VOLATILE). The real guarantee that the paid LLM call is never optimized away is
    # that the FINALFUNC is VOLATILE (the executor re-runs it per query). Guard that, not the aggregate.
    with conn.cursor() as cur:
        cur.execute(
            "SELECT proname, provolatile FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace "
            "WHERE n.nspname='ai' AND p.proname IN ('agg_summarize','_agg_summ_final') ORDER BY proname"
        )
        vol = dict(cur.fetchall())
    assert vol["_agg_summ_final"] == "v", "ai._agg_summ_final must be VOLATILE (it performs the LLM call)"
    assert vol["agg_summarize"] == "i", "PG aggregates are provolatile='i' by design (like string_agg)"


def test_agg_summarize_skips_null_and_empty_rows(conn, chat_server):
    # M10 review: mixed NULL/empty + non-NULL group -> one summary (accum NULL/empty-skip branch);
    # and an all-empty-string group short-circuits to NULL with NO LLM call (symmetry with all-NULL).
    with conn.cursor() as cur:
        _set_endpoint(cur, chat_server)
        cur.execute("SELECT ai.agg_summarize(c) FROM (VALUES ('a'),(NULL::text),(''),('b')) v(c)")
        out = cur.fetchone()[0]
        assert isinstance(out, str) and out.startswith("A concise summary")
        cur.execute("SELECT ai.agg_summarize(c) FROM (VALUES (''::text),('')) v(c)")
        assert cur.fetchone()[0] is None  # all-empty -> NULL, no LLM call


def test_agg_summarize_propagates_empty_completion_typed(conn, chat_server):
    # M10 review: a failure in the per-group LLM call propagates ai._chat's typed error through the aggregate.
    with conn.cursor() as cur:
        _set_endpoint(cur, chat_server)
        with pytest.raises(psycopg2.errors.ExternalRoutineException):  # 38000 — empty completion via finalfunc
            cur.execute("SELECT ai.agg_summarize(c) FROM (VALUES ('__EMPTY__ please summarize')) v(c)")


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


@pytest.mark.skipif(
    not (os.environ.get("THEODB_LLM_ENDPOINT") and os.environ.get("OPENAI_API_KEY")),
    reason="real-OpenAI test: set THEODB_LLM_ENDPOINT + OPENAI_API_KEY to enable",
)
def test_real_openai_agg_summarize_shape(conn):
    # M10 real evidence: aggregate N rows -> one non-empty summary (shape only; LLM non-determinism).
    with conn.cursor() as cur:
        cur.execute("SET theodb.llm_endpoint = %s", (os.environ["THEODB_LLM_ENDPOINT"],))
        cur.execute("SET theodb.llm_model = %s", (os.environ.get("THEODB_LLM_MODEL", "gpt-4o-mini"),))
        cur.execute("SET theodb.llm_api_key = %s", (os.environ["OPENAI_API_KEY"],))
        cur.execute("DROP TABLE IF EXISTS it_agg_real")
        cur.execute("CREATE TABLE it_agg_real (id int, content text)")
        cur.execute(
            "INSERT INTO it_agg_real VALUES "
            "(1,'The deployment failed because the database ran out of disk space.'),"
            "(2,'A second outage was caused by an expired TLS certificate.'),"
            "(3,'The team added monitoring alerts for disk usage and certificate expiry.')"
        )
        cur.execute("SELECT ai.agg_summarize(content) FROM it_agg_real")
        out = cur.fetchone()[0]
    assert isinstance(out, str) and len(out) > 0  # a real, non-empty summary of the 3 incident notes


@pytest.mark.skipif(
    not (os.environ.get("THEODB_LLM_ENDPOINT") and os.environ.get("OPENAI_API_KEY")),
    reason="real-OpenAI test: set THEODB_LLM_ENDPOINT + OPENAI_API_KEY to enable",
)
def test_real_openai_generate_batch_shape(conn):
    # M11 real evidence: N prompts -> N answers in ONE round-trip (shape only; LLM non-determinism).
    with conn.cursor() as cur:
        cur.execute("SET theodb.llm_endpoint = %s", (os.environ["THEODB_LLM_ENDPOINT"],))
        cur.execute("SET theodb.llm_model = %s", (os.environ.get("THEODB_LLM_MODEL", "gpt-4o-mini"),))
        cur.execute("SET theodb.llm_api_key = %s", (os.environ["OPENAI_API_KEY"],))
        cur.execute(
            "SELECT ai.generate_batch(ARRAY["
            "'Capital of France? one word','2+2? a number only','Opposite of hot? one word'])"
        )
        res = cur.fetchone()[0]
    assert isinstance(res, list) and len(res) == 3  # exactly N answers, real model, one request
    assert all(isinstance(x, str) and len(x) > 0 for x in res)


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
