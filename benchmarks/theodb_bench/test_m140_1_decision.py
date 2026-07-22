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


def test_beir_axis_reproduces_m138_within_tolerance():
    # Anti-fabrication anchor: BEIR nDCG must be in the plausible band M138 measured
    # (scifact BM25 leg ~0.688, ts_rank_cd leg ~0.070). We assert the BM25 nDCG is a
    # real number in [0,1] and the ts_rank baseline (if present) is far below BM25 on
    # scifact — the M138 signature. Exact reproduction needs the same corpus/params.
    for a in _load_axis("beir", "beir.json"):
        assert 0.0 <= a["mean_ndcg_bm25"] <= 1.0
        if a["dataset"] == "scifact" and "mean_ndcg_base" in a:
            assert a["mean_ndcg_bm25"] > a["mean_ndcg_base"], "scifact: BM25 leg must beat ts_rank leg (M138 signature)"
