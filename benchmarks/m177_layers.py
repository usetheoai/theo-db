"""M177 — decompoe o custo POR CAMADA: inferencia pura, Python/servidor, TCP, e Unix domain socket.

O flamegraph mostrou 98,6% do tempo de requisicao em `InferenceSession.run` — ou seja, no ONNX
Runtime, que ja e C++ nativo. Este script mede o que sobra e responde duas perguntas concretas:

  1. Quanto custa o servidor ser em PYTHON?    -> camada B menos camada A
  2. Quanto custa o transporte ser HTTP/TCP?   -> camada C menos camada B, e camada D vs C

Camadas medidas, todas com o MESMO modelo e os MESMOS textos:

  A  in-process        fastembed direto, sem servidor, sem socket     (so ONNX)
  C  HTTP sobre TCP    o desenho de hoje                              (ONNX + Python + TCP + HTTP)
  D  HTTP sobre UDS    Unix domain socket em vez de TCP loopback      (elimina a pilha TCP)

A camada B (servidor Python sem rede) nao e isolavel sem reimplementar o servidor; o que a
substitui e a decomposicao ja medida em `m177_hop_decompose.py`, que mediu o canal SEM modelo.

UDS e a tecnica classica para eliminar a pilha TCP mantendo o mesmo protocolo HTTP: mesmo processo
de servidor, mesmo parser, sem three-way handshake, sem checksum, sem roteamento. E o degrau 3 da
escada de parcimonia (recurso nativo da plataforma) antes de considerar reescrever qualquer coisa.

Uso: python3 benchmarks/m177_layers.py --runs 60 --json out.json
"""
from __future__ import annotations

import argparse
import http.client
import json
import os
import socket
import statistics
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from socketserver import ThreadingUnixStreamServer

MODEL = "BAAI/bge-small-en-v1.5"
TEXTS = ["consulta sobre banco de dados vetorial e busca semantica"]


def build_handler(model):
    class H(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"
        disable_nagle_algorithm = True

        def log_message(self, *a):
            pass

        def do_POST(self):
            n = int(self.headers.get("Content-Length", 0))
            body = json.loads(self.rfile.read(n))
            inp = body["input"]
            inp = [inp] if isinstance(inp, str) else inp
            vecs = [list(map(float, v)) for v in model.embed(inp)]
            out = json.dumps({"data": [{"embedding": v} for v in vecs]}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(out)))
            self.end_headers()
            self.wfile.write(out)

    return H


class UnixHTTPServer(ThreadingUnixStreamServer):
    """ThreadingHTTPServer sobre AF_UNIX. BaseHTTPRequestHandler nao se importa com a familia
    do socket — ele fala com um file object —, entao a troca e literalmente uma linha de servidor."""
    daemon_threads = True

    def get_request(self):
        req, _ = super().get_request()
        return req, ("localhost", 0)  # BaseHTTPRequestHandler espera um par (host, porta)


class UnixConnection(http.client.HTTPConnection):
    def __init__(self, path):
        super().__init__("localhost")
        self._path = path

    def connect(self):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect(self._path)


def timed(fn, runs: int, warmup: int = 10) -> dict:
    for _ in range(warmup):
        fn()
    s = [fn() for _ in range(runs)]
    s.sort()
    return {
        "ms_mean": round(statistics.mean(s), 3),
        "ms_stdev": round(statistics.stdev(s), 3),
        "ms_p50": round(s[len(s) // 2], 3),
        "ms_p99": round(s[min(int(len(s) * 0.99), len(s) - 1)], 3),
        "n": len(s),
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", type=int, default=60)
    ap.add_argument("--json", default=None)
    a = ap.parse_args()

    from fastembed import TextEmbedding

    model = TextEmbedding(model_name=MODEL)
    list(model.embed(TEXTS))  # warm-up do grafo

    report = {"model": MODEL, "layers": {}}

    # ---- A: inferencia pura, sem servidor nem socket
    def layer_a():
        t = time.perf_counter()
        list(model.embed(TEXTS))
        return (time.perf_counter() - t) * 1000

    report["layers"]["A_inproc"] = timed(layer_a, a.runs)

    handler = build_handler(model)
    payload = json.dumps({"input": TEXTS[0], "model": MODEL}).encode()
    hdrs = {"Content-Type": "application/json"}

    # ---- C: HTTP sobre TCP loopback (o desenho de hoje), conexao reutilizada
    tcp = ThreadingHTTPServer(("127.0.0.1", 8096), handler)
    threading.Thread(target=tcp.serve_forever, daemon=True).start()
    time.sleep(0.3)
    ctcp = http.client.HTTPConnection("127.0.0.1", 8096, timeout=30)
    ctcp.connect()
    ctcp.sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)

    def layer_c():
        t = time.perf_counter()
        ctcp.request("POST", "/v1/embeddings", payload, hdrs)
        ctcp.getresponse().read()
        return (time.perf_counter() - t) * 1000

    report["layers"]["C_http_tcp"] = timed(layer_c, a.runs)
    ctcp.close()
    tcp.shutdown()

    # ---- D: HTTP sobre Unix domain socket
    # `disable_nagle_algorithm` seta TCP_NODELAY, que NAO existe em AF_UNIX (Errno 95). O handler
    # precisa da flag desligada aqui — e Nagle nao se aplica a socket Unix de qualquer forma.
    class UdsHandler(handler):  # type: ignore[misc,valid-type]
        disable_nagle_algorithm = False

    sockpath = os.path.join(tempfile.mkdtemp(), "embed.sock")
    uds = UnixHTTPServer(sockpath, UdsHandler)
    threading.Thread(target=uds.serve_forever, daemon=True).start()
    time.sleep(0.3)
    cuds = UnixConnection(sockpath)
    cuds.connect()

    def layer_d():
        t = time.perf_counter()
        cuds.request("POST", "/v1/embeddings", payload, hdrs)
        cuds.getresponse().read()
        return (time.perf_counter() - t) * 1000

    report["layers"]["D_http_uds"] = timed(layer_d, a.runs)
    cuds.close()
    uds.shutdown()

    A = report["layers"]["A_inproc"]["ms_p50"]
    C = report["layers"]["C_http_tcp"]["ms_p50"]
    D = report["layers"]["D_http_uds"]["ms_p50"]
    report["deltas_p50_ms"] = {
        "servidor_python_mais_http_tcp": round(C - A, 3),
        "servidor_python_mais_http_uds": round(D - A, 3),
        "ganho_do_uds_sobre_tcp": round(C - D, 3),
        "overhead_tcp_pct_do_total": round(100 * (C - A) / C, 2),
        "overhead_uds_pct_do_total": round(100 * (D - A) / D, 2),
    }
    print(json.dumps(report, indent=2))
    if a.json:
        json.dump(report, open(a.json, "w"), indent=2)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
