"""M63 — unit tests for the vector-JOIN harness claim-bearing arithmetic (container-free).

The join-recall math, dedup precision/recall, and per-axis verdict are pure stdlib and MUST be
provable in isolation (mirrors test_run_m45_pareto.py / test_run_m46_highrecall.py). The integration
`run()` needs a container (PGPORT env) and is gated by pytest.mark.integration.

Rules cited: `.claude/rules/testing.md` §4.1 — both edge cases (identical/disjoint/empty) AND
negative cases (empty-found → precision undefined) are covered, not just the happy path.
"""
import os

import pytest

from run_m63_vector_join import dedup_metrics, join_recall, verdict


# ----------------------------- join_recall (the R2-critical metric) -----------------------------

def test_join_recall_identical_sets_is_one():
    ann = [{1, 2, 3}, {4, 5, 6}]
    exact = [{1, 2, 3}, {4, 5, 6}]
    r = join_recall(ann, exact)
    assert r["mean"] == 1.0 and r["min"] == 1.0 and r["n"] == 2


def test_join_recall_disjoint_sets_is_zero():
    r = join_recall([{7, 8, 9}], [{1, 2, 3}])
    assert r["mean"] == 0.0 and r["min"] == 0.0


def test_join_recall_min_surfaces_a_recall_zero_row():
    # R2: one perfect row + one total-miss row → mean 0.5 but MIN 0.0 (the mean would hide the miss).
    ann = [{1, 2, 3}, {90, 91, 92}]
    exact = [{1, 2, 3}, {4, 5, 6}]
    r = join_recall(ann, exact)
    assert r["mean"] == 0.5
    assert r["min"] == 0.0, "min must surface the recall-0 row a mean would mask"


def test_join_recall_partial_overlap():
    r = join_recall([{1, 2, 9}], [{1, 2, 3}])  # 2 of 3 hit
    assert r["mean"] == pytest.approx(0.6667, abs=1e-4)


def test_join_recall_skips_empty_exact_rows():
    # a row whose exact top-k is empty is not a recall data point (edge: filter matched nothing)
    r = join_recall([{1}, set()], [{1}, set()])
    assert r["n"] == 1 and r["mean"] == 1.0


def test_join_recall_no_data_returns_none_not_crash():
    r = join_recall([set()], [set()])
    assert r == {"min": None, "mean": None, "std": None, "n": 0}


# ----------------------------- dedup_metrics (precision AND recall, both) -----------------------------

def test_dedup_perfect_recovery():
    m = dedup_metrics([(1, 200), (2, 201)], [(1, 200), (2, 201)])
    assert m["precision"] == 1.0 and m["recall"] == 1.0 and m["hits"] == 2


def test_dedup_pair_order_is_normalized():
    # (200,1) must equal (1,200) — unordered pair identity
    m = dedup_metrics([(200, 1)], [(1, 200)])
    assert m["hits"] == 1 and m["precision"] == 1.0 and m["recall"] == 1.0


def test_dedup_false_positive_lowers_precision():
    m = dedup_metrics([(1, 200), (3, 999)], [(1, 200)])  # one real, one spurious
    assert m["precision"] == 0.5 and m["recall"] == 1.0


def test_dedup_missed_dup_lowers_recall():
    m = dedup_metrics([(1, 200)], [(1, 200), (2, 201)])  # one of two recovered
    assert m["precision"] == 1.0 and m["recall"] == 0.5


def test_dedup_empty_found_precision_undefined_not_crash():
    # negative case: nothing found → precision is undefined (None), recall 0, no ZeroDivision
    m = dedup_metrics([], [(1, 200)])
    assert m["precision"] is None and m["recall"] == 0.0


# ----------------------------- verdict (honest per-axis, no cherry-pick) -----------------------------

def _arm(mean, p50):
    return {"recall": {"mean": mean, "min": mean, "std": 0.0}, "p50_ms": p50, "p95_ms": p50}


def test_verdict_parity_within_tolerance():
    agg = {"T1_lateral_index": _arm(0.98, 2.0), "T3_pgvector": _arm(0.985, 1.5),
           "T2_naive_sort": _arm(1.0, 40.0)}
    v = verdict(agg)
    assert v["join_recall"]["status"] == "PARITY"  # 0.98 vs 0.985 within ±0.01


def test_verdict_gap_when_theodb_below_control():
    agg = {"T1_lateral_index": _arm(0.90, 2.0), "T3_pgvector": _arm(0.99, 1.5)}
    assert verdict(agg)["join_recall"]["status"] == "GAP"


def test_verdict_superior_when_theodb_above_control():
    agg = {"T1_lateral_index": _arm(0.99, 2.0), "T3_pgvector": _arm(0.90, 1.5)}
    assert verdict(agg)["join_recall"]["status"] == "SUPERIOR"


def test_verdict_unbenchmarked_when_control_missing():
    # honest-negative: if the pgvector control arm errored/absent, the axis is UNBENCHMARKED, not faked
    agg = {"T1_lateral_index": _arm(0.99, 2.0), "T3_pgvector": {"error": "container absent"}}
    assert verdict(agg)["join_recall"]["status"] == "UNBENCHMARKED"


def test_verdict_records_dod_index_vs_naive():
    agg = {"T1_lateral_index": _arm(0.98, 2.0), "T2_naive_sort": _arm(1.0, 40.0)}
    d = verdict(agg)["dod_index_not_nested_loop"]
    assert d["t1_lateral_p50_ms"] == 2.0 and d["t2_naive_p50_ms"] == 40.0


# ----------------------------- integration (needs a container) -----------------------------

@pytest.mark.integration
def test_run_emits_arms_and_dedup_schema():
    import run_m63_vector_join
    os.environ.setdefault("PGPORT", os.environ.get("PORT", "55492"))
    data = run_m63_vector_join.run(n_a=20, n_b=500, dim=16, k=5, runs=2, seed=2026)
    assert set(data["arms"]) == {"T1_lateral_index", "T2_naive_sort", "T3_pgvector"}
    assert "verdict" in data and "join_recall" in data["verdict"]
    assert "dedup" in data
    t1 = data["per_arm"]["T1_lateral_index"]
    if "recall" in t1:  # T1 must produce recall data (index present in theodb image)
        assert 0.0 <= t1["recall"]["mean"] <= 1.0
