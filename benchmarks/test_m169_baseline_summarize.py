"""M169 unit tests — the non-vacuity gate over the 100M ClickBench baseline.

Pure logic, NO box and NO database: the summarizer takes the per-query records as data. That is what makes it a
regression test rather than something that only runs after a multi-hour benchmark.

Why this exists: T1.2's whole output is one number — how many of 43 queries complete. A run that dies halfway
publishes "19/43 completam" that is INDISTINGUISHABLE from "19 completed and 24 genuinely failed". The M162 run
was OOM-killed at 24/43 and the remaining 19 were never executed; without this gate, that run's headline would
have been a claim about the product made from a claim about the harness.
"""
from __future__ import annotations

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import m169_baseline_summarize as s


def _rec(i, verdict="ok", **over):
    r = dict(q=i, verdict=verdict, elapsed_s=1.0)
    r.update(over)
    return r


def _full(overrides: dict | None = None):
    """A complete run: all 43 queries carry an explicit verdict. `overrides` maps query index -> verdict; it is
    a dict rather than **kwargs because the keys are integers, which Python refuses as keyword names."""
    recs = [_rec(i) for i in range(43)]
    for i, v in (overrides or {}).items():
        recs[i]["verdict"] = v
    return recs


# ---------- the non-vacuity gate — the whole point of the file --------------------------------------------------
def test_summarizer_rejects_a_run_that_did_not_reach_every_query():
    """The M162 shape: the harness died at 24/43 and 19 queries were never executed."""
    partial = [_rec(i) for i in range(24)]
    r = s.summarize(partial, total_expected=43)
    assert r.ok is False
    assert "incompleto" in r.reason
    assert "24/43" in r.reason


def test_summarizer_rejects_a_record_with_no_verdict():
    """A query that ran but whose outcome was never classified is a hole, not a pass."""
    recs = _full()
    recs[7]["verdict"] = None
    r = s.summarize(recs, total_expected=43)
    assert r.ok is False
    assert "sem veredito" in r.reason


def test_summarizer_accepts_a_complete_run_even_when_most_queries_failed():
    """Completeness is about COVERAGE of verdicts, not about them being good. A run where 40 of 43 fail is a
    valid measurement of a bad state; a run missing 19 verdicts is not a measurement at all."""
    recs = _full({i: "timeout" for i in range(3, 43)})
    r = s.summarize(recs, total_expected=43)
    assert r.ok is True
    assert r.completed == 3


# ---------- the verdict vocabulary ------------------------------------------------------------------------------
def test_completed_counts_only_ok():
    recs = _full({0: "timeout", 1: "error:22003", 2: "crash"})
    r = s.summarize(recs, total_expected=43)
    assert r.completed == 40
    assert r.by_verdict["timeout"] == 1
    assert r.by_verdict["error:22003"] == 1
    assert r.by_verdict["crash"] == 1


def test_unknown_verdict_token_is_rejected_not_silently_counted():
    """An unrecognised token must fail loudly. Silently bucketing it is how a harness bug becomes a product
    claim — the class this project keeps paying for."""
    recs = _full({5: "weird"})
    r = s.summarize(recs, total_expected=43)
    assert r.ok is False
    assert "veredito desconhecido" in r.reason


# ---------- classification: never call it OOM without evidence ---------------------------------------------------
def test_statement_timeout_sqlstate_becomes_timeout():
    assert s.classify_failure("57014", "canceling statement due to statement timeout") == "timeout"


def test_a_sql_error_keeps_its_sqlstate():
    assert s.classify_failure("22003", "byte array offset overflow") == "error:22003"


def test_a_dropped_connection_is_crash_not_oom():
    """A backend that vanished MIGHT have been OOM-killed — or segfaulted, or been terminated. Calling it 'oom'
    without reading the kernel log is a diagnosis invented to fit the narrative."""
    assert s.classify_failure(None, "server closed the connection unexpectedly") == "crash"


def test_crash_is_upgraded_to_oom_only_with_kernel_evidence():
    assert s.classify_failure(None, "server closed the connection unexpectedly",
                              oom_evidence=True) == "oom:kernel"


def test_pg_caught_allocation_failure_is_distinct_from_a_kernel_kill():
    """53200 means PostgreSQL caught it and the backend SURVIVED — a different fact from a kernel OOM kill, and
    a different remediation."""
    assert s.classify_failure("53200", "out of memory") == "oom:palloc"


def test_a_neighbours_crash_is_never_attributed_to_this_query():
    """57P02 = the backend was recycled because ANOTHER one crashed. Counting it against this query blames it
    for a neighbour's death."""
    assert s.classify_failure("57P02", "terminating connection due to crash of another server process") \
        == "crash:collateral"


# ---------- the false-headline guard ------------------------------------------------------------------------------
def test_ab_headline_is_refused_when_zero_comparisons_were_made():
    """`decide_ab_verdict` in run_m128_clickbench publishes 'byte-identical (columnar==heap)' when
    result_ab_identical is None for every query — which is exactly what happens with `hits_heap` absent. Zero
    comparisons is not agreement."""
    recs = [_rec(i, ab_identical=None) for i in range(43)]
    r = s.summarize(recs, total_expected=43)
    assert r.ab_verdict == "n/a — nenhuma comparação executada"
    assert "byte-identical" not in r.ab_verdict


def test_ab_headline_is_allowed_when_comparisons_actually_ran():
    recs = [_rec(i, ab_identical=True) for i in range(43)]
    r = s.summarize(recs, total_expected=43)
    assert "byte-identical" in r.ab_verdict


def test_ab_headline_reports_divergence_when_any_comparison_diverged():
    recs = [_rec(i, ab_identical=True) for i in range(43)]
    recs[11]["ab_identical"] = False
    r = s.summarize(recs, total_expected=43)
    assert "diverg" in r.ab_verdict.lower()
    assert "byte-identical" not in r.ab_verdict


# ---------- NEGATIVE lens: malformed input ---------------------------------------------------------------------------
def test_summarizer_rejects_an_empty_run():
    r = s.summarize([], total_expected=43)
    assert r.ok is False
    assert "0/43" in r.reason


def test_summarizer_rejects_duplicate_query_ids():
    """43 records with a duplicate means one query was never run — a shape that passes a naive length check."""
    recs = _full()
    recs[42]["q"] = 0
    r = s.summarize(recs, total_expected=43)
    assert r.ok is False
    assert "duplicad" in r.reason
