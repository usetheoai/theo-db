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


def ndcg_at_k(ranked_ids: list, qrels: dict, k: int) -> float:
    """Normalized DCG@k over graded relevance (BEIR primary metric, Thakur et al. 2021).

    qrels: {doc_id: relevance_grade (int >= 0)}. DCG = sum_{i<k} rel_i / log2(i+2);
    IDCG = DCG of the ideal (qrels sorted desc). Returns 0.0 when no relevant docs (IDCG==0).
    """
    if k <= 0:
        raise ValueError(f"k must be > 0 (got {k})")

    def _dcg(grades: list) -> float:
        return float(sum(g / np.log2(i + 2) for i, g in enumerate(grades[:k])))

    gains = [qrels.get(doc_id, 0) for doc_id in ranked_ids]
    ideal = sorted(qrels.values(), reverse=True)
    idcg = _dcg(ideal)
    if idcg == 0.0:
        return 0.0
    return _dcg(gains) / idcg


def recall_at_n(ranked_ids: list, qrels: dict, n: int) -> float:
    """Recall@n: fraction of relevant docs (grade > 0) present in the top-n ranking (BEIR secondary).

    Returns 0.0 when qrels has no relevant doc.
    """
    if n <= 0:
        raise ValueError(f"n must be > 0 (got {n})")
    relevant = {doc_id for doc_id, grade in qrels.items() if grade > 0}
    if not relevant:
        return 0.0
    hit = sum(1 for doc_id in ranked_ids[:n] if doc_id in relevant)
    return hit / len(relevant)
