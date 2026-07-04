"""M46 — unit tests for the hardened high-recall re-measurement post-processing.

The claim-bearing arithmetic (harden median/trimmed-mean, recall-neutral comparison + verdict, pages_read
parsing) is pure stdlib — provable WITHOUT a container, mirroring test_run_m45_pareto.py. The integration
structure test (`emits_hardened_frontier`) needs one container (PORT env) and is pytest.mark.integration.
"""
import os

import pytest

from run_m46_highrecall import (
    harden_frontier,
    parse_pages_read,
    recall_neutral_compare,
    recall_neutral_verdict,
)


# ----------------------------- harden_frontier (median + trimmed mean) -----------------------------

def test_harden_adds_median_and_trimmed_mean():
    pts = [{"ef": 200, "recall": 0.99, "qps_mean": 44.0, "qps_std": 20.0,
            "qps_runs": [43.0, 44.0, 45.0, 46.0, 100.0]}]  # 100.0 is the dev-box stall outlier
    out = harden_frontier(pts)
    assert out[0]["qps_median"] == 45.0  # median ignores the 100.0 turbo/stall
    assert out[0]["qps_trimmed_mean"] == pytest.approx(45.0)  # drop min(43)+max(100) → mean(44,45,46)=45
    assert out[0]["hardened"] is True
    assert out[0]["pages_read"] is None  # honest default until captured


def test_harden_falls_back_without_runs():
    pts = [{"ef": 200, "recall": 0.99, "qps_mean": 44.0, "qps_std": 20.0}]  # no qps_runs
    out = harden_frontier(pts)
    assert out[0]["qps_median"] == 44.0
    assert out[0]["qps_trimmed_mean"] == 44.0
    assert out[0]["hardened"] is False


def test_trimmed_mean_handles_small_samples():
    # < 3 runs → nothing to trim → plain mean, no crash (edge case)
    out = harden_frontier([{"ef": 100, "qps_mean": 10.0, "qps_runs": [8.0, 12.0]}])
    assert out[0]["qps_trimmed_mean"] == 10.0


# ----------------------------- the plan's named RED test -----------------------------

def test_run_m46_emits_median_and_pages_read():
    """T1.1 RED (plan): a hardened operating point carries ef, recall, qps_mean, qps_std, qps_median,
    pages_read, and the effect>variance signal flows through unchanged from the M45 frontier."""
    pts = [{"ef": 200, "recall": 0.993, "qps_mean": 44.0, "qps_std": 6.0,
            "qps_runs": [41.0, 43.0, 44.0, 45.0, 47.0], "pages_read": 3120}]
    out = harden_frontier(pts)[0]
    for key in ("ef", "recall", "qps_mean", "qps_std", "qps_median", "pages_read"):
        assert key in out, f"hardened point missing {key}"
    assert out["qps_median"] == 44.0
    assert out["pages_read"] == 3120  # preserved when already present on the point


# ----------------------------- recall_neutral_compare + verdict (the HARD gate) -----------------------------

def _pt(ef, recall, qps_median, std=1.0, pages=None):
    return {"ef": ef, "recall": recall, "qps_mean": qps_median, "qps_std": std,
            "qps_median": qps_median, "pages_read": pages}


def test_recall_identical_and_throughput_win():
    base = [_pt(200, 0.993, 40.0, std=18.0), _pt(400, 0.998, 30.0, std=13.0)]
    post = [_pt(200, 0.993, 48.0, std=4.0), _pt(400, 0.998, 34.0, std=3.0)]
    rows = recall_neutral_compare(base, post)
    assert all(r["recall_identical"] for r in rows)
    v = recall_neutral_verdict(rows)
    assert v["verdict"] == "THROUGHPUT_WIN"


def test_recall_regression_is_flagged_as_bug():
    # recall moved at ef=200 → NOT recall-neutral → the change is a BUG, not an optimization
    base = [_pt(200, 0.993, 40.0)]
    post = [_pt(200, 0.985, 60.0)]  # faster BUT recall dropped → must NOT be a win
    rows = recall_neutral_compare(base, post)
    assert rows[0]["recall_identical"] is False
    v = recall_neutral_verdict(rows)
    assert v["verdict"] == "RECALL_REGRESSION"


def test_variance_win_when_recall_neutral_and_variance_drops_below_target():
    # QPS not clearly up, but the ef>=200 variance collapses from ~45% to <15% → the plan's honest-negative alt
    base = [_pt(200, 0.993, 44.0, std=20.0)]   # var ≈ 45%
    post = [_pt(200, 0.993, 44.0, std=4.0)]    # var ≈ 9%
    rows = recall_neutral_compare(base, post)
    assert rows[0]["variance_reduced"] is True
    v = recall_neutral_verdict(rows)
    assert v["verdict"] == "VARIANCE_WIN"


def test_pages_read_divergence_flags_visit_order_changed():
    base = [_pt(200, 0.993, 40.0, pages=3120)]
    post = [_pt(200, 0.993, 40.0, pages=3999)]  # same recall but visited different pages → order moved
    rows = recall_neutral_compare(base, post)
    assert rows[0]["pages_read_identical"] is False
    v = recall_neutral_verdict(rows)
    assert v["verdict"] == "VISIT_ORDER_CHANGED"


def test_missing_ef_is_handled_not_crash():
    rows = recall_neutral_compare([_pt(200, 0.99, 40.0)], [])  # post missing ef=200
    assert rows[0]["present"] is False
    # verdict on no comparable points is NEUTRAL, never a crash
    assert recall_neutral_verdict(rows)["verdict"] == "NEUTRAL"


# ----------------------------- parse_pages_read (server-log parsing) -----------------------------

def test_parse_pages_read_extracts_last_per_ef():
    log = (
        "LOG:  theodb hnsw scan profile: pages_read=100 ef=200 m=16 m0=32 results=10\n"
        "LOG:  theodb hnsw scan profile: pages_read=3120 ef=200 m=16 m0=32 results=10\n"  # last for ef=200
        "LOG:  theodb hnsw scan profile: pages_read=5000 ef=400 m=16 m0=32 results=10\n"
        "some unrelated line\n"
    )
    got = parse_pages_read(log)
    assert got == {200: 3120, 400: 5000}


def test_parse_pages_read_ignores_malformed():
    log = "theodb hnsw scan profile: pages_read=notanint ef=200\nnoise\n"
    assert parse_pages_read(log) == {}  # never crashes on a format drift


# ----------------------------- integration structure (needs a container) -----------------------------

@pytest.mark.integration
def test_run_m46_emits_hardened_frontier():
    import run_m46_highrecall
    port = int(os.environ.get("PORT", os.environ.get("PGPORT", "5474")))
    res = run_m46_highrecall.run_hardened(port=port, hdf5=None, runs=5, ef_grid=(64, 100),
                                          n=800, dim=16, nq=40, seed=2026)
    assert "theodb_hardened" in res
    for p in res["theodb_hardened"]:
        for key in ("ef", "recall", "qps_mean", "qps_std", "qps_median", "qps_trimmed_mean", "pages_read"):
            assert key in p, f"hardened point missing {key}"
        assert 0.0 <= p["recall"] <= 1.0
        assert p["qps_median"] > 0.0
