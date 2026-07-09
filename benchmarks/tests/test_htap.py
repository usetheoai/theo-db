"""M62 — HTAP unified surface integration tests (CODEGEN edition — statement-level flow).

pg_duckdb PROHIBITS DuckDB execution inside a function (`ERROR: DuckDB execution is not supported inside
functions` — measured, no GUC to relax it). So the honest surface is codegen: theodb.htap_refresh_sql /
theodb.olap_sql BUILD the COPY / duckdb.query statements as text and the CLIENT runs them at the CONNECTION LEVEL
(psycopg2 autocommit), NEVER inside a function. theodb.htap_register / theodb.htap_freshness are pure SQL (no
DuckDB) and run in a function fine. These tests exercise exactly that flow. See sql/85-theodb-htap.sql (ADR-0021).

Gated on pg_duckdb being CREATEd (the shipped TheoDB image, ADR-0020); skips cleanly on a plain image (no
silent green — `.claude/rules/testing.md`). Correctness oracle: the OLAP aggregate over the Parquet snapshot
must checksum-match the same GROUP BY on the fresh heap (`_results_match`, reused from theodb_bench.columnar —
Rule 9). Negative/edge cases assert the SPECIFIC typed error (`.claude/rules/error-handling.md` §4.1 — a
negative-case test asserts the typed error + message, not merely "it throws").

The COPY→Parquet + read_parquet mechanism needs pg_duckdb; those tests run on the built image (Phase 4, on the
droplet). The codegen strings (refresh_sql/olap_sql), the catalog/register, the freshness-lag and no-snapshot
typed-error paths do NOT touch pg_duckdb and were validated on a plain postgres:17 at implement time.
"""
import os
import threading
import time

import psycopg2
import pytest

from run_m62_htap import measure_mixed_load
from theodb_bench.columnar import _AGG, _results_match
from theodb_bench.db import VectorDB

pytestmark = pytest.mark.integration

_TABLE = "htap_orders"


def _dsn() -> str:
    return (
        f"host={os.environ.get('PGHOST', 'localhost')} "
        f"port={os.environ.get('PGPORT', '5432')} "
        f"dbname={os.environ.get('PGDATABASE', 'postgres')} "
        f"user={os.environ.get('PGUSER', 'postgres')} "
        f"password={os.environ.get('PGPASSWORD', 'postgres')}"
    )


@pytest.fixture
def db():
    d = VectorDB(_dsn()).connect()
    d.ping()
    yield d
    d.close()


def _require_duckdb(db):
    if not db.pg_duckdb_available():
        pytest.skip("pg_duckdb not available — run against the shipped TheoDB image (ADR-0020)")


def _raw_conn():
    """A raw psycopg2 connection (autocommit) for negative-case tests that must inspect the SPECIFIC pgcode. The
    VectorDB._cursor wrapper re-raises every psycopg2.Error as DBUnavailableError (losing pgcode), so typed-error
    assertions (`.claude/rules/error-handling.md` §4.1 — assert the SQLSTATE, not merely 'it throws') read the
    raw error here instead."""
    c = psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"), port=os.environ.get("PGPORT", "5432"),
        dbname=os.environ.get("PGDATABASE", "postgres"), user=os.environ.get("PGUSER", "postgres"),
        password=os.environ.get("PGPASSWORD", "postgres"),
    )
    c.autocommit = True
    return c


def _seed(db, table: str, n: int) -> None:
    with db._cursor() as cur:
        cur.execute(f"DROP TABLE IF EXISTS {table} CASCADE")
        cur.execute(f"CREATE TABLE {table} (id bigint PRIMARY KEY, category text, amount double precision)")
        cur.execute(
            f"INSERT INTO {table} SELECT g, 'cat' || (g %% 5), (g %% 1000) * 1.5 FROM generate_series(1, %s) g",
            (n,),
        )
        cur.execute(f"ANALYZE {table}")


def _heap_agg(db, table: str):
    with db._cursor() as cur:
        cur.execute(_AGG.format(t=table))
        return cur.fetchall()


