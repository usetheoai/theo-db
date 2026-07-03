"""M40 — structure + non-degeneracy of the carrier head-to-head harness (run_m40_carrier).

Integration test (needs the container with both persisted AMs theodb_hnsw + theodb_ivfflat): asserts the harness
produces a recall×QPS result per AM per knob and a matched-QPS verdict. Tiny scale for speed; the full n=50k run
is the deliverable measurement.
"""
import os

import pytest

import run_m40_carrier
from theodb_bench.db import VectorDB
from theodb_bench.harness import run_benchmark

pytestmark = pytest.mark.integration


def _dsn():
    port = os.environ.get("PGPORT", "5432")
    return (f"host=localhost port={port} dbname=postgres user=postgres password=postgres")


def test_run_m40_carrier_emits_recall_qps_per_am():
    # The spec helpers reference run_m40_carrier._TABLE in their DDL, so the cfg table MUST match it.
    cfg = {
        "seed": 2026, "n": 2000, "dim": 64, "n_queries": 80, "k": 10, "metric": "l2",
        "runs": 1, "table": run_m40_carrier._TABLE, "dataset_label": "m40-test",
        "index_specs": [
            run_m40_carrier._hnsw_spec([10, 100]),
            run_m40_carrier._ivfflat_spec(44, [4, 44]),
        ],
    }
    db = VectorDB(_dsn()).connect()
    db.set_session("SET max_parallel_maintenance_workers = 0")
    try:
        report = run_benchmark(cfg, db, "/tmp")
    finally:
        db.close()

    results = report["results"]
    ams = {r["index"] for r in results}
    assert ams == {"theodb_hnsw", "theodb_ivfflat"}, f"both AMs must appear, got {ams}"
    for r in results:
        assert 0.0 <= r["recall_at_k"] <= 1.0, f"recall out of range: {r}"
        assert r["qps"] > 0.0, f"qps must be positive: {r}"

    verdict, frac, rows = run_m40_carrier.matched_qps_verdict(results)
    assert verdict in ("THEODB_HNSW_WINS", "THEODB_IVFFLAT_WINS", "TIE")
    assert 0.0 <= frac <= 1.0
    assert len(rows) >= 1, "at least one matched-QPS comparison row"
