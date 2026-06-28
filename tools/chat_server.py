"""Minimal OpenAI-compatible chat-completions STUB for deterministic offline contract tests.

This is the configurable endpoint that the `ai.*` generative functions point at in CI — a rule-based
stub (NOT a model) so each function's SQL->HTTP->parse contract is tested deterministically and offline
(no external API, no cost, no flakiness). The same wire contract serves a real cloud provider in
production; the real end-to-end is exercised by an opt-in test against OpenAI (key from gitignored .env).

Run: ``python tools/chat_server.py --port 8099``
Endpoint: ``POST /v1/chat/completions``  body ``{"model": "...", "messages": [{"role","content"}]}``
Response (OpenAI shape): ``{"choices":[{"message":{"role":"assistant","content":"..."}}], ...}``

The stub shapes its reply from the system + user message so every function's parser succeeds:
  - sentiment system prompt  -> 'positive' (or 'negative' if the user text contains 'bad'/'terrible')
  - yes/no system prompt     -> 'yes'      (or 'no' if the user text contains 'not'/'no')
  - number system prompt     -> '0.8'
  - otherwise (generate/summarize) -> a fixed canned sentence echoing the prompt
A user prompt containing the token ``__MALFORMED__`` makes the stub return prose, so the fail-fast
typed-error path (plan D4) is testable. Stdlib HTTP only — zero dependencies.
"""
from __future__ import annotations

import argparse
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

_CANNED = "TheoDB is a PostgreSQL-compatible database with generative-AI SQL functions."


def _decide(system: str, user: str) -> str:
    sys_l = (system or "").lower()
    usr_l = (user or "").lower()
    # Test seams (magic tokens) to exercise the production error/parse branches deterministically:
    if "__empty__" in usr_l:
        return ""  # -> ai._chat empty-completion guard (38000)
    if "__malformed__" in usr_l:
        return "Well, it depends on how you look at it, there is no single answer here."
    if "__neutral__" in usr_l:
        return "neutral"  # sentiment neutral-label path
    if "__no__" in usr_l:
        return "no"  # ai.if false branch, decoupled from English-'not' heuristic
    # M7-S4 NL→SQL injection seams: the stub "complies" with the injection on demand, so the tests prove the
    # GUARDS (L2 static validation / L3 read-only sandbox) catch it — not that the LLM refused.
    if "__nlinject_drop__" in usr_l:
        return "DROP TABLE secret"
    if "__nlinject_write__" in usr_l:
        return "UPDATE documents SET content = 'pwned'"
    if "__nlinject_multi__" in usr_l:
        return "SELECT 1; DROP TABLE secret"
    if "__nlinject_exfil__" in usr_l:
        return "SELECT pg_read_file('/etc/passwd')"
    if "__nlinject_relation__" in usr_l:
        return "SELECT * FROM pg_authid"  # a non-allowlisted relation
    if "__nlinject_commajoin__" in usr_l:
        return "SELECT documents.content, secret.k FROM documents, secret"  # comma-join exfil (L4 bypass attempt)
    if "__nlinject_quoted__" in usr_l:
        return 'SELECT k FROM "secret"'  # quoted-identifier exfil (regex-L4 bypass attempt)
    if "__nlinject_statfile__" in usr_l:
        return "SELECT pg_stat_file('/etc/passwd')"  # no-FROM exfil function (denylist must catch)
    if "__nlinject_funcwrite__" in usr_l:
        return "SELECT nextval('s')"  # passes L2 keyword denylist; a real write -> L3 must block (25006)
    if "__nlcte__" in usr_l:
        return "WITH c AS (SELECT content FROM documents) SELECT count(*) AS n FROM c"  # benign CTE
    if "__nllimit__" in usr_l:
        return "SELECT doc_id FROM documents LIMIT 5"  # already has LIMIT (outer-LIMIT wrap must not error)
    if "read-only postgresql select" in sys_l:  # ai.nl_to_sql / ai.nl_query (L1 prompt)
        # benign: a count question -> a safe SELECT over the (allowlisted) documents table
        return "SELECT count(*) AS n FROM documents"
    if "positive, negative, neutral" in sys_l:  # ai.analyze_sentiment
        if any(w in usr_l for w in ("bad", "terrible", "boring", "hate", "awful")):
            return "negative"
        return "positive"
    if "yes or no" in sys_l:  # ai.if
        return "no" if any(w in usr_l for w in (" not ", "no ", "isn't", "cannot")) else "yes"
    if "single number" in sys_l:  # ai.rank
        return "0.8"
    if "summarize" in sys_l:  # ai.summarize
        return "A concise summary: " + _CANNED
    return _CANNED  # ai.generate (raw)


def build_handler(model_name: str):
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args):  # silence default access log
            pass

        def _json(self, code: int, payload: dict) -> None:
            body = json.dumps(payload).encode()
            self.send_response(code)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_GET(self):  # health
            if self.path == "/health":
                self._json(200, {"status": "ok", "model": model_name})
            else:
                self._json(404, {"error": {"message": f"no route {self.path}"}})

        def do_POST(self):
            if self.path.rstrip("/") not in ("/v1/chat/completions", "/chat/completions"):
                self._json(404, {"error": {"message": f"no route {self.path}"}})
                return
            try:
                length = int(self.headers.get("Content-Length", 0))
                req = json.loads(self.rfile.read(length) or b"{}")
            except (ValueError, json.JSONDecodeError) as e:
                self._json(400, {"error": {"message": f"invalid JSON body: {e}"}})
                return
            messages = req.get("messages") or []
            system = next((m.get("content", "") for m in messages if m.get("role") == "system"), "")
            user = next((m.get("content", "") for m in messages if m.get("role") == "user"), "")
            if "__badshape__" in (user or "").lower():
                # 200 with a body missing choices[0].message -> ai._chat response-shape guard (38000)
                self._json(200, {"object": "chat.completion", "choices": []})
                return
            content = _decide(system, user)
            self._json(
                200,
                {
                    "id": "stub-chatcmpl",
                    "object": "chat.completion",
                    "model": req.get("model") or model_name,
                    "choices": [
                        {"index": 0, "message": {"role": "assistant", "content": content}, "finish_reason": "stop"}
                    ],
                    "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0},
                },
            )

    return Handler


def main(argv=None) -> int:
    p = argparse.ArgumentParser(prog="chat_server")
    p.add_argument("--host", default="127.0.0.1")
    p.add_argument("--port", type=int, default=8099)
    p.add_argument("--model", default="stub-chat")
    args = p.parse_args(argv)

    server = ThreadingHTTPServer((args.host, args.port), build_handler(args.model))
    print(f"chat_server (stub): http://{args.host}:{args.port}/v1/chat/completions", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
