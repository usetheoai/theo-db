#!/usr/bin/env python3
"""Mock embedding endpoint for the M122 xmin-not-pinned benchmark. Sleeps SLEEP_SECS then returns a valid
OpenAI-shaped `{"data":[{"embedding":[...],"index":i}, ...]}` for each input. The sleep simulates a slow/hung
endpoint — M122's point is that the in-place worker must NOT hold a txn (pin xmin) during this sleep."""
import json, os, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

SLEEP_SECS = float(os.environ.get("SLEEP_SECS", "12"))
DIM = int(os.environ.get("DIM", "8"))
PORT = int(os.environ.get("PORT", "9199"))


class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass

    def do_GET(self):  # health-check
        self.send_response(200); self.send_header("Content-Length", "2"); self.end_headers(); self.wfile.write(b"ok")

    def do_POST(self):
        n = 1
        try:
            length = int(self.headers.get("Content-Length", "0"))
            req = json.loads(self.rfile.read(length) or b"{}") if length else {}
            inp = req.get("input", [])
            n = len(inp) if isinstance(inp, list) and inp else 1
            with open("/tmp/mock_hits.log", "a") as f:
                f.write(f"{time.time():.2f} POST n={n} sleeping {SLEEP_SECS}s\n")
            time.sleep(SLEEP_SECS)
            vec = [round(0.01 * (i + 1), 4) for i in range(DIM)]
            resp = {"object": "list", "model": "mock",
                    "data": [{"object": "embedding", "index": i, "embedding": vec} for i in range(n)]}
            payload = json.dumps(resp).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
        except Exception as e:  # never die — an error would make later requests get "connection refused"
            try:
                with open("/tmp/mock_hits.log", "a") as f:
                    f.write(f"{time.time():.2f} ERROR {e!r}\n")
                self.send_response(500); self.send_header("Content-Length", "0"); self.end_headers()
            except Exception:
                pass


if __name__ == "__main__":
    print(f"mock_slow_embed sleep={SLEEP_SECS}s dim={DIM} :{PORT}", flush=True)
    ThreadingHTTPServer(("127.0.0.1", PORT), H).serve_forever()
