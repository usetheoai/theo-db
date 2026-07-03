"""M44 — structure + non-degeneracy of the parallel-build A/B harness (run_m44_parallel_build.run).

Integration test: needs TWO containers — a sequential build (SEQ_PORT, e.g. theo-db:m43) and a parallel build
(PAR_PORT, e.g. theo-db:m44). Asserts the harness returns build-time mean/std + recall for both arms and a verdict.
Tiny scale for speed; the full n=50k run is the D3 deliverable.
"""
import os

import pytest

import run_m44_parallel_build

pytestmark = pytest.mark.integration


def test_run_m44_emits_build_times_and_recall():
    seq_port = int(os.environ.get("SEQ_PORT", "5461"))
    par_port = int(os.environ.get("PAR_PORT", "5464"))
    # Tiny: n=200 (below threshold on BOTH → both sequential; the harness structure is what's tested here).
    res = run_m44_parallel_build.run(seq_port=seq_port, par_port=par_port, n=200, dim=32, nq=20, runs=2, seed=2026)
    for arm in ("sequential", "parallel"):
        assert arm in res
        for key in ("build_s_mean", "build_s_std", "recall_at_10"):
            assert key in res[arm], f"{arm} missing {key}"
        assert 0.0 <= res[arm]["recall_at_10"] <= 1.0
        assert res[arm]["build_s_mean"] >= 0.0
    assert res["verdict"] in ("PARALLEL_WINS", "NO_SPEEDUP", "RECALL_REGRESSION")
    assert "build_speedup" in res
