"""M64 — unit tests for the RAG-over-SQL harness claim-bearing arithmetic (container-free).

The round-trip delta, the recall-match fairness gate, and the per-axis verdict are pure stdlib and MUST
be provable in isolation (mirrors test_run_m63_vector_join.py). The integration `run()` needs a container
(PGPORT env) and is not exercised here.

Rules cited: `.claude/rules/testing.md` §4.1 — both edge cases (identical/disjoint/empty sets, exact
tolerance boundary) AND negative cases (round-trips < 1 → typed error; gate-failed → latency UNCOMPARABLE,
not hidden) are covered, not just the happy path.
"""
import pytest

from run_m64_rag_over_sql import recall_match_gate, round_trip_delta, verdict


# ----------------------------- round_trip_delta (the structural mechanism) -----------------------

def test_round_trip_delta_app_layer_pays_extra_hop():
    d = round_trip_delta(1, 2)
    assert d["a"] == 1 and d["b"] == 2 and d["saved"] == 1 and d["ratio"] == 2.0


def test_round_trip_delta_no_saving_when_equal():
    d = round_trip_delta(1, 1)
    assert d["saved"] == 0 and d["ratio"] == 1.0


def test_round_trip_delta_rejects_zero_round_trips():
    # NEGATIVE case: a round-trip count < 1 is impossible (every arm hits the DB at least once).
    with pytest.raises(ValueError):
        round_trip_delta(0, 2)
    with pytest.raises(ValueError):
        round_trip_delta(1, 0)


# ----------------------------- recall_match_gate (the fairness gate) -----------------------------

def test_gate_matches_identical_sets():
    g = recall_match_gate({1, 2, 3}, {1, 2, 3})
    assert g["matched"] is True and g["jaccard"] == 1.0


def test_gate_fails_disjoint_sets():
    g = recall_match_gate({1, 2, 3}, {4, 5, 6})
    assert g["matched"] is False and g["jaccard"] == 0.0
    assert "different sets" in g["reason"]


def test_gate_partial_overlap_within_tolerance():
    # 3 of 4 shared → jaccard = 3/4 = 0.75; tol 0.25 → matched (boundary).
    g = recall_match_gate({1, 2, 3}, {1, 2, 3, 4}, tol=0.25)
    assert g["jaccard"] == 0.75 and g["matched"] is True


def test_gate_partial_overlap_outside_tolerance():
    g = recall_match_gate({1, 2, 3}, {1, 2, 3, 4}, tol=0.10)
    assert g["matched"] is False


def test_gate_empty_both_is_not_matched():
    # NEGATIVE case: both arms retrieved nothing → not a valid match (no comparison possible).
    g = recall_match_gate(set(), set())
    assert g["matched"] is False and g["jaccard"] is None


# ----------------------------- verdict (honest per-axis) ----------------------------------------

def _agg(a_rt=1, b_rt=2, a_p50=0.5, b_p50=0.9):
    return {
        "A_unified": {"round_trips": a_rt, "p50_ms": a_p50},
        "B_app_layer": {"round_trips": b_rt, "p50_ms": b_p50},
    }


def test_verdict_reports_round_trips_saved():
    v = verdict(_agg(), recall_match_gate({1, 2}, {1, 2}))
    assert v["round_trips"]["saved"] == 1 and v["round_trips"]["a"] == 1


def test_verdict_latency_uncomparable_when_gate_fails():
    # NEGATIVE case: gate failed → latency MUST be UNCOMPARABLE, never silently compared.
    v = verdict(_agg(), recall_match_gate({1, 2}, {3, 4}))
    assert v["latency"]["status"] == "UNCOMPARABLE"


def test_verdict_latency_unified_faster():
    v = verdict(_agg(a_p50=0.4, b_p50=0.9), recall_match_gate({1}, {1}))
    assert v["latency"]["status"] == "UNIFIED_FASTER"


def test_verdict_latency_parity_within_tolerance():
    v = verdict(_agg(a_p50=0.50, b_p50=0.505), recall_match_gate({1}, {1}))
    assert v["latency"]["status"] == "PARITY"


def test_verdict_latency_app_faster_is_honest():
    # Honest-negative: if the app-layer is actually faster, say so (no spin toward the unified arm).
    v = verdict(_agg(a_p50=0.9, b_p50=0.4), recall_match_gate({1}, {1}))
    assert v["latency"]["status"] == "APP_FASTER"


def test_verdict_latency_unbenchmarked_when_missing():
    agg = {"A_unified": {"round_trips": 1, "p50_ms": None},
           "B_app_layer": {"round_trips": 2, "p50_ms": 0.9}}
    v = verdict(agg, recall_match_gate({1}, {1}))
    assert v["latency"]["status"] == "UNBENCHMARKED"


def test_verdict_always_carries_the_gate():
    g = recall_match_gate({1, 2}, {1, 2})
    v = verdict(_agg(), g)
    assert v["recall_matched"] == g
