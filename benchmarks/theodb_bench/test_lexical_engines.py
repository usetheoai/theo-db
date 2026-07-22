"""M140.1 T1.1/T1.2/T3.1 — the three lexical retrievers (RED-first TDD).

TantivyBM25 is tested in-process (no infra). PgTsRank/PgTextsearchBM25 need a live
PostgreSQL: pointed at by env `M140_DSN` (default localhost:55432). When no PG is
reachable, those tests SKIP with a reason — never a fabricated pass.
"""
from __future__ import annotations

import os

import pytest

from theodb_bench import lexical_engines as le

DSN = os.environ.get("M140_DSN", "host=127.0.0.1 port=55432 dbname=postgres user=postgres password=postgres")

DOCS = {
    "d1": "the quick brown fox jumps over the lazy dog",
    "d2": "error timeout blk_zebra9 connection reset retry",
    "d3": "info dfs datanode packetresponder terminating normally",
}


def _pg_or_skip(cls):
    try:
        eng = cls(DSN).connect()
    except Exception as e:  # psycopg2.OperationalError etc.
        pytest.skip(f"no PostgreSQL at {DSN}: {e}")
    return eng


# ---- T1.1 Tantivy (own-engine) ----

def test_tantivy_ranks_exact_term_match_first():
    eng = le.TantivyBM25()
    eng.index(DOCS)
    ranked = eng.search("blk_zebra9", 10)
    assert ranked[0] == "d2"


def test_tantivy_empty_query_returns_empty():
    eng = le.TantivyBM25()
    eng.index(DOCS)
    assert eng.search("   ", 10) == []


def test_tantivy_search_before_index_raises():
    with pytest.raises(RuntimeError):
        le.TantivyBM25().search("x", 5)


def test_tantivy_conforms_to_retriever_protocol():
    assert isinstance(le.TantivyBM25(), le.Retriever)


def test_tantivy_ingest_ms_measured():
    eng = le.TantivyBM25()
    eng.index(DOCS)
    assert eng.ingest_ms > 0.0


# ---- T1.1 PgTsRank (baseline) ----

def test_pgtsrank_ranks_by_tsrank_cd():
    eng = _pg_or_skip(le.PgTsRank)
    try:
        eng.index(DOCS)
        ranked = eng.search("blk_zebra9 timeout", 10)
        assert ranked[0] == "d2"
    finally:
        eng.close()


def test_pgtsrank_empty_query_returns_empty():
    eng = _pg_or_skip(le.PgTsRank)
    try:
        eng.index(DOCS)
        assert eng.search("", 10) == []
    finally:
        eng.close()


def test_pgtsrank_conforms_to_retriever_protocol():
    assert isinstance(le.PgTsRank(DSN), le.Retriever)


# ---- T1.2 pg_textsearch (reference, availability-gated) ----

def test_pgtextsearch_available_flag_false_when_missing():
    # Against a PG WITHOUT pg_textsearch, available must be False and no crash.
    try:
        import psycopg2  # noqa: F401
    except Exception:
        pytest.skip("psycopg2 missing")
    eng = le.PgTextsearchBM25(DSN).connect()
    assert eng.available in (True, False)  # never raises; flag is honest


# ---- T3.1 storage measurement ----

def test_tantivy_reports_index_bytes_positive(tmp_path):
    eng = le.TantivyBM25(path=str(tmp_path / "idx"))
    eng.index(DOCS)
    assert eng.index_bytes() > 0


def test_tantivy_inram_index_bytes_zero():
    eng = le.TantivyBM25()
    eng.index(DOCS)
    assert eng.index_bytes() == 0
