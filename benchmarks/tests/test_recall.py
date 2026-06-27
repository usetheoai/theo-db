"""Unit tests for recall@k (distance-thresholded, ANN-Benchmarks semantics) + ground-truth."""
import numpy as np
import pytest

from theodb_bench.recall import brute_force_ground_truth, recall_at_k


def test_recall_perfect_when_run_equals_truth():
    true_d = np.array([[0.1, 0.2]])
    assert recall_at_k(true_d, [np.array([0.1, 0.2])], k=2) == 1.0


def test_recall_half_when_one_of_two_missed():
    true_d = np.array([[0.1, 0.2]])
    assert recall_at_k(true_d, [np.array([0.1, 0.9])], k=2) == 0.5


def test_recall_tie_at_kth_counts_equivalent():
    # An equally-distant alternative (0.2 == k-th true distance) must count — the
    # distance-threshold definition is what makes this correct (id-overlap would miss it).
    true_d = np.array([[0.1, 0.2]])
    assert recall_at_k(true_d, [np.array([0.1, 0.2])], k=2) == 1.0


def test_recall_in_unit_interval():
    rng = np.random.default_rng(0)
    for _ in range(20):
        true_d = np.sort(rng.random((3, 5)), axis=1)
        run = [np.sort(rng.random(5)) for _ in range(3)]
        r = recall_at_k(true_d, run, k=3)
        assert 0.0 <= r <= 1.0


def test_ground_truth_l2_matches_known_neighbors():
    corpus = np.array([[0, 0], [1, 0], [5, 5]], dtype=float)
    queries = np.array([[0, 0]], dtype=float)
    idx, d = brute_force_ground_truth(corpus, queries, k=2, metric="l2")
    assert list(idx[0]) == [0, 1]
    assert np.allclose(d[0], [0.0, 1.0])


def test_ground_truth_cosine_matches_known():
    corpus = np.array([[1, 0], [0, 1], [-1, 0]], dtype=float)
    queries = np.array([[1, 0]], dtype=float)
    idx, d = brute_force_ground_truth(corpus, queries, k=1, metric="cosine")
    assert idx[0][0] == 0
    assert np.allclose(d[0][0], 0.0)


def test_k_greater_than_n_raises_valueerror():
    corpus = np.array([[0, 0], [1, 1]], dtype=float)
    with pytest.raises(ValueError):
        brute_force_ground_truth(corpus, corpus, k=5, metric="l2")


def test_empty_corpus_raises_valueerror():
    with pytest.raises(ValueError):
        brute_force_ground_truth(np.empty((0, 2)), np.array([[0, 0]], dtype=float), k=1, metric="l2")


def test_unknown_metric_raises():
    corpus = np.array([[0, 0], [1, 1]], dtype=float)
    with pytest.raises(ValueError):
        brute_force_ground_truth(corpus, corpus, k=1, metric="manhattan")


def test_empty_queries_raises_valueerror():
    corpus = np.array([[0, 0], [1, 1]], dtype=float)
    with pytest.raises(ValueError):
        brute_force_ground_truth(corpus, np.empty((0, 2)), k=1, metric="l2")


def test_k_less_than_one_raises():
    corpus = np.array([[0, 0], [1, 1]], dtype=float)
    with pytest.raises(ValueError):
        brute_force_ground_truth(corpus, corpus, k=0, metric="l2")


def test_recall_bad_true_shape_raises():
    with pytest.raises(ValueError):
        recall_at_k(np.array([0.1, 0.2]), [np.array([0.1, 0.2])], k=2)  # 1-D, not (Q,k)


def test_recall_run_length_mismatch_raises():
    true_d = np.array([[0.1, 0.2], [0.3, 0.4]])  # 2 queries
    with pytest.raises(ValueError):
        recall_at_k(true_d, [np.array([0.1, 0.2])], k=2)  # only 1 run row
