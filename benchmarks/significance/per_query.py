"""Avaliação por consulta sobre a porta de cliente do VectorDBBench.

# Por que este módulo existe

Um teste pareado de significância precisa do valor **por consulta** de cada sistema. O VectorDBBench os
computa e os descarta: `backend/runner/serial_runner.py` monta as listas `recalls`, `ndcgs` e `mrrs` e o
método devolve apenas as médias. Os arrays morrem no `return`.

Persistí-los lá exigiria mudar a tupla de retorno, todos os chamadores e o dataclass `Metric` — uma
alteração que atravessa `runner/` e `task_runner.py`, o núcleo que a Política de Fork manda não tocar,
porque é o que mantém o fork rebaseável.

Este módulo obtém o mesmo dado **por fora**, dependendo só de três abstrações que o arnês já expõe:

* a porta `VectorDB.search_documents(query, k) -> list[str]`;
* as funções de métrica `calc_ndcg_fts` / `calc_recall_fts` / `calc_mrr_fts`;
* o conjunto de consultas e qrels do dataset.

Importar a métrica do arnês, em vez de reimplementá-la, é o que torna os números **idênticos por
construção** aos da tabela publicada — dois cálculos de NDCG divergem em detalhes (corte em k, empates,
DCG ideal), e um `p` sobre números divergentes é pior que nenhum `p`.

# Por que é sequencial

A ordem das consultas **é** o pareamento. Dirigir o laço aqui garante que todos os sistemas vejam as mesmas
consultas na mesma ordem, que é o pré-requisito do teste pareado — e é uma garantia que persistir dentro do
arnês não daria de graça. Paralelizar trocaria isso por alguns segundos num passe que leva ~4 s.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Protocol

from vectordb_bench.metric import calc_mrr_fts, calc_ndcg_fts, calc_recall_fts


class DocumentSearcher(Protocol):
    """A parte da porta `VectorDB` de que este módulo depende — e só ela.

    Declarada como `Protocol` para que a dependência seja estrutural: qualquer cliente do arnês serve, e um
    duble de três linhas exercita o avaliador inteiro nos testes. Nenhum motor é nomeado neste arquivo.
    """

    def search_documents(self, query: str, k: int = 100, **kwargs) -> list[str]: ...


@dataclass(frozen=True)
class PerQueryScores:
    """Os arrays por consulta de um sistema, alinháveis por `qids`.

    `qids` é o que permite parear sistemas por identificador em vez de por posição — a suposição de mesma
    ordem é verdadeira hoje e invisível quando deixa de ser.
    """

    system: str
    qids: list[str] = field(default_factory=list)
    ndcg: list[float] = field(default_factory=list)
    recall: list[float] = field(default_factory=list)
    mrr: list[float] = field(default_factory=list)

    def __len__(self) -> int:
        return len(self.qids)

    def as_dict(self) -> dict:
        """Forma serializável, para que um terceiro recompute o teste a partir do artefato."""
        return {
            "system": self.system,
            "qids": self.qids,
            "ndcg": self.ndcg,
            "recall": self.recall,
            "mrr": self.mrr,
        }


class PerQueryEvaluator:
    """Dirige o laço de consultas sobre um cliente e devolve os arrays por consulta.

    Reutilizável entre sistemas por construção: depende do `Protocol`, não de nenhum motor.
    """

    def __init__(self, k: int = 10):
        if k <= 0:
            msg = f"k deve ser positivo, recebi {k}"
            raise ValueError(msg)
        self.k = k

    def evaluate(
        self,
        client: DocumentSearcher,
        queries: list[tuple[str, str]],
        qrels: list[dict[str, int]],
        *,
        system: str = "",
    ) -> PerQueryScores:
        """Consulta o cliente uma vez por entrada e computa as três métricas.

        `queries` são pares `(qid, texto)`; `qrels[i]` são os julgamentos da consulta `i`.

        Uma falha do motor **propaga**. Completar o array com zeros produziria um recall artificialmente
        baixo que seria publicado como medição — a classe de defeito que este pacote inteiro existe para
        impedir.
        """
        if len(queries) != len(qrels):
            msg = f"queries e qrels devem ter o mesmo comprimento, recebi {len(queries)} e {len(qrels)}"
            raise ValueError(msg)

        qids: list[str] = []
        ndcgs: list[float] = []
        recalls: list[float] = []
        mrrs: list[float] = []

        for (qid, text), gt in zip(queries, qrels, strict=True):
            got = client.search_documents(text, k=self.k)
            qids.append(qid)
            ndcgs.append(calc_ndcg_fts(self.k, gt, got))
            recalls.append(calc_recall_fts(self.k, gt, got))
            mrrs.append(calc_mrr_fts(self.k, gt, got))

        return PerQueryScores(system=system, qids=qids, ndcg=ndcgs, recall=recalls, mrr=mrrs)
