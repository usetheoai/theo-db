"""M66 — unit tests for the chunking-benchmark claim-bearing arithmetic (container-free, model-free).

doc_recall_at_k, k_adaptive, chunk_verdict are pure stdlib and MUST be provable in isolation (mirrors
test_run_m65_rerank.py). ndcg_at_k is reused from theodb_bench.metrics (already tested).

Rules cited: `.claude/rules/testing.md` §4.1 — edge (exact k boundary, single strategy) AND negative
(k<=0, avg_chunk<=0 → typed error) covered, not just the happy path.
"""
import pytest

from run_m66_chunking import chunk_verdict, doc_recall_at_k, k_adaptive


# ----------------------------- doc_recall_at_k -----------------------------

def test_doc_recall_all_relevant_in_topk():
    assert doc_recall_at_k(["d1", "d2", "d3"], {"d1": 1, "d2": 2}, 3) == 1.0


def test_doc_recall_half_in_topk():
    assert doc_recall_at_k(["d1", "x", "y"], {"d1": 1, "d2": 1}, 3) == 0.5


def test_doc_recall_ignores_grade_zero():
    # grade 0 is not relevant (BEIR) → not counted in the denominator.
    assert doc_recall_at_k(["d0"], {"d0": 0, "d1": 1}, 3) == 0.0


def test_doc_recall_no_relevant_is_zero():
    assert doc_recall_at_k(["a", "b"], {"z": 1}, 3) == 0.0


def test_doc_recall_rejects_non_positive_k():
    with pytest.raises(ValueError):
        doc_recall_at_k(["a"], {"a": 1}, 0)


# ----------------------------- k_adaptive (the fairness equaliser) -----------------------------

def test_k_adaptive_larger_chunks_fewer_k():
    # 2000 target tokens; 512-char chunks (~128 tokens) → ~16 chunks; 128-char (~32 tokens) → ~63.
    k_big = k_adaptive(2000, 512)
    k_small = k_adaptive(2000, 128)
    assert k_small > k_big, "smaller chunks require MORE of them to fill the same context budget"


def test_k_adaptive_at_least_one():
    assert k_adaptive(1, 100000) >= 1


def test_k_adaptive_rejects_zero_avg():
    with pytest.raises(ValueError):
        k_adaptive(2000, 0)


# ----------------------------- chunk_verdict -----------------------------

def _ps(fixed, sentence, recursive):
    return {
        "fixed": {"ndcg10": fixed, "doc_recall": fixed},
        "sentence": {"ndcg10": sentence, "doc_recall": sentence},
        "recursive": {"ndcg10": recursive, "doc_recall": recursive},
    }


def test_verdict_ranks_by_ndcg():
    v = chunk_verdict(_ps(0.60, 0.70, 0.65))
    assert v["best"] == "sentence"
    assert v["ranking"][0][0] == "sentence" and v["ranking"][-1][0] == "fixed"


def test_verdict_strategy_matters_when_spread_wide():
    v = chunk_verdict(_ps(0.50, 0.70, 0.60))
    assert v["status"] == "STRATEGY_MATTERS" and v["spread"] == pytest.approx(0.2)


def test_verdict_honest_negative_when_within_noise():
    # All strategies within the noise band → strategy choice did not move recall (honest).
    v = chunk_verdict(_ps(0.700, 0.702, 0.701), noise_tol=0.005)
    assert v["status"] == "HONEST_NEGATIVE"


def test_verdict_reports_recursive_baseline():
    v = chunk_verdict(_ps(0.60, 0.70, 0.65))
    assert v["baseline_recursive"] == 0.65
