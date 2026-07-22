"""M140.1 T0.1/T0.2 — known-item metrics + log corpus loader (RED-first TDD).

Known-item retrieval (TREC named-page finding): a query targets ONE known doc; the
relevant doc is that doc. No human qrels needed — the gold is the doc itself. This is
the honest methodology for a corpus without relevance judgments (M140.1 ADR D1).
"""
from __future__ import annotations

import random

import pytest

from theodb_bench import knownitem, logcorpus


# ---- T0.1: MRR@k / Success@1 / query generation ----

def test_mrr_target_at_rank1_is_1_0():
    assert knownitem.mrr_at_k(["a", "b", "c"], "a", 10) == 1.0


def test_mrr_target_at_rank3_is_one_third():
    assert knownitem.mrr_at_k(["x", "y", "a"], "a", 10) == pytest.approx(1 / 3)


def test_mrr_target_absent_is_0():
    assert knownitem.mrr_at_k(["x", "y", "z"], "a", 2) == 0.0


def test_mrr_target_beyond_k_is_0():
    # target at rank 3 (index 2) but k=2 → out of window → 0.0
    assert knownitem.mrr_at_k(["x", "y", "a"], "a", 2) == 0.0


def test_mrr_k_zero_raises():
    with pytest.raises(ValueError):
        knownitem.mrr_at_k(["a"], "a", 0)


def test_success_at_1_true_only_when_rank0():
    assert knownitem.success_at_1(["a", "b"], "a") == 1.0
    assert knownitem.success_at_1(["b", "a"], "a") == 0.0


def test_recall_known_item_hit_and_miss():
    assert knownitem.recall_known_item(["a", "b"], "a", 10) == 1.0
    assert knownitem.recall_known_item(["b", "c"], "a", 10) == 0.0


def test_make_query_is_deterministic_under_same_seed():
    doc = "error timeout blk_1234 retry timeout connection refused blk_1234"
    q1 = knownitem.make_known_item_query(doc, random.Random(7))
    q2 = knownitem.make_known_item_query(doc, random.Random(7))
    assert q1 == q2
    assert q1 != ""


def test_make_query_empty_doc_returns_empty():
    assert knownitem.make_known_item_query("", random.Random(0)) == ""


def test_make_query_terms_come_from_doc():
    doc = "alpha beta gamma delta"
    q = knownitem.make_known_item_query(doc, random.Random(1), m=2)
    assert all(term in doc.split() for term in q.split())


# ---- T0.2: log corpus loader ----

def test_load_logcorpus_deterministic_same_seed():
    a = logcorpus.load_logcorpus(dataset="_fixture", n=5, seed=3)
    b = logcorpus.load_logcorpus(dataset="_fixture", n=5, seed=3)
    assert a == b
    assert len(a) == 5


def test_load_logcorpus_unknown_dataset_raises():
    with pytest.raises(ValueError):
        logcorpus.load_logcorpus(dataset="does-not-exist", n=5, seed=0)


def test_load_logcorpus_returns_docs_dict():
    docs = logcorpus.load_logcorpus(dataset="_fixture", n=3, seed=1)
    assert isinstance(docs, dict)
    assert all(isinstance(k, str) and isinstance(v, str) for k, v in docs.items())


def test_load_logcorpus_n_larger_than_source_uses_all():
    # _fixture has a bounded number of lines; n far beyond it returns all, no crash
    docs = logcorpus.load_logcorpus(dataset="_fixture", n=10_000, seed=0)
    assert len(docs) >= 1
