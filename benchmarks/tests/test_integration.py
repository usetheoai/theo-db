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
    # at high sls on low dim, diskann/SBQ reaches high recall (the curve climbs with sls)
    assert max(r["recall_at_k"] for r in diskann) >= 0.80


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
