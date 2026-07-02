"""M32 scale benchmark — unit tests (neighbors-GT + 4-way config) + integration gate (4-way on real SIFT).

The unit tests need no container. The integration test runs the 4-way harness (theodb_ivfflat, theodb_hnsw,
pgvector ivfflat, pgvector hnsw) on a small real-SIFT subsample against the container (ADR-4: CI-safe N; the
≥1M run is the operator artifact). Skips cleanly if SIFT1M is not cached.
"""
from __future__ import annotations

import os
from pathlib import Path

import numpy as np
import pytest

from theodb_bench.__main__ import build_config, build_parser
from theodb_bench.dataset import load_hdf5_full
from theodb_bench.recall import brute_force_ground_truth, neighbors_ground_truth

# Resolve cwd-independently (pytest rootdir may be benchmarks/ or the repo root): the dataset lives at
# benchmarks/.datasets/ relative to this test file's parent-parent.
SIFT = str(Path(__file__).resolve().parents[1] / ".datasets" / "sift-128-euclidean.hdf5")


def _tiny_hdf5(tmp_path, n=200, dim=8, q=15, k=20, seed=1):
    """Write a tiny ANN-Benchmarks-shaped HDF5 (train/test/neighbors) with EXACT neighbors, for GT tests."""
    import h5py

    rng = np.random.default_rng(seed)
    train = rng.standard_normal((n, dim)).astype(np.float32)
    test = rng.standard_normal((q, dim)).astype(np.float32)
    # exact neighbors (float32 contract) — the ground-truth ids the loader must reproduce distances for
    idx, _ = brute_force_ground_truth(train, test, k, "l2")
    path = tmp_path / "tiny.hdf5"
    with h5py.File(path, "w") as f:
        f.create_dataset("train", data=train)
        f.create_dataset("test", data=test)
        f.create_dataset("neighbors", data=idx.astype(np.int32))
    return str(path), train, test


def test_neighbors_ground_truth_matches_brute_force(tmp_path):
    """neighbors-GT (distances recomputed from neighbor VECTORS) must equal exact brute-force GT within eps."""
    _, train, test = _tiny_hdf5(tmp_path)
    k = 10
    nbr_idx, bf_dist = brute_force_ground_truth(train, test, k, "l2")
    gt = neighbors_ground_truth(train, test, nbr_idx, k, "l2")
    assert gt.shape == (test.shape[0], k)
    assert np.allclose(gt, bf_dist, atol=1e-4), f"neighbors-GT diverges from brute force: max {np.abs(gt - bf_dist).max()}"


def test_neighbors_ground_truth_cosine_matches_brute_force(tmp_path):
    """The cosine branch of neighbors-GT must also equal exact brute-force cosine GT within eps."""
    _, train, test = _tiny_hdf5(tmp_path)
    k = 8
    nbr_idx, bf_dist = brute_force_ground_truth(train, test, k, "cosine")
    gt = neighbors_ground_truth(train, test, nbr_idx, k, "cosine")
    assert np.allclose(gt, bf_dist, atol=1e-4), f"cosine neighbors-GT diverges: max {np.abs(gt - bf_dist).max()}"


def test_neighbors_ground_truth_rejects_out_of_range_ids(tmp_path):
    """Out-of-range neighbor ids fail fast (numpy would silently wrap → wrong GT)."""
    _, train, test = _tiny_hdf5(tmp_path)
    bad = np.full((test.shape[0], 10), train.shape[0] + 5, dtype=np.int64)  # all out of range
    with pytest.raises(ValueError):
        neighbors_ground_truth(train, test, bad, 10, "l2")