def _refresh_and_register(db, table: str):
    """The client contract: ask theodb.htap_refresh_sql for the COPY, EXECUTE it at the connection level (where
    pg_duckdb allows DuckDB — never inside a function), then register the snapshot via the pure-SQL
    theodb.htap_register. Returns the registered refreshed_at."""
    with db._cursor() as cur:
        cur.execute("SELECT theodb.htap_refresh_sql(%s::regclass)", (table,))
        copy_stmt = cur.fetchone()[0]
        cur.execute(copy_stmt)  # statement level — the COPY→Parquet DuckDB writer runs here, NOT in a function
        cur.execute("SELECT theodb._htap_path(%s::regclass)", (table,))
        path = cur.fetchone()[0]
        cur.execute("SELECT theodb.htap_register(%s::regclass, %s)", (table, path))
        return cur.fetchone()[0]


def _olap_rows(db, table: str):
    """The client contract: ask theodb.olap_sql for the SELECT over the snapshot, EXECUTE it at the connection
    level (read_parquet via duckdb.query runs here, NOT inside a function), and reshape the (category, c, a)
    tuples so the columnar result is checksum-compared to the heap GROUP BY (_results_match). SELECT * columns
    come back positionally as (category, c, a)."""
    with db._cursor() as cur:
        cur.execute("SELECT theodb.olap_sql(%s::regclass)", (table,))
        olap_stmt = cur.fetchone()[0]
        cur.execute(olap_stmt)  # statement level — duckdb.query(read_parquet) runs here
        return cur.fetchall()


# ---- T1.1: the codegen strings + the raw COPY→Parquet + read_parquet mechanism (does it round-trip?) ----

def test_htap_refresh_sql_builds_copy_statement(db):
    """The codegen contract (no pg_duckdb needed): theodb.htap_refresh_sql returns the exact COPY statement as
    text — string building, NOT execution. Runs on a plain image too (pure SQL)."""
    _seed(db, _TABLE, 100)
    with db._cursor() as cur:
        cur.execute("SELECT theodb.htap_refresh_sql(%s::regclass)", (_TABLE,))
        stmt = cur.fetchone()[0]
        cur.execute("SELECT theodb._htap_path(%s::regclass)", (_TABLE,))
        path = cur.fetchone()[0]
    assert stmt == f"COPY (SELECT * FROM {_TABLE}) TO '{path}' (FORMAT parquet)", stmt


def test_flow_roundtrip_matches_heap(db):
    """The mechanism the whole surface stands on, statement-level: refresh_sql → run the COPY (connection level)
    → register → olap_sql → run the SELECT (connection level) → the aggregate matches the heap GROUP BY. The
    DuckDB statements run at the connection level, NEVER inside a function (Q2 of the plan, resolved by the
    measured pg_duckdb constraint)."""
    _require_duckdb(db)
    _seed(db, _TABLE, 10_000)
    _refresh_and_register(db, _TABLE)
    olap_agg = _olap_rows(db, _TABLE)
    heap_agg = _heap_agg(db, _TABLE)
    assert _results_match(heap_agg, olap_agg), f"heap={heap_agg}\nolap={olap_agg}"


def test_pg_duckdb_available_true_on_image(db):
    """Honesty gate: on the shipped image pg_duckdb_available() is True (else every DuckDB-touching HTAP test
    would skip silently and the surface would go unverified)."""
    if not db.pg_duckdb_available():
        pytest.skip("plain image — pg_duckdb absent-path is covered by the skip on the other tests")
    assert db.pg_duckdb_available() is True


# ---- T1.2: theodb.htap_register materializes a dated snapshot in the catalog ----

def test_register_creates_dated_snapshot(db):
    """After running the COPY, theodb.htap_register registers a catalog row with a recent timestamp and the
    parquet path. The returned timestamp is the registered one."""
    _require_duckdb(db)
    _seed(db, _TABLE, 10_000)
    refreshed_at = _refresh_and_register(db, _TABLE)
    assert refreshed_at is not None
    with db._cursor() as cur:
        cur.execute(
            "SELECT parquet_path, refreshed_at FROM theodb._htap_snapshots WHERE rel = %s::regclass",
            (_TABLE,),
        )
        row = cur.fetchone()
    assert row is not None, "htap_register did not register a snapshot row"
    parquet_path, cat_ts = row
    assert parquet_path.endswith(".parquet")
    assert cat_ts == refreshed_at  # the returned timestamp is the registered one


