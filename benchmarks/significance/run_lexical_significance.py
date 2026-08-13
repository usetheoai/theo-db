"""Aplica o teste pareado à comparação lexical do b047, contra os motores reais.

Faz um passe de consultas por sistema, computa as métricas por consulta com as funções do próprio arnês, e
**verifica que a média reproduz o agregado que a corrida publicou**. Essa verificação é o que torna o `p`
confiável: se a média não bate, estamos medindo outra coisa, e um `p` sobre outra coisa é pior que nenhum.

Uso:
    python run_lexical_significance.py --published ../vectordbbench/results-lexical/json \\
                                       --out per-query/b047.json
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys

from compare import compare_systems, verdict
from per_query import PerQueryEvaluator, PerQueryScores

# Tolerância contra o agregado publicado: o `serial_runner` arredonda em 4 casas (`round(np.mean(...), 4)`),
# então metade da última casa é o máximo que a diferença pode ser sem indicar divergência real. Constante,
# nunca por motor — afrouxar por sistema seria ajustar o gate para caber no resultado.
AGGREGATE_TOLERANCE = 5e-4


def load_published(results_dir: pathlib.Path, db: str, label: str) -> dict:
    """O agregado que a corrida oficial publicou, para o sistema pedido."""
    best = None
    for f in results_dir.glob("*/*.json"):
        try:
            data = json.loads(f.read_text())
        except Exception:
            continue
        for r in data.get("results", []):
            task = r.get("task_config", {}) or {}
            metrics = r.get("metrics", {}) or {}
            if task.get("db") != db:
                continue
            if (task.get("db_config") or {}).get("db_label") not in (label, "", None):
                continue
            if not metrics.get("recall"):
                continue
            stamp = f.stat().st_mtime
            if best is None or stamp > best[0]:
                best = (stamp, metrics)
    if best is None:
        msg = f"nenhum resultado publicado para db={db!r} label={label!r} em {results_dir}"
        raise SystemExit(msg)
    return best[1]


def check_reproduces(scores: PerQueryScores, published: dict) -> list[str]:
    """Compara a média por consulta com o agregado publicado. Devolve as divergências."""
    problems = []
    for metric in ("ndcg", "recall", "mrr"):
        values = getattr(scores, metric)
        if not values or published.get(metric) is None:
            continue
        mean = sum(values) / len(values)
        delta = abs(mean - published[metric])
        if delta > AGGREGATE_TOLERANCE:
            problems.append(
                f"{scores.system}: {metric} médio {mean:.6f} não reproduz o publicado "
                f"{published[metric]:.6f} (delta {delta:.6f} > {AGGREGATE_TOLERANCE})"
            )
    return problems


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--published", type=pathlib.Path, required=True)
    ap.add_argument("--out", type=pathlib.Path, required=True)
    ap.add_argument("--k", type=int, default=10)
    ap.add_argument("--case", default="FTSBm25Performance")
    args = ap.parse_args()

    from vectordb_bench.backend.cases import CaseType

    case = CaseType[args.case].case_cls()
    dataset = case.dataset
    dataset.prepare()
    queries = [(q.query_id, q.text) for q in (dataset.recall_queries_data or dataset.queries_data)]
    qrels = dataset.recall_gt_data or dataset.gt_data
    print(f"consultas com qrel: {len(queries)}", file=sys.stderr)

    evaluator = PerQueryEvaluator(k=args.k)
    systems: list[PerQueryScores] = []
    problems: list[str] = []

    for name, db, label, factory in _systems_under_test():
        client = factory()
        with client.init():
            scores = evaluator.evaluate(client, queries, qrels, system=name)
        published = load_published(args.published, db, label)
        problems.extend(check_reproduces(scores, published))
        systems.append(scores)
        print(f"{name}: n={len(scores)} ndcg={sum(scores.ndcg) / len(scores):.4f}", file=sys.stderr)

    if problems:
        print("DIVERGÊNCIA — nada é publicado:", file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        return 1

    report = compare_systems(systems, metric="ndcg")
    for name, result in report["comparisons"].items():
        print(f"{name}: {verdict(result)}", file=sys.stderr)

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2))
    print(f"relatório em {args.out}", file=sys.stderr)
    return 0


def _systems_under_test():
    """Os três motores do b047. Cada entrada devolve um cliente já configurado.

    Vive aqui, e não em `per_query.py`, de propósito: o avaliador não pode nomear motor nenhum — é o que
    permite acrescentar um quarto sem tocá-lo.
    """
    from pydantic import SecretStr

    from vectordb_bench.backend.clients.api import IndexType, MetricType

    def theodb():
        from vectordb_bench.backend.clients.theodb.config import TheoDBConfig, TheoDBFTSConfig
        from vectordb_bench.backend.clients.theodb.theodb import TheoDB

        cfg = TheoDBConfig(
            user_name=SecretStr("postgres"), password=SecretStr("theo"),
            host="127.0.0.1", port=55435, db_name="theo",
        )
        return TheoDB(dim=0, db_config=cfg.to_dict(),
                      db_case_config=TheoDBFTSConfig(metric_type=MetricType.BM25),
                      # `theodb_collection` é o default do nosso cliente e é o nome que a carga do arnês criou —
                      # verificado em `pg_tables` depois da carga, não suposto. O `index_id` é derivado dele
                      # por hash, então um nome divergente aponta para um índice que não existe: foi o que o
                      # guard do B-041 pegou na primeira tentativa, em vez de devolver 6.980 zeros.
                      collection_name="theodb_collection", drop_old=False)

    def elastic():
        from vectordb_bench.backend.clients import DB
        # `config_cls` e `case_config_cls` são propriedades que devolvem a CLASSE — construir é uma
        # chamada só. `IndexType.FTS`, não a string.
        cfg = DB.ElasticCloud.config_cls(
            host="127.0.0.1", port=9200, scheme="http",
            user="elastic", password=SecretStr("changeme"),
        )
        return DB.ElasticCloud.init_cls(
            dim=0, db_config=cfg.to_dict(),
            db_case_config=DB.ElasticCloud.case_config_cls(IndexType.FTS)(metric_type=MetricType.BM25),
            drop_old=False,
        )

    def opensearch():
        from vectordb_bench.backend.clients import DB
        cfg = DB.OSSOpenSearch.config_cls(host="127.0.0.1", port=9201)
        return DB.OSSOpenSearch.init_cls(
            dim=0, db_config=cfg.to_dict(),
            db_case_config=DB.OSSOpenSearch.case_config_cls(IndexType.FTS)(metric_type=MetricType.BM25),
            drop_old=False,
        )

    return [
        ("theodb", "TheoDB", "theodb-b044", theodb),
        ("elastic", "ElasticCloud", "elastic-english", elastic),
        ("opensearch", "OSSOpenSearch", "opensearch-english2", opensearch),
    ]


if __name__ == "__main__":
    raise SystemExit(main())
