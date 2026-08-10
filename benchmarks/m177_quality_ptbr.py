"""M177 fase 1 — QUALIDADE dos modelos multilingues num corpus pt-BR REAL: o acervo do proprio projeto.

Fecha o unico item da fase 1 que nunca teve numero. Tudo o que o M177 mediu ate aqui e CUSTO
(latencia, memoria, throughput); um modelo barato que recupera mal nao serve, e a escolha do modelo
tem 3,7x de spread de latencia sem nenhuma evidencia de qualidade para pesar contra.

O CORPUS. Nao ha corpus pt-BR com qrels no repositorio, e inventar julgamento de relevancia seria
fabricar evidencia. A saida honesta e usar um corpus REAL do proprio projeto com relevancia
DERIVAVEL, nao inventada: os conceitos da wiki tem `title` + `description` em pt-BR escritos por
maos diferentes em momentos diferentes.

  documento = title + corpo do conceito
  consulta  = a `description` do frontmatter (uma frase que resume o conceito, escrita a parte)
  qrel      = 1:1, o conceito de origem (known-item retrieval)

Isso e known-item search classico, e a relevancia e ground-truth por construcao — nao um juizo meu.
O vies conhecido esta declarado no artefato: description e corpo compartilham vocabulario, entao os
numeros absolutos sao OTIMISTAS. O que a comparacao mede com validade e a ORDEM entre modelos, que e
a decisao em jogo.

Gate D1: so modelos permissivos. Non-commercial nao e medido — medir o que nao pode ser distribuido
produz numero que seduz e nao pode ser usado.

Uso: python3 benchmarks/m177_quality_ptbr.py --wiki wiki --json out.json
"""
from __future__ import annotations

import argparse
import json
import os
import re
import statistics
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from theodb_bench.knownitem import mrr_at_k, recall_known_item, success_at_1  # noqa: E402

D1_OK = {"mit", "apache-2.0", "bsd", "bsd-3-clause", "postgresql"}
FM = re.compile(r"^---\n(.*?)\n---\n(.*)", re.DOTALL)


def load_corpus(wiki_dir: str, max_docs: int) -> tuple[list, list]:
    """Devolve (docs, queries). doc = (id, texto); query = (texto, doc_id_alvo)."""
    docs, queries = [], []
    for root, _, files in os.walk(wiki_dir):
        for f in sorted(files):
            if not f.endswith(".md") or f == "index.md":
                continue
            path = os.path.join(root, f)
            m = FM.match(open(path, encoding="utf-8").read())
            if not m:
                continue
            fm, body = m.group(1), m.group(2)
            t = re.search(r"^title:\s*(.+)$", fm, re.MULTILINE)
            d = re.search(r"^description:\s*(.+)$", fm, re.MULTILINE)
            if not t or not d:
                continue
            title, desc = t.group(1).strip(), d.group(1).strip()
            if len(desc) < 40:  # descricao curta demais nao e consulta
                continue
            # documento: titulo + corpo sem markup pesado, truncado (limite de contexto do encoder)
            clean = re.sub(r"[|#*`>\[\]()]", " ", body)
            clean = re.sub(r"\s+", " ", clean).strip()[:1200]
            doc_id = os.path.relpath(path, wiki_dir)
            docs.append((doc_id, f"{title}. {clean}"))
            queries.append((desc, doc_id))
    return docs[:max_docs], queries[:max_docs]


def evaluate(model_name: str, docs: list, queries: list) -> dict:
    import numpy as np
    from fastembed import TextEmbedding

    t0 = time.perf_counter()
    model = TextEmbedding(model_name=model_name)
    load_s = time.perf_counter() - t0

    doc_ids = [d[0] for d in docs]
    dv = np.array(list(model.embed([d[1] for d in docs])), dtype=np.float32)
    dv /= np.linalg.norm(dv, axis=1, keepdims=True) + 1e-12

    lat = []
    qv = []
    for q, _ in queries:  # uma por vez: e o regime real da consulta
        t = time.perf_counter()
        v = next(iter(model.embed([q])))
        lat.append((time.perf_counter() - t) * 1000)
        qv.append(v)
    qm = np.array(qv, dtype=np.float32)
    qm /= np.linalg.norm(qm, axis=1, keepdims=True) + 1e-12

    sims = qm @ dv.T
    mrr, s1, r10 = [], [], []
    for i, (_, target) in enumerate(queries):
        order = np.argsort(-sims[i])[:20]
        ranked = [doc_ids[j] for j in order]
        mrr.append(mrr_at_k(ranked, target, 10))
        s1.append(success_at_1(ranked, target))
        r10.append(recall_known_item(ranked, target, 10))

    lat.sort()
    return {
        "model": model_name,
        "dim": int(dv.shape[1]),
        "n_docs": len(docs),
        "n_queries": len(queries),
        "mrr_at_10": round(float(statistics.mean(mrr)), 4),
        "success_at_1": round(float(statistics.mean(s1)), 4),
        "recall_at_10": round(float(statistics.mean(r10)), 4),
        "query_latency_p50_ms": round(lat[len(lat) // 2], 2),
        "query_latency_mean_ms": round(statistics.mean(lat), 2),
        "model_load_s": round(load_s, 1),
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--wiki", default="wiki")
    ap.add_argument("--max-docs", type=int, default=250)
    ap.add_argument("--only", default=None, help="mede um modelo (modo subprocesso)")
    ap.add_argument("--max-gb", type=float, default=2.5)
    ap.add_argument("--json", default=None)
    a = ap.parse_args()

    docs, queries = load_corpus(a.wiki, a.max_docs)
    if a.only:
        print(json.dumps(evaluate(a.only, docs, queries)))
        return 0

    from fastembed import TextEmbedding
    import subprocess

    cands = []
    for m in TextEmbedding.list_supported_models():
        desc = str(m.get("description", "")).lower()
        if "multi" not in desc and "multilingual" not in m["model"].lower():
            continue
        lic = str(m.get("license", "?")).lower()
        if lic not in D1_OK or (m.get("size_in_GB") or 9) > a.max_gb:
            continue
        cands.append({"model": m["model"], "license": lic, "size_gb": m.get("size_in_GB")})

    out = {"corpus": {"docs": len(docs), "queries": len(queries), "source": "wiki (pt-BR)"},
           "results": []}
    for c in cands:
        print(f"→ {c['model']}", file=sys.stderr, flush=True)
        p = subprocess.run(
            [sys.executable, __file__, "--only", c["model"], "--wiki", a.wiki,
             "--max-docs", str(a.max_docs)],
            capture_output=True, text=True,
            env={**os.environ, "OMP_NUM_THREADS": "1", "ORT_NUM_THREADS": "1"})
        if p.returncode != 0:
            c["error"] = (p.stderr.strip().splitlines() or ["falhou"])[-1][:120]
        else:
            c.update(json.loads(p.stdout))
        out["results"].append(c)

    print(json.dumps(out, indent=2, ensure_ascii=False))
    if a.json:
        json.dump(out, open(a.json, "w"), indent=2, ensure_ascii=False)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
