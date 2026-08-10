"""M177 — o caminho de embed sob CONCORRENCIA. O gargalo que o ADR 0007 registra e nunca foi medido.

O ADR 0007 declara que cada `theodb.embed()` segura um backend PostgreSQL inteiro pela latencia do
modelo, e que "maquina de fila e complexidade essencial apenas depois de um gargalo medido". Este
script mede esse gargalo.

Pergunta: um servidor de embeddings atende quantos clientes concorrentes antes de saturar, e o que
acontece com a latencia de cauda (p95/p99) — que e o que o usuario sente, nao a media.

Rigor: warm-up descartado; N requisicoes por nivel de concorrencia; percentis reais (nao media);
throughput medido como total/wall-clock, nao como 1/latencia-media (que superestima sob fila).
"""
from __future__ import annotations
import argparse, json, statistics, time
from concurrent.futures import ThreadPoolExecutor
import urllib.request

def one(endpoint: str, text: str) -> float:
    body = json.dumps({"input": text, "model": "BAAI/bge-small-en-v1.5"}).encode()
    req = urllib.request.Request(endpoint, data=body, headers={"Content-Type": "application/json"})
    t = time.perf_counter()
    with urllib.request.urlopen(req, timeout=120) as r:
        json.loads(r.read())
    return (time.perf_counter() - t) * 1000

def level(endpoint: str, clients: int, per_client: int) -> dict:
    texts = [f"consulta numero {i} sobre banco de dados vetorial" for i in range(per_client)]
    def worker(_):
        return [one(endpoint, t) for t in texts]
    t0 = time.perf_counter()
    with ThreadPoolExecutor(max_workers=clients) as ex:
        lat = [x for chunk in ex.map(worker, range(clients)) for x in chunk]
    wall = time.perf_counter() - t0
    lat.sort()
    n = len(lat)
    return {
        "clients": clients, "requests": n,
        "wall_s": round(wall, 2),
        "throughput_rps": round(n / wall, 2),
        "p50_ms": round(lat[n // 2], 1),
        "p95_ms": round(lat[int(n * 0.95)], 1),
        "p99_ms": round(lat[min(int(n * 0.99), n - 1)], 1),
        "mean_ms": round(statistics.mean(lat), 1),
        "max_ms": round(lat[-1], 1),
    }

def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--endpoint", default="http://127.0.0.1:8093/v1/embeddings")
    ap.add_argument("--clients", default="1,2,4,8,16")
    ap.add_argument("--per-client", type=int, default=10)
    ap.add_argument("--json", default=None)
    a = ap.parse_args()
    for _ in range(5):
        one(a.endpoint, "warm up")
    rows = [level(a.endpoint, c, a.per_client) for c in [int(x) for x in a.clients.split(",")]]
    print(json.dumps({"levels": rows}, indent=2))
    if a.json:
        json.dump({"levels": rows}, open(a.json, "w"), indent=2)
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
