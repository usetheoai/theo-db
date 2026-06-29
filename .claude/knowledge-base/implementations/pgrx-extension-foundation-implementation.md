# Implementation summary — pgrx-extension-foundation (M17)

**Plan:** `.claude/knowledge-base/plans/pgrx-extension-foundation-plan.md` (v1.3, SHIPPABLE_WITH_CAVEATS 84)
**Milestone:** M17 (ROADMAP-v2 / ADR 0006 — own code in Rust)
**Commits:** `7f41448` (discover), `71db26a` (plan), `386d073` (implementation)
**Date:** 2026-06-29
**Status:** IMPLEMENTATION_COMPLETE — parity proven, benchmark recorded.

## What shipped

TheoDB's first **own PostgreSQL extension in Rust** (`theodb_rs`, pgrx 0.16.1), and `theodb.embed`
rewritten from plpython3u to Rust **at proven functional parity** + a reproducible latency benchmark.

| Task | Status | Evidence |
|---|---|---|
| T0.1 crate skeleton (pgrx 0.16.1, pg17) | ✅ | `theodb_rs/` compiles; `cargo pgrx install` generates `theodb_rs.so`/`.control`/`--1.0.0.sql` |
| T1.1 embed in Rust (minreq + SSRF + typed errors) | ✅ | `theodb_rs/src/lib.rs`; minreq `https-native` (OpenSSL); SSRF `with_max_redirects(0)`; 22023/38000 parity |
| T2.1 remove from sql/30 + Docker wiring + coexistence | ✅ | `sql/30` only ensures schema; `theodb-rs-builder` stage; both extensions install on fresh DB |
| T3.1 parity (Python oracle) | ✅ | `benchmarks/tests/test_embed_sql.py` **10/10 green** vs the Rust impl in `theo-db:m17` |
| T4.1 benchmark Rust vs plpython3u | ✅ | `docs/benchmarks/m17-embed-rust-vs-plpython.md` — Rust 13.92ms vs py 15.66ms/call; no regression; no claim |
| T5.1 deps-audit + Cargo.lock | ✅ | `cargo audit` 0 CVE on committed `Cargo.lock`; minreq ISC, serde_json MIT/Apache |

## Wiring triad

1. **Caller:** the public SQL wrapper `theodb.embed(text,text)` (extension_sql) calls `theodb_rs._embed_text`; `theodb.embed` is in turn called by `theodb.hybrid_search` (sql/40) and by users — exercised end-to-end by the oracle.
2. **Integration test:** `benchmarks/tests/test_embed_sql.py` (10 tests) runs the real Rust function in the real image against a real model endpoint (host stub). Plus `benchmarks/tests/test_bench_embed.py` (5 tests) for the harness.
3. **Runtime observability:** typed SQLSTATEs (22023/38000) with contextual messages surface every failure path in PG logs (fail-loud — Rule 8); the benchmark harness measures per-call latency.

## Parity — the gate (proven two ways)

1. **Byte-identical:** `theodb.embed('parity check')::text = theodb.embed_py('parity check')::text` → **true** (Rust vs plpython3u, same container/endpoint).
2. **Frozen oracle:** `test_embed_sql.py` 10/10 green UNCHANGED — 384-dim, semantic, deterministic; typed errors 22023 (unset/NULL/scheme) + 38000 (unreachable/empty/malformed/notjson) with identical message needles.

## Error-code parity correction (vs plan v1.1)

The plan initially said "all failures → 22023". Reading the frozen oracle revealed the baseline distinguishes
**22023** (input/config: NULL content, unset endpoint, non-http scheme) from **38000** external_routine_exception
(HTTP/response failures). The Rust impl mirrors this exactly (plan corrected to v1.3). The oracle is authoritative.

## Code-quality evidence (honest)

- **Formal `/code-quality`:** PASS (100), 0 findings — BUT `languages_audited=[]` because `code-quality-languages.txt`
  enables no language; the cargo dead-code detectors (cargo-udeps/cargo-machete) are not installed in this env.
- **Manual Rust audit (the real evidence):** `cargo build --release` 0 warnings; `cargo clippy --release` **0 warnings**;
  no dead code (`_embed_text` has a caller — the wrapper; `err_input`/`err_external`/`guc`/`truncate` all used); no
  symbol fabrication (compiles + the oracle proves `theodb.embed`/`theodb_rs._embed_text` resolve at runtime).
- **Python:** `ruff check benchmarks/` clean.

## Caveats / honest notes

- The 4 Rust `#[pg_test]`s (input-guard unit tests) are in `lib.rs` and the test build compiles them, but
  `cargo pgrx test` requires a pgrx-MANAGED PostgreSQL (the image uses the system PG, "not managed by pgrx").
  The SAME guards (and more) are proven by the Python oracle against the real shipped extension — the blueprint's
  authoritative parity gate (Corner 1). The `#[pg_test]`s remain as source-level documentation runnable in a
  pgrx-managed CI.
- `cargo audit` reports 2 *unmaintained* warnings (`serde_cbor`, `paste`) — both transitive via `pgrx` (the
  framework, pinned to the image's 0.16.1), not exploitable CVEs; accepted (deps-audit report § caveat).
- GHCR image publish (`theo-db:develop`) remains a separate manual step (token scope) — unchanged by M17.
