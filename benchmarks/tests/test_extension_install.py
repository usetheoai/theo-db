"""M15 — CREATE EXTENSION theodb install/upgrade integration tests.

Proves the productization milestone: the AI surface installs as a real, versioned,
upgradeable PostgreSQL extension (not init-scripts). The DB tests require a container whose
extension dir contains theodb.control + sql/theodb--*.sql (the M15 image, or a theo-db:dev
container with those files copied in). Connection via PGHOST/PGPORT (same as the other tests).

The control-parse + extension-safety tests are pure-file and run anywhere (no DB).
"""

import os
import pathlib
import re

import psycopg2
import pytest

ROOT = pathlib.Path(__file__).resolve().parents[2]
CONTROL = ROOT / "theodb.control"
INSTALL_SQL = ROOT / "sql" / "theodb--1.0.sql"          # generated (gitignored); built by `make theodb-build`
UPGRADE_SQL = ROOT / "sql" / "theodb--1.0--1.1.sql"     # committed (upgrade-path skeleton)
# Source bodies concatenated (in load order) to build the install script — mirrors the Makefile PARTS.
PARTS = [
    "30-theodb-embed.sql", "40-theodb-hybrid.sql", "50-theodb-ai.sql",
    "60-theodb-nl.sql", "61-theodb-nl-config.sql", "70-theodb-ml.sql",
]
# Top-level transaction control is forbidden inside an extension script (PG docs). Regex (not a literal
# set) so `BEGIN WORK;`, `BEGIN ;` (space), and `COMMIT; -- note` are all caught — plpgsql `BEGIN`/`END`
# inside `$$ ... $$` bodies have no trailing `;` on the keyword and are NOT matched.
_TX_CONTROL = re.compile(r"^\s*(BEGIN|COMMIT|START\s+TRANSACTION|ROLLBACK)\b[^;]*;", re.IGNORECASE)
# The documented surface that MUST be present (presence-by-name, not a loose count).
# Post-M17 (v0.16.0) theodb.embed + theodb.embed_batch are served by the Rust `theodb_rs` extension, NOT
# the SQL `theodb` extension — so the full documented surface requires installing BOTH (the product ships
# both; the Dockerfile init creates theodb + theodb_rs). embed_batch + import_pinecone_chunked are the
# audit-remediation additions.
_REQUIRED_FUNCS = [
    ("theodb", "embed"), ("theodb", "embed_batch"),
    ("theodb", "import_pinecone"), ("theodb", "import_pinecone_chunked"),
    ("ai", "generate"), ("ai", "analyze_sentiment"), ("ai", "summarize"),
    ("ai", "rank"), ("ai", "generate_batch"), ("ai", "agg_summarize"), ("ai", "hybrid_search"),
    ("ai", "nl_to_sql"), ("ai", "nl_query"), ("theodb_ml", "create_model"), ("theodb_ml", "apply_model"),
]


def _build_install_script_text():
    """Return the assembled install-script text (built from the source bodies if the generated file
    is absent) — so the safety scan NEVER silently skips (the file is gitignored)."""
    if INSTALL_SQL.exists():
        return INSTALL_SQL.read_text()
    return "\n".join((ROOT / "sql" / p).read_text() for p in PARTS)


def _assert_extension_safe(text, label):
    for ln in text.splitlines():
        assert not ln.strip().upper().startswith("CREATE EXTENSION"), f"{label}: forbidden CREATE EXTENSION: {ln}"
        assert not _TX_CONTROL.match(ln), f"{label}: forbidden top-level transaction control: {ln}"


# ---- pure-file tests (no DB) ------------------------------------------------

def test_control_declares_requires():
    """theodb.control declares vector, vectorscale (NO plpython3u since M19 — last plpython3u retired),
    superuser, not-trusted, no module_pathname."""
    text = CONTROL.read_text()
    assert "requires = 'vector, vectorscale'" in text  # M19: plpython3u dropped from requires (100% Rust surface)
    assert "plpython3u" not in text                    # no plpython3u dependency remains
    assert "superuser = true" in text
    assert "relocatable = false" in text
    assert "module_pathname" not in text   # SQL-only: no .so (M15 ADR D1)
    assert "trusted = true" not in text     # SQL umbrella stays untrusted/superuser (M15 ADR D2)


