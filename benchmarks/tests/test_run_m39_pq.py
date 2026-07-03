"""M39 — structure + non-degeneracy of the PQ-vs-SBQ benchmark harness (run_m39_pq.run).

Integration test (needs the container with theodb_rs + pgvector): asserts the harness returns a dict with
recall@10 + qps mean/std for BOTH pq and sbq, and that the numbers are non-degenerate (recall in [0,1], qps>0,
a real D3 verdict token). This is the Phase-3 TDD RED gate; the full recall×QPS comparison is the D3 run itself.
"""
import os

import pytest

import run_m39_pq

pytestmark = pytest.mark.integration


def test_run_m39_pq_emits_recall_and_qps():
    port = int(os.environ.get("PGPORT", "5432"))
    # Tiny corpus for speed: n=300, dim=32, m=4 (32 % 4 == 0), 20 queries, 3 runs.
    res = run_m39_pq.run(
        port=port, n=300, dim=32, m=4, bits=4, nq=20, runs=3, seed=2026,
        lists=16, probes=8, over_fetch=8,
    )
    # Structure
    for arm in ("pq", "sbq"):
        assert arm in res, f"missing arm {arm}"
        for key in ("recall_mean", "recall_std", "qps_mean", "qps_std", "bytes_per_vector"):
            assert key in res[arm], f"{arm} missing {key}"
    assert res["verdict"] in ("PQ_BEATS_SBQ", "SBQ_RETAINED")
    # Non-degeneracy
    assert 0.0 <= res["pq"]["recall_mean"] <= 1.0
    assert 0.0 <= res["sbq"]["recall_mean"] <= 1.0
    assert res["pq"]["qps_mean"] > 0.0
    assert res["sbq"]["qps_mean"] > 0.0
    assert res["pq"]["bytes_per_vector"] == 4  # m bytes/vector
    assert res["f32_bytes_per_vector"] == 32 * 4
