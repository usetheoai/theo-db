"""Minimal OpenAI-compatible embeddings server backed by a REAL local model (fastembed/ONNX).

This is the *configurable model* that `theodb.embed()` points at. The TheoDB image stays lean (the DB
only ships `plpython3u` and calls an HTTP endpoint — the AlloyDB `embedding()` pattern); the model runs
out-of-process here. It is a real model (BAAI/bge-small-en-v1.5, 384-dim, deterministic), not a mock —
the same wire contract serves a self-hosted local model OR any cloud OpenAI-compatible provider.

Run: ``python tools/embedding_server.py --port 8088 [--model BAAI/bge-small-en-v1.5]``
Endpoint: ``POST /v1/embeddings``  body ``{"input": "text" | ["t1","t2"], "model": "..."}``
Response (OpenAI shape): ``{"object":"list","data":[{"object":"embedding","index":0,"embedding":[...]}], ...}``
Stdlib HTTP only (no framework dep); fastembed is the single heavy dep and lives OUTSIDE the DB image.
"""
from __future__ import annotations

import argparse
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def build_handler(model, model_name: str):
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args):  # silence default stderr access log
            pass

        def _json(self, code: int, payload: dict) -> None:
            body = json.dumps(payload).encode()
            self.send_response(code)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_GET(self):  # health check
            if self.path == "/health":
                self._json(200, {"status": "ok", "model": model_name})
            else:
                self._json(404, {"error": {"message": f"no route {self.path}"}})

        def do_POST(self):
            if self.path.rstrip("/") not in ("/v1/embeddings", "/embeddings"):
                self._json(404, {"error": {"message": f"no route {self.path}"}})
                return
            try:
                length = int(self.headers.get("Content-Length", 0))
                req = json.loads(self.rfile.read(length) or b"{}")
            except (ValueError, json.JSONDecodeError) as e:
                self._json(400, {"error": {"message": f"invalid JSON body: {e}"}})
                return
            inp = req.get("input")
            if inp is None:
                self._json(400, {"error": {"message": "missing 'input'"}})
                return
            texts = [inp] if isinstance(inp, str) else list(inp)
            vectors = [list(map(float, v)) for v in model.embed(texts)]
            data = [
                {"object": "embedding", "index": i, "embedding": vec}
                for i, vec in enumerate(vectors)
            ]
            self._json(
                200,
                {
                    "object": "list",
                    "data": data,
                    "model": req.get("model") or model_name,
                    "usage": {"prompt_tokens": 0, "total_tokens": 0},
                },
            )

    return Handler


def main(argv=None) -> int:
    p = argparse.ArgumentParser(prog="embedding_server")
    p.add_argument("--host", default="127.0.0.1")
    p.add_argument("--port", type=int, default=8088)
    p.add_argument("--model", default="BAAI/bge-small-en-v1.5")
    args = p.parse_args(argv)

    from fastembed import TextEmbedding  # heavy import: dev tool, never in the DB image

    model = TextEmbedding(args.model)
    server = ThreadingHTTPServer((args.host, args.port), build_handler(model, args.model))
    print(f"embedding_server: {args.model} on http://{args.host}:{args.port}/v1/embeddings", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
