"""Seeded, reproducible synthetic vector datasets (blueprint ADR D3).

Determinism is the point of the gate: same seed -> bit-identical corpus + queries, so a benchmark
run is reproducible. The OSS analogs (pgvector Perl tests, pgvectorscale) do NOT seed their data —
this module closes that gap.
"""
from __future__ import annotations

from pathlib import Path

import numpy as np


def load_hdf5_subsample(
    path: str, n: int, n_queries: int, seed: int
) -> tuple[np.ndarray, np.ndarray]:
    """Load an ANN-Benchmarks HDF5 (``train`` + ``test``) and return a seeded subsample.

    ANN-Benchmarks reference datasets (e.g. ``glove-25-angular``) ship real embedding
    distributions — unlike :func:`make_dataset`'s synthetic gaussian. We subsample a seeded
    ``n`` corpus rows from ``train`` and ``n_queries`` rows from ``test`` so a full-scale run
    stays tractable while preserving the real (clustered) distribution. Output matches
    :func:`make_dataset`: ``(corpus (n,dim), queries (n_queries,dim))`` — so the existing
    ground-truth + recall + harness pipeline runs unchanged on real data.

    Raises FileNotFoundError if the dataset is absent, ValueError if ``n``/``n_queries`` exceed
    the file's train/test sizes.
    """
    if n < 1:
        raise ValueError(f"n must be >= 1, got {n}")
    if n_queries < 1:
        raise ValueError(f"n_queries must be >= 1, got {n_queries}")
    if not Path(path).is_file():
        raise FileNotFoundError(f"HDF5 dataset not found: {path}")
    import h5py  # local import: only the real-dataset path needs h5py

    with h5py.File(path, "r") as f:
        train, test = f["train"], f["test"]
        if n > train.shape[0]:
            raise ValueError(f"n={n} exceeds train size {train.shape[0]} in {path}")
        if n_queries > test.shape[0]:
            raise ValueError(f"n_queries={n_queries} exceeds test size {test.shape[0]} in {path}")
        rng = np.random.default_rng(seed)
        corpus_idx = np.sort(rng.choice(train.shape[0], size=n, replace=False))
        query_idx = np.sort(rng.choice(test.shape[0], size=n_queries, replace=False))
        # h5py fancy-indexing requires sorted, unique indices (satisfied above).
        corpus = train[corpus_idx].astype(np.float64)
        queries = test[query_idx].astype(np.float64)
    return corpus, queries


def load_hdf5_full(
    path: str, n_queries: int, seed: int, k: int, metric: str = "l2"
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Load the FULL ``train`` (no subsample) + a seeded ``n_queries`` subsample of ``test`` from an
    ANN-Benchmarks HDF5, and return exact GT distances derived from the file's precomputed ``neighbors``
    (M32 ADR-2 — the ≥1M scale path). Returns ``(corpus (N,dim) float32, queries (Q,dim) float32,
    gt_dist (Q,k) float64 ascending)``.

    Unlike :func:`load_hdf5_subsample` (which subsamples the corpus and recomputes brute-force GT — fine at
    small N), this keeps the full 1M train so the ``neighbors`` ids stay valid, and computes GT in 10⁶ ops
    via :func:`~theodb_bench.recall.neighbors_ground_truth` instead of the 10¹⁰ brute force.

    Raises FileNotFoundError if absent, ValueError if the file lacks ``neighbors`` or sizes are too small.
    """
    if n_queries < 1:
        raise ValueError(f"n_queries must be >= 1, got {n_queries}")
    if k < 1:
        raise ValueError(f"k must be >= 1, got {k}")
    if not Path(path).is_file():
        raise FileNotFoundError(f"HDF5 dataset not found: {path}")
    import h5py  # local import: only the real-dataset path needs h5py

    from .recall import neighbors_ground_truth

    with h5py.File(path, "r") as f:
        if "neighbors" not in f:
            raise ValueError(f"{path} has no 'neighbors' dataset — needed for neighbors-GT (use --hdf5 subsample)")
        train_ds, test_ds, neigh_ds = f["train"], f["test"], f["neighbors"]
        if n_queries > test_ds.shape[0]:
            raise ValueError(f"n_queries={n_queries} exceeds test size {test_ds.shape[0]} in {path}")
        if neigh_ds.shape[1] < k:
            raise ValueError(f"k={k} exceeds precomputed neighbor count {neigh_ds.shape[1]} in {path}")
        corpus = train_ds[:].astype(np.float32)  # FULL train (float32 bounds RSS ~ N*dim*4)
        rng = np.random.default_rng(seed)
        query_idx = np.sort(rng.choice(test_ds.shape[0], size=n_queries, replace=False))
        queries = test_ds[query_idx].astype(np.float32)
        neighbor_ids = neigh_ds[query_idx].astype(np.int64)  # (Q, k') ids into train
    gt_dist = neighbors_ground_truth(corpus, queries, neighbor_ids, k, metric)
    return corpus, queries, gt_dist


def make_dataset(
    n: int, dim: int, n_queries: int, seed: int
) -> tuple[np.ndarray, np.ndarray]:
    """Return (corpus (n,dim), queries (n_queries,dim)) drawn from a seeded RNG.

    Raises ValueError on non-positive n / dim / n_queries.
    """
    if n < 1:
        raise ValueError(f"n must be >= 1, got {n}")
    if dim < 1:
        raise ValueError(f"dim must be >= 1, got {dim}")
    if n_queries < 1:
        raise ValueError(f"n_queries must be >= 1, got {n_queries}")
    rng = np.random.default_rng(seed)
    corpus = rng.standard_normal((n, dim)).astype(np.float64)
    queries = rng.standard_normal((n_queries, dim)).astype(np.float64)
    return corpus, queries
