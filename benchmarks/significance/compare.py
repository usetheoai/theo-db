"""Comparação pareada de N sistemas sobre arrays por consulta.

Recebe os `PerQueryScores` de vários sistemas, alinha-os **por identificador de consulta**, e roda
`paired_significance` em cada par não-ordenado. O relatório carrega os arrays por consulta, para que um
terceiro recompute — inclusive com outro teste estatístico, se discordar da escolha.

# Por que alinhar por qid e não por posição

Hoje a posição bastaria: o mesmo `PerQueryEvaluator` dirige todos os sistemas, na mesma ordem. É exatamente
por isso que a suposição é perigosa — ela é verdadeira e **invisível**. Um sistema que pule uma consulta por
erro, uma execução paralela, ou um artefato lido de outra corrida quebram o pareamento sem levantar nada, e
o `p` sai sobre consultas diferentes: um número com aparência de rigor sobre uma comparação que não existe.

Alinhar por chave torna esse caso um erro nomeado em vez de um resultado errado.
"""

from __future__ import annotations

from itertools import combinations

from per_query import PerQueryScores
from significance import paired_significance

METRICS = ("ndcg", "recall", "mrr")


def _indexed(s: PerQueryScores, metric: str) -> dict[str, float]:
    values = getattr(s, metric)
    if len(values) != len(s.qids):
        msg = f"{s.system}: {len(s.qids)} qids mas {len(values)} valores de {metric}"
        raise ValueError(msg)
    out: dict[str, float] = {}
    for qid, value in zip(s.qids, values, strict=True):
        if qid in out:
            msg = f"{s.system}: qid duplicado {qid!r} — o pareamento seria ambíguo"
            raise ValueError(msg)
        out[qid] = value
    return out


def align(a: PerQueryScores, b: PerQueryScores, *, metric: str) -> tuple[list[float], list[float], list[str]]:
    """Devolve os valores dos dois sistemas alinhados por qid, e os qids na ordem usada.

    Um qid presente num sistema e ausente no outro **levanta**, nomeando-o: pareamento quebrado é resultado
    inválido, não um caso a contornar.
    """
    if metric not in METRICS:
        msg = f"métrica desconhecida {metric!r}; disponíveis: {', '.join(METRICS)}"
        raise ValueError(msg)

    ia, ib = _indexed(a, metric), _indexed(b, metric)
    faltando = (set(ia) ^ set(ib))
    if faltando:
        amostra = ", ".join(sorted(faltando)[:5])
        msg = (
            f"{a.system} e {b.system} não cobrem as mesmas consultas: "
            f"{len(faltando)} divergente(s), por exemplo {amostra}"
        )
        raise ValueError(msg)

    qids = sorted(ia)
    return [ia[q] for q in qids], [ib[q] for q in qids], qids


def compare_systems(
    systems: list[PerQueryScores],
    *,
    metric: str = "ndcg",
    seed: int = 20260720,
    n_resamples: int = 100_000,
) -> dict:
    """Roda o teste pareado em cada par não-ordenado e devolve o relatório.

    O sinal de `mean_diff` segue a ordem do par: `a_vs_b` positivo significa que `a` foi melhor.
    """
    if metric not in METRICS:
        msg = f"métrica desconhecida {metric!r}; disponíveis: {', '.join(METRICS)}"
        raise ValueError(msg)
    if len(systems) < 2:
        msg = f"é preciso ao menos dois sistemas para comparar, recebi {len(systems)}"
        raise ValueError(msg)

    comparisons: dict[str, dict] = {}
    for a, b in combinations(systems, 2):
        va, vb, qids = align(a, b, metric=metric)
        result = paired_significance(va, vb, seed=seed, n_resamples=n_resamples)
        result["systems"] = f"{a.system}_vs_{b.system}"
        result["metric"] = metric
        result["n_queries"] = len(qids)
        comparisons[f"{a.system}_vs_{b.system}"] = result

    return {
        "metric": metric,
        "seed": seed,
        "n_resamples": n_resamples,
        "means": {s.system: (sum(getattr(s, metric)) / len(s) if len(s) else 0.0) for s in systems},
        "comparisons": comparisons,
        # Persistidos para recomputo por terceiro — um `p` sem o dado é inverificável.
        "per_query": {s.system: s.as_dict() for s in systems},
    }


def verdict(result: dict, *, alpha: float = 0.05) -> str:
    """Leitura em uma linha, distinguindo os dois casos que um `p` alto pode significar.

    Um `p` alto com IC estreito em torno de zero é evidência de **equivalência**; um `p` alto com IC largo é
    **falta de poder**. Tratá-los como a mesma coisa é como se afirma paridade sem tê-la medido.
    """
    p = result["p_permutation"]
    low, high = result["ci95_low"], result["ci95_high"]
    if p < alpha:
        direcao = "melhor" if result["mean_diff"] > 0 else "pior"
        return f"significativo (p={p:.4g}): o primeiro é {direcao}, diferença média {result['mean_diff']:+.4f}"
    largura = high - low
    if largura <= 0.02:
        return f"não-significativo (p={p:.4g}) com IC estreito [{low:+.4f}, {high:+.4f}] — evidência de equivalência"
    return (
        f"não-significativo (p={p:.4g}) com IC largo [{low:+.4f}, {high:+.4f}] — "
        f"falta de poder, NÃO evidência de equivalência"
    )
