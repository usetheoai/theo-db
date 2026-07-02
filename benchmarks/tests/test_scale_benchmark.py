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

SIFT = "benchmarks/.datasets/sift-128-euclidean.hdf5"


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
