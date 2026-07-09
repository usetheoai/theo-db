"""M62 — HTAP unified surface integration tests (theodb.htap_refresh / theodb.olap / theodb.htap_freshness).

Gated on pg_duckdb being CREATEd (the shipped TheoDB image, ADR-0020); skips cleanly on a plain image (no
silent green — `.claude/rules/testing.md`). Correctness oracle: the OLAP aggregate over the Parquet snapshot
must checksum-match the same GROUP BY on the fresh heap (`_results_match`, reused from theodb_bench.columnar —
Rule 9). Negative/edge cases assert the SPECIFIC typed error (`.claude/rules/error-handling.md` §4.1 — a
negative-case test asserts the typed error + message, not merely "it throws").

The COPY→Parquet + read_parquet mechanism needs pg_duckdb; those tests run on the built image (Phase 4, on the
droplet). The freshness-lag and no-snapshot typed-error paths were also validated on a plain postgres:17 at
implement time (they do not touch pg_duckdb).
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


def _olap_rows(db, table: str):
    """Call theodb.olap and reshape the jsonb {"rows":[{category,c,a}]} into the (category, c, a) tuples that
    _results_match expects, so the columnar snapshot result is checksum-compared to the heap GROUP BY."""
    with db._cursor() as cur:
        cur.execute(f"SELECT theodb.olap('{table}')")
        payload = cur.fetchone()[0]
    return payload, [(r["category"], r["c"], r["a"]) for r in payload["rows"]]


# ---- T1.1: the raw COPY→Parquet + read_parquet mechanism (does the snapshot round-trip match the heap?) ----

def test_copy_to_parquet_roundtrip_matches_heap(db):
    """The mechanism the whole surface stands on: materialize the heap to Parquet, aggregate it back via
    DuckDB, and assert the result matches the heap GROUP BY (Q2 of the plan — COPY TO from a function context)."""
    _require_duckdb(db)
    _seed(db, _TABLE, 10_000)
    path = "/tmp/theodb_htap_roundtrip.parquet"
    with db._cursor() as cur:
        cur.execute(f"COPY (SELECT * FROM {_TABLE}) TO '{path}' (FORMAT parquet)")
        cur.execute(
            "SELECT category, c, a FROM duckdb.query($DUCK$ "
            "SELECT category, count(*) AS c, round(avg(amount), 4) AS a "
            f"FROM read_parquet('{path}') GROUP BY category ORDER BY category $DUCK$) t"
        )
        parquet_agg = cur.fetchall()
    heap_agg = _heap_agg(db, _TABLE)
    assert _results_match(heap_agg, parquet_agg), f"heap={heap_agg}\nparquet={parquet_agg}"


def test_pg_duckdb_available_true_on_image(db):
    """Honesty gate: on the shipped image pg_duckdb_available() is True (else every HTAP test would skip
    silently and the surface would go unverified)."""
    if not db.pg_duckdb_available():
        pytest.skip("plain image — pg_duckdb absent-path is covered by the skip on the other tests")
    assert db.pg_duckdb_available() is True


# ---- T1.2: theodb.htap_refresh materializes a dated snapshot + registers it ----

def test_htap_refresh_creates_dated_snapshot(db):
    _require_duckdb(db)
    _seed(db, _TABLE, 10_000)
    with db._cursor() as cur:
        cur.execute(f"SELECT theodb.htap_refresh('{_TABLE}')")
        refreshed_at = cur.fetchone()[0]
        assert refreshed_at is not None
        # a row is registered in the catalog with a recent timestamp and an existing parquet path
        cur.execute(
            "SELECT parquet_path, refreshed_at FROM theodb._htap_snapshots WHERE rel = %s::regclass",
            (_TABLE,),
        )
        row = cur.fetchone()
    assert row is not None, "htap_refresh did not register a snapshot row"
    parquet_path, cat_ts = row
    assert parquet_path.endswith(".parquet")
    assert cat_ts == refreshed_at  # the returned timestamp is the registered one


def test_htap_refresh_is_upsert_latest_wins(db):
    """Re-refreshing the same table updates (not duplicates) the catalog row — the latest snapshot wins."""
    _require_duckdb(db)
    _seed(db, _TABLE, 5_000)
    with db._cursor() as cur:
        cur.execute(f"SELECT theodb.htap_refresh('{_TABLE}')")
        first = cur.fetchone()[0]
        time.sleep(0.05)
        cur.execute(f"SELECT theodb.htap_refresh('{_TABLE}')")
        second = cur.fetchone()[0]
        cur.execute("SELECT count(*) FROM theodb._htap_snapshots WHERE rel = %s::regclass", (_TABLE,))
        n = cur.fetchone()[0]
    assert n == 1, "upsert must keep exactly one row per relation"
    assert second > first, "the second refresh must have a newer timestamp"


# ---- T1.3: theodb.olap routes to the snapshot; theodb.htap_freshness exposes the lag ----

def test_olap_matches_fresh_heap(db):
    """After a refresh, theodb.olap('t').rows equals the GROUP BY on the fresh heap (checksum-matched)."""
    _require_duckdb(db)
    _seed(db, _TABLE, 10_000)
    with db._cursor() as cur:
        cur.execute(f"SELECT theodb.htap_refresh('{_TABLE}')")
    payload, olap_agg = _olap_rows(db, _TABLE)
    heap_agg = _heap_agg(db, _TABLE)
    assert _results_match(heap_agg, olap_agg), f"heap={heap_agg}\nolap={olap_agg}"
    assert "snapshot_at" in payload, "theodb.olap must expose the snapshot timestamp (freshness honesty)"


def test_freshness_reflects_lag_and_olap_is_stale_until_refresh(db):
    """The core honesty contract: after INSERTs WITHOUT a refresh, htap_freshness grows AND theodb.olap still
    returns the OLD snapshot (staleness is dated, not a bug); a new refresh moves it forward."""
    _require_duckdb(db)
    _seed(db, _TABLE, 10_000)
    with db._cursor() as cur:
        cur.execute(f"SELECT theodb.htap_refresh('{_TABLE}')")
    _, olap_before = _olap_rows(db, _TABLE)
    old_total = sum(int(c) for _, c, _ in olap_before)

    # mutate the heap WITHOUT refreshing — the snapshot must NOT change; freshness lag must grow
    with db._cursor() as cur:
        cur.execute(f"INSERT INTO {_TABLE} SELECT g, 'cat' || (g %% 5), (g %% 1000) * 1.5 "
                    f"FROM generate_series(1000001, 1005000) g")
        time.sleep(1.0)
        cur.execute(f"SELECT theodb.htap_freshness('{_TABLE}')")
        lag = cur.fetchone()[0]
    assert lag.total_seconds() >= 1.0, f"freshness lag should reflect elapsed time, got {lag}"

    _, olap_stale = _olap_rows(db, _TABLE)
    stale_total = sum(int(c) for _, c, _ in olap_stale)
    assert stale_total == old_total, "olap must return the STALE snapshot until a new refresh (dated freshness)"

    # after a fresh refresh, olap reflects the new heap state
    with db._cursor() as cur:
        cur.execute(f"SELECT theodb.htap_refresh('{_TABLE}')")
    _, olap_after = _olap_rows(db, _TABLE)
    after_total = sum(int(c) for _, c, _ in olap_after)
    assert after_total > old_total, "after re-refresh, olap must reflect the 5000 new rows"


def test_force_execution_fallback_is_fresh(db):
    """The ad-hoc fallback: SET duckdb.force_execution=true; SELECT over the heap returns 100%-fresh data
    (no refresh needed) — slower, but correct. Confirms the fallback exists and is correct."""
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

def test_olap_without_snapshot_raises_typed_error(db):
    """No snapshot → theodb.olap raises a clear typed error (SQLSTATE P0002), never a silent NULL."""
    _require_duckdb(db)
    _seed(db, _TABLE, 100)
    with db._cursor() as cur:
        cur.execute("DELETE FROM theodb._htap_snapshots WHERE rel = %s::regclass", (_TABLE,))
    with pytest.raises(psycopg2.Error) as exc:
        with db._cursor() as cur:
            cur.execute(f"SELECT theodb.olap('{_TABLE}')")
            cur.fetchone()
    assert getattr(exc.value, "pgcode", None) == "P0002"
    assert "no snapshot" in str(exc.value)
    assert "htap_refresh" in str(exc.value)


def test_freshness_without_snapshot_raises_typed_error(db):
    """No snapshot → theodb.htap_freshness raises a typed error (P0002), not a silent NULL/zero."""
    _require_duckdb(db)
    _seed(db, _TABLE, 100)
    with db._cursor() as cur:
        cur.execute("DELETE FROM theodb._htap_snapshots WHERE rel = %s::regclass", (_TABLE,))
    with pytest.raises(psycopg2.Error) as exc:
        with db._cursor() as cur:
            cur.execute(f"SELECT theodb.htap_freshness('{_TABLE}')")
            cur.fetchone()
    assert getattr(exc.value, "pgcode", None) == "P0002"
    assert "no snapshot" in str(exc.value)


def test_olap_corrupt_or_missing_parquet_raises_typed_error(db):
    """Snapshot registered but the Parquet file is gone → theodb.olap raises a clear io_error, not a crash
    or a silent empty result (§ Failure scenarios: filesystem snapshot missing)."""
    _require_duckdb(db)
    _seed(db, _TABLE, 100)
    with db._cursor() as cur:
        cur.execute(f"SELECT theodb.htap_refresh('{_TABLE}')")
        # point the catalog at a non-existent Parquet to simulate a missing/corrupt snapshot
        cur.execute(
            "UPDATE theodb._htap_snapshots SET parquet_path = '/tmp/theodb_htap_does_not_exist.parquet' "
            "WHERE rel = %s::regclass",
            (_TABLE,),
        )
    with pytest.raises(psycopg2.Error) as exc:
        with db._cursor() as cur:
            cur.execute(f"SELECT theodb.olap('{_TABLE}')")
            cur.fetchone()
    assert "cannot read snapshot" in str(exc.value) or "missing/corrupt" in str(exc.value)


# ---- T2.1: mixed load / non-interference (race-aware, threading.Barrier) ----

def test_oltp_latency_not_degraded_under_concurrent_olap(db):
    """Race-aware: OLTP INSERT p95 under concurrent OLAP must stay within LATENCY_DEGRADATION_FACTOR of the
    alone-baseline, AND the overlap must be confirmed (OLAP iterations ran during the INSERTs). Without the
    overlap guard a sequential execution would pass falsely — the guard makes the test a real race test."""
    _require_duckdb(db)
    result = measure_mixed_load(table="htap_mixed", seed_n=20_000, n_inserts=1_000)
    assert result["overlap_confirmed"], (
        "OLAP did not run during the INSERTs — the concurrency test degenerated to sequential (false green)")
    assert result["not_degraded"], (
        f"OLTP p95 degraded {result['degradation_factor']}× under concurrent OLAP "
        f"(threshold {result['latency_factor_threshold']}×): baseline {result['baseline_p95_ms']}ms "
        f"mixed {result['mixed_p95_ms']}ms")


def test_olap_reads_consistent_snapshot_during_concurrent_inserts(db):
    """The OLAP snapshot is a point-in-time: concurrent INSERTs into the heap must NOT change what theodb.olap
    returns (it reads the immutable Parquet, never a partial write). Barrier-synchronized overlap."""
    _require_duckdb(db)
    _seed(db, _TABLE, 10_000)
    with db._cursor() as cur:
        cur.execute(f"SELECT theodb.htap_refresh('{_TABLE}')")
    _, before = _olap_rows(db, _TABLE)
    before_total = sum(int(c) for _, c, _ in before)

    barrier = threading.Barrier(2)
    olap_totals = []

    def _inserter():
        conn = VectorDB(_dsn()).connect()
        barrier.wait()
        with conn._cursor() as cur:
            cur.execute(f"INSERT INTO {_TABLE} SELECT g, 'cat' || (g %% 5), (g %% 1000) * 1.5 "
                        f"FROM generate_series(2000001, 2010000) g")
        conn.close()

    def _reader():
        conn = VectorDB(_dsn()).connect()
        barrier.wait()
        for _ in range(20):
            with conn._cursor() as cur:
                cur.execute(f"SELECT theodb.olap('{_TABLE}')")
                payload = cur.fetchone()[0]
            olap_totals.append(sum(int(r["c"]) for r in payload["rows"]))
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
    """Smoke: the three functions resolve on the image and a minimal refresh→olap→freshness round-trip works
    end-to-end (fail-closed if the surface did not ship in the extension concat)."""
    _require_duckdb(db)
    with db._cursor() as cur:
        cur.execute(
            "SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace "
            "WHERE n.nspname = 'theodb' AND p.proname IN ('htap_refresh', 'olap', 'htap_freshness')"
        )
        assert cur.fetchone()[0] == 3, "the 3 HTAP functions must resolve on the image"
    _seed(db, _TABLE, 1_000)
    with db._cursor() as cur:
        cur.execute(f"SELECT theodb.htap_refresh('{_TABLE}')")
        cur.execute(f"SELECT theodb.htap_freshness('{_TABLE}')")
        assert cur.fetchone()[0] is not None
    payload, olap_agg = _olap_rows(db, _TABLE)
    assert _results_match(_heap_agg(db, _TABLE), olap_agg)
