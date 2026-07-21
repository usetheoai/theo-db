"""M127 — byte-identical result regression A/B (the wrap-layer capability the official tools lack).

ann-benchmarks/VectorDBBench/ClickBench/BenchBase all ship NO byte-identical result-regression check (blueprint
Q11, unanimous). This is the retained TheoDB capability: given two per-query neighbour-id result sets (a baseline
and a candidate — two engine builds, or run N vs N-1), assert they are byte-identical and report the FIRST
diverging query, so a code change that silently reorders rankings is caught (it caught M114/M126-class bugs).

Aligns by query id (like `run_m53_hybrid_beir._align`); a mismatched qid set is a typed error, never a silent
partial compare (a dropped query must not masquerade as byte-identical).
"""
from __future__ import annotations


class QidMismatchError(ValueError):
    """Baseline and candidate cover different query ids — refuse to compare (fail-closed)."""


def assert_byte_identical(baseline: dict, candidate: dict) -> dict:
    """Compare two {qid: [neighbour_id, ...]} result sets.

    Returns {"identical": bool, "n": int, "first_divergence": qid|None, "diverged": int}.
    Raises QidMismatchError if the two cover different query-id sets.
    """
    b_keys, c_keys = set(baseline), set(candidate)
    if b_keys != c_keys:
        missing = sorted(b_keys ^ c_keys)[:5]
        raise QidMismatchError(
            f"qid sets differ (baseline {len(b_keys)} vs candidate {len(c_keys)}); symmetric-diff sample: {missing}")

    first_divergence = None
    diverged = 0
    for qid in sorted(b_keys):  # deterministic order → deterministic first-divergence
        if list(baseline[qid]) != list(candidate[qid]):
            diverged += 1
            if first_divergence is None:
                first_divergence = qid
    return {
        "identical": diverged == 0,
        "n": len(b_keys),
        "first_divergence": first_divergence,
        "diverged": diverged,
    }
