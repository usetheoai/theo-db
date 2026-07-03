# M18 — `ai._chat` latency: Rust (pgrx) vs plpython3u

**Date:** 2026-06-30
**Milestone:** M18 (ROADMAP-v2 — own code in Rust; the generative ai.* surface)
**Purpose:** measurement-first evidence (ADR 0002, CTO requirement "DEVE TER DADOS EM BENCHMARK") that porting `ai._chat` from plpython3u to Rust (pgrx) does **not regress latency**.

> **Honest framing (ADR 0002 / `public-copy.md`):** `ai._chat` is **I/O-bound** — every call makes one synchronous HTTP round-trip to the chat endpoint, which dominates wall-clock. The rewrite is about **owning the code in Rust** (ROADMAP-v2), proven at **functional parity** (the 36-test `test_ai_sql.py` oracle), NOT a speed win. The numbers below document *no regression*; they are **not** a performance claim. Per-call latency is governed by the endpoint, not the function language.

## Method (reproducible)

- **Same container, same PostgreSQL, same endpoint, same stub** for both arms — the ONLY variable is the function language (Rust `ai._chat` vs plpython3u `ai._chat_py`, the latter recreated for the bench).
- **Endpoint:** the deterministic local stub `benchmarks/servers/chat_server.py`, reached via `host.docker.internal`.
- **Workload:** 100 serial calls/run, **5 runs**, 5 warmup discarded. Per-call latency = run wall-clock / n. Reported as mean ± std dev (population) over the runs.
- **Harness:** `benchmarks/bench_chat.py`.
- **Hardware:** 13th Gen Intel(R) Core(TM) i7-1355U. **PostgreSQL:** PostgreSQL 17.10 (Debian, pgdg). **Toolchain:** Rust 1.91, pgrx 0.16.1, minreq 2 (https-native/OpenSSL).

## Results

| Implementation | mean ± std (ms/call) |
|---|---|
| `ai._chat` (Rust/pgrx) | 1.447 ± 0.269 |
| `ai._chat_py` (plpython3u) | 1.995 ± 0.059 |

**Delta (Rust − plpython3u): -0.547 ms/call** — within I/O-bound noise; no regression (both arms are dominated by the same endpoint round-trip).
