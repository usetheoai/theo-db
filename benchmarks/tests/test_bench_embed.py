"""Unit test for the benchmark harness (bench_embed.bench) — no DB needed (fake connection).

Proves the harness computes mean/std over the requested runs and validates its inputs. The real
latency numbers (against a live container) are produced by running bench_embed.py and recorded in
wiki/benchmarks/m17-embed-rust-vs-plpython.md.
"""
import os
import sys

import pytest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from bench_embed import bench  # noqa: E402


class _FakeCursor:
    def __init__(self):
        self.calls = 0

    def __enter__(self):
        return self

    def __exit__(self, *_a):
        return False

    def execute(self, _q, _p=None):
        self.calls += 1


class _FakeConn:
    def __init__(self):
        self._cur = _FakeCursor()

    def cursor(self):
        return self._cur


def test_bench_returns_mean_std_n_runs():
    r = bench(_FakeConn(), n=3, runs=2, warmup=1)
    assert {"mean", "std", "n", "runs", "per_call_samples"} <= set(r)
    assert r["n"] == 3 and r["runs"] == 2
    assert r["mean"] >= 0.0 and r["std"] >= 0.0
    assert len(r["per_call_samples"]) == 2


def test_bench_issues_warmup_plus_n_times_runs_calls():
    conn = _FakeConn()
    bench(conn, n=4, runs=3, warmup=2)
    # warmup (2) + n*runs (4*3=12) = 14 execute calls
    assert conn._cur.calls == 2 + 4 * 3


@pytest.mark.parametrize("n,runs", [(0, 3), (3, 0), (-1, 1)])
def test_bench_rejects_nonpositive(n, runs):
    with pytest.raises(ValueError):
        bench(_FakeConn(), n=n, runs=runs)
