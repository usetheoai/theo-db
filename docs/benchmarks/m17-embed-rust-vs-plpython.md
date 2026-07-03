# M17 — `theodb.embed` latency: Rust (pgrx) vs plpython3u

**Date:** 2026-06-29
**Milestone:** M17 (ROADMAP-v2 / ADR 0006 — own code in Rust)
**Purpose:** measurement-first evidence (ADR 0002, CTO requirement "DEVE TER DADOS EM BENCHMARK") that
rewriting `theodb.embed` from plpython3u to Rust (pgrx) does **not regress latency**.

> **Honest framing (ADR 0002 / `.claude/rules/public-copy.md`):** `theodb.embed` is **I/O-bound** — every
> call makes one synchronous HTTP round-trip to the embeddings endpoint, which dominates wall-clock. The
> rewrite is about **owning the code in Rust** (ROADMAP-v2), proven at **functional parity**, not about a
> speed win. The numbers below document *no regression*; they are **not** a performance claim. Per-call
> latency is governed by the endpoint, not the function's language.

## Method (reproducible)

- **Same container, same PostgreSQL, same endpoint, same model** for both implementations — the ONLY
  variable is the function language (Rust vs plpython3u). This isolates the language difference and removes
  image/network variance.
- Both functions installed in one `theo-db:m17` container:
  - `theodb.embed` — the Rust impl (theodb_rs extension; `theodb.embed` SQL wrapper → `theodb_rs._embed_text`, Rust/pgrx).
  - `theodb.embed_py` — the previous plpython3u impl (created from the pre-M17 `sql/30-theodb-embed.sql` body, renamed).
- **Endpoint:** the deterministic local stub `benchmarks/servers/embedding_server.py` (real model BAAI/bge-small-en-v1.5,
  384-dim, ONNX/fastembed), reached via `host.docker.internal`.
- **Workload:** 200 serial `SELECT <func>('benchmark text')` calls per run, **5 runs**, 5 warmup calls
  discarded. Per-call latency = run wall-clock / 200. Reported as mean ± std dev (population) over the 5 runs.
- **Harness:** `benchmarks/bench_embed.py` (`bench(conn, n=200, runs=5, func=...)`).
- **Hardware:** 13th Gen Intel Core i7-1355U. **PostgreSQL:** 17.10 (Debian, pgdg). **Toolchain:** Rust 1.91,
  pgrx 0.16.1, minreq 2.14.1 (`https-native`/OpenSSL TLS).

### Reproduce

```bash
# 1. build + run the image, with host access for the stub
docker build -t theo-db:m17 .
docker run -d --name theodb-m17-test --add-host=host.docker.internal:host-gateway \
  -e POSTGRES_PASSWORD=postgres -e POSTGRES_HOST_AUTH_METHOD=trust -p 55432:5432 theo-db:m17

# 2. start the deterministic embedding stub on the host
python3 benchmarks/servers/embedding_server.py --host 0.0.0.0 --port 8099 --model BAAI/bge-small-en-v1.5 &

# 3. create the plpython3u baseline (theodb.embed_py) in the container from the pre-M17 sql/30 body,
#    then run benchmarks/bench_embed.py against theodb.embed (Rust) and theodb.embed_py (plpython3u)
#    with theodb.embedding_endpoint = http://host.docker.internal:8099/v1/embeddings
```

## Results

| Implementation | Language | Mean (ms/call) | Std dev (ms) | Per-run samples (ms/call) |
|---|---|---|---|---|
| `theodb.embed` | **Rust (pgrx + minreq)** | **13.919** | 0.210 | 13.791, 14.166, 13.615, 13.880, 14.142 |
| `theodb.embed_py` | plpython3u + urllib | 15.660 | 0.234 | 15.366, 15.466, 15.610, 15.889, 15.969 |

- **N = 200 calls/run, 5 runs each.**
- Both are dominated by the ~14–16 ms HTTP round-trip to the stub (I/O-bound). The ~1.7 ms/call delta is
  small relative to the endpoint latency and is **not** advertised as a speedup.
- **Conclusion: no latency regression** from the plpython3u → Rust rewrite. The rewrite's value is owning
  the code in Rust (ROADMAP-v2 / ADR 0006), proven below at functional parity.

## Functional parity (the real gate)

Latency is secondary; **parity is the gate**. Proven two ways, both green:

1. **Byte-identical output** — in the same container, for the same input and endpoint:
   `theodb.embed('parity check')::text = theodb.embed_py('parity check')::text` → **true**. The Rust and
   plpython3u implementations return the *same* vector.
2. **The frozen Python e2e oracle** `benchmarks/tests/test_embed_sql.py` (10 tests) passes UNCHANGED against
   the Rust impl in the rebuilt image: 384-dim non-zero vector, semantic ordering (real model), determinism,
   and every typed error — unset endpoint / NULL content / non-http(s) scheme → SQLSTATE **22023**;
   unreachable / empty / malformed / non-JSON response → SQLSTATE **38000** with the same message needles as
   the baseline.

## Caveats

- The embedding **dimension follows the endpoint's model** (384 here because the stub is bge-small); the
  function does not hard-code 384 — the parity oracle pins 384 only because the stub does.
- Numbers are single-host, single-session, serial calls — they measure per-call latency, not throughput or
  concurrency. They are sufficient for the "no regression" claim, which is the only claim made.
