"""Unit tests for latency percentiles + best-of-N QPS."""
import pytest

from theodb_bench.metrics import latency_percentiles, qps_best_of_n


def test_percentiles_ordered():
    r = latency_percentiles([1, 2, 3, 4, 5, 100])
    assert r["p50"] <= r["p95"] <= r["p99"]


def test_percentiles_known_values():
    r = latency_percentiles([10.0] * 10)
    assert r["p50"] == 10.0
    assert r["mean"] == 10.0
    assert r["std"] == 0.0


def test_percentiles_empty_raises():
    with pytest.raises(ValueError):
        latency_percentiles([])


def test_qps_best_of_n_uses_min():
    assert qps_best_of_n([0.01, 0.008, 0.012]) == pytest.approx(125.0)


def test_qps_empty_raises():
    with pytest.raises(ValueError):
        qps_best_of_n([])


def test_qps_nonpositive_latency_raises():
    with pytest.raises(ValueError):
        qps_best_of_n([0.0, 0.01])
