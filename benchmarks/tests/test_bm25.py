"""M7-S2 BM25 retriever contract test (pg_textsearch).

Gated on pg_textsearch being available (the throwaway `packaging/Dockerfile.bm25` image, started with
`-c shared_preload_libraries=pg_textsearch`). Against the shipped image (no pg_textsearch) the test skips
cleanly — no silent green. This is the measurement-first gate: it proves the permissive BM25 piece works on
the TheoDB engine AND measures its recall vs the other legs on the BEIR-style fixture.
"""
import os

import pytest

from theodb_bench.beir import EMBED_DIM, lexical_embed, synthetic_dataset
from theodb_bench.db import DBUnavailableError, VectorDB
from theodb_bench.hybrid import run_three_retrievers

pytestmark = pytest.mark.integration


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


def _require_bm25(db):
    if not db.pg_textsearch_available():
        pytest.skip("pg_textsearch not available — run against packaging/Dockerfile.bm25 "
                    "with -c shared_preload_libraries=pg_textsearch")


def test_bm25_skips_without_extension(db):
    # When pg_textsearch is absent this test is a clean skip (documents the gate; no silent green).
    if db.pg_textsearch_available():
        pytest.skip("pg_textsearch IS available here — the absent-path is covered on the shipped image")
    # creating the extension without it installed fails LOUD (not a silent NULL) — the adapter wraps the
    # psycopg2 UndefinedFile into DBUnavailableError; the message names the missing control file.
    with pytest.raises(DBUnavailableError) as exc:
        db.ensure_bm25_extension()
    assert "pg_textsearch" in str(exc.value)


def test_bm25_retriever_reports_metrics(db):
    """The pg_textsearch BM25 leg returns finite nDCG@10 + Recall@100 (measured, real build — no mock)."""
    _require_bm25(db)
    db.ensure_extension()
    results = run_three_retrievers(
        db, synthetic_dataset(), lexical_embed, EMBED_DIM, table="it_bm25_eval", include_bm25=True
    )
    assert "bm25" in results, f"bm25 leg missing: {results}"
    m = results["bm25"]
    assert 0.0 <= m["ndcg10"] <= 1.0, f"bm25 nDCG@10 out of range: {m}"
    assert 0.0 <= m["recall100"] <= 1.0, f"bm25 Recall@100 out of range: {m}"
    # the BM25 leg must produce real signal on the labelled corpus (not all-zero)
    assert m["ndcg10"] > 0, f"bm25 leg scored zero — not functional: {results}"


def test_bm25_ranks_relevant_first(db):
    """Direct functional proof: a clearly-matching doc ranks above a non-matching one via BM25."""
    _require_bm25(db)
    db.ensure_bm25_extension()
    with db._cursor() as cur:
        cur.execute("DROP TABLE IF EXISTS it_bm25_rank")
        cur.execute("CREATE TABLE it_bm25_rank (doc_id text PRIMARY KEY, content text)")
        cur.execute("INSERT INTO it_bm25_rank VALUES "
                    "('d1','postgresql database management system'),"
                    "('d2','cooking recipes and baking bread')")
    db.create_bm25_index("it_bm25_rank")
    ranked = db.bm25_query("it_bm25_rank", "database system", 5)
    assert ranked and ranked[0] == "d1", f"BM25 must rank the matching doc first: {ranked}"


def test_bm25_empty_content_index_and_nonmatch(db):
    """Failure scenario #3: a BM25 index build over an empty-content row does not crash, and a query whose
    terms match nothing in the corpus executes without error. (Note: bm25_query is a top-k ranker with NO
    `WHERE @@` match filter — by design — so a non-matching query still returns docs ranked; the failure
    mode being guarded is a *crash/error*, not non-emptiness.)"""
    _require_bm25(db)
    db.ensure_bm25_extension()
    with db._cursor() as cur:
        cur.execute("DROP TABLE IF EXISTS it_bm25_empty")
        cur.execute("CREATE TABLE it_bm25_empty (doc_id text PRIMARY KEY, content text)")
        cur.execute("INSERT INTO it_bm25_empty VALUES ('d1',''),('d2','postgresql database')")
    db.create_bm25_index("it_bm25_empty")  # must not crash on the empty-content row
    ranked = db.bm25_query("it_bm25_empty", "zzznomatchterm", 5)  # must not error
    assert isinstance(ranked, list)  # top-k ranker returns a (possibly full) list, never raises
