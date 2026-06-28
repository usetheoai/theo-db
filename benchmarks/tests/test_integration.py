"""Integration tests — run against a real theo-db:dev container (pgvector).

Marked `integration`; skipped by `pytest -m "not integration"`. Connection from PG* env vars
(PGHOST/PGPORT/PGUSER/PGPASSWORD/PGDATABASE).
"""
import os

import pytest

from theodb_bench.db import IndexNotUsedError, VectorDB
from theodb_bench.dataset import make_dataset
from theodb_bench.harness import run_benchmark

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


def test_extension_load_and_topk_query(db):
    db.ensure_extension()
    db.create_table("it_vectors", 8)
    corpus, queries = make_dataset(300, 8, 5, seed=3)
    db.load_vectors("it_vectors", corpus)
    db.build_index("CREATE INDEX it_hnsw ON it_vectors USING hnsw (embedding vector_l2_ops)")
    # Force the index on (pgvector recall-test methodology) — on a small table the planner would
    # otherwise pick a seqscan, and we want to exercise the index path.
    db.set_session("SET enable_seqscan = off")
    db.set_session("SET hnsw.ef_search = 100")
    db.assert_index_used("it_vectors", queries[0], 5, "l2")
    ids, dists, latency = db.query_topk("it_vectors", queries[0], 5, "l2")
    assert len(ids) == 5
    assert latency > 0
    assert db.index_size_bytes("it_hnsw") > 0


def test_index_not_used_raises(db):
    db.ensure_extension()
    db.create_table("it_seq", 8)
    corpus, queries = make_dataset(100, 8, 2, seed=5)
    db.load_vectors("it_seq", corpus)
    db.build_index("CREATE INDEX it_seq_hnsw ON it_seq USING hnsw (embedding vector_l2_ops)")
    db.set_session("SET enable_indexscan = off")  # force the planner away from the index
    db.set_session("SET enable_bitmapscan = off")
    with pytest.raises(IndexNotUsedError):
        db.assert_index_used("it_seq", queries[0], 5, "l2")


def test_vectorscale_extension_and_diskann_index(db):
    db.ensure_extension()
    db.set_session("CREATE EXTENSION IF NOT EXISTS vectorscale CASCADE")
    db.create_table("it_scale", 32)
    corpus, queries = make_dataset(500, 32, 3, seed=9)
    db.load_vectors("it_scale", corpus)
    db.build_index("CREATE INDEX it_scale_dann ON it_scale USING diskann (embedding vector_cosine_ops)")
    db.set_session("SET enable_seqscan = off")
    db.set_session("SET diskann.query_search_list_size = 100")
    db.assert_index_used("it_scale", queries[0], 5, "cosine")  # diskann index actually used
    ids, dists, latency = db.query_topk("it_scale", queries[0], 5, "cosine")
    assert len(ids) == 5
    assert latency > 0
    assert db.index_size_bytes("it_scale_dann") > 0


def test_harness_measures_diskann(db, tmp_path):
    from theodb_bench.__main__ import build_config, build_parser
    from theodb_bench.harness import run_benchmark

    args = build_parser().parse_args(
        ["--index", "diskann", "--metric", "cosine", "--seed", "7",
         "--n", "2000", "--dim", "32", "--n-queries", "30", "--k", "10", "--runs", "2"]
    )
    report = run_benchmark(build_config(args), db, tmp_path)
    diskann = [r for r in report["results"] if r["index"] == "diskann"]
    assert diskann, "harness produced no diskann results"
    assert all(0.0 <= r["recall_at_k"] <= 1.0 and r["qps"] > 0 for r in diskann)
    # at high sls on low dim, diskann/SBQ reaches high recall (rescore scales with sls up to the
    # engine ceiling, so the curve climbs to the plan's >= 0.90 acceptance bound)
    assert max(r["recall_at_k"] for r in diskann) >= 0.90


def test_hnsw_recall_is_high_vs_exact(db, tmp_path):
    config = {
        "seed": 7, "n": 2000, "dim": 32, "n_queries": 50, "k": 10, "metric": "l2", "runs": 3,
        "table": "it_bench",
        "index_specs": [
            {
                "name": "hnsw", "index_name": "it_bench_hnsw",
                "ddl": "CREATE INDEX it_bench_hnsw ON it_bench USING hnsw (embedding vector_l2_ops)",
                "sweep": [{"label": "ef_search=100", "session": ["SET enable_seqscan = off", "SET hnsw.ef_search = 100"]}],
            }
        ],
    }
    report = run_benchmark(config, db, tmp_path)
    r = report["results"][0]
    assert 0.0 <= r["recall_at_k"] <= 1.0
    assert r["recall_at_k"] >= 0.90  # HNSW vs exact ground-truth should recall high
    assert r["qps"] > 0
    assert r["build_ms"] > 0
    assert r["index_bytes"] > 0


# --- M7-S1: ai.hybrid_search_rrf contract (FTS + vector + RRF) -----------------------------------
# Deterministic: explicit query_vector (no embedding endpoint call). vector(3) toy space.
import psycopg2  # noqa: E402