def test_load_hdf5_full_returns_full_train_and_gt(tmp_path):
    """load_hdf5_full returns the FULL train (no subsample) + GT distances matching brute force."""
    path, train, _ = _tiny_hdf5(tmp_path, n=200, q=15)
    corpus, queries, gt = load_hdf5_full(path, n_queries=10, seed=42, k=10, metric="l2")
    assert corpus.shape[0] == train.shape[0], "full train must not be subsampled"
    assert queries.shape[0] == 10
    assert gt.shape == (10, 10)
    _, bf = brute_force_ground_truth(corpus, queries, 10, "l2")
    assert np.allclose(gt, bf, atol=1e-4)


def test_load_hdf5_full_missing_file_raises():
    with pytest.raises(FileNotFoundError):
        load_hdf5_full("/nonexistent/sift.hdf5", n_queries=10, seed=42, k=10, metric="l2")


def test_load_hdf5_full_missing_neighbors_raises(tmp_path):
    """An HDF5 without a `neighbors` dataset cannot use neighbors-GT — fail fast, not a silent wrong GT."""
    import h5py

    rng = np.random.default_rng(3)
    path = tmp_path / "no_nbr.hdf5"
    with h5py.File(path, "w") as f:
        f.create_dataset("train", data=rng.standard_normal((50, 8)).astype(np.float32))
        f.create_dataset("test", data=rng.standard_normal((10, 8)).astype(np.float32))
    with pytest.raises(ValueError, match="neighbors"):
        load_hdf5_full(str(path), n_queries=5, seed=42, k=5, metric="l2")


def test_load_hdf5_full_k_exceeds_neighbors_raises(tmp_path):
    """k larger than the file's precomputed neighbor count must fail fast."""
    path, _, _ = _tiny_hdf5(tmp_path, k=8)  # neighbors has only 8 columns
    with pytest.raises(ValueError):
        load_hdf5_full(path, n_queries=5, seed=42, k=20, metric="l2")


def test_build_config_4way_has_four_specs():
    """--index 4way yields the 4 index families with correct theodb DDL + NO sweep for theodb (ADR-3)."""
    cfg = build_config(build_parser().parse_args(["--index", "4way", "--metric", "l2", "--n", "20000"]))
    names = {s["name"] for s in cfg["index_specs"]}
    assert names == {"hnsw", "ivfflat", "theodb_ivfflat", "theodb_hnsw"}
    by = {s["name"]: s for s in cfg["index_specs"]}
    assert "USING theodb_ivfflat (embedding theodb_ivfflat_l2_ops)" in by["theodb_ivfflat"]["ddl"]
    assert "USING theodb_hnsw (embedding theodb_hnsw_l2_ops)" in by["theodb_hnsw"]["ddl"]
    for tname in ("theodb_ivfflat", "theodb_hnsw"):
        sweep = by[tname]["sweep"]
        assert len(sweep) == 1, f"{tname} must be a single fixed op-point (ADR-3)"
        assert sweep[0]["session"] == ["SET enable_seqscan = off"], "theodb has no ef/probes knob (ADR-3)"


def test_theodb_specs_l2_only_skips_on_cosine():
    """theodb AMs are l2-only (ADR-2); --metric cosine --index 4way must NOT emit a fabricated cosine opclass."""
    cfg = build_config(build_parser().parse_args(["--index", "4way", "--metric", "cosine", "--n", "20000"]))
    names = {s["name"] for s in cfg["index_specs"]}
    assert "theodb_ivfflat" not in names and "theodb_hnsw" not in names
    for s in cfg["index_specs"]:
        assert "theodb_" not in s["ddl"]


def test_theodb_only_cosine_raises_empty_specs():
    """theodb-only + cosine leaves zero specs — fail fast, not a silent empty benchmark after the expensive load."""
    with pytest.raises(ValueError, match="no index specs"):
        build_config(build_parser().parse_args(["--index", "theodb_hnsw", "--metric", "cosine", "--n", "20000"]))


def test_query_cap_must_be_positive():
    with pytest.raises(ValueError, match="query-cap must be >= 1"):
        build_config(build_parser().parse_args(["--index", "4way", "--n", "20000", "--theodb-hnsw-query-cap", "0"]))


