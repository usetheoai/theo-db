"""M18 tests for the 1.1->1.2 ai.* retirement migration (the plpython3u ai.* -> Rust theodb_rs move).

Runs the ACTUAL `sql/theodb--1.1--1.2.sql` delta:

  * test_upgrade_drops_plpython_ai_then_theodb_rs_installs_clean — a v0.17-shaped install (plpython3u ai._chat
    + the 4 wrappers as theodb members, with an SQL ai.generate depending on ai._chat) -> ALTER EXTENSION
    theodb UPDATE TO '1.2' must (a) re-define the keepers as plpgsql (breaking the sql dependency on ai._chat),
    (b) DROP all 5 legacy plpython3u ai.*, then (c) CREATE EXTENSION theodb_rs installs with NO clash and the
    Rust ai._chat is present. Uses a throwaway database so the real version chain runs.
  * test_owned_ai_preserved — running the delta against the shipped state (theodb_rs-owned ai.*, LANGUAGE sql
    wrappers) is a no-op for the DROP guard: the Rust ai.* survive. Rolled-back tx on the main DB.
  * test_default_version_is_current — theodb.control ships default_version = '1.3' (M19).
"""
import os

import psycopg2
import pytest

pytestmark = pytest.mark.integration

_REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
_MIGRATION = os.path.join(_REPO, "sql", "theodb--1.1--1.2.sql")


def _connect(dbname=None):
    return psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"),
        port=os.environ.get("PGPORT", "5432"),
        dbname=dbname or os.environ.get("PGDATABASE", "postgres"),
        user=os.environ.get("PGUSER", "postgres"),
        password=os.environ.get("PGPASSWORD", "postgres"),
    )


def _migration_sql():
    with open(_MIGRATION) as f:
        return f.read()


def _drop_db(admin, name):
    admin.autocommit = True
    with admin.cursor() as cur:
        cur.execute(f"DROP DATABASE IF EXISTS {name} WITH (FORCE)")


