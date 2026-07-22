"""M140.1 T2.1/T2.2 — runner smoke + offline decision gate over the REAL emitted JSON.

The gate is what makes the verdict auditable and non-fabricated (mirrors
test_m138_decision.py): it reads the JSON the runner wrote and asserts the verdict is
DERIVED from the paired test, not hardcoded. If the real result JSON is absent, the
data-backed tests SKIP with an instruction to run the runner (never a false green).
"""
from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

import pytest

BENCH = Path(__file__).resolve().parent.parent
DATA = BENCH.parent / "docs" / "benchmarks" / "m140-1-data"


# ---- T2.1 runner smoke ----

def test_runner_smoke_emits_valid_json(tmp_path):
    out = tmp_path / "smoke.json"
    rc = subprocess.run(
        [sys.executable, "run_m140_1_lexical.py", "--smoke", "--out", str(out)],
        cwd=BENCH,
        capture_output=True,
        text=True,
    )
    assert rc.returncode == 0, rc.stderr
    d = json.loads(out.read_text())
    assert d["schema_version"] == "m140.1-v1"
    axes = {a["axis"] for a in d["axes"]}
    assert {"beir", "logproxy"} <= axes
    for a in d["axes"]:
        assert "verdict" in a


def _load_axis(kind: str, filename: str):
    f = DATA / filename
    if not f.exists():
        pytest.skip(f"{f} absent — run: python3 run_m140_1_lexical.py ... --out {f}")
    d = json.loads(f.read_text())
    ax = [a for a in d["axes"] if a["axis"] == kind]
    if not ax:
        pytest.skip(f"no '{kind}' axis in {f}")
    return ax


# ---- T2.2 decision gate over the real JSON ----

def test_each_axis_verdict_has_paired_fields():
    for a in _load_axis("logproxy", "logproxy.json"):
        v = a["verdict"]
        if v.get("skipped"):
            pytest.skip(f"logproxy verdict skipped: {v.get('reason')}")
        for field in ("p_permutation", "mean_diff", "wins", "losses", "ties", "flip"):
            assert field in v, f"missing {field}"


def test_verdict_flip_is_derived_not_hardcoded():
    for a in _load_axis("logproxy", "logproxy.json"):
        v = a["verdict"]
        if v.get("skipped"):
            pytest.skip("logproxy verdict skipped")
        expected = bool(v["p_permutation"] < 0.05 and v["mean_diff"] > 0)
        assert v["flip"] == expected, "flip must equal (p<0.05 and mean_diff>0)"


# M138 measured ts_rank_cd leg (docs/benchmarks/m138-bm25-fusion.md): the anti-fabrication
# anchor. Our ts_rank_cd pipeline must reproduce these within tolerance — if it drifts, the
# harness changed something silently and the numbers are no longer comparable to M138.
M138_TSRANK_LEG = {"scifact": 0.070275, "nfcorpus": 0.206117}
NDCG_TOL = 0.03


def test_beir_ts_rank_reproduces_m138_within_tolerance():
    checked = 0
    for a in _load_axis("beir", "beir.json"):
        assert 0.0 <= a["mean_ndcg_bm25"] <= 1.0
        anchor = M138_TSRANK_LEG.get(a["dataset"])
        if anchor is not None and "mean_ndcg_base" in a:
            drift = abs(a["mean_ndcg_base"] - anchor)
            assert drift <= NDCG_TOL, (
                f"{a['dataset']}: ts_rank nDCG {a['mean_ndcg_base']:.4f} drifted "
                f"{drift:.4f} from M138 anchor {anchor:.4f} (tol {NDCG_TOL})"
            )
            checked += 1
    if checked == 0:
        pytest.skip("no ts_rank baseline present (PG absent at run time)")


def test_beir_bm25_beats_ts_rank_on_both_datasets():
    for a in _load_axis("beir", "beir.json"):
        if "mean_ndcg_base" in a:
            assert a["mean_ndcg_bm25"] > a["mean_ndcg_base"], (
                f"{a['dataset']}: BM25 leg must beat ts_rank leg (M138 signature)"
            )
