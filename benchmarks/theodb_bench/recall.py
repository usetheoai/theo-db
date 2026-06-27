"""Recall@k (distance-thresholded) + exact brute-force ground-truth.

Recall semantics follow ANN-Benchmarks (Aumüller, Bernhardsson, Faithfull, arXiv:1807.05614 §2.1):
recall = |{ returned point at distance <= dist(k-th true neighbour) + eps }| / k, computed from
DISTANCES (not id-overlap). This is the load-bearing correctness choice — id-overlap diverges from
the field standard under tied/duplicate distances (blueprint ADR D2).

Distances are computed in the SAME metric the database returns, so the threshold comparison is
apples-to-apples: 'l2' == Euclidean (pgvector `<->`), 'cosine' == 1 - cosine_similarity (pgvector `<=>`).
"""
from __future__ import annotations

import numpy as np

_METRICS = ("l2", "cosine")


def _pairwise_distances(corpus: np.ndarray, queries: np.ndarray, metric: str) -> np.ndarray:
    corpus = np.asarray(corpus, dtype=np.float64)
    queries = np.asarray(queries, dtype=np.float64)
    if corpus.shape[0] == 0:
        raise ValueError("corpus is empty")
    if queries.shape[0] == 0:
        raise ValueError("queries is empty")
    if metric == "l2":
        c2 = np.sum(corpus ** 2, axis=1)[None, :]
        q2 = np.sum(queries ** 2, axis=1)[:, None]
        d2 = q2 + c2 - 2.0 * (queries @ corpus.T)
        np.maximum(d2, 0.0, out=d2)  # clamp tiny negatives from float error
        return np.sqrt(d2)
    if metric == "cosine":
        cn = corpus / np.linalg.norm(corpus, axis=1, keepdims=True)
        qn = queries / np.linalg.norm(queries, axis=1, keepdims=True)
        return 1.0 - (qn @ cn.T)
    raise ValueError(f"unknown metric: {metric!r} (expected one of {_METRICS})")


def brute_force_ground_truth(
    corpus: np.ndarray, queries: np.ndarray, k: int, metric: str = "l2"
) -> tuple[np.ndarray, np.ndarray]:
    """Exact k-NN ground truth. Returns (indices (Q,k) int, distances (Q,k) float), sorted ascending.

    Raises ValueError on empty corpus/queries, unknown metric, or k > corpus size.
    """
    dists = _pairwise_distances(corpus, queries, metric)
    n = dists.shape[1]
    if k < 1:
        raise ValueError(f"k must be >= 1, got {k}")
    if k > n:
        raise ValueError(f"k={k} exceeds corpus size {n}")
    part = np.argpartition(dists, k - 1, axis=1)[:, :k]
    rows = np.arange(dists.shape[0])[:, None]
    part_d = dists[rows, part]
    order = np.argsort(part_d, axis=1)
    return part[rows, order], part_d[rows, order]


def recall_at_k(
    true_distances: np.ndarray, run_distances: list, k: int, eps: float = 1e-3
) -> float:
    """Mean recall@k over all queries, distance-thresholded.

    true_distances: (Q,k') exact sorted distances (k' >= k).
    run_distances: list of length Q; each is the distances returned by the index for that query.
    """
    true_distances = np.asarray(true_distances, dtype=np.float64)
    if true_distances.ndim != 2 or true_distances.shape[1] < k:
        raise ValueError(f"true_distances must be (Q, >= {k}); got shape {true_distances.shape}")
    if len(run_distances) != true_distances.shape[0]:
        raise ValueError(
            f"run_distances length {len(run_distances)} != number of queries {true_distances.shape[0]}"
        )
    recalls = []
    for i, run in enumerate(run_distances):
        run = np.asarray(run, dtype=np.float64)
        threshold = true_distances[i, k - 1] + eps
        hits = int(np.sum(run[:k] <= threshold))
        recalls.append(hits / k)
    return float(np.mean(recalls)) if recalls else 0.0
