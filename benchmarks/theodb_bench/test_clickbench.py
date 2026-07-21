"""M128 — DB-free unit tests for the ClickBench entry contract + the driver's pure helpers.

The ClickBench create.sql/queries.sql are CC-BY-NC-SA → fetched at runtime, never vendored (D1); the
contract tests fetch-or-skip so an offline CI does not fail.
"""
import os

import pytest

import run_m128_clickbench as m
from theodb_bench.regression import assert_byte_identical

ENTRY = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "clickbench", "theodb")


@pytest.fixture(scope="module")
def entry_sql():
    if not m.ensure_entry_sql():  # fetches ClickBench SQL (CC-BY-NC-SA) if absent; offline → skip
        pytest.skip("ClickBench SQL unavailable (offline) — contract tests need the fetched create/queries.sql")


def test_entry_has_43_queries(entry_sql):
    assert len(m._load_queries()) == 43


def test_create_sql_uses_theodb_columnar_and_105_cols(entry_sql):
    ddl = open(os.path.join(ENTRY, "create.sql")).read()
    assert "USING theodb_columnar" in ddl
    assert "CREATE TABLE hits" in ddl
    cols = [ln for ln in ddl.splitlines() if ln.strip().endswith("NOT NULL,") or ln.strip().endswith("NOT NULL")]
    assert len([c for c in cols if "TABLE" not in c]) >= 100


def test_canonical_is_order_insensitive():
    a = m._canonical([(2, "b"), (1, "a")])
    b = m._canonical([(1, "a"), (2, "b")])
    assert a == b  # sorted → order-independent


def test_canonical_stringifies_for_stable_compare():
    assert m._canonical([(1,)]) == m._canonical([("1",)])  # int vs str → same canonical


def test_result_ab_identical_and_divergent():
    rc = {0: ("a",), 1: ("b",)}
    assert assert_byte_identical(rc, {0: ("a",), 1: ("b",)})["identical"] is True
    assert assert_byte_identical(rc, {0: ("a",), 1: ("X",)})["identical"] is False
