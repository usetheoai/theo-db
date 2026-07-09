"""Minimal cross-encoder rerank server backed by a REAL local model (sentence-transformers CrossEncoder).

This is the *configurable reranker* that `ai.rerank()` (M65) points at. The TheoDB image stays lean (the
DB's Rust extension `theodb_rs` just calls an HTTP endpoint — the same AlloyDB `ai.rank`/`ai.embed` pattern);
the cross-encoder runs out-of-process here. It is a real model (BAAI/bge-reranker-base by default, Apache 2.0,
deterministic), not a mock — the same wire contract serves a self-hosted model OR a Cohere/Jina/Voyage/TEI
endpoint.

Run: ``python benchmarks/servers/rerank_server.py --port 8090 [--model BAAI/bge-reranker-base]``
Endpoint: ``POST /rerank``  body ``{"query": "q", "documents": ["d1","d2"], "model": "..."}``
Response (cross-encoder shape): ``{"results":[{"index":0,"relevance_score":0.9},...]}``  (Cohere/Jina/TEI/vLLM)
Stdlib HTTP only (no framework dep); sentence-transformers is the single heavy dep and lives OUTSIDE the DB image.
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
            if self.path.rstrip("/") not in ("/rerank", "/v1/rerank"):
                self._json(404, {"error": {"message": f"no route {self.path}"}})
                return
            try:
                length = int(self.headers.get("Content-Length", 0))
                req = json.loads(self.rfile.read(length) or b"{}")
            except (ValueError, json.JSONDecodeError) as e:
                self._json(400, {"error": {"message": f"invalid JSON body: {e}"}})
                return
            query = req.get("query")
            documents = req.get("documents")
            if query is None or documents is None:
                self._json(400, {"error": {"message": "missing 'query' or 'documents'"}})
                return
            # Cross-encoder: score each (query, doc) pair. Returns one relevance score per document,
            # index-aligned to the input order (the theodb_rs parser maps back by `index`).
            pairs = [(query, d) for d in documents]
            scores = [float(s) for s in model.predict(pairs)] if pairs else []
            results = [
                {"index": i, "relevance_score": s} for i, s in enumerate(scores)
            ]
            self._json(200, {"results": results, "model": req.get("model") or model_name})

    return Handler


def main(argv=None) -> int:
    p = argparse.ArgumentParser(prog="rerank_server")
    p.add_argument("--host", default="127.0.0.1")
    p.add_argument("--port", type=int, default=8090)
    p.add_argument("--model", default="BAAI/bge-reranker-base")
    args = p.parse_args(argv)

    from sentence_transformers import CrossEncoder  # heavy import: dev tool, never in the DB image

    model = CrossEncoder(args.model)
    server = ThreadingHTTPServer((args.host, args.port), build_handler(model, args.model))
    print(f"rerank_server: {args.model} on http://{args.host}:{args.port}/rerank", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
