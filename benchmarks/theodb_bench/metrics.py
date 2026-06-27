"""Latency percentiles + best-of-N QPS (blueprint ADR D3 / §T3, ANN-Benchmarks protocol).

QPS = 1 / best (min) per-query mean latency across N runs — the "best-of-N" reduces scheduler
jitter and reports near-peak attainable throughput (ANN-Benchmarks runner.py).
"""
from __future__ import annotations

import numpy as np


def latency_percentiles(samples_ms: list) -> dict:
    """Return p50/p95/p99/mean/std (milliseconds) of per-query latencies."""
    if len(samples_ms) == 0:
        raise ValueError("no latency samples")
    a = np.asarray(samples_ms, dtype=np.float64)
    p50, p95, p99 = np.percentile(a, [50, 95, 99])
    return {
        "p50": float(p50),
        "p95": float(p95),
        "p99": float(p99),
        "mean": float(a.mean()),
        "std": float(a.std()),
    }


def qps_best_of_n(run_mean_latencies_s: list) -> float:
    """Queries per second = 1 / min(per-run mean latency in seconds)."""
    if len(run_mean_latencies_s) == 0:
        raise ValueError("no runs")
    best = min(run_mean_latencies_s)
    if best <= 0:
        raise ValueError(f"non-positive best latency: {best}")
    return 1.0 / best