def test_upgrade_drops_plpython_ai_then_theodb_rs_installs_clean():
    admin = _connect()
    _drop_db(admin, "ai_retire")
    with admin.cursor() as cur:
        cur.execute("CREATE DATABASE ai_retire")
    admin.close()

    conn = _connect("ai_retire")
    conn.autocommit = True
    try:
        with conn.cursor() as cur:
            cur.execute("CREATE EXTENSION theodb VERSION '1.1' CASCADE")
            # M19: theodb's requires no longer pulls plpython3u via CASCADE — install it explicitly to
            # reconstruct the v0.17 state (throwaway DB, dropped in teardown).
            cur.execute("CREATE EXTENSION IF NOT EXISTS plpython3u")
            # Simulate a v0.17-shaped install: a plpython3u ai._chat + the 4 wrappers as theodb members,
            # and an SQL ai.generate that DEPENDS on ai._chat (the dependency the migration must break).
            cur.execute(
                "CREATE OR REPLACE FUNCTION ai._chat(prompt text, system text DEFAULT NULL, model text DEFAULT NULL) "
                "RETURNS text LANGUAGE plpython3u AS $py$ return 'x' $py$"
            )
            cur.execute("ALTER EXTENSION theodb ADD FUNCTION ai._chat(text,text,text)")
            for name, args in [
                ('"if"', "prompt text, model text DEFAULT NULL"),
                ("analyze_sentiment", "content text, model text DEFAULT NULL"),
                ("rank", "prompt text, model text DEFAULT NULL"),
            ]:
                cur.execute(
                    f"CREATE OR REPLACE FUNCTION ai.{name}({args}) RETURNS text "
                    f"LANGUAGE plpython3u AS $py$ return 'x' $py$"
                )
            # signature variants need the right RETURNS — recreate with correct types
            cur.execute("DROP FUNCTION ai.\"if\"(text,text); DROP FUNCTION ai.rank(text,text)")
            cur.execute("CREATE FUNCTION ai.\"if\"(prompt text, model text DEFAULT NULL) RETURNS boolean LANGUAGE plpython3u AS $py$ return True $py$")
            cur.execute("CREATE FUNCTION ai.rank(prompt text, model text DEFAULT NULL) RETURNS real LANGUAGE plpython3u AS $py$ return 0.5 $py$")
            cur.execute("CREATE FUNCTION ai.generate_batch(prompts text[], model text DEFAULT NULL) RETURNS text[] LANGUAGE plpython3u AS $py$ return prompts $py$")
            for sig in ['ai."if"(text,text)', "ai.analyze_sentiment(text,text)", "ai.rank(text,text)",
                        "ai.generate_batch(text[],text)"]:
                cur.execute(f"ALTER EXTENSION theodb ADD FUNCTION {sig}")
            # ai.generate as LANGUAGE sql depending on ai._chat (the v0.17 shape)
            cur.execute(
                "CREATE OR REPLACE FUNCTION ai.generate(prompt text, model text DEFAULT NULL) "
                "RETURNS text LANGUAGE sql VOLATILE AS $sql$ SELECT ai._chat(prompt, NULL, model) $sql$"
            )

            # sanity: the 5 plpython3u ai.* are present before the upgrade
            cur.execute("SELECT count(*) FROM pg_proc p JOIN pg_language l ON l.oid=p.prolang "
                        "WHERE p.pronamespace='ai'::regnamespace AND l.lanname='plpython3u' "
                        "AND p.proname IN ('_chat','if','analyze_sentiment','rank','generate_batch')")
            assert cur.fetchone()[0] == 5

            # The REAL upgrade path.
            cur.execute("ALTER EXTENSION theodb UPDATE TO '1.2'")

            # (a)+(b): every legacy plpython3u ai.* is retired.
            cur.execute("SELECT count(*) FROM pg_proc p JOIN pg_language l ON l.oid=p.prolang "
                        "WHERE p.pronamespace='ai'::regnamespace AND l.lanname='plpython3u' "
                        "AND p.proname IN ('_chat','if','analyze_sentiment','rank','generate_batch')")
            assert cur.fetchone()[0] == 0
            # the keeper is now plpgsql (dependency on ai._chat broken)
            cur.execute("SELECT l.lanname FROM pg_proc p JOIN pg_language l ON l.oid=p.prolang "
                        "WHERE p.pronamespace='ai'::regnamespace AND p.proname='generate'")
            assert cur.fetchone()[0] == "plpgsql"

            # (c): theodb_rs installs with NO duplicate-definition clash; the Rust ai._chat owns the slot.
            cur.execute("CREATE EXTENSION theodb_rs CASCADE")
            cur.execute("SELECT to_regprocedure('ai._chat(text,text,text)') IS NOT NULL")
            assert cur.fetchone()[0] is True
    finally:
        conn.close()
        admin = _connect()
        _drop_db(admin, "ai_retire")
        admin.close()


def test_owned_ai_preserved():
    # Running the delta against the shipped state (theodb_rs-owned ai.*, LANGUAGE sql) must NOT drop them.
    conn = _connect()
    conn.autocommit = False
    try:
        with conn.cursor() as cur:
            cur.execute("SELECT to_regprocedure('ai._chat(text,text,text)') IS NOT NULL")
            assert cur.fetchone()[0] is True  # present before
            cur.execute(_migration_sql())  # CREATE OR REPLACE keepers (idempotent) + DROP guard (no-op)
            cur.execute("SELECT to_regprocedure('ai._chat(text,text,text)') IS NOT NULL")
            assert cur.fetchone()[0] is True  # still present — the Rust ai._chat is not plpython3u
    finally:
        conn.rollback()
        conn.close()


def test_default_version_is_current():
    # M19 bumped default_version to 1.3 (nl/hybrid/import retirement); this file tests the 1.1->1.2 link.
    with open(os.path.join(_REPO, "theodb.control")) as f:
        assert "default_version = '1.3'" in f.read()
