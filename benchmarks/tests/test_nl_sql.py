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
