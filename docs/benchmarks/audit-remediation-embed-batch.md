# Audit-remediation — `theodb.embed_batch` N→1 latency benchmark

**Date:** 2026-06-29
**Slice:** audit-remediation (system-design audit, CRITICAL embed N+1 — finding #1)
**Purpose:** measurement-first evidence (ADR 0002, CTO requirement "DEVE TER DADOS EM BENCHMARK") that `theodb.embed_batch` collapses N synchronous HTTP round-trips to ONE — a genuine structural win.

> **Honest framing (ADR 0002 / `.claude/rules/public-copy.md`):** unlike the M17 per-row benchmark (an I/O-bound *no-regression* check), this measures a real N→1 collapse. Per-row embedding issues **N** HTTP round-trips to the endpoint; `embed_batch` issues **1**. The speedup below is therefore dominated by the saved round-trips and grows with N. Numbers are from the deterministic local stub (no network variance); against a remote endpoint with real latency the absolute win is larger.

## Method (reproducible)

- **Same container, same PostgreSQL, same endpoint, same model** for both arms — the ONLY variable is the round-trip count (N per-row vs 1 batch).
- Both arms run as a SINGLE SQL statement so client/parse overhead is identical:
  - per-row: `SELECT theodb.embed(v) FROM unnest($1::text[]) AS v` (N round-trips)
  - batch:   `SELECT theodb.embed_batch($1::text[])` (1 round-trip)
- **Endpoint:** the deterministic local stub `tools/embedding_server.py` (BAAI/bge-small-en-v1.5, 384-dim, ONNX/fastembed), reached via `host.docker.internal`.
- **Workload:** distinct inputs per N; **5 runs** per arm, 2 warmup discarded. Reported as mean ± std dev (population) of the per-statement wall-clock.
- **Harness:** `benchmarks/bench_embed_batch.py` (`bench_n1(conn, sizes, runs)`).
- **Hardware:** 13th Gen Intel(R) Core(TM) i7-1355U. **PostgreSQL:** PostgreSQL 17.10 (Debian, pgdg). **Toolchain:** Rust 1.91, pgrx 0.16.1, minreq 2 (https-native/OpenSSL).

### Reproduce

```bash
docker build -t theo-db:audit-rem .
docker run -d --name theodb-audit-rem --add-host=host.docker.internal:host-gateway \
  -e POSTGRES_PASSWORD=postgres -e POSTGRES_HOST_AUTH_METHOD=trust -p 55432:5432 theo-db:audit-rem
python3 tools/embedding_server.py --host 0.0.0.0 --port 8099 --model BAAI/bge-small-en-v1.5 &
PGHOST=localhost PGPORT=55432 python3 benchmarks/bench_embed_batch.py \
  --endpoint http://host.docker.internal:8099/v1/embeddings \
  --report docs/benchmarks/audit-remediation-embed-batch.md
```

## Results

| N (batch size) | per-row (N round-trips) mean ± std | batch (1 round-trip) mean ± std | speedup |
|---|---|---|---|
| 8 | 123.69 ± 6.29 ms | 42.24 ± 16.87 ms | **2.93×** |
| 32 | 533.79 ± 77.17 ms | 71.27 ± 2.61 ms | **7.49×** |
| 128 | 2016.48 ± 69.94 ms | 252.37 ± 12.05 ms | **7.99×** |

## Interpretation

The batch path is materially faster and the gap widens with N — consistent with collapsing N round-trips into 1. This is the delivered mitigation for ADR 0007's recorded N+1 consequence (bulk embedding is the most common case). The speedup is bounded below by N on a network-latency-dominated endpoint; on the zero-latency local stub it reflects per-request overhead saved.
