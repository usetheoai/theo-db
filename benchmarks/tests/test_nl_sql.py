"""M7-S4 safe NL→SQL contract + anti-prompt-injection tests (ai.nl_to_sql / ai.nl_query).

The stub (tools/chat_server.py) "complies" with each injection on demand (magic tokens), so these tests prove
the GUARDS catch it — L2 static validation (22023 at generate-time) and/or L3 the PG-native read-only sandbox
(25006 at execute-time) — NOT that the LLM refused. Every injection test also asserts the target table is
UNCHANGED (the database is never mutated). Gated on the LLM endpoint (the S3 stub); requires the container
started with `--add-host=host.docker.internal:host-gateway`.
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
        [sys.executable, os.path.join(_REPO, "tools", "chat_server.py"), "--host", "0.0.0.0", "--port", str(port)],
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


def _setup(cur, endpoint):
    cur.execute("SET theodb.llm_endpoint = %s", (endpoint,))
    cur.execute("SET theodb.llm_model = 'stub-chat'")
    cur.execute("DROP TABLE IF EXISTS documents")
    cur.execute("CREATE TABLE documents (doc_id text PRIMARY KEY, content text)")
    cur.execute("INSERT INTO documents VALUES ('d1','postgresql database'),('d2','cooking recipes')")
    cur.execute("DROP TABLE IF EXISTS secret")
    cur.execute("CREATE TABLE secret (k text)")
    cur.execute("INSERT INTO secret VALUES ('classified')")


# --- ai.nl_to_sql: generate + static validation (L1/L2/L4) ---------------------------------------

def test_nl_to_sql_benign_returns_select(conn, chat_server):
    with conn.cursor() as cur:
        _setup(cur, chat_server)
        cur.execute("SELECT ai.nl_to_sql('how many documents are there', ARRAY['documents'])")
        sql = cur.fetchone()[0]
    assert sql.lower().startswith("select")
    assert "documents" in sql.lower()


def test_nl_to_sql_inject_drop_rejected(conn, chat_server):
    with conn.cursor() as cur:
        _setup(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023 — L2 (not SELECT / banned 'drop')
            cur.execute("SELECT ai.nl_to_sql('__NLINJECT_DROP__ drop it', ARRAY['documents'])")


def test_nl_to_sql_inject_multi_statement_rejected(conn, chat_server):
    with conn.cursor() as cur:
        _setup(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023 — L2 multi-statement
            cur.execute("SELECT ai.nl_to_sql('__NLINJECT_MULTI__', ARRAY['documents'])")


def test_nl_to_sql_inject_exfil_function_rejected(conn, chat_server):
    with conn.cursor() as cur:
        _setup(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023 — L2 banned pg_read_file
            cur.execute("SELECT ai.nl_to_sql('__NLINJECT_EXFIL__', ARRAY['documents'])")


def test_nl_to_sql_inject_non_allowlisted_relation_rejected(conn, chat_server):
    with conn.cursor() as cur:
        _setup(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023 — L4 relation allowlist (pg_authid)
            cur.execute("SELECT ai.nl_to_sql('__NLINJECT_RELATION__', ARRAY['documents'])")


def test_nl_to_sql_unplannable_query_rejected_22023(conn, chat_server):
    # L4: a generated SELECT over a NON-EXISTENT relation cannot be planned. The Rust port traps the planner
    # error (PgTryBuilder) and re-raises it as the contracted SQLSTATE 22023 "did not plan (rejected)" — parity
    # with the plpython3u try/except → plpy.error(22023). Fail-closed: the query is never executed.
    with conn.cursor() as cur:
        _setup(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue) as exc:  # 22023, not the planner's 42P01
            cur.execute("SELECT ai.nl_to_sql('__NLINJECT_NOPLAN__', ARRAY['no_such_relation_xyz'])")
        assert "did not plan" in str(exc.value)


def test_nl_to_sql_empty_allowlist_rejected(conn, chat_server):
    with conn.cursor() as cur:
        _setup(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023
            cur.execute("SELECT ai.nl_to_sql('anything', ARRAY[]::text[])")


def test_nl_to_sql_revoked_from_public(conn):
    with conn.cursor() as cur:
        cur.execute("SELECT has_function_privilege('public','ai.nl_to_sql(text,text[],text)','execute')")
        assert cur.fetchone()[0] is False


# --- ai.nl_query: read-only sandbox execution (L3) -----------------------------------------------

def test_nl_query_benign_returns_rows(conn, chat_server):
    with conn.cursor() as cur:
        _setup(cur, chat_server)
        cur.execute("SELECT ai.nl_query('how many documents', ARRAY['documents'])")
        rows = cur.fetchone()[0]
    assert isinstance(rows, list) and rows and rows[0].get("n") == 2  # SELECT count(*) AS n -> 2


def test_nl_query_inject_drop_blocked_and_table_intact(conn, chat_server):
    with conn.cursor() as cur:
        _setup(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023 — rejected before execution
            cur.execute("SELECT ai.nl_query('__NLINJECT_DROP__', ARRAY['documents'])")
    # the database is UNMODIFIED: the secret table still exists with its row
    with conn.cursor() as cur:
        cur.execute("SELECT count(*) FROM secret")
        assert cur.fetchone()[0] == 1, "secret table must be intact after a DROP injection attempt"


def test_nl_query_inject_write_blocked_and_data_intact(conn, chat_server):
    with conn.cursor() as cur:
        _setup(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023 — banned 'update'
            cur.execute("SELECT ai.nl_query('__NLINJECT_WRITE__', ARRAY['documents'])")
    with conn.cursor() as cur:
        cur.execute("SELECT content FROM documents WHERE doc_id='d1'")
        assert cur.fetchone()[0] == "postgresql database", "documents must be unmodified after a write injection"


def test_nl_query_max_rows_invalid_raises(conn, chat_server):
    with conn.cursor() as cur:
        _setup(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023
            cur.execute("SELECT ai.nl_query('how many documents', ARRAY['documents'], NULL, 0)")


def test_nl_query_revoked_from_public(conn):
    with conn.cursor() as cur:
        cur.execute("SELECT has_function_privilege('public','ai.nl_query(text,text[],text,integer)','execute')")
        assert cur.fetchone()[0] is False


def test_readonly_sandbox_blocks_write_directly(conn):
    """Prove L3 (the load-bearing backstop) independently of L2: even if a write reached the sandbox, the
    PG-native read-only transaction rejects it with SQLSTATE 25006 and the table is unchanged."""
    with conn.cursor() as cur:
        cur.execute("DROP TABLE IF EXISTS l3_probe")
        cur.execute("CREATE TABLE l3_probe (x int)")
        cur.execute("INSERT INTO l3_probe VALUES (1)")
        cur.execute("""
            DO $$
            BEGIN
              PERFORM set_config('transaction_read_only','on',true);
              BEGIN
                EXECUTE 'UPDATE l3_probe SET x = 2';
                RAISE EXCEPTION 'L3 FAILED: write succeeded in a read-only sandbox';
              EXCEPTION WHEN read_only_sql_transaction THEN
                NULL;  -- expected: SQLSTATE 25006
              END;
            END $$;
        """)
        cur.execute("SELECT x FROM l3_probe")
        assert cur.fetchone()[0] == 1, "L3 read-only sandbox must block the write (x unchanged)"


# --- review-added: read-exfil bypass coverage (L4 must catch these) + L3 end-to-end + arch fixes ----

def test_nl_to_sql_inject_comma_join_relation_rejected(conn, chat_server):
    """Comma-join exfil (SELECT ... FROM documents, secret) — the EXPLAIN-based allowlist must catch the
    second relation that a from/join regex missed (review BLOCKER)."""
    with conn.cursor() as cur:
        _setup(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023 — L4: 'secret' not allowlisted
            cur.execute("SELECT ai.nl_to_sql('__NLINJECT_COMMAJOIN__', ARRAY['documents'])")


def test_nl_to_sql_inject_quoted_identifier_rejected(conn, chat_server):
    """Quoted-identifier exfil (FROM \"secret\") — EXPLAIN resolves it as a relation; allowlist rejects."""
    with conn.cursor() as cur:
        _setup(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023 — L4
            cur.execute("SELECT ai.nl_to_sql('__NLINJECT_QUOTED__', ARRAY['documents'])")


def test_nl_to_sql_inject_stat_file_rejected(conn, chat_server):
    """No-FROM exfil function pg_stat_file — the extended denylist must catch it (EXPLAIN shows no relation)."""
    with conn.cursor() as cur:
        _setup(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023 — L2 banned
            cur.execute("SELECT ai.nl_to_sql('__NLINJECT_STATFILE__', ARRAY['documents'])")


def test_nl_query_comma_join_exfil_blocked_no_leak(conn, chat_server):
    """End-to-end: a comma-join read of `secret` via ai.nl_query is rejected (22023) — `secret` is never read."""
    with conn.cursor() as cur:
        _setup(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue):
            cur.execute("SELECT ai.nl_query('__NLINJECT_COMMAJOIN__', ARRAY['documents'])")


def test_nl_query_func_write_blocked_by_readonly_sandbox(conn, chat_server):
    """L3 PROVEN END-TO-END through ai.nl_query: a write-performing function (nextval) passes the L2 keyword
    denylist (starts with SELECT, no banned token) and is stopped ONLY by the read-only sandbox (25006),
    leaving the sequence unadvanced (review HIGH — L3 must be exercised on the real path)."""
    with conn.cursor() as cur:
        _setup(cur, chat_server)
        cur.execute("DROP SEQUENCE IF EXISTS s")
        cur.execute("CREATE SEQUENCE s")
        with pytest.raises(psycopg2.errors.ReadOnlySqlTransaction):  # 25006
            cur.execute("SELECT ai.nl_query('__NLINJECT_FUNCWRITE__', ARRAY['s'])")
    with conn.cursor() as cur:
        cur.execute("SELECT last_value, is_called FROM s")
        last_value, is_called = cur.fetchone()
        assert is_called is False, f"nextval must not have advanced the sequence (read-only blocked it): {last_value},{is_called}"


def test_nl_query_benign_cte_returns_rows(conn, chat_server):
    """A benign WITH/CTE query (allowed per L2 ^select|with) executes through the sandbox + jsonb wrap."""
    with conn.cursor() as cur:
        _setup(cur, chat_server)
        cur.execute("SELECT ai.nl_query('__NLCTE__ how many', ARRAY['documents'])")
        rows = cur.fetchone()[0]
    assert isinstance(rows, list) and rows and rows[0].get("n") == 2


def test_nl_query_generated_limit_no_syntax_error(conn, chat_server):
    """A generated SELECT that already ends in LIMIT must not produce a double-LIMIT syntax error (review LOW)."""
    with conn.cursor() as cur:
        _setup(cur, chat_server)
        cur.execute("SELECT ai.nl_query('__NLLIMIT__ list docs', ARRAY['documents'], NULL, 100)")
        rows = cur.fetchone()[0]
    assert isinstance(rows, list)  # executes cleanly; <=5 rows from the inner LIMIT


def test_nl_query_readonly_is_failsafe_in_explicit_txn(conn, chat_server):
    """Documented fail-safe contract: inside an EXPLICIT transaction, ai.nl_query marks it read-only and a
    subsequent write is BLOCKED (25006) — restrictive, never permissive (PostgreSQL forbids restoring to
    read-write after a query; SQLSTATE 25001). Normal single-statement usage has no leak (its txn ends at once)."""
    with conn.cursor() as cur:
        _setup(cur, chat_server)
    c = psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"), port=os.environ.get("PGPORT", "5432"),
        dbname=os.environ.get("PGDATABASE", "postgres"), user=os.environ.get("PGUSER", "postgres"),
        password=os.environ.get("PGPASSWORD", "postgres"),
    )
    try:
        c.autocommit = False  # explicit multi-statement transaction
        with c.cursor() as cur:
            cur.execute("SET theodb.llm_endpoint = %s", (chat_server,))
            cur.execute("SET theodb.llm_model = 'stub-chat'")
            cur.execute("SELECT ai.nl_query('how many documents', ARRAY['documents'])")
            assert cur.fetchone()[0][0]["n"] == 2
            with pytest.raises(psycopg2.errors.ReadOnlySqlTransaction):  # 25006 — fail-safe, not a vuln
                cur.execute("INSERT INTO documents VALUES ('d3','should be blocked')")
        c.rollback()
    finally:
        c.close()
    with conn.cursor() as cur:
        cur.execute("SELECT count(*) FROM documents")
        assert cur.fetchone()[0] == 2, "no row written (blocked INSERT + rollback left the table at 2)"


def test_nl_to_sql_empty_question_rejected(conn, chat_server):
    with conn.cursor() as cur:
        _setup(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023
            cur.execute("SELECT ai.nl_to_sql('   ', ARRAY['documents'])")


# --- M12: theodb_ai_nl config surface (ai.nl_config / nl_templates / nl_value_index / ai.nl_query_cfg) ----

def _setup_cfg(cur, endpoint):
    _setup(cur, endpoint)
    cur.execute("DELETE FROM ai.nl_value_index WHERE config_id = 'cfg1'")
    cur.execute("SELECT ai.nl_add_template('tmpl1', 'When asked about counts, return a COUNT query.')")
    cur.execute(
        "SELECT ai.nl_add_config('cfg1', ARRAY['documents'], 'documents(doc_id, content) holds docs.', 'tmpl1', 'stub-chat')"
    )


def test_nl_query_cfg_benign_returns_rows(conn, chat_server):
    # the config drives the query (allowed_relations from cfg1); the gate still runs -> benign count.
    with conn.cursor() as cur:
        _setup_cfg(cur, chat_server)
        cur.execute("SELECT ai.nl_query_cfg('how many documents are there', 'cfg1')")
        rows = cur.fetchone()[0]
    assert rows[0]["n"] == 2


def test_nl_query_cfg_injection_blocked_and_db_intact(conn, chat_server):
    # anti-injection PRESERVED through the config path: an injection -> 22023, target table untouched.
    with conn.cursor() as cur:
        _setup_cfg(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023 — gate rejects the DROP
            cur.execute("SELECT ai.nl_query_cfg('__NLINJECT_DROP__ delete everything', 'cfg1')")
    with conn.cursor() as cur:
        cur.execute("SELECT count(*) FROM documents")
        assert cur.fetchone()[0] == 2  # DB intact — the config layer did not weaken the gate


def test_nl_query_cfg_config_not_found_raises(conn, chat_server):
    with conn.cursor() as cur:
        _setup_cfg(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023
            cur.execute("SELECT ai.nl_query_cfg('anything', 'no_such_config')")


def test_nl_query_cfg_relation_exfil_blocked_by_l4(conn, chat_server):
    # PROVES the config's UNIQUE security job: cfg1.allowed_relations=['documents'] is forwarded to the gate's
    # L4 relation allowlist. A comma-join exfil of 'secret' (NOT in the config) must be rejected by L4 (not just
    # the L2 keyword denylist) — and 'secret' must be untouched.
    with conn.cursor() as cur:
        _setup_cfg(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023 — L4: secret not in cfg allowlist
            cur.execute("SELECT ai.nl_query_cfg('__NLINJECT_COMMAJOIN__ read secret', 'cfg1')")
    with conn.cursor() as cur:
        cur.execute("SELECT count(*) FROM secret")
        assert cur.fetchone()[0] == 1  # secret intact — config allowlist reached L4


def test_nl_query_cfg_empty_question_raises(conn, chat_server):
    with conn.cursor() as cur:
        _setup_cfg(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023
            cur.execute("SELECT ai.nl_query_cfg('   ', 'cfg1')")


def test_nl_set_template_enabled_disables_and_not_found(conn, chat_server):
    with conn.cursor() as cur:
        _setup_cfg(cur, chat_server)
        # disable the template -> nl_query_cfg still works (disabled template excluded from enrichment), benign
        cur.execute("SELECT ai.nl_set_template_enabled('tmpl1', false)")
        cur.execute("SELECT ai.nl_query_cfg('how many documents', 'cfg1')")
        assert cur.fetchone()[0][0]["n"] == 2
        cur.execute("SELECT enabled FROM ai.nl_templates WHERE template_id='tmpl1'")
        assert cur.fetchone()[0] is False
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023 — unknown template
            cur.execute("SELECT ai.nl_set_template_enabled('no_such_template', true)")


def test_nl_set_value_index_explicit_and_guards(conn, chat_server):
    with conn.cursor() as cur:
        _setup_cfg(cur, chat_server)
        cur.execute("SELECT ai.nl_set_value_index('cfg1', 'documents', 'content', ARRAY['x','y'])")
        cur.execute("SELECT values FROM ai.nl_value_index WHERE config_id='cfg1' AND column_name='content'")
        assert cur.fetchone()[0] == ["x", "y"]
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023 — NULL values
            cur.execute("SELECT ai.nl_set_value_index('cfg1', 'documents', 'content', NULL)")
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023 — unknown config
            cur.execute("SELECT ai.nl_set_value_index('nope', 'documents', 'content', ARRAY['x'])")


def test_nl_refresh_value_index_rejects_nonpositive_max(conn, chat_server):
    with conn.cursor() as cur:
        _setup_cfg(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023
            cur.execute("SELECT ai.nl_refresh_value_index('cfg1', 'documents', 'content', 0)")


def test_nl_refresh_value_index_populates_from_data(conn, chat_server):
    # auto-populate the value-index for documents.content (allowed in cfg1); stored distinct values.
    with conn.cursor() as cur:
        _setup_cfg(cur, chat_server)
        cur.execute("SELECT ai.nl_refresh_value_index('cfg1', 'documents', 'content', 50)")
        n = cur.fetchone()[0]
        assert n == 2
        cur.execute("SELECT values FROM ai.nl_value_index WHERE config_id='cfg1' AND relation='documents' AND column_name='content'")
        vals = cur.fetchone()[0]
    assert sorted(vals) == ["cooking recipes", "postgresql database"]


def test_nl_refresh_value_index_rejects_non_allowlisted_relation(conn, chat_server):
    # D3 guard: 'secret' is NOT in cfg1.allowed_relations -> refresh must reject (no arbitrary-table read).
    with conn.cursor() as cur:
        _setup_cfg(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023
            cur.execute("SELECT ai.nl_refresh_value_index('cfg1', 'secret', 'k', 50)")


def test_nl_refresh_value_index_rejects_bad_column_identifier(conn, chat_server):
    with conn.cursor() as cur:
        _setup_cfg(cur, chat_server)
        with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023 — non-identifier column
            cur.execute("SELECT ai.nl_refresh_value_index('cfg1', 'documents', 'content; DROP', 50)")


def test_nl_config_functions_revoked_from_public(conn):
    with conn.cursor() as cur:
        for fn in ("ai.nl_query_cfg(text,text,int)", "ai.nl_add_config(text,text[],text,text,text)",
                   "ai.nl_add_template(text,text)", "ai.nl_set_template_enabled(text,boolean)",
                   "ai.nl_set_value_index(text,text,text,text[])", "ai.nl_refresh_value_index(text,text,text,int)"):
            cur.execute("SELECT has_function_privilege('public', %s, 'execute')", (fn,))
            assert cur.fetchone()[0] is False, f"{fn} must not be PUBLIC-executable"


@pytest.mark.skipif(
    not (os.environ.get("THEODB_LLM_ENDPOINT") and os.environ.get("OPENAI_API_KEY")),
    reason="real-OpenAI test: set THEODB_LLM_ENDPOINT + OPENAI_API_KEY to enable",
)
def test_real_openai_nl_query_cfg(conn):
    # M12 real evidence: a registered config drives a real NL->SQL query returning rows.
    with conn.cursor() as cur:
        cur.execute("SET theodb.llm_endpoint = %s", (os.environ["THEODB_LLM_ENDPOINT"],))
        cur.execute("SET theodb.llm_model = %s", (os.environ.get("THEODB_LLM_MODEL", "gpt-4o-mini"),))
        cur.execute("SET theodb.llm_api_key = %s", (os.environ["OPENAI_API_KEY"],))
        cur.execute("DROP TABLE IF EXISTS documents")
        cur.execute("CREATE TABLE documents (doc_id text PRIMARY KEY, content text)")
        cur.execute("INSERT INTO documents VALUES ('d1','a'),('d2','b'),('d3','c')")
        cur.execute(
            "SELECT ai.nl_add_config('rcfg', ARRAY['documents'], 'documents(doc_id, content) is a table of documents.', NULL, %s)",
            (os.environ.get("THEODB_LLM_MODEL", "gpt-4o-mini"),),
        )
        cur.execute("SELECT ai.nl_query_cfg('how many documents are there?', 'rcfg')")
        rows = cur.fetchone()[0]
    assert isinstance(rows, list) and len(rows) >= 1  # a real read-only result over the configured relation
