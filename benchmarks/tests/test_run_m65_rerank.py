"""M65 — unit tests for the rerank harness claim-bearing arithmetic (container-free, model-free).

mrr_at_k and rerank_verdict are pure stdlib and MUST be provable in isolation (mirrors
test_run_m63_vector_join.py). ndcg_at_k / recall_at_n are reused from theodb_bench.metrics (already tested).

Rules cited: `.claude/rules/testing.md` §4.1 — both edge cases (first-position hit, no relevant, exact
noise boundary) AND negative cases (k <= 0 → typed error; regression → HONEST_NEGATIVE not spin) are covered.
"""
import pytest

from run_m65_rerank import mrr_at_k, rerank_verdict


# ----------------------------- mrr_at_k -----------------------------

def test_mrr_first_position_is_one():
    # relevant doc at rank 1 → 1/1.
    assert mrr_at_k(["d1", "d2"], {"d1": 1}, 10) == 1.0


def test_mrr_third_position_is_one_third():
    assert mrr_at_k(["x", "y", "d1"], {"d1": 2}, 10) == pytest.approx(1.0 / 3.0)


def test_mrr_no_relevant_in_topk_is_zero():
    # relevant doc exists but beyond k → 0.0 (edge: relevant just past the cutoff).
    assert mrr_at_k(["a", "b", "d1"], {"d1": 1}, 2) == 0.0


def test_mrr_no_relevant_at_all_is_zero():
    assert mrr_at_k(["a", "b"], {"z": 1}, 10) == 0.0


def test_mrr_ignores_zero_grade():
    # grade 0 is NOT relevant (BEIR convention) → the grade-0 doc does not count.
    assert mrr_at_k(["d0", "d1"], {"d0": 0, "d1": 1}, 10) == pytest.approx(0.5)


def test_mrr_rejects_non_positive_k():
    with pytest.raises(ValueError):
        mrr_at_k(["a"], {"a": 1}, 0)


# ----------------------------- rerank_verdict -----------------------------

def test_verdict_improvement_is_pass():
    v = rerank_verdict(0.60, 0.70)
    assert v["status"] == "PASS" and v["delta"] == 0.1


def test_verdict_regression_is_honest_negative():
    # Rerank made it WORSE — must be HONEST_NEGATIVE, never spun as a win.
    v = rerank_verdict(0.70, 0.65)
    assert v["status"] == "HONEST_NEGATIVE" and v["delta"] == -0.05


def test_verdict_within_noise_is_honest_negative():
    # delta below the noise tolerance → not a claimable improvement.
    v = rerank_verdict(0.700, 0.703, noise_tol=0.005)
    assert v["status"] == "HONEST_NEGATIVE"


def test_verdict_just_above_noise_is_pass():
    v = rerank_verdict(0.700, 0.706, noise_tol=0.005)
    assert v["status"] == "PASS"


def test_verdict_carries_both_numbers():
    v = rerank_verdict(0.60, 0.70)
    assert v["baseline"] == 0.6 and v["rerank"] == 0.7 and v["axis"] == "ndcg_at_10"
