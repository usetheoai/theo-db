"""Audit-remediation tests for the 1.0->1.1 retirement migration (T4.1, audit #9).

Runs the ACTUAL `sql/theodb--1.0--1.1.sql` delta (read from disk, not a copy) against two simulated
states, each inside a transaction that is ROLLED BACK (non-destructive):

  * test_upgrade_drops_plpython_embed — simulate an existing v0.x install: theodb_rs absent, a legacy
    plpython3u theodb.embed present. The delta must DROP it (so a later CREATE EXTENSION theodb_rs won't
    clash).
  * test_owned_embed_preserved        — the real shipped state: theodb.embed is the Rust (LANGUAGE sql)
    wrapper owned by theodb_rs. The delta must NOT drop it (the guard spares the Rust one).
  * test_default_version_is_1_1       — theodb.control ships default_version = '1.1' (static check).

Run against a rebuilt container started with `--add-host=host.docker.internal:host-gateway`, PG* env set.
"""
import os

import psycopg2
import pytest

pytestmark = pytest.mark.integration

_REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
_MIGRATION = os.path.join(_REPO, "sql", "theodb--1.0--1.1.sql")


def _connect():
    return psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"),
        port=os.environ.get("PGPORT", "5432"),
        dbname=os.environ.get("PGDATABASE", "postgres"),
        user=os.environ.get("PGUSER", "postgres"),
        password=os.environ.get("PGPASSWORD", "postgres"),
    )


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


def test_default_version_is_1_1():
    # theodb.control must ship default_version = '1.1' so fresh installs + UPDATE land on the retired state.
    with open(os.path.join(_REPO, "theodb.control")) as f:
        control = f.read()
    assert "default_version = '1.1'" in control
