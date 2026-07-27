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
    assert "'-0.0'" in h.EDGE_CATALOG["f8"]["edges"]          # M154 IEEE
    assert "'NaN'" in h.EDGE_CATALOG["f8"]["edges"]


def test_catalog_completeness_fails_when_a_type_is_missing():
    # coverage-rot guard: drop a routed type -> the completeness check must fail
    trimmed = {k: v for k, v in h.EDGE_CATALOG.items() if v["pg"] != "date"}
    assert h.catalog_covers_routed_types(trimmed) is False


def test_plan_routes_detects_custom_scan():
    assert h.plan_routes(["Custom Scan (theodb_columnar_agg)"]) is True
    assert h.plan_routes([" ->  Custom Scan (theodb_columnar_project) on hits"]) is True
    assert h.plan_routes(["GroupAggregate", "  Group Key: c2", "  ->  Seq Scan on hits"]) is False


def test_plan_routes_node_specific_rejects_wrong_node():
    # a columnar PROJECT child under a query expecting the AGG node must NOT count as routed-to-agg
    plan = ["GroupAggregate", "  ->  Custom Scan (theodb_columnar_project) on hits"]
    assert h.plan_routes(plan, "theodb_columnar_agg") is False   # not the agg node
    assert h.plan_routes(plan, "theodb_columnar_project") is True
    assert h.plan_routes(["Custom Scan (theodb_columnar_agg)"], "theodb_columnar_agg") is True


def test_off_sql_substitutes_hits_word_boundary():
    assert h._off_sql("SELECT count(*) FROM hits") == "SELECT count(*) FROM hits_heap"
    # must not double-substitute an already-heap reference
    assert h._off_sql("SELECT * FROM hits_heap") == "SELECT * FROM hits_heap"
    # word boundary: a column literally named with 'hits' inside is not our table alias here (kept simple)
    assert h._off_sql("SELECT c4 FROM hits WHERE c4 IN (1,2)") == "SELECT c4 FROM hits_heap WHERE c4 IN (1,2)"


def test_case_matrix_has_route_and_decline_expectations():
    cases = h.build_cases()
    kinds = {expect for _, _, expect, _ in cases}
    assert "route" in kinds and "decline" in kinds
    names = {n for n, _, _, _ in cases}
    # the M161 BLOCKER edge + the temporal-gate-leak decline must both be exercised
    assert "intpk_i2" in names          # int2+5 @ 32767 -> int4 (the BLOCKER shape)
    assert "date_plus" in names         # date+1 must decline (the HIGH gate leak)
    assert "intpk_i8_result" in names   # int8 result must decline (fail-closed)
    assert "group_f8" in names          # M163 found: float GROUP BY diverges -> must decline


def test_every_routed_type_appears_in_a_case():
    # case-matrix rot-guard (council-benchmark MEDIUM): the catalog guard checks the dict; this checks the actual case
    # matrix touches every routed type via a column of that type, so a type cannot be 'covered' with zero cases.
    cases = h.build_cases()
    all_sql = " ".join(sql for _, sql, _, _ in cases)
    col_by_type = {spec["pg"]: name for name, spec in h.EDGE_CATALOG.items()}
    for pg_type in h.ROUTED_TYPES:
        col = col_by_type[pg_type]
        # the column (or an expression on it) must appear in at least one case
        assert col in all_sql or f"extract(" in all_sql and pg_type in ("timestamp",), \
            f"routed type {pg_type} (col {col}) has no case in build_cases()"


def test_route_cases_pin_a_specific_node():
    # every route case must name the specific columnar node (not None), so 'routed' means the tested path ran
    for name, _sql, expect, node in h.build_cases():
        if expect == "route":
            assert node and node.startswith("theodb_columnar"), f"route case {name} lacks a specific expect_node"


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
    # the oracle self-test, THROUGH ab_check: a seeded divergence MUST come back status='diverged' (diverged>0)
    pc = h.positive_control(live)
    assert pc["status"] == "diverged" and pc["diverged"] > 0


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
    for name, sql, expect, node in h.build_cases():
        r = h.ab_check(live, sql, expect_node=node)
        ok = (expect == "route" and r["status"] == "ok") or (expect == "decline" and r["status"] == "declined")
        if not ok:
            failures.append((name, expect, r["status"], r.get("diverged")))
    assert not failures, f"type-coverage A/B failures: {failures}"
