"""M177 Fase 1 — quanto do custo de um embedding é a INFERÊNCIA e quanto é o HOP HTTP local.

Este é o número que decide se a Fase 2 (modelo como extensão instalável, in-process) se justifica
por latência. O desenho atual do TheoDB chama um endpoint (`theodb.embed` → HTTP); a proposta é
carregar o modelo dentro do host. Se o hop for ruído diante da inferência, a extensão in-process é
complexidade acidental — e o milestone fecha na Fase 1.

Dois braços sobre o MESMO modelo, MESMOS textos, MESMA máquina:

    A (in-process)  fastembed direto, sem rede         → só inferência
    B (via HTTP)    POST no servidor local :8088        → inferência + serialização + loopback TCP

    custo do hop = B - A

Rigor (rules/testing.md; lição M123/M130/M131 — CV não é significância):
  * warm-up descartado (a 1ª chamada carrega/aquece o grafo ONNX)
  * braços ALTERNADOS A,B,A,B — blocos A×N seguidos de B×N sofrem deriva térmica
  * N repetições, média e desvio reportados; delta com bootstrap pareado
  * vários batch sizes: o hop é custo FIXO por requisição, então seu peso relativo cai com o batch

Uso:
    python3 benchmarks/m177_hop_cost.py --endpoint http://127.0.0.1:8088/v1/embeddings \
        --model BAAI/bge-small-en-v1.5 --runs 15 --json out.json
"""
from __future__ import annotations

import argparse
import json
import os
import statistics
import sys
import time
import urllib.request

# reutiliza o bootstrap pareado do harness em vez de reimplementar (parsimony rung 4)
sys.path.insert(0, os.path.join(os.path.dirname(__file__)))
from theodb_bench.significance import paired_significance  # noqa: E402

CORPUS = [
    "PostgreSQL is an open source object-relational database system.",
    "Vector similarity search retrieves the nearest neighbours of a query embedding.",
    "The HNSW index builds a navigable small world graph over the vector space.",
    "Columnar storage decodes only the columns a query references.",
    "A background worker keeps the embedding column fresh as content changes.",
    "Quantisation buys memory, and repeatedly measured as not buying QPS.",
    "Reciprocal rank fusion combines a vector leg and a lexical leg.",
    "Crash safety comes from the write-ahead log of the host database.",
]


def rss_mb() -> float:
    """RSS deste processo, em MB. Linux only — lido de /proc, sem dependência nova."""
    with open("/proc/self/status") as fh:
        for line in fh:
            if line.startswith("VmRSS:"):
                return int(line.split()[1]) / 1024.0
    return float("nan")


def embed_http(endpoint: str, model: str, texts: list) -> float:
    """Uma requisição ao servidor local. Devolve o tempo de parede em ms."""
    payload = json.dumps({"input": texts, "model": model}).encode()
    req = urllib.request.Request(
        endpoint, data=payload, headers={"Content-Type": "application/json"}
    )
    t0 = time.perf_counter()
    with urllib.request.urlopen(req, timeout=60) as resp:
        body = json.loads(resp.read())
    dt = (time.perf_counter() - t0) * 1000.0
    got = len(body["data"])
    if got != len(texts):
        raise ValueError(f"servidor devolveu {got} embeddings para {len(texts)} textos")
    return dt


def embed_inproc(model_obj, texts: list) -> float:
    """A mesma inferência, no processo. Devolve o tempo de parede em ms."""
    t0 = time.perf_counter()
    out = list(model_obj.embed(texts))
    dt = (time.perf_counter() - t0) * 1000.0
    if len(out) != len(texts):
        raise ValueError(f"in-process devolveu {len(out)} embeddings para {len(texts)} textos")
    return dt


def main() -> int:
    ap = argparse.ArgumentParser(description="M177 fase 1 — custo do hop HTTP local vs inferência")
    ap.add_argument("--endpoint", default="http://127.0.0.1:8088/v1/embeddings")
    ap.add_argument("--model", default="BAAI/bge-small-en-v1.5")
    ap.add_argument("--runs", type=int, default=15, help="repetições por braço, por batch size")
    ap.add_argument("--batches", default="1,8", help="tamanhos de batch, separados por vírgula")
    ap.add_argument("--json", default=None, help="onde gravar o artefato bruto")
    args = ap.parse_args()

    rss_before = rss_mb()
    from fastembed import TextEmbedding  # dep pesada: importada só aqui

    t_load0 = time.perf_counter()
    model_obj = TextEmbedding(model_name=args.model)
    load_ms = (time.perf_counter() - t_load0) * 1000.0
    list(model_obj.embed(CORPUS[:2]))  # warm-up do grafo ONNX, descartado
    rss_after = rss_mb()

    report = {
        "model": args.model,
        "endpoint": args.endpoint,
        "runs_per_arm": args.runs,
        "model_load_ms": round(load_ms, 1),
        "rss_before_mb": round(rss_before, 1),
        "rss_after_mb": round(rss_after, 1),
        "rss_model_cost_mb": round(rss_after - rss_before, 1),
        "batches": {},
    }

    for bs in [int(x) for x in args.batches.split(",")]:
        texts = (CORPUS * ((bs // len(CORPUS)) + 1))[:bs]
        embed_http(args.endpoint, args.model, texts)  # warm-up do braço HTTP
        a_samples, b_samples = [], []
        for _ in range(args.runs):  # ALTERNADO: A,B,A,B — nunca A×N depois B×N
            a_samples.append(embed_inproc(model_obj, texts))
            b_samples.append(embed_http(args.endpoint, args.model, texts))

        a_mean, b_mean = statistics.mean(a_samples), statistics.mean(b_samples)
        sig = paired_significance(a_samples, b_samples)
        report["batches"][str(bs)] = {
            "inproc_ms_mean": round(a_mean, 3),
            "inproc_ms_stdev": round(statistics.stdev(a_samples), 3),
            "http_ms_mean": round(b_mean, 3),
            "http_ms_stdev": round(statistics.stdev(b_samples), 3),
            "hop_ms_mean": round(b_mean - a_mean, 3),
            "hop_pct_of_http": round(100.0 * (b_mean - a_mean) / b_mean, 2),
            "paired_significance": sig,
            "inproc_raw": [round(x, 3) for x in a_samples],
            "http_raw": [round(x, 3) for x in b_samples],
        }

    print(json.dumps(report, indent=2, ensure_ascii=False, default=str))
    if args.json:
        with open(args.json, "w") as fh:
            json.dump(report, fh, indent=2, default=str)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