def test_register_is_upsert_latest_wins(db):
    """Re-registering the same table updates (not duplicates) the catalog row — the latest snapshot wins. Pure
    SQL, no pg_duckdb needed for the register itself (guard on the image for the COPY)."""
    _require_duckdb(db)
    _seed(db, _TABLE, 5_000)
    first = _refresh_and_register(db, _TABLE)
    time.sleep(0.05)
    second = _refresh_and_register(db, _TABLE)
    with db._cursor() as cur:
        cur.execute("SELECT count(*) FROM theodb._htap_snapshots WHERE rel = %s::regclass", (_TABLE,))
        n = cur.fetchone()[0]
    assert n == 1, "upsert must keep exactly one row per relation"
    assert second > first, "the second register must have a newer timestamp"


# ---- T1.3: olap_sql routes to the snapshot; htap_freshness exposes the lag ----

def test_olap_sql_builds_select_over_snapshot(db):
    """The codegen contract: theodb.olap_sql returns the exact SELECT * FROM duckdb.query(...read_parquet...)
    statement pointing at the registered path — string building, NOT execution. Uses SELECT * (named columns
    break duckdb.query record projection — measured)."""
    _require_duckdb(db)
    _seed(db, _TABLE, 1_000)
    _refresh_and_register(db, _TABLE)
    with db._cursor() as cur:
        cur.execute("SELECT parquet_path FROM theodb._htap_snapshots WHERE rel = %s::regclass", (_TABLE,))
        path = cur.fetchone()[0]
        cur.execute("SELECT theodb.olap_sql(%s::regclass)", (_TABLE,))
        stmt = cur.fetchone()[0]
    assert stmt.startswith("SELECT * FROM duckdb.query("), stmt
    assert f"read_parquet('{path}')" in stmt, stmt
    assert "GROUP BY category ORDER BY category" in stmt, stmt


def test_olap_matches_fresh_heap(db):
    """After a refresh+register, running olap_sql equals the GROUP BY on the fresh heap (checksum-matched)."""
    _require_duckdb(db)
    _seed(db, _TABLE, 10_000)
    _refresh_and_register(db, _TABLE)
    olap_agg = _olap_rows(db, _TABLE)
    heap_agg = _heap_agg(db, _TABLE)
    assert _results_match(heap_agg, olap_agg), f"heap={heap_agg}\nolap={olap_agg}"


def test_freshness_reflects_lag_and_olap_is_stale_until_refresh(db):
    """The core honesty contract: after INSERTs WITHOUT a re-refresh, htap_freshness grows AND olap_sql still
    points at the OLD snapshot (running it returns the stale total — staleness is dated, not a bug); a new
    refresh+register moves it forward."""
    _require_duckdb(db)
    _seed(db, _TABLE, 10_000)
    _refresh_and_register(db, _TABLE)
    olap_before = _olap_rows(db, _TABLE)
    old_total = sum(int(c) for _, c, _ in olap_before)

    # mutate the heap WITHOUT re-refreshing — the snapshot must NOT change; freshness lag must grow
    with db._cursor() as cur:
        cur.execute(f"INSERT INTO {_TABLE} SELECT g, 'cat' || (g % 5), (g % 1000) * 1.5 "
                    f"FROM generate_series(1000001, 1005000) g")
        time.sleep(1.0)
        cur.execute("SELECT theodb.htap_freshness(%s::regclass)", (_TABLE,))
        lag = cur.fetchone()[0]
    assert lag.total_seconds() >= 1.0, f"freshness lag should reflect elapsed time, got {lag}"

    olap_stale = _olap_rows(db, _TABLE)
    stale_total = sum(int(c) for _, c, _ in olap_stale)
    assert stale_total == old_total, "olap_sql must still point at the STALE snapshot until a re-refresh (dated)"

    # after a fresh refresh+register, olap_sql reflects the new heap state
    _refresh_and_register(db, _TABLE)
    olap_after = _olap_rows(db, _TABLE)
    after_total = sum(int(c) for _, c, _ in olap_after)
    assert after_total > old_total, "after re-refresh, olap must reflect the 5000 new rows"


