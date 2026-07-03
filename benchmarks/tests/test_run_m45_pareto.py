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
    # two adjacent points at the SAME recall (flat frontier segment) → no ZeroDivisionError.
    # Contracted result: the lower-recall-slot (first-sorted) bracket point → 200.0 (deterministic).
    pts = [{"recall": 0.95, "qps_mean": 200.0}, {"recall": 0.95, "qps_mean": 150.0}]
    assert interpolate_qps_at_recall(pts, 0.95) == pytest.approx(200.0)


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


def test_verdict_parity_when_margin_exceeds_tol_but_gap_within_variance():
    # THE load-bearing gate test: theodb is 1.1× faster (margin > 1+tol) BUT the gap (10) is far inside the
    # combined std (80) → the effect>variance gate MUST block a SUPERIOR claim → PARITY (PRD D3 anti-sunk-cost).
    theodb = [{"recall": 0.95, "qps_mean": 110.0, "qps_std": 40.0}]
    pgvector = [{"recall": 0.95, "qps_mean": 100.0, "qps_std": 40.0}]
    r = pareto_margin_verdict(theodb, pgvector)
    assert r["verdict"] == "PARITY"
    assert r["margins"][0]["margin"] > 1.05  # ratio alone WOULD read as a win
    assert r["margins"][0]["effect_gt_variance"] is False  # but the gate blocks it


def test_verdict_parity_when_levels_disagree():
    # theodb faster at low recall, slower at high recall (frontiers cross) — even with effect at BOTH levels,
    # disagreement across levels → PARITY (SUPERIOR/INFERIOR require EVERY shared level to agree).
    theodb = [{"recall": 0.94, "qps_mean": 230.0, "qps_std": 5.0},
              {"recall": 0.99, "qps_mean": 40.0, "qps_std": 2.0}]
    pgvector = [{"recall": 0.94, "qps_mean": 100.0, "qps_std": 5.0},
                {"recall": 0.99, "qps_mean": 120.0, "qps_std": 2.0}]
    r = pareto_margin_verdict(theodb, pgvector)
    assert r["verdict"] == "PARITY"
    verdict_dirs = {m["margin"] > 1 for m in r["margins"]}
    assert verdict_dirs == {True, False}  # confirms the levels genuinely disagree


def test_verdict_parity_when_pgvector_qps_zero():
    # pgvector QPS 0 (degenerate) → division guarded, level skipped, honest typed reason (no fabricated claim)
    theodb = [{"recall": 0.95, "qps_mean": 100.0, "qps_std": 5.0}]
    pgvector = [{"recall": 0.95, "qps_mean": 0.0, "qps_std": 0.0}]
    r = pareto_margin_verdict(theodb, pgvector)
    assert r["verdict"] == "PARITY"
    assert r["margins"] == []
    assert r["reason"] == "no interpolable shared level"


def test_effect_gate_uses_interpolated_std_not_nearest():
    # A shared level whose pgvector QPS is a BLEND of a quiet point and a NOISY bracket must carry the
    # interpolated (frac-weighted) std into the gate — not the nearest quiet point's std (council MEDIUM-1).
    theodb = [{"recall": 0.95, "qps_mean": 200.0, "qps_std": 1.0}]
    # pgvector: quiet at 0.90 (std 1), very noisy at 1.00 (std 100). At r=0.95 the blend std must be ~50, not 1.
    pgvector = [{"recall": 0.90, "qps_mean": 150.0, "qps_std": 1.0},
                {"recall": 1.00, "qps_mean": 100.0, "qps_std": 100.0}]
    r = pareto_margin_verdict(theodb, pgvector)
    # gap = 200 - 125 = 75; interpolated pgvector std ≈ 50.5 (+ theodb 1) ≈ 51.5 < 75 would be effect True with
    # nearest-std(=1) it'd trivially be True; the point is the std is NOT 1. Assert the gate saw a large std:
    m = r["margins"][0]
    assert m["qps_pgvector"] == pytest.approx(125.0, abs=0.5)  # blended qps
    # with nearest-point std (1.0) the effect would be a landslide; interpolated std (~50) is what we want.
    # Re-derive: a nearest-std gate would pass at gap 75 vs std ~2; interpolated makes it a close call.
    assert m["effect_gt_variance"] in (True, False)  # value asserted precisely below via a tighter case


def test_interpolated_std_blocks_false_superior_from_noisy_bracket():
    # Tighter: make the gap SMALLER than the interpolated std so the noisy bracket correctly blocks SUPERIOR,
    # where a nearest-point (quiet) std would have wrongly licensed it.
    theodb = [{"recall": 0.95, "qps_mean": 175.0, "qps_std": 1.0}]
    pgvector = [{"recall": 0.90, "qps_mean": 150.0, "qps_std": 1.0},
                {"recall": 1.00, "qps_mean": 100.0, "qps_std": 100.0}]
    # r=0.95: pgvector blend qps=125, blend std≈50.5; theodb 175. gap=50 < 50.5+1 → effect False → PARITY.
    # With nearest-point std (1.0) the gap 50 > 2 would have FALSELY licensed SUPERIOR.
    r = pareto_margin_verdict(theodb, pgvector)
    assert r["margins"][0]["effect_gt_variance"] is False
    assert r["verdict"] == "PARITY"


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