def test_theodb_hnsw_query_cap_wired():
    """--theodb-hnsw-query-cap caps ONLY theodb_hnsw's query sample (its scan is O(N)-per-query)."""
    cfg = build_config(build_parser().parse_args(
        ["--index", "4way", "--metric", "l2", "--n", "20000", "--theodb-hnsw-query-cap", "50"]
    ))
    by = {s["name"]: s for s in cfg["index_specs"]}
    assert by["theodb_hnsw"].get("query_cap") == 50
    # the other three are NOT capped (full query set)
    for name in ("hnsw", "ivfflat", "theodb_ivfflat"):
        assert by[name].get("query_cap") is None


def test_query_cap_limits_queries_in_harness(tmp_path):
    """A spec with query_cap=N must run exactly N queries (recall/percentiles over the capped sample)."""
    from theodb_bench.harness import run_benchmark

    class _CountingDB:
        def __init__(self):
            self.topk_calls = 0
            self.corpus = None

        def ensure_extension(self):
            pass

        def create_table(self, *a, **k):
            pass

        def load_vectors(self, table, corpus, embed_col="embedding"):
            self.corpus = np.asarray(corpus, dtype=np.float64)

        def build_index(self, ddl):
            return 0.001

        def set_session(self, stmt):
            pass

        def assert_index_used(self, *a, **k):
            pass

        def index_size_bytes(self, name):
            return 123

        def query_topk(self, table, qvec, k, metric="l2", embed_col="embedding"):
            self.topk_calls += 1
            # exact top-k from the loaded corpus so recall is well-defined
            idx, dist = brute_force_ground_truth(self.corpus, np.asarray([qvec]), k, metric)
            return list(idx[0]), list(dist[0]), 0.001

    db = _CountingDB()
    cfg = {
        "seed": 1, "n": 100, "dim": 8, "n_queries": 40, "k": 5, "metric": "l2", "runs": 2,
        "table": "t",
        "index_specs": [{"name": "capped", "index_name": "ix", "ddl": "CREATE INDEX ix ...",
                         "sweep": [{"label": "fixed", "session": []}], "query_cap": 10}],
    }
    report = run_benchmark(cfg, db, tmp_path)
    # warmup(10) + runs(2)*10 = 30 topk calls for the capped spec (NOT 40*3=120)
    assert db.topk_calls == 30, db.topk_calls
    assert "[q=10]" in report["results"][0]["params"]


@pytest.mark.integration
def test_4way_scale_harness_runs_on_real_sift():
    """4-way harness end-to-end on a small real-SIFT subsample against the container (ADR-4 CI-safe N)."""
    if not Path(SIFT).is_file():
        pytest.skip(f"SIFT1M not cached at {SIFT}; run the download in the M32 repro command")
    from theodb_bench.db import VectorDB
    from theodb_bench.harness import run_benchmark

    dsn = (
        f"host={os.environ.get('PGHOST','localhost')} port={os.environ.get('PGPORT','5432')} "
        f"dbname={os.environ.get('PGDATABASE','postgres')} user={os.environ.get('PGUSER','postgres')} "
        f"password={os.environ.get('PGPASSWORD','postgres')}"
    )
    cfg = build_config(build_parser().parse_args(
        ["--index", "4way", "--metric", "l2", "--n", "20000", "--n-queries", "200", "--runs", "3",
         "--hdf5", SIFT]
    ))
    db = VectorDB(dsn).connect()
    try:
        import tempfile
        report = run_benchmark(cfg, db, tempfile.mkdtemp())
    finally:
        db.close()
    fams = {r["index"] for r in report["results"]}
    assert fams == {"hnsw", "ivfflat", "theodb_ivfflat", "theodb_hnsw"}, f"missing families: {fams}"
    for r in report["results"]:
        assert 0.0 <= r["recall_at_k"] <= 1.0, r
        assert r["qps"] > 0 and r["build_ms"] > 0 and r["index_bytes"] > 0, r
