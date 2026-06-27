"""TheoDB vector recall@k benchmark harness.

The measurement-first gate of M2 (ADR 0002): measures recall@k + latency/QPS + build-time +
index size of a PostgreSQL vector index against the `theo-db:dev` container, with exact
ground-truth and ANN-Benchmarks-grade (distance-thresholded) recall semantics.
"""

__all__ = ["recall", "dataset", "metrics", "db", "harness"]
