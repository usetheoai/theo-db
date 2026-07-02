"""M33 CI-safe guard: ScaNN is scored by the SAME recall path as theodb (fairness), and the exact-distance
recomputation makes recall reproducible. The full SIFT1M run is the operator artifact (run_m33_scann.py),
NOT a CI test — here we only prove the scoring path is identical + honest on tiny data.

Skips cleanly when scann is absent (no AVX2 / not installed) so CI stays green on hosts without the dev lib.
"""
from __future__ import annotations

import numpy as np
import pytest

from theodb_bench.recall import brute_force_ground_truth, recall_at_k

scann = pytest.importorskip("scann", reason="scann not installed (dev-only baseline; needs AVX2)")

from run_m33_scann import _exact_l2  # noqa: E402  (import after skip guard)


def test_exact_l2_matches_bruteforce_metric():
    # GIVEN a tiny corpus + one query
    rng = np.random.default_rng(7)
    corpus = rng.standard_normal((50, 8)).astype("float32")
    query = rng.standard_normal((8,)).astype("float32")
    # WHEN _exact_l2 distances the query to a few corpus rows
    ids = np.array([3, 17, 42])
    got = _exact_l2(query, corpus[ids])
    # THEN they equal the brute-force Euclidean distances (same float32-value contract, l2 metric)
    gt_idx, gt_d = brute_force_ground_truth(corpus, query[None, :], k=50, metric="l2")
    pos = {int(g): float(d) for g, d in zip(gt_idx[0], gt_d[0])}
    for j, dist in zip(ids, got):
        assert dist == pytest.approx(pos[int(j)], rel=1e-6)


def test_scann_recall_matches_theodb_recall_semantics():
    # GIVEN a tiny corpus + queries with exact GT (the SAME path theodb recall uses)
    rng = np.random.default_rng(0)
    corpus = rng.standard_normal((2000, 16)).astype("float32")
    queries = rng.standard_normal((20, 16)).astype("float32")
    k = 10
    _gt_idx, gt_dist = brute_force_ground_truth(corpus, queries, k, metric="l2")

    # WHEN ScaNN searches with brute-force scoring (exact) and we recompute exact distances of the returned ids
    searcher = (
        scann.scann_ops_pybind.builder(corpus, k, "squared_l2")
        .score_brute_force()
        .build()
    )
    run_distances = []
    for i in range(queries.shape[0]):
        idx, _ = searcher.search(queries[i], final_num_neighbors=k)
        run_distances.append(_exact_l2(queries[i], corpus[idx]))

    # THEN recall@10 (via the SAME theodb recall_at_k, distance-thresholded) is ~1.0 — exact scoring finds true NN
    recall = recall_at_k(gt_dist, run_distances, k)
    assert recall >= 0.99, f"exact ScaNN scoring must reach recall ~1.0, got {recall}"
    assert 0.0 <= recall <= 1.0
