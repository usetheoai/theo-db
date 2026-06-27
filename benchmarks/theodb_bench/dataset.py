"""Seeded, reproducible synthetic vector datasets (blueprint ADR D3).

Determinism is the point of the gate: same seed -> bit-identical corpus + queries, so a benchmark
run is reproducible. The OSS analogs (pgvector Perl tests, pgvectorscale) do NOT seed their data —
this module closes that gap.
"""
from __future__ import annotations

import numpy as np


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
