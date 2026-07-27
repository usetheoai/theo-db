"""M163 unit tests for the type-coverage A/B harness.

Two tiers: (1) PURE-logic tests that need no live DB (catalog completeness, EXPLAIN-route detection, the hits→hits_heap
substitution) — always run; (2) LIVE tests against a real theodb_columnar instance — skipped when no DB is reachable, so
`pytest` is green locally AND fully exercised on a TheoDB box.
"""
from __future__ import annotations

import os

import pytest

import columnar_type_ab as h


# ---------------- Tier 1: pure logic (no DB) --------------------------------------------------------------------------
def test_edge_catalog_has_all_routed_types():
    # every routed PG type present, and the M161/M154 traps seeded
    assert h.catalog_covers_routed_types() is True
    present = {v["pg"] for v in h.EDGE_CATALOG.values()}
    assert h.ROUTED_TYPES.issubset(present)
    assert 32767 in h.EDGE_CATALOG["c2"]["edges"]          # the M161 BLOCKER trigger
    assert "-0.0" in h.EDGE_CATALOG["f8"]["edges"]          # M154 IEEE
    assert "'NaN'" in h.EDGE_CATALOG["f8"]["edges"]


def test_catalog_completeness_fails_when_a_type_is_missing():
    # coverage-rot guard: drop a routed type -> the completeness check must fail
    trimmed = {k: v for k, v in h.EDGE_CATALOG.items() if v["pg"] != "date"}
    assert h.catalog_covers_routed_types(trimmed) is False


def test_plan_routes_detects_custom_scan():
    assert h.plan_routes(["Custom Scan (theodb_columnar_agg)"]) is True
    assert h.plan_routes([" ->  Custom Scan (theodb_columnar_project) on hits"]) is True
    assert h.plan_routes(["GroupAggregate", "  Group Key: c2", "  ->  Seq Scan on hits"]) is False


def test_off_sql_substitutes_hits_word_boundary():
    assert h._off_sql("SELECT count(*) FROM hits") == "SELECT count(*) FROM hits_heap"
    # must not double-substitute an already-heap reference
    assert h._off_sql("SELECT * FROM hits_heap") == "SELECT * FROM hits_heap"
    # word boundary: a column literally named with 'hits' inside is not our table alias here (kept simple)
    assert h._off_sql("SELECT c4 FROM hits WHERE c4 IN (1,2)") == "SELECT c4 FROM hits_heap WHERE c4 IN (1,2)"


def test_case_matrix_has_route_and_decline_expectations():
    cases = h.build_cases()
    kinds = {expect for _, _, expect in cases}
    assert "route" in kinds and "decline" in kinds
    names = {n for n, _, _ in cases}
    # the M161 BLOCKER edge + the temporal-gate-leak decline must both be exercised
    assert "intpk_i2" in names          # int2+5 @ 32767 -> int4 (the BLOCKER shape)
    assert "date_plus" in names         # date+1 must decline (the HIGH gate leak)
    assert "intpk_i8_result" in names   # int8 result must decline (fail-closed)


# ---------------- Tier 2: live (needs theodb_columnar) ----------------------------------------------------------------
def _live_conn_or_skip():
    if h.psycopg2 is None:
        pytest.skip("psycopg2 unavailable")
    try:
        c = h._conn()
        cur = c.cursor()
        cur.execute("SELECT 1 FROM pg_am WHERE amname='theodb_columnar'")
        if cur.fetchone() is None:
            pytest.skip("theodb_columnar AM not installed")
        h.session_setup(cur)
        return c, cur
    except Exception as e:  # noqa: BLE001
        pytest.skip(f"no live TheoDB: {e}")


@pytest.fixture(scope="module")
def live():
    c, cur = _live_conn_or_skip()
    h.setup_tables(cur)
    yield cur
    c.close()


def test_setup_loads_equal_nonzero_rowcount(live):
    live.execute("SELECT count(*) FROM hits")
    n_col = live.fetchone()[0]
    live.execute("SELECT count(*) FROM hits_heap")
    assert n_col == live.fetchone()[0] and n_col > 0


def test_positive_control_catches_seeded_divergence(live):
    # the oracle self-test: a deliberately-divergent pair MUST report diverged>0
    assert h.positive_control(live) > 0


def test_identical_query_is_diverged_zero(live):
    r = h.ab_check(live, "SELECT sum(c4) FROM hits")
    assert r["status"] in ("ok", "declined")
    if r["status"] == "ok":
        assert r["diverged"] == 0


def test_declined_query_is_not_a_false_divergence(live):
    # a query that declines to native must be reported 'declined', never a spurious diverged failure
    r = h.ab_check(live, "SELECT d+1, count(*) FROM hits GROUP BY d+1")   # date+1 -> declines (M161)
    assert r["status"] == "declined"


def test_full_matrix_holds_the_m161_contract(live):
    # every routed case diverged=0, every decline case declined — the harness would catch the M161 out_typoid bug
    failures = []
    for name, sql, expect in h.build_cases():
        r = h.ab_check(live, sql)
        ok = (expect == "route" and r["status"] == "ok") or (expect == "decline" and r["status"] == "declined")
        if not ok:
            failures.append((name, expect, r["status"], r.get("diverged")))
    assert not failures, f"type-coverage A/B failures: {failures}"
