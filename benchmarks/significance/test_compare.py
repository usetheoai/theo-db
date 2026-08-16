"""Testes do `compare` — alinhamento por qid e comparação par a par de N sistemas.

Duas exigências vêm do consumidor antigo (`run_m53_hybrid_beir._paired_sig`), que a D4 do plano decidiu não
recuperar por ser engessado em três sistemas nomeados. As exigências valem; o código não servia:

  1. **Alinhar por `qid`, nunca por posição.** Um sistema que pule uma consulta quebra o pareamento *sem
     erro*, e o `p` sai sobre consultas diferentes.
  2. **Persistir os arrays.** Um `p` sem o dado é uma afirmação que ninguém pode verificar.
"""

from __future__ import annotations

import pytest

from compare import align, compare_systems
from per_query import PerQueryScores


def scores(system: str, qids: list[str], ndcg: list[float]) -> PerQueryScores:
    n = len(qids)
    return PerQueryScores(system=system, qids=qids, ndcg=ndcg, recall=[1.0] * n, mrr=[1.0] * n)


def test_aligns_by_qid_not_by_position():
    a = scores("a", ["q1", "q2", "q3"], [0.9, 0.5, 0.7])
    b = scores("b", ["q2", "q3", "q1"], [0.4, 0.6, 0.8])  # ordem diferente de propósito
    va, vb, qids = align(a, b, metric="ndcg")
    assert qids == ["q1", "q2", "q3"]
    # alinhado: q1 0,9−0,8  q2 0,5−0,4  q3 0,7−0,6  → todas +0,1
    assert [round(x - y, 10) for x, y in zip(va, vb)] == [0.1, 0.1, 0.1]


def test_missing_qid_fails_loudly_naming_it():
    a = scores("a", ["q1", "q2"], [0.5, 0.5])
    b = scores("b", ["q1"], [0.5])
    with pytest.raises(ValueError, match="q2"):
        align(a, b, metric="ndcg")


def test_duplicate_qid_fails_loudly():
    a = scores("a", ["q1", "q1"], [0.5, 0.6])
    b = scores("b", ["q1", "q2"], [0.5, 0.6])
    with pytest.raises(ValueError, match="duplicad"):
        align(a, b, metric="ndcg")


def test_pairwise_covers_every_unordered_pair():
    systems = [
        scores("a", ["q1", "q2"], [0.6, 0.6]),
        scores("b", ["q1", "q2"], [0.5, 0.5]),
        scores("c", ["q1", "q2"], [0.4, 0.4]),
    ]
    rep = compare_systems(systems, metric="ndcg")
    assert set(rep["comparisons"]) == {"a_vs_b", "a_vs_c", "b_vs_c"}


def test_sign_of_mean_diff_follows_the_pair_order():
    systems = [
        scores("better", ["q1", "q2"], [0.6, 0.6]),
        scores("worse", ["q1", "q2"], [0.5, 0.5]),
    ]
    r = compare_systems(systems, metric="ndcg")["comparisons"]["better_vs_worse"]
    assert r["mean_diff"] > 0
    assert r["wins"] == 2 and r["losses"] == 0


def test_per_query_arrays_are_persisted_for_third_party_recompute():
    systems = [
        scores("a", ["q1", "q2"], [0.6, 0.7]),
        scores("b", ["q1", "q2"], [0.5, 0.5]),
    ]
    pq = compare_systems(systems, metric="ndcg")["per_query"]
    assert set(pq) == {"a", "b"}
    assert pq["a"]["qids"] == ["q1", "q2"]
    assert pq["a"]["ndcg"] == [0.6, 0.7]


def test_report_is_deterministic_for_a_fixed_seed():
    systems = [
        scores("a", [f"q{i}" for i in range(20)], [0.5 + (i % 3) * 0.01 for i in range(20)]),
        scores("b", [f"q{i}" for i in range(20)], [0.5 for _ in range(20)]),
    ]
    one = compare_systems(systems, metric="ndcg", seed=7)["comparisons"]["a_vs_b"]
    two = compare_systems(systems, metric="ndcg", seed=7)["comparisons"]["a_vs_b"]
    assert one["p_permutation"] == two["p_permutation"]
    assert one["ci95_low"] == two["ci95_low"]


def test_fewer_than_two_systems_is_refused():
    with pytest.raises(ValueError, match="ao menos dois"):
        compare_systems([scores("a", ["q1", "q2"], [0.5, 0.5])], metric="ndcg")


def test_unknown_metric_is_refused():
    systems = [scores("a", ["q1", "q2"], [0.5, 0.5]), scores("b", ["q1", "q2"], [0.4, 0.4])]
    with pytest.raises(ValueError, match="métrica"):
        compare_systems(systems, metric="inexistente")
