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
