"""M177 Fase 1 — custo REAL por modelo multilíngue: memória residente, load e latência.

Complementa `m177_hop_cost.py`. Aquele mede o transporte; este mede o que o transporte carrega.

A pergunta que decide a Fase 2 não é "o hop custa?" — é "o hop custa MAIS que o preço de ter o
modelo residente em cada processo?". O prior art (`wiki/references/embedding-local-como-extensao-2026-08.md`)
mostra que `pg_gembed` cacheia o modelo POR BACKEND e que o NeurStore construiu deduplicação e buffers
compartilhados justamente para conter esse overhead. Este script põe número nisso, com modelos
multilíngues — que é o regime real deste produto, não o inglês.

Cada modelo roda em SUBPROCESSO ISOLADO: RSS medido no mesmo processo que já carregou outro modelo é
contaminado pelo anterior. É a diferença entre medir e estimar.

Gate de licença (D1 — Apache-2.0 / MIT / BSD / PostgreSQL): modelos non-commercial (cc-by-nc) são
listados como BARRADO e NÃO medidos. Medir o que não pode ser distribuído produz um número que
seduz e não pode ser usado.

Uso:
    python3 benchmarks/m177_model_survey.py --json out.json
    python3 benchmarks/m177_model_survey.py --only intfloat/multilingual-e5-large
"""
from __future__ import annotations

import argparse
import json
import os
import statistics
import subprocess
import sys
import time

D1_OK = {"mit", "apache-2.0", "bsd", "bsd-3-clause", "postgresql"}

TEXTS = [
    "O banco de dados armazena vetores e faz busca por similaridade.",
    "PostgreSQL is an open source object-relational database system.",
    "La base de datos indexa los vectores para la búsqueda semántica.",
    "El índice HNSW construye un grafo navegable sobre el espacio vectorial.",
    "A busca híbrida combina a perna vetorial com a perna lexical.",
    "Crash safety comes from the write-ahead log of the host database.",
    "O worker mantém a coluna de embedding fresca conforme o conteúdo muda.",
    "Quantisation buys memory and repeatedly measures as not buying QPS.",
]


def rss_mb() -> float:
    with open("/proc/self/status") as fh:
        for line in fh:
            if line.startswith("VmRSS:"):
                return int(line.split()[1]) / 1024.0
    return float("nan")


def measure_one(model_name: str, runs: int) -> dict:
    """Roda DENTRO do subprocesso: carrega um único modelo e mede."""
    from fastembed import TextEmbedding

    rss0 = rss_mb()
    t0 = time.perf_counter()
    model = TextEmbedding(model_name=model_name)
    load_ms = (time.perf_counter() - t0) * 1000.0
    vecs = list(model.embed(TEXTS[:2]))  # warm-up, descartado
    rss1 = rss_mb()

    out = {"model": model_name, "dim": len(vecs[0]), "load_ms": round(load_ms, 1),
           "rss_baseline_mb": round(rss0, 1), "rss_loaded_mb": round(rss1, 1),
           "rss_model_cost_mb": round(rss1 - rss0, 1), "latency": {}}

    for bs in (1, 8):
        texts = (TEXTS * ((bs // len(TEXTS)) + 1))[:bs]
        list(model.embed(texts))  # warm-up por batch size
        samples = []
        for _ in range(runs):
            t = time.perf_counter()
            list(model.embed(texts))
            samples.append((time.perf_counter() - t) * 1000.0)
        out["latency"][str(bs)] = {
            "ms_mean": round(statistics.mean(samples), 2),
            "ms_stdev": round(statistics.stdev(samples), 2) if len(samples) > 1 else 0.0,
            "n": len(samples),
        }
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description="M177 — custo por modelo multilíngue")
    ap.add_argument("--runs", type=int, default=10)
    ap.add_argument("--only", default=None, help="mede um único modelo (usado no subprocesso)")
    ap.add_argument("--json", default=None)
    ap.add_argument("--max-gb", type=float, default=2.5, help="pula modelos maiores que isto")
    args = ap.parse_args()

    if args.only:  # modo subprocesso: um modelo, saída JSON pura
        print(json.dumps(measure_one(args.only, args.runs)))
        return 0

    from fastembed import TextEmbedding

    catalog, barred = [], []
    for m in TextEmbedding.list_supported_models():
        desc = str(m.get("description", "")).lower()
        if "multi" not in desc and "multilingual" not in m["model"].lower():
            continue
        lic = str(m.get("license", "?")).lower()
        entry = {"model": m["model"], "dim": m.get("dim"),
                 "size_gb": m.get("size_in_GB"), "license": lic}
        if lic not in D1_OK:
            entry["d1"] = "BARRADO"
            barred.append(entry)
        else:
            entry["d1"] = "OK"
            catalog.append(entry)

    report = {"barred_by_d1": barred, "measured": [], "skipped_too_large": []}

    for entry in catalog:
        if entry["size_gb"] and entry["size_gb"] > args.max_gb:
            report["skipped_too_large"].append(entry)
            continue
        print(f"→ medindo {entry['model']} ({entry['size_gb']} GB, {entry['license']})",
              file=sys.stderr, flush=True)
        proc = subprocess.run(
            [sys.executable, __file__, "--only", entry["model"], "--runs", str(args.runs)],
            capture_output=True, text=True,
            env={**os.environ, "OMP_NUM_THREADS": "1", "ORT_NUM_THREADS": "1"},
        )
        if proc.returncode != 0:
            entry["error"] = proc.stderr.strip().splitlines()[-1] if proc.stderr else "falhou"
            report["measured"].append(entry)
            continue
        entry.update(json.loads(proc.stdout))
        report["measured"].append(entry)

    print(json.dumps(report, indent=2, ensure_ascii=False))
    if args.json:
        with open(args.json, "w") as fh:
            json.dump(report, fh, indent=2, ensure_ascii=False)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
