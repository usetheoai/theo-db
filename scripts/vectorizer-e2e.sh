#!/usr/bin/env bash
# M54 e2e: prove the declarative vectorizer end-to-end WITH the background worker actually running
# (shared_preload_libraries=theodb_rs). Installs the CURRENT theodb_rs into a fresh pgdata, starts a
# deterministic stub embedding endpoint, and asserts: INSERT → embedding appears; UPDATE → re-embed;
# endpoint failure → bounded retry → typed `failed` state (never swallowed). Runs inside the builder image
# (toolchain + pg18 + vector + theodb umbrella). Mirrors scripts/pgrx-test-in-builder.sh.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/.." && pwd)"

docker run --rm \
  -v "$REPO/theodb_rs":/build \
  -v /tmp/m51-target:/build/target \
  -w /build theodb-builder:m51-test \
  bash -lc '
    set -e
    id builder >/dev/null 2>&1 || useradd -m -u 1001 builder
    chmod a+rx /root 2>/dev/null || true
    chmod -R a+rX /root/.cargo /root/.rustup 2>/dev/null || true
    mkdir -p /home/builder/.pgrx && cp -rf /root/.pgrx/* /home/builder/.pgrx/ 2>/dev/null || true
    chown -R builder /home/builder /build/target 2>/dev/null || true
    PKGLIB=$(/usr/bin/pg_config --pkglibdir); EXT=$(/usr/bin/pg_config --sharedir)/extension
    chmod -R a+w "$PKGLIB" "$EXT" 2>/dev/null || true
    BINDIR=$(/usr/bin/pg_config --bindir)

    # 1. Install the CURRENT theodb_rs into the system pg18 (overwrites the image-baked one).
    su builder -c "export PATH=/root/.cargo/bin:\$PATH PGRX_HOME=/home/builder/.pgrx CARGO_HOME=/root/.cargo; cd /build && cargo pgrx install --pg-config /usr/bin/pg_config --release 2>&1 | tail -3"

    # 2. Fresh pgdata with the worker preloaded.
    PGDATA=/tmp/e2e-pgdata; rm -rf "$PGDATA"; mkdir -p "$PGDATA"; chown builder "$PGDATA"
    su builder -c "$BINDIR/initdb -D $PGDATA -U postgres --no-sync -A trust >/dev/null"
    echo "shared_preload_libraries = '"'"'theodb_rs'"'"'" >> "$PGDATA/postgresql.conf"
    echo "port = 5599" >> "$PGDATA/postgresql.conf"
    su builder -c "$BINDIR/pg_ctl -D $PGDATA -l /tmp/e2e-pg.log -w start"

    # 3. Deterministic stub embedding endpoint (OpenAI shape). Returns a fixed dim-3 vector; HTTP 400 for
    #    any input containing BOOM (non-recoverable → the worker marks the job failed after attempts).
    cat > /tmp/stub.py <<'"'"'PY'"'"'
import json
from http.server import BaseHTTPRequestHandler, HTTPServer
class H(BaseHTTPRequestHandler):
    def do_POST(self):
        n = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(n).decode("utf-8", "replace")
        if "BOOM" in body:
            self.send_response(400); self.end_headers(); self.wfile.write(b"{}"); return
        out = json.dumps({"data":[{"embedding":[0.1,0.2,0.3],"index":0}]}).encode()
        self.send_response(200); self.send_header("Content-Type","application/json")
        self.send_header("Content-Length",str(len(out))); self.end_headers(); self.wfile.write(out)
    def log_message(self, *a): pass
HTTPServer(("127.0.0.1",9099), H).serve_forever()
PY
    python3 /tmp/stub.py & STUB=$!
    sleep 1

    export PGPASSWORD=postgres
    Q() { $BINDIR/psql -h 127.0.0.1 -p 5599 -U postgres -tAc "$1"; }

    Q "CREATE EXTENSION IF NOT EXISTS vector CASCADE"
    Q "CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE"
    Q "ALTER SYSTEM SET theodb.embedding_endpoint = '"'"'http://127.0.0.1:9099/v1/embeddings'"'"'"
    Q "SELECT pg_reload_conf()" >/dev/null

    Q "CREATE TABLE docs (id int PRIMARY KEY, body text, emb vector(3))"
    Q "SELECT theodb.create_vectorizer('"'"'docs'"'"'::regclass, '"'"'id'"'"', '"'"'body'"'"', '"'"'docs'"'"', '"'"'emb'"'"', NULL, 3)" >/dev/null

    echo "=== INSERT → embedding appears ==="
    Q "INSERT INTO docs (id, body) VALUES (1, '"'"'hello world'"'"')"
    for i in $(seq 1 20); do E=$(Q "SELECT emb::text FROM docs WHERE id=1"); [ -n "$E" ] && break; sleep 1; done
    echo "emb(id=1) after INSERT: ${E:-<empty>}"
    if [ -z "$E" ]; then
        echo "--- DIAG: queue state ---"; Q "SELECT job_id, state, attempts, owner, last_error FROM theodb.vectorizer_queue"
        echo "--- DIAG: pg log (worker/theodb/error) ---"; grep -iE "worker|theodb|error|fatal|background" /tmp/e2e-pg.log | tail -25 || true
        echo "FAIL: embedding never appeared"; exit 1
    fi

    echo "=== UPDATE → re-embed (job enqueued) ==="
    Q "UPDATE docs SET emb=NULL WHERE id=1"   # clear so we can see the re-embed repopulate
    Q "UPDATE docs SET body='"'"'changed'"'"' WHERE id=1"
    for i in $(seq 1 20); do E2=$(Q "SELECT emb::text FROM docs WHERE id=1"); [ -n "$E2" ] && break; sleep 1; done
    echo "emb(id=1) after UPDATE: ${E2:-<empty>}"
    [ -n "$E2" ] || { echo "FAIL: re-embed never repopulated"; exit 1; }

    echo "=== endpoint failure → bounded retry → typed failed state ==="
    Q "INSERT INTO docs (id, body) VALUES (2, '"'"'BOOM please fail'"'"')"
    for i in $(seq 1 30); do S=$(Q "SELECT state FROM theodb.vectorizer_queue WHERE source_pk='"'"'2'"'"'"); [ "$S" = "failed" ] && break; sleep 1; done
    echo "job(id=2) final state: ${S:-<gone>}"
    LE=$(Q "SELECT last_error FROM theodb.vectorizer_queue WHERE source_pk='"'"'2'"'"'")
    echo "job(id=2) last_error: ${LE:-<none>}"
    EMB2=$(Q "SELECT emb IS NULL FROM docs WHERE id=2")
    [ "$S" = "failed" ] || { echo "FAIL: failing job did not reach failed state (got: ${S:-gone})"; exit 1; }
    [ "$EMB2" = "t" ] || { echo "FAIL: failing job wrote a bogus embedding"; exit 1; }

    echo "=== metric queryable ==="
    Q "SELECT '"'"'stats: processed=%s failed=%s pending=%s processing=%s failed_jobs=%s'"'"' AS s, processed, failed, pending, processing, failed_jobs FROM theodb.vectorizer_stats()"

    echo "E2E_OK"
    kill $STUB 2>/dev/null || true
    su builder -c "$BINDIR/pg_ctl -D $PGDATA stop -m immediate" >/dev/null 2>&1 || true
  '
