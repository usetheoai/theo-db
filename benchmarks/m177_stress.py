"""M177 — STRESS do servidor de embeddings: o que quebra primeiro, e como ele degrada.

Diferente de `m177_concurrency.py`, que mede carga curta ate a saturacao, este script empurra ALEM
do ponto de saturacao e procura o modo de falha. As perguntas sao outras:

  * o servidor degrada graciosamente ou COLAPSA?
  * qual e a taxa de ERRO sob sobrecarga (recusa de conexao, timeout, reset)?
  * a memoria cresce sob carga sustentada (vazamento)?
  * ele se RECUPERA depois do pico, ou fica degradado?

ERRO DE MEDICAO QUE ESTE SCRIPT EVITA (e que e a armadilha classica do stress test): medir apenas a
latencia das requisicoes BEM-SUCEDIDAS. Sob sobrecarga, se metade falha rapido, a latencia das
sobreviventes MELHORA — e o relatorio fica bonito enquanto o sistema quebra. Aqui todo pedido e
contabilizado: sucesso, erro HTTP, timeout e falha de conexao, com a taxa de erro reportada ao lado
da latencia. Uma latencia boa com 30% de erro e um sistema quebrado, nao um sistema rapido.

O `ThreadingHTTPServer` do stdlib cria UMA THREAD POR CONEXAO. Em alta concorrencia isso e um modo
de falha esperado — e e exatamente o tipo de coisa que so aparece empurrando.

Uso:
  python3 benchmarks/m177_stress.py --endpoint http://127.0.0.1:8098/v1/embeddings \
      --clients 8,32,64,128 --duration 20 --json out.json
"""
from __future__ import annotations

import argparse
import json
import os
import statistics
import threading
import time
import urllib.error
import urllib.request

TEXT = "consulta de stress sobre banco de dados vetorial e busca semantica em portugues"


def server_rss_mb(pid: int) -> float:
    try:
        with open(f"/proc/{pid}/status") as fh:
            for line in fh:
                if line.startswith("VmRSS:"):
                    return int(line.split()[1]) / 1024.0
    except OSError:
        pass
    return float("nan")


def hammer(endpoint: str, stop: threading.Event, out: list, timeout: float) -> None:
    """Bate no endpoint ate `stop`; registra (latencia_ms, classe) de TODO pedido."""
    body = json.dumps({"input": TEXT, "model": "BAAI/bge-small-en-v1.5"}).encode()
    while not stop.is_set():
        t = time.perf_counter()
        try:
            req = urllib.request.Request(
                endpoint, data=body, headers={"Content-Type": "application/json"}
            )
            with urllib.request.urlopen(req, timeout=timeout) as r:
                r.read()
            out.append(((time.perf_counter() - t) * 1000, "ok"))
        except urllib.error.HTTPError:
            out.append(((time.perf_counter() - t) * 1000, "http_error"))
        except (TimeoutError, urllib.error.URLError) as e:
            kind = "timeout" if isinstance(getattr(e, "reason", None), TimeoutError) else "conn_error"
            out.append(((time.perf_counter() - t) * 1000, kind))
        except OSError:
            out.append(((time.perf_counter() - t) * 1000, "conn_error"))


def level(endpoint: str, clients: int, duration: float, timeout: float, pid: int | None) -> dict:
    results: list = []
    stop = threading.Event()
    lock_free = [[] for _ in range(clients)]  # cada thread escreve na SUA lista (sem contencao)
    threads = [
        threading.Thread(target=hammer, args=(endpoint, stop, lock_free[i], timeout), daemon=True)
        for i in range(clients)
    ]
    rss0 = server_rss_mb(pid) if pid else float("nan")
    t0 = time.perf_counter()
    for th in threads:
        th.start()
    time.sleep(duration)
    stop.set()
    for th in threads:
        th.join(timeout=timeout + 5)
    wall = time.perf_counter() - t0
    rss1 = server_rss_mb(pid) if pid else float("nan")

    for chunk in lock_free:
        results.extend(chunk)
    lat_ok = sorted(x for x, k in results if k == "ok")
    total = len(results)
    errs = {k: sum(1 for _, kk in results if kk == k) for k in ("http_error", "timeout", "conn_error")}
    n_err = sum(errs.values())

    row = {
        "clients": clients,
        "wall_s": round(wall, 1),
        "total_requests": total,
        "ok": len(lat_ok),
        "errors": n_err,
        "error_rate_pct": round(100.0 * n_err / total, 2) if total else 0.0,
        "error_breakdown": errs,
        "throughput_ok_rps": round(len(lat_ok) / wall, 2) if wall else 0.0,
        "server_rss_before_mb": round(rss0, 1),
        "server_rss_after_mb": round(rss1, 1),
    }
    if lat_ok:
        n = len(lat_ok)
        row.update({
            "p50_ms": round(lat_ok[n // 2], 1),
            "p95_ms": round(lat_ok[int(n * 0.95)], 1),
            "p99_ms": round(lat_ok[min(int(n * 0.99), n - 1)], 1),
            "max_ms": round(lat_ok[-1], 1),
            "mean_ms": round(statistics.mean(lat_ok), 1),
        })
    return row


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--endpoint", default="http://127.0.0.1:8098/v1/embeddings")
    ap.add_argument("--clients", default="8,32,64,128")
    ap.add_argument("--duration", type=float, default=20.0)
    ap.add_argument("--timeout", type=float, default=30.0)
    ap.add_argument("--pid", type=int, default=None, help="pid do servidor, para medir RSS")
    ap.add_argument("--recover", action="store_true", help="mede a recuperacao pos-pico a 1 cliente")
    ap.add_argument("--json", default=None)
    a = ap.parse_args()

    rows = []
    for c in [int(x) for x in a.clients.split(",")]:
        print(f"→ {c} clientes por {a.duration}s", flush=True)
        rows.append(level(a.endpoint, c, a.duration, a.timeout, a.pid))
        time.sleep(3)  # deixa o servidor drenar a fila entre niveis

    report = {"levels": rows}
    if a.recover:
        print("→ recuperacao pos-pico (1 cliente)", flush=True)
        time.sleep(5)
        report["recovery"] = level(a.endpoint, 1, 10.0, a.timeout, a.pid)

    print(json.dumps(report, indent=2))
    if a.json:
        json.dump(report, open(a.json, "w"), indent=2)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
