"""Unit tests (offline, deterministic) for RRF fusion + BEIR-style metrics (M7-S1, T2.1)."""
import math

from theodb_bench.hybrid import rrf_fuse
from theodb_bench.metrics import ndcg_at_k, recall_at_n


def test_rrf_fuse_matches_handcalc():
    # legs: A,B,C and B,D  (k=60). B appears in both -> highest.
    fused = rrf_fuse([["A", "B", "C"], ["B", "D"]], k=60)
    ids = [i for i, _ in fused]
    assert ids == ["B", "A", "D", "C"], fused
    score = dict(fused)
    # B: leg1 rank2 (1/62) + leg2 rank1 (1/61); A: leg1 rank1; D: leg2 rank2; C: leg1 rank3
    assert math.isclose(score["B"], 1.0 / 62 + 1.0 / 61, rel_tol=1e-9)
    assert math.isclose(score["A"], 1.0 / 61, rel_tol=1e-9)
    assert math.isclose(score["D"], 1.0 / 62, rel_tol=1e-9)
    assert math.isclose(score["C"], 1.0 / 63, rel_tol=1e-9)


def test_rrf_fuse_empty_leg():
    fused = rrf_fuse([["A", "B"], []], k=60)
    assert [i for i, _ in fused] == ["A", "B"]  # other leg preserved, no crash


def test_ndcg_at_k_perfect_is_1():
    qrels = {"a": 3, "b": 2, "c": 1}
    assert math.isclose(ndcg_at_k(["a", "b", "c"], qrels, 10), 1.0, rel_tol=1e-9)


def test_ndcg_at_k_no_relevant_is_0():
    assert ndcg_at_k(["a", "b"], {}, 10) == 0.0  # IDCG == 0 -> 0.0, no div-by-zero


def test_ndcg_at_k_worse_order_below_1():
    qrels = {"a": 3, "b": 2, "c": 1}
    assert ndcg_at_k(["c", "b", "a"], qrels, 10) < 1.0  # bad order penalized


def test_recall_at_n_counts_relevant():
    qrels = {"a": 1, "b": 1, "c": 1}  # 3 relevant
    assert math.isclose(recall_at_n(["a", "x", "b", "y"], qrels, 100), 2 / 3, rel_tol=1e-9)


def test_recall_at_n_no_relevant_is_0():
    assert recall_at_n(["a", "b"], {}, 100) == 0.0


def test_beir_loader_roundtrip():
    from theodb_bench.beir import EMBED_DIM, lexical_embed, synthetic_dataset
    ds = synthetic_dataset()
    assert len(ds.corpus) == 12
    assert len(ds.queries) == 4
    assert ds.qrels["q1"]["d2"] == 3  # known graded label
    v = lexical_embed("postgresql database", EMBED_DIM)
    assert len(v) == EMBED_DIM
    assert abs(sum(x * x for x in v) ** 0.5 - 1.0) < 1e-9  # L2-normalized
