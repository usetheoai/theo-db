"""M123 — deterministic unit tests for `paired_significance` (no BEIR / DB / network needed).

Proves the test behaves as the theory predicts before we trust its verdict on real data:
null (a==b) → not significant; a clear uniform shift → significant; ties are counted; bad input → typed error.
"""

import numpy as np
import pytest

from theodb_bench.significance import paired_significance


def test_paired_significance_shapes():
    a = [0.5, 0.6, 0.7, 0.4, 0.9]
    b = [0.4, 0.6, 0.5, 0.4, 0.8]
    r = paired_significance(a, b, n_resamples=2000)
    assert set(r) >= {"n", "mean_diff", "ci95_low", "ci95_high", "p_permutation", "p_ttest",
                      "wins", "losses", "ties", "cohens_dz", "seed", "n_resamples"}
    assert r["wins"] + r["losses"] + r["ties"] == r["n"] == 5


def test_null_not_significant():
    # identical systems → no effect: p ≈ 1, CI straddles 0, no wins/losses, all ties.
    a = [0.1, 0.5, 0.9, 0.3, 0.7, 0.2]
    r = paired_significance(a, a, n_resamples=5000)
    assert r["mean_diff"] == 0.0
    assert r["wins"] == 0 and r["losses"] == 0 and r["ties"] == r["n"]
    assert r["p_permutation"] >= 0.99
    assert r["ci95_low"] <= 0.0 <= r["ci95_high"]


def test_clear_shift_significant():
    # b = a - 0.2 on every query → a strictly wins by a constant: significant, CI strictly > 0.
    rng = np.random.default_rng(0)
    a = rng.uniform(0.3, 0.7, size=40)
    b = a - 0.2
    r = paired_significance(list(a), list(b), n_resamples=10000)
    assert r["mean_diff"] > 0.19
    assert r["wins"] == r["n"] and r["losses"] == 0
    assert r["p_permutation"] < 0.05
    assert r["ci95_low"] > 0.0


def test_ties_counted():
    a = [0.5, 0.5, 0.7, 0.2, 0.5]
    b = [0.5, 0.4, 0.7, 0.3, 0.5]  # pairs 0,2,4 tie; 1 a-win; 3 b-win
    r = paired_significance(a, b, n_resamples=2000)
    assert r["ties"] == 3 and r["wins"] == 1 and r["losses"] == 1


def test_unequal_length_raises():
    with pytest.raises(ValueError):
        paired_significance([0.1, 0.2, 0.3], [0.1, 0.2])


def test_too_few_pairs_raises():
    with pytest.raises(ValueError):
        paired_significance([0.5], [0.4])


def test_paired_sig_wiring_aligns_by_qid():
    """T2 — the harness helper aligns hybrid vs vector per-query nDCG@10 by qid and emits a significance block."""
    from run_m53_hybrid_beir import _paired_sig
    pq = {
        "hybrid": {"qids": ["q1", "q2", "q3"], "ndcg10": [0.9, 0.5, 0.7], "recall100": [1, 1, 1]},
        "vector": {"qids": ["q2", "q3", "q1"], "ndcg10": [0.4, 0.6, 0.8], "recall100": [1, 1, 1]},  # different order
    }
    sig = _paired_sig(pq)
    assert sig["systems"] == "hybrid_vs_vector" and sig["metric"] == "ndcg10"
    assert sig["n"] == 3
    # aligned by qid: q1 0.9-0.8=+0.1, q2 0.5-0.4=+0.1, q3 0.7-0.6=+0.1 → mean +0.1, all wins
    assert abs(sig["mean_diff"] - 0.1) < 1e-9
    assert sig["wins"] == 3 and sig["losses"] == 0 and sig["ties"] == 0


def test_paired_sig_none_when_no_per_query():
    from run_m53_hybrid_beir import _paired_sig
    assert _paired_sig(None) is None
    assert _paired_sig({"vector": {}}) is None
