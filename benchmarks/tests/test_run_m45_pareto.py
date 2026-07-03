"""M45 — unit tests for the pure Pareto post-processing (interpolation + margin verdict) and the driver's
zero-latency guard, plus an integration structure test for the full harness.

The pure-logic tests (interpolate/verdict/qps-guard) need NO container — they are the claim-bearing
arithmetic and MUST be provable in isolation. The integration test (`emits_two_frontiers`) needs one
container (PORT env, e.g. theo-db:m44) and is gated by pytest.mark.integration, mirroring
test_run_m44_parallel_build.py.
"""
import os

import pytest

from m45_pareto import interpolate_qps_at_recall, pareto_margin_verdict


# ----------------------------- interpolate_qps_at_recall (EDGE + NEGATIVE) -----------------------------

def test_interpolate_midpoint_linear():
    pts = [{"recall": 0.94, "qps_mean": 230.0}, {"recall": 0.99, "qps_mean": 110.0}]
    # 0.965 is halfway → 170.0
    assert interpolate_qps_at_recall(pts, 0.965) == pytest.approx(170.0, abs=1e-6)


def test_interpolate_exact_recall_returns_point_qps():
    pts = [{"recall": 0.94, "qps_mean": 230.0}, {"recall": 0.99, "qps_mean": 110.0}]
    assert interpolate_qps_at_recall(pts, 0.94) == pytest.approx(230.0)
    assert interpolate_qps_at_recall(pts, 0.99) == pytest.approx(110.0)


def test_interpolate_out_of_range_returns_none():
    pts = [{"recall": 0.94, "qps_mean": 230.0}, {"recall": 0.99, "qps_mean": 110.0}]
    assert interpolate_qps_at_recall(pts, 0.80) is None  # below min
    assert interpolate_qps_at_recall(pts, 1.00) is None  # above max


def test_interpolate_equal_recall_no_div_by_zero():
    # two adjacent points at the SAME recall (flat frontier segment) → no ZeroDivisionError
    pts = [{"recall": 0.95, "qps_mean": 200.0}, {"recall": 0.95, "qps_mean": 150.0}]
    got = interpolate_qps_at_recall(pts, 0.95)
    assert got in (200.0, 150.0)  # returns a bracketing point's qps, never raises


def test_interpolate_single_point_frontier_only_covers_its_recall():
    # EC-2: a 1-point frontier returns its qps only at its exact recall, None elsewhere (no extrapolation)
    pts = [{"recall": 0.96, "qps_mean": 180.0}]
    assert interpolate_qps_at_recall(pts, 0.96) == pytest.approx(180.0)
    assert interpolate_qps_at_recall(pts, 0.95) is None
    assert interpolate_qps_at_recall(pts, 0.97) is None


def test_interpolate_empty_frontier_returns_none():
    assert interpolate_qps_at_recall([], 0.95) is None


# ----------------------------- pareto_margin_verdict (honest effect>variance gate) -----------------------------

def test_verdict_superior_when_margin_gt_1_and_effect_exceeds_variance():
    theodb = [{"recall": 0.94, "qps_mean": 230.0, "qps_std": 5.0},
              {"recall": 0.99, "qps_mean": 110.0, "qps_std": 3.0}]
    pgvector = [{"recall": 0.92, "qps_mean": 133.0, "qps_std": 4.0},
                {"recall": 0.98, "qps_mean": 74.0, "qps_std": 2.0}]
    r = pareto_margin_verdict(theodb, pgvector)
    assert r["verdict"] == "SUPERIOR"
    assert all(m["margin"] > 1.0 for m in r["margins"])
    assert all(m["effect_gt_variance"] for m in r["margins"])


def test_verdict_parity_when_gap_within_variance():
    # near-equal QPS with huge std → the gap does NOT exceed variance → no claim
    theodb = [{"recall": 0.95, "qps_mean": 100.0, "qps_std": 50.0}]
    pgvector = [{"recall": 0.95, "qps_mean": 98.0, "qps_std": 50.0}]
    r = pareto_margin_verdict(theodb, pgvector)
    assert r["verdict"] == "PARITY"


def test_verdict_inferior_when_theodb_slower():
    theodb = [{"recall": 0.94, "qps_mean": 74.0, "qps_std": 2.0},
              {"recall": 0.99, "qps_mean": 40.0, "qps_std": 2.0}]
    pgvector = [{"recall": 0.92, "qps_mean": 230.0, "qps_std": 5.0},
                {"recall": 0.98, "qps_mean": 133.0, "qps_std": 5.0}]
    r = pareto_margin_verdict(theodb, pgvector)
    assert r["verdict"] == "INFERIOR"


def test_verdict_parity_when_no_recall_overlap():
    theodb = [{"recall": 0.94, "qps_mean": 230.0, "qps_std": 5.0},
              {"recall": 0.99, "qps_mean": 110.0, "qps_std": 3.0}]
    pgvector = [{"recall": 0.80, "qps_mean": 300.0, "qps_std": 5.0},
                {"recall": 0.90, "qps_mean": 200.0, "qps_std": 5.0}]
    r = pareto_margin_verdict(theodb, pgvector)
    assert r["verdict"] == "PARITY"
    assert "no recall overlap" in r["reason"]


def test_verdict_empty_inputs_parity():
    assert pareto_margin_verdict([], [])["verdict"] == "PARITY"
    assert pareto_margin_verdict([{"recall": 0.9, "qps_mean": 1, "qps_std": 0}], [])["verdict"] == "PARITY"


# ----------------------------- driver zero-latency guard (EC-3) -----------------------------

def test_qps_guards_zero_latency():
    # a mean latency of 0 (clock granularity) must not raise ZeroDivisionError
    from run_m45_pareto import qps_from_latency
    assert qps_from_latency(0.0) > 0  # clamped via epsilon, finite
    assert qps_from_latency(0.01) == pytest.approx(100.0, rel=1e-6)


# ----------------------------- integration structure (needs a container) -----------------------------

@pytest.mark.integration
def test_run_m45_emits_two_frontiers_with_mean_std():
    import run_m45_pareto
    port = int(os.environ.get("PORT", os.environ.get("PGPORT", "5474")))
    # tiny synthetic run: n small, both indexes build fast; asserts SHAPE, not the real SIFT1M margin.
    res = run_m45_pareto.run(port=port, hdf5=None, n=800, dim=16, nq=40, runs=2,
                             ef_grid=[40, 64], seed=2026)
    for index in ("theodb_hnsw", "pgvector_hnsw"):
        assert index in res["frontier"], f"missing frontier {index}"
        pts = res["frontier"][index]
        assert len(pts) == 2  # two ef points
        for p in pts:
            for key in ("recall", "qps_mean", "qps_std", "ef", "nq"):
                assert key in p, f"{index} point missing {key}"
            assert 0.0 <= p["recall"] <= 1.0
            assert p["qps_mean"] > 0.0
            assert p["nq"] == 40  # both indexes measured on the identical query subset
    assert res["verdict"]["verdict"] in ("SUPERIOR", "PARITY", "INFERIOR")
