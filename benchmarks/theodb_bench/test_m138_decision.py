"""M138 — the flip-decision gate is a pure function, unit-tested offline.

The milestone's crux (ADR-1 of the plan): the lexical default flips to BM25 **if and only if** the
FUSION with BM25 beats the FUSION with ts_rank_cd, with paired significance over the queries — NOT
because the isolated BM25 leg (0.688) dwarfs the ts_rank_cd leg (0.070). RRF fuses by rank, so a
stronger leg need not move the fusion; the M53 already showed the ts_rank_cd fusion tying the pure
vector. `decide_flip` encodes that gate, and this test pins its three outcomes offline (no DB), the
same way M134 unit-tested egress policy standalone.
"""
import numpy as np

from run_m138_bm25_fusion import decide_flip


def test_flip_true_when_bm25_fusion_significantly_better():
    # bm25 fusion beats ts_rank_cd fusion on nearly every query, by a clear margin → flip.
    rng = np.random.default_rng(1)
    tsrank = rng.uniform(0.60, 0.66, size=300)
    bm25 = tsrank + 0.05  # uniformly better per-query
    d = decide_flip(bm25, tsrank, alpha=0.05)
    assert d["flip"] is True
    assert d["mean_diff"] > 0
    assert d["p"] < 0.05


def test_flip_false_when_no_significant_difference():
    # The honest-negative the plan explicitly allows: fusions tie (RRF washed the leg gap) → no flip.
    rng = np.random.default_rng(2)
    base = rng.uniform(0.60, 0.66, size=300)
    tsrank = base.copy()
    bm25 = base.copy()
    bm25[0] += 0.001  # negligible, single-query nudge — must NOT clear significance
    d = decide_flip(bm25, tsrank, alpha=0.05)
    assert d["flip"] is False
    assert d["p"] >= 0.05


def test_flip_false_when_bm25_fusion_worse():
    # bm25 fusion significantly WORSE → never flip, even though the isolated leg is stronger.
    rng = np.random.default_rng(3)
    tsrank = rng.uniform(0.60, 0.66, size=300)
    bm25 = tsrank - 0.05
    d = decide_flip(bm25, tsrank, alpha=0.05)
    assert d["flip"] is False
    assert d["mean_diff"] < 0


def test_rejects_mismatched_lengths():
    import pytest

    with pytest.raises(ValueError):
        decide_flip([0.1, 0.2, 0.3], [0.1, 0.2], alpha=0.05)
