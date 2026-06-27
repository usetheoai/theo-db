"""Unit tests for the ANN-Benchmarks HDF5 loader (tiny synthetic fixture — no 122MB dependency)."""
import h5py
import numpy as np
import pytest

from theodb_bench.dataset import load_hdf5_subsample


@pytest.fixture
def tiny_hdf5(tmp_path):
    """An ANN-Benchmarks-shaped HDF5: train (50,4), test (10,4) — like glove-*-angular but tiny."""
    path = tmp_path / "tiny-angular.hdf5"
    rng = np.random.default_rng(0)
    with h5py.File(path, "w") as f:
        f.create_dataset("train", data=rng.standard_normal((50, 4)).astype(np.float32))
        f.create_dataset("test", data=rng.standard_normal((10, 4)).astype(np.float32))
        f.attrs["distance"] = "angular"
    return str(path)


def test_load_hdf5_returns_requested_shapes(tiny_hdf5):
    corpus, queries = load_hdf5_subsample(tiny_hdf5, n=20, n_queries=5, seed=42)
    assert corpus.shape == (20, 4)
    assert queries.shape == (5, 4)


def test_load_hdf5_is_deterministic(tiny_hdf5):
    c1, q1 = load_hdf5_subsample(tiny_hdf5, n=20, n_queries=5, seed=42)
    c2, q2 = load_hdf5_subsample(tiny_hdf5, n=20, n_queries=5, seed=42)
    assert np.array_equal(c1, c2)
    assert np.array_equal(q1, q2)


def test_load_hdf5_subsample_depends_on_seed(tiny_hdf5):
    c1, _ = load_hdf5_subsample(tiny_hdf5, n=20, n_queries=5, seed=1)
    c2, _ = load_hdf5_subsample(tiny_hdf5, n=20, n_queries=5, seed=2)
    assert not np.array_equal(c1, c2)  # different seed -> different subsample


def test_load_hdf5_full_corpus_when_n_equals_train(tiny_hdf5):
    corpus, _ = load_hdf5_subsample(tiny_hdf5, n=50, n_queries=10, seed=7)
    assert corpus.shape == (50, 4)  # edge: request the whole train set


def test_load_hdf5_rejects_oversized_n(tiny_hdf5):
    # negative case: asking for more corpus than the file holds is a typed, clear failure
    with pytest.raises(ValueError, match=r"n=\d+ exceeds train size"):
        load_hdf5_subsample(tiny_hdf5, n=999, n_queries=5, seed=42)


def test_load_hdf5_rejects_oversized_queries(tiny_hdf5):
    with pytest.raises(ValueError, match=r"n_queries=\d+ exceeds test size"):
        load_hdf5_subsample(tiny_hdf5, n=20, n_queries=999, seed=42)


def test_load_hdf5_missing_file_raises(tmp_path):
    with pytest.raises(FileNotFoundError):
        load_hdf5_subsample(str(tmp_path / "nope.hdf5"), n=10, n_queries=2, seed=42)


@pytest.mark.parametrize("n,nq", [(0, 5), (-1, 5), (20, 0), (20, -3)])
def test_load_hdf5_rejects_nonpositive(tiny_hdf5, n, nq):
    # negative: non-positive sizes are a typed domain error (consistent with make_dataset)
    with pytest.raises(ValueError, match=r"must be >= 1"):
        load_hdf5_subsample(tiny_hdf5, n=n, n_queries=nq, seed=42)