def test_force_execution_fallback_is_fresh(db):
    """The ad-hoc fallback: SET duckdb.force_execution=true; SELECT over the heap returns 100%-fresh data
    (no refresh needed) — slower, but correct. Runs at the connection level (SET + SELECT, not in a function)."""
    _require_duckdb(db)
    _seed(db, _TABLE, 10_000)
    heap_agg = _heap_agg(db, _TABLE)
    with db._cursor() as cur:
        cur.execute("SET duckdb.force_execution = true")
        cur.execute(_AGG.format(t=_TABLE))
        fresh_agg = cur.fetchall()
        cur.execute("SET duckdb.force_execution = false")
    assert _results_match(heap_agg, fresh_agg), f"heap={heap_agg}\nforce_execution={fresh_agg}"


# ---- Negative / failure scenarios (typed errors, fail-closed) ----

def test_olap_sql_without_snapshot_raises_typed_error(db):
    """No snapshot → theodb.olap_sql raises a clear typed error (SQLSTATE P0002), never a silent NULL. Pure SQL
    path — runs on a plain image too. Read via a raw connection to preserve pgcode."""
    _seed(db, _TABLE, 100)
    with db._cursor() as cur:
        cur.execute("DELETE FROM theodb._htap_snapshots WHERE rel = %s::regclass", (_TABLE,))
    conn = _raw_conn()
    try:
        with pytest.raises(psycopg2.Error) as exc:
            with conn.cursor() as cur:
                cur.execute("SELECT theodb.olap_sql(%s::regclass)", (_TABLE,))
                cur.fetchone()
    finally:
        conn.close()
    assert exc.value.pgcode == "P0002"
    assert "no snapshot" in str(exc.value)
    assert "htap_refresh_sql" in str(exc.value)


def test_freshness_without_snapshot_raises_typed_error(db):
    """No snapshot → theodb.htap_freshness raises a typed error (P0002), not a silent NULL/zero. Pure SQL."""
    _seed(db, _TABLE, 100)
    with db._cursor() as cur:
        cur.execute("DELETE FROM theodb._htap_snapshots WHERE rel = %s::regclass", (_TABLE,))
    conn = _raw_conn()
    try:
        with pytest.raises(psycopg2.Error) as exc:
            with conn.cursor() as cur:
                cur.execute("SELECT theodb.htap_freshness(%s::regclass)", (_TABLE,))
                cur.fetchone()
    finally:
        conn.close()
    assert exc.value.pgcode == "P0002"
    assert "no snapshot" in str(exc.value)


def test_register_nonexistent_table_raises_typed_error(db):
    """theodb.htap_register('does_not_exist', path) → the regclass cast rejects the missing relation with a
    typed undefined_table error (42P01), never a silent bogus catalog row. Pure SQL."""
    conn = _raw_conn()
    try:
        with pytest.raises(psycopg2.Error) as exc:
            with conn.cursor() as cur:
                cur.execute("SELECT theodb.htap_register('theodb_no_such_table_xyz'::regclass, '/tmp/x.parquet')")
                cur.fetchone()
    finally:
        conn.close()
    assert exc.value.pgcode == "42P01"  # undefined_table
    assert "does not exist" in str(exc.value)


def test_register_blank_path_raises_typed_error(db):
    """theodb.htap_register(tbl, '') → typed invalid_parameter_value (22023): a blank path would register a
    snapshot olap_sql could never read. Fail-closed, not stored. Pure SQL."""
    _seed(db, _TABLE, 100)
    conn = _raw_conn()
    try:
        with pytest.raises(psycopg2.Error) as exc:
            with conn.cursor() as cur:
                cur.execute("SELECT theodb.htap_register(%s::regclass, %s)", (_TABLE, "   "))
                cur.fetchone()
    finally:
        conn.close()
    assert exc.value.pgcode == "22023"
    assert "non-empty path" in str(exc.value)


# ---- T2.1: mixed load / non-interference (race-aware, threading.Barrier) ----

