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
    # pgvector stores `vector` as float4 (single precision); the exact oracle must rank the SAME
    # float32-precision values the index sees, else near-ties diverge between oracle and SUT.
    # Round to float32, then compute the distance itself in float64 for numerical stability. (review M1)
    corpus = np.asarray(corpus, dtype=np.float32).astype(np.float64)
    queries = np.asarray(queries, dtype=np.float32).astype(np.float64)
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


def neighbors_ground_truth(
    train: np.ndarray, queries: np.ndarray, neighbor_ids: np.ndarray, k: int, metric: str = "l2"
) -> np.ndarray:
    """Exact GT distances from precomputed neighbor ids (ANN-Benchmarks HDF5 ``neighbors``) — the ≥1M scale
    unlock (M32 ADR-2). Instead of the O(N·Q) brute force, distance each query ONLY to its own precomputed
    true-neighbor vectors (``train[neighbor_ids[q]]``): 10⁶ ops at SIFT1M, not 10¹⁰. Recomputes distances from
    the neighbor VECTORS (never trusts a shipped ``distances`` array) so the float32 contract matches
    :func:`brute_force_ground_truth` exactly.

    train: (N,dim); queries: (Q,dim); neighbor_ids: (Q, k') int ids into train with k' >= k.
    Returns gt_dist (Q,k) float64, sorted ascending per row. Chunked over queries to bound RSS at 1M scale.
    """
    if k < 1:
        raise ValueError(f"k must be >= 1, got {k}")
    neighbor_ids = np.asarray(neighbor_ids)
    if neighbor_ids.ndim != 2 or neighbor_ids.shape[1] < k:
        raise ValueError(f"neighbor_ids must be (Q, >= {k}); got shape {neighbor_ids.shape}")
    if neighbor_ids.shape[0] != queries.shape[0]:
        raise ValueError(
            f"neighbor_ids rows {neighbor_ids.shape[0]} != number of queries {queries.shape[0]}"
        )
    if metric not in _METRICS:
        raise ValueError(f"unknown metric: {metric!r} (expected one of {_METRICS})")
    # Keep train in float32 (bounds RSS at 1M×128 ≈ 512 MB); cast only the small per-chunk gather to float64
    # for a numerically stable distance — the SAME float32-value contract as _pairwise_distances.
    train = np.asarray(train, dtype=np.float32)
    # Fail-fast on out-of-range / negative ids: numpy would silently wrap a bad id to a valid row (wrong GT).
    if train.shape[0] == 0:
        raise ValueError("train is empty")
    if neighbor_ids.min() < 0 or neighbor_ids.max() >= train.shape[0]:
        raise ValueError(
            f"neighbor_ids out of range [0, {train.shape[0]}): min={neighbor_ids.min()} max={neighbor_ids.max()}"
        )
    q_all = np.asarray(queries, dtype=np.float32).astype(np.float64)
    n_queries = q_all.shape[0]
    out = np.empty((n_queries, k), dtype=np.float64)
    chunk = 512
    for start in range(0, n_queries, chunk):
        end = min(start + chunk, n_queries)
        ids = neighbor_ids[start:end, :]  # (c, k')
        nbr = train[ids].astype(np.float64)  # (c, k', dim) — small gather
        qs = q_all[start:end]  # (c, dim)
        if metric == "l2":
            diff = qs[:, None, :] - nbr  # (c, k', dim)
            d2 = np.sum(diff * diff, axis=2)
            np.maximum(d2, 0.0, out=d2)
            d = np.sqrt(d2)
        else:  # cosine
            nn = nbr / np.linalg.norm(nbr, axis=2, keepdims=True)
            qn = qs / np.linalg.norm(qs, axis=1, keepdims=True)
            d = 1.0 - np.einsum("cd,ckd->ck", qn, nn)
        d.sort(axis=1)  # ascending — the k-th is the recall threshold
        out[start:end, :] = d[:, :k]
    return out


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
