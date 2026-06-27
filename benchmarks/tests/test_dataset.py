"""Unit tests for seeded dataset generation."""
import numpy as np
import pytest

from theodb_bench.dataset import make_dataset


def test_dataset_deterministic_for_same_seed():
    c1, q1 = make_dataset(10, 4, 3, seed=42)
    c2, q2 = make_dataset(10, 4, 3, seed=42)
    assert np.array_equal(c1, c2)
    assert np.array_equal(q1, q2)


def test_dataset_differs_for_different_seed():
    c1, _ = make_dataset(10, 4, 3, seed=1)
    c2, _ = make_dataset(10, 4, 3, seed=2)
    assert not np.array_equal(c1, c2)


def test_dataset_shapes():
    corpus, queries = make_dataset(10, 4, 3, seed=0)
    assert corpus.shape == (10, 4)
    assert queries.shape == (3, 4)


def test_dataset_zero_n_raises():
    with pytest.raises(ValueError):
        make_dataset(0, 4, 3, seed=0)


def test_dataset_zero_dim_raises():
    with pytest.raises(ValueError):
        make_dataset(10, 0, 3, seed=0)
