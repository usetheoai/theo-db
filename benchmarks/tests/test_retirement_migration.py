"""Audit-remediation tests for the 1.0->1.1 retirement migration (T4.1, audit #9).

Runs the ACTUAL `sql/theodb--1.0--1.1.sql` delta (read from disk, not a copy) against two simulated
states, each inside a transaction that is ROLLED BACK (non-destructive):

  * test_upgrade_drops_plpython_embed — simulate an existing v0.x install: theodb_rs absent, a legacy
    plpython3u theodb.embed present. The delta must DROP it (so a later CREATE EXTENSION theodb_rs won't
    clash).
  * test_owned_embed_preserved        — the real shipped state: theodb.embed is the Rust (LANGUAGE sql)
    wrapper owned by theodb_rs. The delta must NOT drop it (the guard spares the Rust one).
  * test_default_version_is_current   — theodb.control ships default_version = '1.3' (M19; static check).

Run against a rebuilt container started with `--add-host=host.docker.internal:host-gateway`, PG* env set.
"""
import os

import psycopg2
import pytest

pytestmark = pytest.mark.integration

_REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
_MIGRATION = os.path.join(_REPO, "sql", "theodb--1.0--1.1.sql")


def _connect(dbname=None):
    return psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"),
        port=os.environ.get("PGPORT", "5432"),
        dbname=dbname or os.environ.get("PGDATABASE", "postgres"),
        user=os.environ.get("PGUSER", "postgres"),
        password=os.environ.get("PGPASSWORD", "postgres"),
    )


def _fresh_db(admin, name):
    admin.autocommit = True
    with admin.cursor() as cur:
        cur.execute(f"DROP DATABASE IF EXISTS {name} WITH (FORCE)")
        cur.execute(f"CREATE DATABASE {name}")


def _migration_sql() -> str:
    with open(_MIGRATION) as f:
        return f.read()


def test_upgrade_drops_plpython_embed():
    # Simulate a v0.x install (theodb_rs absent, legacy plpython3u embed present) and run the real delta.
    conn = _connect()
    conn.autocommit = False
    try:
        with conn.cursor() as cur:
            cur.execute("DROP EXTENSION theodb_rs")  # remove the Rust theodb.embed (+ embed_batch)
            # M19: theodb no longer requires plpython3u, so the language is not auto-installed — create it
            # explicitly to simulate a v0.x install that had it (rolled back at teardown; non-destructive).
            cur.execute("CREATE EXTENSION IF NOT EXISTS plpython3u")
            cur.execute(
                "CREATE FUNCTION theodb.embed(content text, model text DEFAULT NULL) "
                "RETURNS text LANGUAGE plpython3u AS $py$ return '[0,0]' $py$"
            )
            # sanity: the legacy plpython3u embed exists before the migration
            cur.execute("SELECT to_regprocedure('theodb.embed(text,text)') IS NOT NULL")
            assert cur.fetchone()[0] is True

            cur.execute(_migration_sql())  # the ACTUAL 1.0->1.1 retirement delta

            cur.execute("SELECT to_regprocedure('theodb.embed(text,text)')")
            assert cur.fetchone()[0] is None  # the stale plpython3u embed was dropped
    finally:
        conn.rollback()  # restore theodb_rs + its theodb.embed
        conn.close()


def test_owned_embed_preserved():
    # The real shipped state: theodb.embed is the Rust (LANGUAGE sql) wrapper owned by theodb_rs.
    # The delta's guard (plpython3u AND NOT theodb_rs-owned) must NOT match it.
    conn = _connect()
    conn.autocommit = False
    try:
        with conn.cursor() as cur:
            cur.execute("SELECT to_regprocedure('theodb.embed(text,text)') IS NOT NULL")
            assert cur.fetchone()[0] is True  # present before

            cur.execute(_migration_sql())  # no-op for the Rust-owned embed

            cur.execute("SELECT to_regprocedure('theodb.embed(text,text)') IS NOT NULL")
            assert cur.fetchone()[0] is True  # still present — not dropped
    finally:
        conn.rollback()
        conn.close()


def test_real_upgrade_path_drops_member_embed_then_theodb_rs_installs_clean():
    """End-to-end proof of the audit #9 claim, on the REAL upgrade path (not a bare delta run):

    a v0.x-shaped install where the plpython3u theodb.embed is a `theodb` MEMBER -> ALTER EXTENSION theodb
    UPDATE TO '1.1' (runs the actual delta in extension-update context) drops it -> CREATE EXTENSION
    theodb_rs then installs WITHOUT a duplicate-definition clash and the Rust theodb.embed is present.
    Uses a throwaway database so the real version chain (1.0 -> UPDATE 1.1) can be exercised.
    """
    admin = _connect()
    _fresh_db(admin, "retire_upgrade")
    admin.close()
    conn = _connect("retire_upgrade")
    conn.autocommit = True
    try:
        with conn.cursor() as cur:
            cur.execute("CREATE EXTENSION theodb VERSION '1.0' CASCADE")
            # M19: theodb's requires no longer pulls plpython3u via CASCADE — install it explicitly to
            # reconstruct the v0.x state where the legacy plpython3u embed existed (throwaway DB, teardown drops).
            cur.execute("CREATE EXTENSION IF NOT EXISTS plpython3u")
            # Seed the legacy plpython3u embed AS A theodb MEMBER (mirrors a real v0.x install).
            cur.execute(
                "CREATE FUNCTION theodb.embed(content text, model text DEFAULT NULL) "
                "RETURNS text LANGUAGE plpython3u AS $py$ return '[0,0]' $py$"
            )
            cur.execute("ALTER EXTENSION theodb ADD FUNCTION theodb.embed(text, text)")
            # The real upgrade path runs the delta in extension-update context (can drop its own member).
            cur.execute("ALTER EXTENSION theodb UPDATE TO '1.1'")
            cur.execute("SELECT to_regprocedure('theodb.embed(text,text)')")
            assert cur.fetchone()[0] is None  # legacy plpython3u embed retired on UPDATE
            # And now theodb_rs installs with NO duplicate-definition clash — the load-bearing claim.
            cur.execute("CREATE EXTENSION theodb_rs CASCADE")
            cur.execute("SELECT to_regprocedure('theodb.embed(text,text)') IS NOT NULL")
            assert cur.fetchone()[0] is True  # the Rust theodb.embed now owns the slot
    finally:
        conn.close()
        admin = _connect()
        admin.autocommit = True
        with admin.cursor() as cur:
            cur.execute("DROP DATABASE IF EXISTS retire_upgrade WITH (FORCE)")
        admin.close()


def test_default_version_is_current():
    # theodb.control ships default_version = '1.3' (M19) so fresh installs + UPDATE land on the fully-retired
    # state (the M17 1.0->1.1 embed delta tested here is one link in the 1.0->1.1->1.2->1.3 chain).
    with open(os.path.join(_REPO, "theodb.control")) as f:
        control = f.read()
    assert "default_version = '1.3'" in control