def _raw_conn():
    c = psycopg2.connect(_dsn())
    c.autocommit = True
    return c


def _seed_documents(cur, table: str, rows: list[tuple]) -> None:
    """rows = [(doc_id, content, embedding_or_None)]. tsv is GENERATED from content (english)."""
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    cur.execute(
        f"CREATE TABLE {table} ("
        f"  doc_id text PRIMARY KEY,"
        f"  content text,"
        f"  text_tsv tsvector GENERATED ALWAYS AS (to_tsvector('english', coalesce(content,''))) STORED,"
        f"  embedding vector(3))"
    )
    cur.execute(f"CREATE INDEX {table}_gin ON {table} USING gin (text_tsv)")
    for doc_id, content, emb in rows:
        cur.execute(
            f"INSERT INTO {table}(doc_id, content, embedding) VALUES (%s, %s, %s)",
            (doc_id, content, emb),
        )


def _hybrid(cur, table, *, query_text=None, query_vector=None, k=60, per_leg_limit=20, result_limit=5):
    cur.execute(
        "SELECT id, score FROM ai.hybrid_search_rrf("
        "  tbl => %s::regclass, id_col => 'doc_id', content_tsv_col => 'text_tsv', vector_col => 'embedding',"
        "  query_text => %s, query_vector => %s, k => %s, per_leg_limit => %s, result_limit => %s)",
        (table, query_text, query_vector, k, per_leg_limit, result_limit),
    )
    return cur.fetchall()


def test_hybrid_fuses_both_legs():
    conn = _raw_conn()
    try:
        with conn.cursor() as cur:
            _seed_documents(cur, "hyb_both", [
                ("d1", "database systems", "[1,0,0]"),   # matches FTS 'database' AND near query vector
                ("d2", "database tuning",  "[0,1,0]"),   # matches FTS 'database', far vector
                ("d3", "cooking recipes",  "[1,0,0]"),   # near query vector, no FTS 'database' match
            ])
            rows = _hybrid(cur, "hyb_both", query_text="database", query_vector="[1,0,0]", result_limit=5)
            ids = [r[0] for r in rows]
            assert ids[0] == "d1", f"both-legs doc must rank first, got {rows}"
            assert set(ids) == {"d1", "d2", "d3"}, f"all docs surface via fusion, got {ids}"
    finally:
        conn.close()


def test_hybrid_empty_fts_leg():
    conn = _raw_conn()
    try:
        with conn.cursor() as cur:
            _seed_documents(cur, "hyb_nofts", [
                ("d1", "cooking recipes", "[1,0,0]"),
                ("d2", "garden tools",    "[0,1,0]"),
            ])
            # query_text matches NO row via @@ → FTS leg empty; vector-only docs still returned.
            rows = _hybrid(cur, "hyb_nofts", query_text="zzznomatch", query_vector="[1,0,0]")
            ids = [r[0] for r in rows]
            assert "d1" in ids, f"vector-only doc must surface when FTS leg empty, got {rows}"
            assert all(r[1] > 0 for r in rows), "scores positive (vector leg contributes)"
    finally:
        conn.close()


def test_hybrid_empty_vector_leg():
    conn = _raw_conn()
    try:
        with conn.cursor() as cur:
            # embeddings are NULL → vector leg empty (WHERE embedding IS NOT NULL); FTS-only docs surface.
            _seed_documents(cur, "hyb_novec", [
                ("d1", "database systems", None),
                ("d2", "database tuning",  None),
            ])
            rows = _hybrid(cur, "hyb_novec", query_text="database", query_vector="[1,0,0]")
            ids = [r[0] for r in rows]
            assert set(ids) == {"d1", "d2"}, f"FTS-only docs must surface when vector leg empty, got {rows}"
    finally:
        conn.close()


def test_hybrid_invalid_k_raises():
    conn = _raw_conn()
    try:
        with conn.cursor() as cur:
            _seed_documents(cur, "hyb_k0", [("d1", "database", "[1,0,0]")])
            with pytest.raises(psycopg2.errors.InvalidParameterValue):  # SQLSTATE 22023
                _hybrid(cur, "hyb_k0", query_text="database", query_vector="[1,0,0]", k=0)
    finally:
        conn.close()


def test_hybrid_k_param_changes_score():
    conn = _raw_conn()
    try:
        with conn.cursor() as cur:
            _seed_documents(cur, "hyb_kp", [
                ("d1", "database systems", "[1,0,0]"),
                ("d2", "database tuning",  "[0,1,0]"),
            ])
            top_k1 = _hybrid(cur, "hyb_kp", query_text="database", query_vector="[1,0,0]", k=1)[0]
            top_k60 = _hybrid(cur, "hyb_kp", query_text="database", query_vector="[1,0,0]", k=60)[0]
            assert top_k1[0] == top_k60[0] == "d1"
            assert abs(top_k1[1] - top_k60[1]) > 1e-4, "k must change the fused score (param wired, not hardcoded)"
    finally:
        conn.close()
