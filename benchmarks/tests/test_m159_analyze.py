"""Unit tests for the M159 same-box gap analysis (benchmarks/m159_analyze.py).
Protects the ratio/geomean/classification logic that turns the raw TheoDB+ClickHouse timings into the verdict — a
wrong bucket boundary or a fabricated ratio would corrupt the honest number the milestone exists to produce."""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from m159_analyze import build_rows, classify, geomean  # noqa: E402


def test_geomean_is_geometric_not_arithmetic():
    # geomean(2,8)=4 (arithmetic would be 5); positives only, empty -> NaN.
    assert geomean([2, 8]) == 4
    assert geomean([1, 1, 1]) == 1
    assert math.isnan(geomean([]))
    assert math.isnan(geomean([0, -3]))  # non-positive filtered -> empty -> NaN


def test_ratio_is_theodb_over_clickhouse_and_never_fabricated():
    theodb = {"queries": [
        {"q": 0, "hot": 0.068, "columnar_customscan": True, "result_ab_identical": True},   # 0.068/0.003 = 22.67x
        {"q": 1, "hot": 0.098, "columnar_customscan": True, "result_ab_identical": True},   # 0.098/0.045 = 2.18x on-target
        {"q": 2, "hot": 12.6, "columnar_customscan": False, "result_ab_identical": True},   # non-pushdown, structural
        {"q": 3, "error": "timeout", "columnar_customscan": False},                          # errored -> no ratio
        {"q": 4, "hot": 0.01, "columnar_customscan": True, "result_ab_identical": True},    # CH ~0 -> no ratio (floor)
    ]}
    ch = {0: 0.003, 1: 0.045, 2: 0.096, 3: 0.05, 4: 0.0}
    rows = build_rows(theodb, ch)
    assert abs(rows[0]["ratio"] - 22.666) < 0.01
    assert abs(rows[1]["ratio"] - 2.177) < 0.01
    assert rows[3]["ratio"] is None and "ERROR" in rows[3]["note"]      # errored -> never a fabricated ratio
    assert rows[4]["ratio"] is None and "below timer" in rows[4]["note"]  # CH floor -> not comparable


def test_classify_buckets_and_split_geomean():
    theodb = {"queries": [
        {"q": 0, "hot": 0.098, "columnar_customscan": True},   # 2.18x  on-target, pushdown
        {"q": 1, "hot": 0.5, "columnar_customscan": True},     # 5.0x   gap, pushdown
        {"q": 2, "hot": 12.6, "columnar_customscan": False},   # 131x   structural, non-pushdown
        {"q": 3, "hot": 0.001, "columnar_customscan": True},   # 0.17x  faster, pushdown
    ]}
    ch = {0: 0.045, 1: 0.1, 2: 0.096, 3: 0.006}
    s = classify(build_rows(theodb, ch))
    assert s["n_comparable"] == 4
    assert s["on_target"] == 2 and s["faster"] == 1   # 2.18x and 0.17x are <=3x; 0.17x also counts as faster
    assert s["gap"] == 1 and s["structural"] == 1
    assert s["n_pushdown"] == 3 and s["n_nonpushdown"] == 1
    # non-pushdown geomean = the single 131x; pushdown geomean is over {2.18, 5.0, 0.17}
    assert abs(s["geomean_nonpushdown"] - (12.6 / 0.096)) < 0.1
    assert s["geomean_pushdown"] < s["geomean_nonpushdown"]