def test_oltp_latency_not_degraded_under_concurrent_olap(db):
    """Race-aware: OLTP INSERT p95 under concurrent statement-level OLAP (olap_sql → run it) must stay within
    LATENCY_DEGRADATION_FACTOR of the alone-baseline, AND the overlap must be confirmed (OLAP iterations ran
    during the INSERTs). Without the overlap guard a sequential execution would pass falsely — the guard makes
    the test a real race test. The DuckDB statements run at the connection level, never inside a function."""
    _require_duckdb(db)
    result = measure_mixed_load(table="htap_mixed", seed_n=20_000, n_inserts=1_000)
    assert result["overlap_confirmed"], (
        "OLAP did not run during the INSERTs — the concurrency test degenerated to sequential (false green)")
    assert result["not_degraded"], (
        f"OLTP p95 degraded {result['degradation_factor']}× under concurrent OLAP "
        f"(threshold {result['latency_factor_threshold']}×): baseline {result['baseline_p95_ms']}ms "
        f"mixed {result['mixed_p95_ms']}ms")


def test_olap_reads_consistent_snapshot_during_concurrent_inserts(db):
    """The OLAP snapshot is a point-in-time / read-only Parquet: concurrent INSERTs into the heap must NOT
    change what running olap_sql returns (it reads the immutable Parquet, never a partial write). Barrier-
    synchronized overlap. The olap_sql statement runs at the connection level, never inside a function."""
    _require_duckdb(db)
    _seed(db, _TABLE, 10_000)
    _refresh_and_register(db, _TABLE)
    before = _olap_rows(db, _TABLE)
    before_total = sum(int(c) for _, c, _ in before)

    barrier = threading.Barrier(2)
    olap_totals = []

    def _inserter():
        conn = VectorDB(_dsn()).connect()
        barrier.wait()
        with conn._cursor() as cur:
            cur.execute(f"INSERT INTO {_TABLE} SELECT g, 'cat' || (g % 5), (g % 1000) * 1.5 "
                        f"FROM generate_series(2000001, 2010000) g")
        conn.close()

    def _reader():
        conn = VectorDB(_dsn()).connect()
        barrier.wait()
        for _ in range(20):
            with conn._cursor() as cur:
                cur.execute("SELECT theodb.olap_sql(%s::regclass)", (_TABLE,))
                olap_stmt = cur.fetchone()[0]
                cur.execute(olap_stmt)  # statement level — consistent, read-only Parquet snapshot
                rows = cur.fetchall()
            olap_totals.append(sum(int(c) for _, c, _ in rows))
        conn.close()

    ti = threading.Thread(target=_inserter)
    tr = threading.Thread(target=_reader)
    ti.start()
    tr.start()
    ti.join(timeout=30)
    tr.join(timeout=30)

    # every OLAP read during the concurrent INSERTs saw the SAME consistent snapshot total (never a partial)
    assert olap_totals, "reader collected no OLAP results"
    assert all(t == before_total for t in olap_totals), (
        f"OLAP saw an inconsistent snapshot under concurrent INSERTs: {set(olap_totals)} != {before_total}")


# ---- Phase 4 smoke: the surface loads on the image ----

def test_htap_surface_loads_on_image(db):
    """Smoke: the four functions resolve on the image and a minimal refresh_sql→run→register→olap_sql→run→
    freshness round-trip works end-to-end (fail-closed if the surface did not ship in the extension concat)."""
    _require_duckdb(db)
    with db._cursor() as cur:
        cur.execute(
            "SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace "
            "WHERE n.nspname = 'theodb' "
            "AND p.proname IN ('htap_refresh_sql', 'htap_register', 'olap_sql', 'htap_freshness')"
        )
        assert cur.fetchone()[0] == 4, "the 4 HTAP functions must resolve on the image"
    _seed(db, _TABLE, 1_000)
    _refresh_and_register(db, _TABLE)
    with db._cursor() as cur:
        cur.execute("SELECT theodb.htap_freshness(%s::regclass)", (_TABLE,))
        assert cur.fetchone()[0] is not None
    olap_agg = _olap_rows(db, _TABLE)
    assert _results_match(_heap_agg(db, _TABLE), olap_agg)
