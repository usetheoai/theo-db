"""M15 — CREATE EXTENSION theodb install/upgrade integration tests.

Proves the productization milestone: the AI surface installs as a real, versioned,
upgradeable PostgreSQL extension (not init-scripts). The DB tests require a container whose
extension dir contains theodb.control + sql/theodb--*.sql (the M15 image, or a theo-db:dev
container with those files copied in). Connection via PGHOST/PGPORT (same as the other tests).

The control-parse + extension-safety tests are pure-file and run anywhere (no DB).
"""

import os
import pathlib

import psycopg2
import pytest

ROOT = pathlib.Path(__file__).resolve().parents[2]
CONTROL = ROOT / "theodb.control"
INSTALL_SQL = ROOT / "sql" / "theodb--1.0.sql"  # generated (gitignored); built by `make theodb-build`


# ---- pure-file tests (no DB) ------------------------------------------------

def test_control_declares_requires():
    """theodb.control declares vector, vectorscale AND plpython3u, superuser, no module_pathname."""
    text = CONTROL.read_text()
    assert "requires = 'vector, vectorscale, plpython3u'" in text
    assert "superuser = true" in text
    assert "relocatable = false" in text
    assert "module_pathname" not in text  # SQL-only: no .so (M15 ADR D1)
    assert "trusted" not in text          # untrusted plpython3u -> not trusted (M15 ADR D2)


def test_built_script_is_extension_safe():
    """The built install script has no top-level transaction control and no CREATE EXTENSION."""
    if not INSTALL_SQL.exists():
        pytest.skip("run `make theodb-build` first to generate sql/theodb--1.0.sql")
    lines = INSTALL_SQL.read_text().splitlines()
    for ln in lines:
        s = ln.strip().upper()
        assert not s.startswith("CREATE EXTENSION"), f"forbidden in ext script: {ln}"
        assert s not in ("BEGIN;", "COMMIT;", "ROLLBACK;", "START TRANSACTION;"), f"tx control: {ln}"


# ---- DB integration tests ---------------------------------------------------

@pytest.fixture(scope="module")
def admin_conn():
    """Autocommit connection to the maintenance DB, for CREATE/DROP DATABASE."""
    c = psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"),
        port=os.environ.get("PGPORT", "5432"),
        user=os.environ.get("PGUSER", "postgres"),
        password=os.environ.get("PGPASSWORD", "postgres"),
        dbname="postgres",
    )
    c.autocommit = True
    yield c
    c.close()


def _fresh_db(admin_conn, name):
    with admin_conn.cursor() as cur:
        cur.execute(f"DROP DATABASE IF EXISTS {name}")
        cur.execute(f"CREATE DATABASE {name}")


def _connect(name):
    return psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"),
        port=os.environ.get("PGPORT", "5432"),
        user=os.environ.get("PGUSER", "postgres"),
        password=os.environ.get("PGPASSWORD", "postgres"),
        dbname=name,
    )


def test_extension_installs_full_surface(admin_conn):
    """CREATE EXTENSION theodb CASCADE on a fresh DB installs >=24 functions across ai/theodb/theodb_ml."""
    _fresh_db(admin_conn, "m15_surface")
    conn = _connect("m15_surface")
    try:
        with conn.cursor() as cur:
            cur.execute("CREATE EXTENSION theodb CASCADE")
            cur.execute(
                "SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace "
                "WHERE n.nspname IN ('ai','theodb','theodb_ml')"
            )
            assert cur.fetchone()[0] >= 24
            cur.execute("SELECT extversion FROM pg_extension WHERE extname='theodb'")
            assert cur.fetchone()[0] == "1.0"
            # requires resolved via CASCADE
            cur.execute(
                "SELECT count(*) FROM pg_extension WHERE extname IN ('vector','vectorscale','plpython3u')"
            )
            assert cur.fetchone()[0] == 3
    finally:
        conn.close()


def test_upgrade_path_1_0_to_1_1(admin_conn):
    """ALTER EXTENSION theodb UPDATE TO '1.1' chains the upgrade script."""
    _fresh_db(admin_conn, "m15_upgrade")
    conn = _connect("m15_upgrade")
    try:
        with conn.cursor() as cur:
            cur.execute("CREATE EXTENSION theodb VERSION '1.0' CASCADE")
            cur.execute("ALTER EXTENSION theodb UPDATE TO '1.1'")
            cur.execute("SELECT extversion FROM pg_extension WHERE extname='theodb'")
            assert cur.fetchone()[0] == "1.1"
    finally:
        conn.close()


def test_create_extension_is_idempotent(admin_conn):
    """CREATE EXTENSION IF NOT EXISTS theodb CASCADE twice yields exactly one extension row."""
    _fresh_db(admin_conn, "m15_idem")
    conn = _connect("m15_idem")
    try:
        with conn.cursor() as cur:
            cur.execute("CREATE EXTENSION IF NOT EXISTS theodb CASCADE")
            conn.commit()
            cur.execute("CREATE EXTENSION IF NOT EXISTS theodb CASCADE")
            conn.commit()
            cur.execute("SELECT count(*) FROM pg_extension WHERE extname='theodb'")
            assert cur.fetchone()[0] == 1
    finally:
        conn.close()


def test_create_without_cascade_errors_clearly(admin_conn):
    """Without CASCADE on a bare DB, CREATE EXTENSION theodb fails with a typed missing-dependency error."""
    _fresh_db(admin_conn, "m15_bare")
    conn = _connect("m15_bare")
    try:
        with conn.cursor() as cur:
            with pytest.raises(psycopg2.errors.UndefinedObject) as exc:
                cur.execute("CREATE EXTENSION theodb")  # no CASCADE, no deps
            assert "vector" in str(exc.value).lower()
    finally:
        conn.close()
