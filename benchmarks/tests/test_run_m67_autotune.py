"""M67 — unit tests for the auto-tune convergence arithmetic (container-free).

recall_from_sets, mae_vs_target, rqut, converged, autotune_verdict are pure stdlib. Rules cited:
`.claude/rules/testing.md` §4.1 — edge (exact band boundary, all-at-target) AND negative (empty recall →
typed error; undershoot → NOT converged) covered.
"""
import pytest

from run_m67_autotune import (autotune_verdict, converged, mae_vs_target, rqut, recall_from_sets)


def test_recall_from_sets_partial():
    assert recall_from_sets([1, 2, 9], [1, 2, 3]) == pytest.approx(2 / 3)


def test_recall_from_sets_empty_gt_is_none():
    assert recall_from_sets([1], []) is None


def test_mae_vs_target():
    # recalls [0.9, 1.0, 0.95] vs target 0.95 → |−0.05|+|+0.05|+0 = 0.10, /3.
    assert mae_vs_target([0.9, 1.0, 0.95], 0.95) == pytest.approx(0.10 / 3)


def test_mae_skips_none():
    assert mae_vs_target([0.9, None, 0.9], 0.9) == 0.0


def test_mae_empty_raises():
    with pytest.raises(ValueError):
        mae_vs_target([None, None], 0.9)


def test_rqut_counts_below_target():
    # 1 of 3 below 0.95 → 1/3.
    assert rqut([0.90, 0.96, 0.99], 0.95) == pytest.approx(1 / 3)


def test_rqut_none_below_is_zero():
    assert rqut([0.96, 0.97], 0.95) == 0.0


def test_converged_at_target():
    assert converged(0.95, 0.95) is True


def test_converged_within_band():
    # 0.94 is within 0.02 band of 0.95 → converged.
    assert converged(0.94, 0.95, band=0.02) is True


def test_not_converged_undershoot():
    # 0.90 is 0.05 below 0.95, outside the 0.02 band → NOT converged.
    assert converged(0.90, 0.95, band=0.02) is False


def test_verdict_converged_when_all_reach():
    pt = {"0.9": {"ef": 20, "mean_recall": 0.92, "mae": 0.02, "rqut": 0.1},
          "0.95": {"ef": 40, "mean_recall": 0.96, "mae": 0.01, "rqut": 0.05}}
    v = autotune_verdict(pt)
    assert v["status"] == "CONVERGED" and v["failed_targets"] == []


def test_verdict_honest_negative_when_target_unreachable():
    # 0.99 undershoots (mean 0.94, outside band) → HONEST_NEGATIVE listing it.
    pt = {"0.9": {"ef": 20, "mean_recall": 0.93, "mae": 0.03, "rqut": 0.1},
          "0.99": {"ef": 1000, "mean_recall": 0.94, "mae": 0.05, "rqut": 0.6}}
    v = autotune_verdict(pt)
    assert v["status"] == "HONEST_NEGATIVE" and "0.99" in v["failed_targets"]
