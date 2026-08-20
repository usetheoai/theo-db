"""Testes do `PerQueryEvaluator` — sem banco, sem rede, sem motor.

O avaliador existe porque o VectorDBBench computa métrica por consulta e a descarta: `serial_runner.py`
monta `recalls`/`ndcgs`/`mrrs` e devolve só as médias. Um teste pareado precisa dos arrays.

O que estes testes protegem, em ordem de importância:

  1. **A métrica é a do arnês, não uma reimplementação.** Se divergir, o `p` sai sobre números que não são
     os da tabela publicada — pior que não ter `p`.
  2. **O avaliador não conhece motor nenhum.** Depende da porta `VectorDB.search_documents`, e um cliente
     falso de 3 linhas o exercita inteiro.
  3. **Entrada inconsistente falha alto.** Consultas e qrels de comprimentos diferentes não produzem um
     array pela metade.
"""

from __future__ import annotations

import pytest

from per_query import PerQueryEvaluator


class FakeClient:
    """Implementa só o que o avaliador usa: a porta `VectorDB.search_documents`.

    Que três linhas bastem é a evidência de que a dependência é a porta, não um motor.
    """

    def __init__(self, answers: dict[str, list[str]]):
        self.answers = answers

    def search_documents(self, query: str, k: int = 100, **_kwargs) -> list[str]:
        return self.answers.get(query, [])[:k]


QUERIES = [("q1", "lazy dog"), ("q2", "quick fox")]
QRELS = [{"d1": 1}, {"d9": 1}]


def test_returns_one_score_per_query_in_order():
    ev = PerQueryEvaluator(k=10)
    r = ev.evaluate(FakeClient({"lazy dog": ["d1", "d2"], "quick fox": ["d3"]}), QUERIES, QRELS)
    assert r.qids == ["q1", "q2"]
    assert len(r.ndcg) == len(r.recall) == len(r.mrr) == 2


def test_query_with_no_relevant_hit_scores_zero_and_is_still_counted():
    """Consulta que não acha nada relevante é dado, não falha: entra no array com 0."""
    ev = PerQueryEvaluator(k=10)
    r = ev.evaluate(FakeClient({"lazy dog": ["d1"], "quick fox": ["d3"]}), QUERIES, QRELS)
    assert r.ndcg[0] > 0
    assert r.ndcg[1] == 0
    assert len(r.ndcg) == 2, "a consulta sem acerto não pode sumir do array"


def test_scores_equal_the_harness_metric_called_directly():
    """A garantia que torna o `p` confiável: mesma função, não uma reimplementação equivalente."""
    from vectordb_bench.metric import calc_ndcg_fts, calc_recall_fts

    got = ["d1", "d2"]
    ev = PerQueryEvaluator(k=10)
    r = ev.evaluate(FakeClient({"lazy dog": got, "quick fox": []}), QUERIES, QRELS)
    assert r.ndcg[0] == calc_ndcg_fts(10, QRELS[0], got)
    assert r.recall[0] == calc_recall_fts(10, QRELS[0], got)


def test_mismatched_lengths_raise_before_any_query_is_sent():
    class ExplodingClient:
        def search_documents(self, *_a, **_k):
            raise AssertionError("nenhuma consulta deveria ter sido enviada")

    with pytest.raises(ValueError, match="queries e qrels"):
        PerQueryEvaluator(k=10).evaluate(ExplodingClient(), QUERIES, [])


def test_engine_failure_propagates_instead_of_padding_with_zeros():
    """Um motor que cai no meio do passe não pode virar um array de zeros — seria recall 0 publicado."""

    class FlakyClient:
        def __init__(self):
            self.calls = 0

        def search_documents(self, *_a, **_k):
            self.calls += 1
            if self.calls == 2:
                raise RuntimeError("motor caiu")
            return ["d1"]

    with pytest.raises(RuntimeError, match="motor caiu"):
        PerQueryEvaluator(k=10).evaluate(FlakyClient(), QUERIES, QRELS)


def test_evaluator_names_no_engine():
    """OCP: um motor novo entra implementando a porta, sem tocar aqui."""
    import pathlib

    src = pathlib.Path(__file__).with_name("per_query.py").read_text().lower()
    for engine in ("theodb", "elastic", "opensearch", "milvus"):
        assert engine not in src, f"o avaliador não pode nomear o motor {engine!r}"


def test_evaluator_is_deliberately_sequential():
    """A ordem das consultas É o pareamento; paralelizar trocaria a garantia por segundos."""
    import pathlib

    src = pathlib.Path(__file__).with_name("per_query.py").read_text()
    for parallel in ("ThreadPool", "multiprocessing", "concurrent.futures", "asyncio"):
        assert parallel not in src, f"{parallel} quebraria a ordem que define o pareamento"