def test_built_script_is_extension_safe():
    """The assembled install script has no top-level transaction control and no CREATE EXTENSION.

    Builds the script in-memory if the generated file is absent, so this gate never no-ops (M-review fix)."""
    _assert_extension_safe(_build_install_script_text(), "install script")


def test_upgrade_script_is_extension_safe():
    """The committed upgrade script (theodb--1.0--1.1.sql) is also extension-safe (T3.1 AC; review M2 fix)."""
    _assert_extension_safe(UPGRADE_SQL.read_text(), "upgrade script")


def test_make_builds_install_script(tmp_path):
    """Concatenating the source bodies (the Makefile build path) yields a non-empty script (review M4 fix)."""
    text = _build_install_script_text()
    assert len(text) > 1000  # ~1031 lines of real SQL
    # M17/M19: theodb.embed + ai.hybrid_search + theodb.import_pinecone(FUNCTION) + ai.nl_to_sql moved to the
    # Rust theodb_rs extension, so they no longer appear in the concatenated SQL install script. Assert markers
    # for surface that STAYS in the SQL umbrella PARTS (30-70): ai.nl_query (plpgsql L3 keeper) + ai.summarize
    # (generative). (PARTS excludes sql/80 by design — its chunked PROCEDURE has a COMMIT the safety scan flags.)
    assert "ai.nl_query" in text and "ai.summarize" in text


# ---- DB integration tests ---------------------------------------------------

@pytest.fixture(scope="module")
def admin_conn():
    """Autocommit connection to the maintenance DB, for CREATE/DROP DATABASE. Drops test DBs on teardown."""
    c = psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"),
        port=os.environ.get("PGPORT", "5432"),
        user=os.environ.get("PGUSER", "postgres"),
        password=os.environ.get("PGPASSWORD", "postgres"),
        dbname="postgres",
    )
    c.autocommit = True
    yield c
    # teardown: drop the DBs this module created (review L3 fix — no server residue)
    with c.cursor() as cur:
        for name in ("m15_surface", "m15_upgrade", "m15_idem", "m15_bare", "m15_residue"):
            cur.execute(f"DROP DATABASE IF EXISTS {name}")
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
    """CREATE EXTENSION theodb CASCADE installs every documented function (presence-by-name, not a count)."""
    _fresh_db(admin_conn, "m15_surface")
    conn = _connect("m15_surface")
    try:
        with conn.cursor() as cur:
            cur.execute("CREATE EXTENSION theodb CASCADE")
            # M17: theodb.embed (+ embed_batch) ship in the Rust theodb_rs extension; the full documented
            # surface requires it too (theodb_rs requires theodb, so this is the shipped pair).
            cur.execute("CREATE EXTENSION theodb_rs CASCADE")
            for schema, fn in _REQUIRED_FUNCS:
                cur.execute(
                    "SELECT 1 FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace "
                    "WHERE n.nspname=%s AND p.proname=%s",
                    (schema, fn),
                )
                assert cur.fetchone() is not None, f"missing documented function {schema}.{fn}"
            cur.execute("SELECT extversion FROM pg_extension WHERE extname='theodb'")
            assert cur.fetchone()[0] == "1.3"  # default_version bumped to 1.3 (M19 nl/hybrid/import retirement)
            cur.execute(
                "SELECT count(*) FROM pg_extension WHERE extname IN ('vector','vectorscale')"
            )
            assert cur.fetchone()[0] == 2  # requires (vector, vectorscale) resolved via CASCADE; plpython3u dropped (M19)
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


def test_transactional_install_leaves_no_residue(admin_conn):
    """Install inside a transaction then ROLLBACK leaves no theodb objects (supabase model; ADR D5)."""
    _fresh_db(admin_conn, "m15_residue")
    conn = _connect("m15_residue")
    try:
        with conn.cursor() as cur:
            cur.execute("BEGIN")
            cur.execute("CREATE EXTENSION theodb CASCADE")
            cur.execute("ROLLBACK")
            cur.execute("SELECT count(*) FROM pg_extension WHERE extname='theodb'")
            assert cur.fetchone()[0] == 0  # rolled back -> gone
            cur.execute("SELECT count(*) FROM pg_namespace WHERE nspname='theodb_ml'")
            assert cur.fetchone()[0] == 0  # no orphan schema left behind
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
