"""M30 — structure test for the columnar-at-scale sweep (run_m30_columnar_scale.run).

Integration: needs a pg_mooncake container (PORT env, e.g. mooncakelabs/pg_mooncake). Gated — skips when
pg_mooncake is absent (the shipped TheoDB image does NOT ship it; this measures on the canonical substrate).
Tiny scales for speed; the full 100k→5M sweep is the M30 data deliverable.
"""
import os
import pathlib

import pytest

import run_m30_columnar_scale
from theodb_bench.db import VectorDB

_REPO = pathlib.Path(__file__).resolve().parents[2]


def test_adr_0013_present_and_grounded():
    """SG1: the M30 decision ADR exists, decides KEEP for both pillars, cites the evidence + a rejected
    alternative. Not gated on a container (pure doc check)."""
    adr = (_REPO / "docs" / "adr" / "0013-v1-legacy-columnar-bm25-scope.md").read_text()
    assert "MANTER" in adr                                   # decision = keep
    assert "columnar" in adr.lower() and "bm25" in adr.lower()
    assert "m30-columnar-scale" in adr                       # cites the new scale benchmark
    assert "m7-bm25-vs-tsrank" in adr                        # cites the BM25 evidence
    assert "Deprecar" in adr or "deprecar" in adr            # lists the rejected alternative


def _mooncake_or_skip(port):
    db = VectorDB(f"host=localhost port={port} dbname=postgres user=postgres password=postgres").connect()
    ok = db.pg_mooncake_available()
    db.close()
    if not ok:
        pytest.skip("pg_mooncake not available — run against mooncakelabs/pg_mooncake")


@pytest.mark.integration
def test_run_m30_scale_emits_meanstd_points_and_verdict():
    port = int(os.environ.get("PORT", os.environ.get("PGPORT", "5492")))
    _mooncake_or_skip(port)
    res = run_m30_columnar_scale.run(port, scales=(1000, 5000), runs=2)
    assert res["substrate"].startswith("mooncakelabs")
    assert res["runs"] == 2
    assert len(res["points"]) == 2
    for p in res["points"]:
        for key in ("n", "row_ms_mean", "row_ms_std", "columnar_ms_mean", "columnar_ms_std",
                    "speedup", "effect_gt_variance", "columnar_plan_duckdb", "match"):
            assert key in p, f"point missing {key}"
        assert p["match"] is True                        # correctness: columnar == row (within eps)
        assert p["columnar_plan_duckdb"] is True          # the mirror plans as DuckDBScan
        assert p["row_ms_mean"] >= 0 and p["columnar_ms_mean"] >= 0
        assert isinstance(p["effect_gt_variance"], bool)
    # crossover_n = smallest scale with an effect>variance columnar win, or None — both are honest outcomes
    assert res["crossover_n"] is None or isinstance(res["crossover_n"], int)
