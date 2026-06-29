---
slug: pgrx-extension-foundation
milestone_id: M17
created_at: 2026-06-29
goal: Replace the plpython3u theodb.embed with an own pgrx Rust extension (theodb_rs) at functional parity, measured by benchmarks/tests/test_embed_sql.py passing against the rebuilt image plus a reproducible latency benchmark.
---

# Plan: pgrx Extension Foundation — TheoDB's own Rust extension + theodb.embed (parity + benchmark)

> **Version 1.3** — (v1.3 corrects ADR D5 at implement time: the `pgvector` Rust crate has no `pgrx` feature (measured), so the `vector` return type is produced by a thin `extension_sql!` wrapper casting `text::vector` — the exact plpython3u parity path — and the `pgvector` crate dependency is DROPPED. Also: error-code parity corrected to mirror the oracle exactly — **22023** for input errors AND **38000** (ExternalRoutineException) for HTTP/response failures.) (v1.1 absorbed the edge-case review: MUST-FIX EC-1 → ADR D5 pins the `vector` return-type binding; SHOULD-TEST EC-2/EC-3; DOCUMENT EC-4. v1.2 absorbs the deps-audit `pgrx-extension-foundation-deps-audit-2026-06-29.md`: `minreq` uses the **`https-native`** (OpenSSL) TLS feature, NOT rustls — measured to eliminate 3 `rustls-webpki` advisories RUSTSEC-2026-0098/0099/0104; the `serde_cbor`-unmaintained warning via `pgrx` is an accepted LOW caveat.) Stands up TheoDB's **own** PostgreSQL extension in Rust (**pgrx**) — the ROADMAP-v2 / ADR 0006 foundation (M17) — and rewrites the simplest existing surface (`theodb.embed`, today plpython3u + urllib) in Rust as a separate transition extension `theodb_rs`, with **proven functional parity** (the existing Python e2e is the oracle) and a **reproducible latency benchmark** (measurement-first, ADR 0002 — no perf claim, embed is I/O-bound). The reference is **pgvectorscale** (a real pgrx extension, already cloned).

## Goal

> "Enable TheoDB to serve `theodb.embed` from its own pgrx Rust extension (`theodb_rs`) so that the plpython3u→Rust pattern is proven at parity, measured by `benchmarks/tests/test_embed_sql.py` passing green against the rebuilt image AND a reproducible latency benchmark (Rust vs plpython3u, mean±std, ≥3 runs) committed to `docs/benchmarks/m17-embed-rust-vs-plpython.md`."

## Context

ADR 0006 (`docs/adr/0006-own-code-postgres-based-rust-go.md`) pivots TheoDB to **own code in Rust (pgrx) + Go**, keeping the PostgreSQL engine (wire-compat, ADR 0001 A3 still bars an engine-from-scratch). M17 is the **foundation milestone** of ROADMAP-v2: it stands up the first pgrx crate and rewrites the smallest, lowest-risk surface — `theodb.embed` (one function, `sql/30-theodb-embed.sql`, plpython3u + urllib) — to prove the "plpython3u → own Rust extension" pattern end-to-end (crate → Docker build → install → parity → benchmark) before the heavier surfaces (ai.* in M18, NL→SQL in M19) follow.

The CTO requirement (verbatim, this milestone): "DEVE TER DADOS E VALIDAÇÕES EM BENCHMARK." So the benchmark is a **gate**, not a nicety. Per ADR 0002 (measurement-first) + `.claude/rules/public-copy.md`, the benchmark reports measured numbers only — and since `embed` is I/O-bound (the embeddings endpoint dominates wall-clock), the honest expected result is "no latency regression", not a speed win.

The whole milestone is anchored on the SHIPPABLE_WITH_CAVEATS blueprint `pgrx-extension-foundation-blueprint.md` (4 ADRs D1–D4, 6 recommendations) produced by the just-completed `/discover` cycle.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `sql/30-theodb-embed.sql` | 88 | `6c1dddb` (2026-06-28) | Defines `theodb.embed(content text, model text DEFAULT NULL)` in plpython3u: POST to `theodb.embedding_endpoint` GUC, SSRF guard (http(s)-only → SQLSTATE 22023), `REVOKE ALL FROM PUBLIC`, `CREATE SCHEMA IF NOT EXISTS theodb` | After edit: `CREATE SCHEMA IF NOT EXISTS theodb` MUST remain (other SQL files qualify `theodb.*`); the `theodb.embed(text,text)` SIGNATURE + behavior (384-dim vector, 22023 on bad input, REVOKE from PUBLIC) MUST be preserved — now served by `theodb_rs` |
| `Dockerfile` | ~85 | `6c1dddb` (2026-06-28) | Builds the image: scale-builder stage (Rust + cargo-pgrx 0.16.1, lines 10-28) compiles pgvectorscale; runtime stage COPYs `vectorscale*` artifacts (lines 52-53); init creates extensions | The scale-builder stage + vectorscale COPY MUST keep working; `PGRX_VERSION`/`PG_MAJOR` build args unchanged; runtime image gets NO Rust toolchain (artifacts COPYed only) |
| `theodb_rs/Cargo.toml` (NEW) | 0 | — | (cargo-pgrx crate manifest) | — |
| `theodb_rs/src/lib.rs` (NEW) | 0 | — | (pgrx crate: pg_module_magic + #[pg_schema] mod theodb + #[pg_extern] fn embed) | — |
| `theodb_rs/src/bin/pgrx_embed.rs` (NEW) | 0 | — | (pgrx schema-gen bin, per pgvectorscale convention) | — |
| `theodb_rs/theodb_rs.control` (NEW) | 0 | — | (extension control file) | — |
| `benchmarks/tests/test_embed_sql.py` | 188 | (M17 oracle) | Parity oracle: asserts `theodb.embed` returns 384-dim non-zero vector via `tools/embedding_server.py` stub (`host.docker.internal`), semantic check, typed-error checks | The 8 test functions are the contract; they must pass UNCHANGED against the Rust impl (oracle is not rewritten — blueprint Corner 1) |
| `tools/embedding_server.py` | (stub) | (existing) | Deterministic OpenAI-compatible embeddings server (BAAI/bge-small-en-v1.5, 384-dim) for tests + benchmark | Used unchanged by both the parity test and the benchmark (same stub → comparable numbers) |
| `docs/benchmarks/m17-embed-rust-vs-plpython.md` (NEW) | 0 | — | (benchmark report: methodology + mean±std for Rust vs plpython3u) | — |
| `benchmarks/bench_embed.py` (NEW) | 0 | — | (reproducible latency harness: N calls, ≥3 runs, mean±std) | — |
| `CHANGELOG.md` | (existing) | — | Public contract (Unbreakable Rule 6) | `[Unreleased]` updated with the M17 change |

Every file in a `#### Files to edit` block below appears in this table.

### Current callers / dependents

- **Symbol:** `theodb.embed(content text, model text DEFAULT NULL)` — defined in `sql/30-theodb-embed.sql:14`.
  - **Callers (production SQL):** other `sql/*.sql` files reference `theodb.*` schema-qualified; `theodb.embed` itself is called by user/application SQL and by the AI/import surfaces added in M16 (`sql/80-theodb-migrate.sql` is independent — it does not call embed). Verified: `grep -rln 'theodb.embed' sql/` → only `sql/30` defines it.
  - **Callers (tests):** `benchmarks/tests/test_embed_sql.py` (the parity oracle — 8 `test_` functions).
  - **External (public API consumed by other repos):** YES — `theodb.embed` is a documented user-facing SQL surface (`docs/`). The signature + SQLSTATE contract is the public API; it MUST NOT change. The implementation language is an internal detail.
- **Symbol:** GUCs `theodb.embedding_endpoint`, `theodb.embedding_model`, `theodb.embedding_api_key` — read via `current_setting(...)` in `sql/30:18-30`. The Rust impl reads the SAME GUC names (no new GUC namespace).

### Domain glossary

- **pgrx** — Rust framework to write PostgreSQL extensions; macros (`pg_module_magic!`, `#[pg_extern]`, `#[pg_schema]`, `#[pg_test]`) generate the C ABI glue + the `.sql`/`.control` install artifacts via `cargo pgrx schema`/`install`.
- **theodb_rs** — the NEW transition extension name (ADR D1) that owns the Rust `theodb.embed` during the incremental rewrite; coexists with the SQL-only `theodb` extension without a duplicate-definition clash.
- **GUC** — Grand Unified Configuration: a PostgreSQL runtime setting (`theodb.embedding_endpoint`, etc.) read with `current_setting`.
- **SSRF guard** — the `embed` function rejects non-`http(s)://` endpoints (and follows no redirects) → SQLSTATE `22023` (`invalid_parameter_value`), preventing a GUC-controlled request from reaching internal addresses.
- **Parity oracle** — `benchmarks/tests/test_embed_sql.py`: the cross-language proof that the Rust impl behaves identically to the plpython3u one (same vector shape, same typed errors).
- **scale-builder** — the existing Dockerfile builder stage (lines 10-28) that compiles pgvectorscale with cargo-pgrx; the template M17's `theodb-rs-builder` stage mirrors.

### Architecture boundaries affected

Per `.claude/rules/architecture.md`: the change replaces an **infrastructure adapter** (the embeddings HTTP client) implementation from plpython3u to Rust, **without changing the domain contract** (the `theodb.embed` SQL signature + SQLSTATE error type). The composition root (Docker image build + `CREATE EXTENSION`) gains a second builder stage and a second extension install — wiring at the top, not inside business logic (DIP-aligned). No inner layer learns about Rust; callers still see `theodb.embed`.

## Prior Art & Related Work

- **Internal blueprint:** `.claude/knowledge-base/discoveries/blueprints/pgrx-extension-foundation-blueprint.md` (SHIPPABLE_WITH_CAVEATS 89) — ADRs D1 (separate `theodb_rs` ext), D2 (minreq ISC + ureq fallback), D3 (second Docker builder stage), D4 (measurement-first benchmark); Recommendations 1–6; Corner 1 (parity oracle = existing Python suite), Corner 4 T1/T2/T3.
- **Reference project:** `.claude/knowledge-base/references/pgvectorscale/` — a real pgrx extension. `pgvectorscale/Cargo.toml:31` (`pgrx =0.16.1`, pgNN features, `crate-type=["cdylib","rlib"]`, `[[bin]] pgrx_embed`); `src/lib.rs:1-27` (`use pgrx::prelude::*; pgrx::pg_module_magic!();`); `src/access_method/mod.rs:284-285` (`#[pg_extern]` shape); `Makefile:55-71` (`cargo install cargo-pgrx`, `cargo pgrx init --pgN`, `cargo pgrx install --release`). Study-only contrast: `vectorchord` (AGPL — pattern only, never copied; D1).
- **Edge-case review:** `.claude/knowledge-base/reviews/pgrx-extension-foundation-edge-cases-2026-06-29.md` — EC-1 (HTTP crate research repointed docs.rs→github.com), EC-2 (coexistence → separate `theodb_rs` ext = ADR D1), EC-3 (Docker build, disk OK).
- **Strategy anchor:** `docs/adr/0006-own-code-postgres-based-rust-go.md`, `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md` (measurement-first), `ROADMAP-v2.md` (### M17 DoD).
- **External literature:** pgrx (`github.com/pgcentralfoundation/pgrx`) — extension framework docs; minreq (`github.com/neonmoe/minreq`) — ISC, minimal HTTP client; ureq (`github.com/algesten/ureq`) — MIT/Apache fallback with explicit redirect control.

## Objective

- [ ] A `theodb_rs` cargo-pgrx crate exists (`pgrx =0.16.1`, feature `pg17`) and compiles, exposing `theodb.embed(content text, model text DEFAULT NULL) RETURNS vector` via `#[pg_schema] mod theodb { #[pg_extern] }`.
- [ ] The Rust `embed` POSTs to the `theodb.embedding_endpoint` GUC via a minimal audited HTTP crate (minreq/ISC, or ureq fallback), with Content-Type + Authorization headers, a timeout, SSRF guard (http(s)-only, no redirects) → SQLSTATE 22023 typed errors (parity with `sql/30:37-46`).
- [ ] `theodb.embed` is removed from `sql/30-theodb-embed.sql` (the SQL `theodb` no longer defines it; `CREATE SCHEMA` + `REVOKE` posture preserved); no duplicate-definition clash.
- [ ] The Dockerfile builds `theodb_rs` in a second builder stage (mirroring scale-builder) and `CREATE EXTENSION theodb_rs` runs at init.
- [ ] `benchmarks/tests/test_embed_sql.py` passes UNCHANGED against the rebuilt image (Rust `theodb.embed`); a Rust `#[pg_test]` covers SSRF reject + error mapping.
- [ ] A reproducible latency benchmark (Rust vs plpython3u, same stub, N calls, ≥3 runs, mean±std) is committed to `docs/benchmarks/m17-embed-rust-vs-plpython.md` with NO perf claim beyond the measured numbers.
- [ ] `/deps-audit` runs clean on the chosen HTTP crate (CVE) and its license (ISC/MIT) is recorded.

## ADRs

### D1 — Ship the Rust `theodb.embed` as a separate transition extension `theodb_rs`
**Decision:** the Rust function ships as a new extension `theodb_rs`; `theodb.embed` is removed from `sql/30` so the SQL `theodb` no longer defines it; both extensions install on the image; surfaces consolidate at M19.
**Rationale:** two extensions cannot both define `theodb.embed` (duplicate-definition error on `CREATE EXTENSION`). A separate `theodb_rs` + removing the SQL body keeps the rewrite incremental and clash-free (blueprint D1 / EC-2). The user still calls `theodb.embed` (same schema-qualified name). Honors `.claude/rules/parsimony-ladder.md` (smallest change that resolves the need — no big-bang).
**Alternatives considered:** (a) migrate the whole `theodb` extension to pgrx now — rejected: big-bang, contradicts ADR 0006's incremental-with-parity mandate; (b) keep both definitions — rejected: duplicate-definition error; (c) name the Rust function differently (e.g. `theodb.embed_rs`) — rejected: breaks the public `theodb.embed` contract / callers.
**Consequences:** one transition extension to consolidate at M19; clean per-feature migration; the `vector` return type still comes from pgvector (`requires`/present).

### D2 — HTTP via `minreq` (ISC), confirm the no-redirect API in M17; `ureq` fallback
**Decision:** use `minreq` (ISC, minimal) for the embed POST with the **`https-native`** (native-tls/OpenSSL) TLS feature, NOT the rustls `https` feature; during M17 confirm POST-body + header + timeout + **no-redirect** support against the crate source; if no-redirect control is absent, fall back to `ureq` (MIT/Apache, which exposes redirect config).
**Rationale:** Rule 9 (don't reinvent HTTP) + parsimony-ladder rung 4 (minimal audited dep, not a heavy `reqwest` async stack). ISC and MIT/Apache are D1-permissive. **The deps-audit measured that minreq's rustls `https` feature pulls a vulnerable `rustls-webpki 0.101.7` (RUSTSEC-2026-0098/0099/0104, cert name-constraint bypass); the `https-native` feature uses the builder stage's existing OpenSSL (libssl-dev) → 0 CVEs and no new system dep.** SSRF parity requires no-redirect, so the crate choice is contingent on that capability — confirmed at implement time, not assumed.
**Alternatives considered:** (a) `reqwest` — rejected: heavy dep tree + async runtime for one blocking POST; (b) hand-rolled TCP/TLS — rejected: reinventing the wheel + security risk; (c) call out to a sidecar — rejected: over-engineering (YAGNI).
**Consequences:** small pinned dep via `Cargo.lock`; the no-redirect confirmation is an explicit M17 step (SSRF parity depends on it); `/deps-audit` gates the CVE.

### D3 — Build `theodb_rs` in a second Docker builder stage mirroring scale-builder
**Decision:** add a `theodb-rs-builder` stage mirroring the existing `scale-builder` (lines 10-28); COPY the generated `.so` + `.control` + `--{version}.sql` into the runtime image (same pattern as the vectorscale artifact COPY, lines 52-53).
**Rationale:** the scale-builder pattern already compiles a pgrx extension successfully and reproducibly (image-side, pinned cargo-pgrx 0.16.1). Reusing it (DRY + Rule 9) avoids inventing a new build path. Disk was freed to ~34 GB; `cargo pgrx init` compiles a PG (~GB) only in the builder stage, never in runtime.
**Alternatives considered:** (a) local-toolchain build — rejected: non-reproducible, env-dependent; (b) a single shared builder stage for both extensions — rejected: couples two independently-versioned crates' build caches, harder to reason about; deferred until there's a second Rust crate (YAGNI).
**Consequences:** longer image build (compiles our crate); runtime image stays toolchain-free; pinned `Cargo.lock` for reproducibility.

### D5 — Produce the `vector` return type via a thin SQL wrapper casting `text::vector` (no extra crate)
**Decision:** the Rust `#[pg_extern]` function `theodb._embed_text(content text, model text) RETURNS text` returns the embedding as the same `"[x,y,z]"` string the plpython3u version produced; an `extension_sql!` wrapper `theodb.embed(content text, model text DEFAULT NULL) RETURNS vector LANGUAGE sql AS 'SELECT theodb._embed_text($1,$2)::vector'` casts it to `vector` using pgvector's text input function. No extra Rust crate.
**Rationale:** **measured fact (deps-audit follow-up): the `pgvector` Rust crate 0.4.x has NO `pgrx` feature** (only `postgres`/`sqlx`/`diesel` client-side bindings — `cargo` resolve: "pgvector does not have that feature"). So the original "pgvector crate Vector type" idea (EC-1 option b) is not available for pgrx server-side. The `text::vector` cast is exactly what the plpython3u baseline relies on (it returns the `"[...]"` string and the `RETURNS vector` declaration triggers pgvector's input function) — it is the canonical, definitely-available path, and it DROPS a dependency (parsimony — Rule 9/YAGNI). The oracle's `<=>` (cosine distance) + `::text` assertions still see a real `vector`.
**Alternatives considered:** (a) `pgvector` crate `Vector` type — **rejected: the crate has no pgrx feature (measured)**; (b) return `Vec<f32>` (→ `float4[]`) + `float4[]::vector` cast — viable but `text::vector` is the exact plpython3u parity path and needs no array round-trip; (c) hand-roll the `vector` binary send/recv in Rust — rejected: reinventing pgvector's protocol (Rule 9 + risk).
**Consequences:** the `theodb_rs` control keeps `requires = 'vector'` (the cast + type come from the pgvector extension); two functions ship (`theodb._embed_text` Rust + `theodb.embed` SQL wrapper), both REVOKEd from PUBLIC; **no extra Rust dependency** (the deps set shrinks — pgvector crate removed).

### D4 — Benchmark is measurement-first; parity via the existing Python oracle; no perf claim
**Decision:** prove functional parity with `benchmarks/tests/test_embed_sql.py` (unchanged); measure latency (Rust vs plpython3u) with a reproducible harness — N calls against the same `tools/embedding_server.py` stub, ≥3 runs, report mean±std → `docs/benchmarks/m17-embed-rust-vs-plpython.md`; assert no regression, claim nothing beyond the measured numbers.
**Rationale:** ADR 0002 (measurement-first) + `.claude/rules/public-copy.md` (no perf claim without reproducible benchmark) + `.claude/rules/testing.md` (parity = existing tests, don't rewrite the oracle). `embed` is I/O-bound (the stub/endpoint dominates), so the honest outcome is "no regression", not a speed win — stating otherwise would be a false claim.
**Alternatives considered:** (a) skip the benchmark — rejected: the CTO requires data this milestone; (b) claim a speedup — rejected: unbenchmarked/false (I/O-bound); (c) rewrite the parity oracle in Rust — rejected: loses the cross-language proof (the Python suite IS the independent check).
**Consequences:** the benchmark is both a gate and honest evidence; the report explicitly documents the I/O-bound caveat.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| `cargo pgrx init` compiles a full PostgreSQL (~GB) in the builder stage; image build time + disk grow | Medium | Reuse the proven scale-builder stage (D3); disk freed to ~34 GB; pinned `Cargo.lock`; build is image-side (cacheable layer) | TheoDB maintainers |
| `minreq` may lack explicit no-redirect control → SSRF-via-redirect parity gap if not caught | High | D2 mandates confirming the no-redirect API against the crate source BEFORE locking; `ureq` fallback (exposes redirect config); a `#[pg_test]` asserts SSRF reject | TheoDB maintainers |
| Two extensions defining schema `theodb` could clash on objects other than `embed` | Medium | D1: `theodb_rs` defines ONLY `embed`; `CREATE SCHEMA IF NOT EXISTS theodb` is idempotent; verify `CREATE EXTENSION theodb_rs` + `theodb` both install on a fresh DB in the integration test | TheoDB maintainers |
| Benchmark could be misread as a performance win, violating public-copy | Low | D4: report states "no regression (I/O-bound)" explicitly; no comparative claim in README; numbers + methodology only | TheoDB maintainers |
| pgrx GUC access pattern differs from plpython3u `current_setting` → behavioral drift on unset GUC | Medium | Read GUCs via the SAME names; map "endpoint unset" to the SAME 22023 error; the parity oracle asserts the unset-GUC error path | TheoDB maintainers |

## Unresolved Questions

- Q1 — Does `minreq` expose explicit no-redirect control and a request timeout in its current ISC release? (Resolved AT implement time per D2 — confirm against crate source; `ureq` fallback if not. This is the one open item flagged by the blueprint.)
- Q2 — Should `theodb_rs` carry `requires = 'vector'` in its control file (so `CREATE EXTENSION theodb_rs CASCADE` pulls pgvector for the `vector` return type)? (Resolved: YES — declare `requires = 'vector'` for self-containment; needed by D5's `pgvector::Vector` type; confirm no double-install clash in the integration test.)
- Q3 — (Resolved by ADR D5) The `vector` return-type binding is the `pgvector` Rust crate (MIT), not a `float4[]` cast — see D5; audited in T5.1.

## Dependencies

New Rust dependencies introduced by this plan (the `theodb_rs` crate). All pinned in `theodb_rs/Cargo.toml` + `theodb_rs/Cargo.lock`; all D1-permissive (Apache/MIT/BSD/ISC only); CVE-audited in T5.1.

| Ecosystem | Package | Version | License (D1) | Why (Rule 9 — don't reinvent) | Alternative rejected |
|---|---|---|---|---|---|
| Rust (cargo) | `pgrx` | `=0.16.1` | Apache-2.0 / MIT | The PostgreSQL-extension-in-Rust framework — matches the image's `PGRX_VERSION=0.16.1` (no toolchain drift) | hand-rolled C ABI glue (reinvents pgrx) |
| Rust (cargo) | `minreq` | `2.x` (pin exact in Cargo.lock), feature **`https-native`** (OpenSSL/native-tls) + `json-using-serde` | ISC | Minimal audited HTTP client for the embed POST — don't reinvent HTTP, don't pull heavy `reqwest`. **`https-native` (NOT rustls `https`) — deps-audit measured the rustls path pulls vulnerable `rustls-webpki 0.101.7` (RUSTSEC-2026-0098/0099/0104); native-tls uses the builder's existing OpenSSL → 0 CVEs.** D2: confirm no-redirect/timeout API at implement time; `ureq` (MIT/Apache) fallback if absent | `reqwest` (heavy async tree); rustls `https` feature (vulnerable rustls-webpki); hand-rolled TCP/TLS (security risk) |
| Rust (cargo) | `serde_json` *(or minreq's `json` feature)* | `1.x` | Apache-2.0 / MIT | Parse the embeddings JSON response — don't hand-roll a JSON parser (Rule 9) | hand-rolled JSON parsing |

> **D5 correction (v1.3):** the `pgvector` Rust crate was REMOVED from the dep set — it has no `pgrx` feature (measured). The `vector` type is produced by an SQL wrapper casting `text::vector` (pgvector *extension* input function), so no extra Rust crate is needed.

> `minreq` vs `ureq` is resolved at implement time (D2) based on no-redirect support; whichever is chosen is the one audited in T5.1. If `minreq`'s `json` feature covers response parsing, `serde_json` is dropped (parsimony — one fewer dep). No AGPL/Elastic dep enters the distribution (D1).

## Dependency Graph

```
Phase 0 (crate skeleton compiles) ──▶ Phase 1 (embed in Rust: HTTP+SSRF+errors) ──▶ Phase 2 (remove from sql/30 + Docker wiring)
                                                                                            │
                                                                                            ▼
                                                                              Phase 3 (parity: Python oracle + #[pg_test])
                                                                                            │
                                                                                            ▼
                                                                              Phase 4 (benchmark Rust vs plpython3u)
                                                                                            │
                                                                                            ▼
                                                                              Phase 5 (deps-audit + Integration Validation)
```

Sequential: each phase blocks the next (the crate must compile before embed; embed must exist before wiring; wiring must work before parity can run against the image; parity must hold before the benchmark is meaningful). No parallelism — single transition extension, one function.

---

## Phase 0: pgrx crate skeleton

**Objective:** a `theodb_rs` cargo-pgrx crate that compiles and exposes a stub `theodb.embed` signature.

### T0.1 — Create the `theodb_rs` pgrx crate

#### Objective
Stand up the crate (`Cargo.toml`, `src/lib.rs`, `src/bin/pgrx_embed.rs`, `theodb_rs.control`) so `cargo pgrx schema` generates the `theodb.embed(content text, model text DEFAULT NULL) RETURNS vector` SQL.

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — creates a minimal cargo-pgrx crate mirroring pgvectorscale's manifest + lib skeleton, with a stub `embed` returning a placeholder vector, wired so `cargo pgrx schema` emits the SQL surface in the `theodb` schema.
2. **Why it is necessary now** — the build/install path (D3) and every later phase depend on a crate that compiles and generates the right SQL signature; standing up the skeleton first isolates "does pgrx build at all in our image" from "is the HTTP logic correct" (Baseline: greenfield Rust, no Cargo.toml — `find` returned nothing). Cites blueprint Corner 4 T1 + Recommendation 1.

#### Evidence
- pgvectorscale crate skeleton: `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/Cargo.toml:1-45` (pgrx `=0.16.1`, `crate-type=["cdylib","rlib"]`, `[[bin]] pgrx_embed`, pgNN features), `src/lib.rs:1-27` (`use pgrx::prelude::*; pgrx::pg_module_magic!();`).
- `#[pg_extern]` shape: `pgvectorscale/src/access_method/mod.rs:284-285`.
- Target signature: Blueprint §"T1" line 86 (`theodb.embed(content text, model text DEFAULT NULL) RETURNS vector`).

#### Files to edit
```
theodb_rs/Cargo.toml (NEW) — pgrx =0.16.1, feature pg17 (default), pg_test feature, crate-type cdylib+rlib, [[bin]] pgrx_embed; pgvector crate (MIT, pgrx feature — D5) for the Vector return type; minreq dep added in Phase 1 (skeleton has no HTTP yet)
theodb_rs/src/lib.rs (NEW) — use pgrx::prelude::*; pgrx::pg_module_magic!(); #[pg_schema] mod theodb { #[pg_extern] fn embed(content: &str, model: default!(Option<&str>, "NULL")) -> pgvector::Vector { stub } }  (D5: return type is pgvector::Vector → SQL vector)
theodb_rs/src/bin/pgrx_embed.rs (NEW) — ::pgrx::pgrx_embed!(); (schema-gen bin, pgvectorscale convention)
theodb_rs/theodb_rs.control (NEW) — comment, default_version='1.0', relocatable=false, superuser=true, requires='vector'
```

#### Deep file dependency analysis
- All NEW (greenfield). No existing file imports them yet. `theodb_rs.control` + the generated `theodb_rs--1.0.sql` are consumed by Phase 2's Dockerfile COPY. The `vector` return type is produced via the `pgvector` Rust crate's pgrx `Vector` type (**ADR D5** — MIT, audited in T5.1): `#[pg_extern] fn embed(...) -> pgvector::Vector` maps natively to SQL `vector`, with no `float4[]` intermediary or cast surface (resolves edge-case EC-1).

#### Deep Dives
- **Invariants:** the generated SQL MUST place `embed` in schema `theodb` (so callers keep `theodb.embed(...)`); the control `requires='vector'` (Q2) so the `vector` type resolves.
- **Edge cases:** `model` defaults to NULL (matches `sql/30` signature). The stub returns a fixed-dim vector so Phase 0's test can assert the SQL surface exists before HTTP is wired.

#### Pseudo-code / Signatures
```rust
use pgrx::prelude::*;
pgrx::pg_module_magic!();

#[pg_schema]
mod theodb {
    use pgrx::prelude::*;
    use pgvector::Vector;   // D5 — MIT crate, pgrx feature → SQL `vector`
    // Phase 0 stub: returns a fixed-length zero vector so the SQL surface + dim are testable
    #[pg_extern]
    fn embed(content: &str, model: default!(Option<&str>, "NULL")) -> Vector {
        // Phase 1 replaces the body with the HTTP call.
        Vector::from(vec![0.0_f32; 384])
    }
}
```

#### Tasks
1. Write `theodb_rs/Cargo.toml` (pgrx `=0.16.1`, features `pg17`/`pg_test`, crate-type, `[[bin]] pgrx_embed`).
2. Write `theodb_rs/src/lib.rs` (pg_module_magic + `#[pg_schema] mod theodb` + stub `embed`).
3. Write `theodb_rs/src/bin/pgrx_embed.rs` (`::pgrx::pgrx_embed!();`).
4. Write `theodb_rs/theodb_rs.control` (default_version, relocatable=false, superuser=true, requires='vector').
5. Confirm `cargo pgrx schema` (in the Docker builder, T2.1) emits `CREATE FUNCTION theodb.embed(...)`.

#### TDD
```
RED:     test_crate_compiles — `cargo build` (in builder) fails until Cargo.toml+lib.rs are valid
RED:     test_schema_has_embed — `cargo pgrx schema` output contains "theodb.embed" with (text, text) → vector
GREEN:   Write the four files so both pass
REFACTOR: None expected (skeleton)
VERIFY:  docker build --target theodb-rs-builder . (compiles the crate + runs cargo pgrx schema)
```

#### Concurrency tests (only when applicable)

(none — single-threaded) — the embed function is a synchronous per-call SQL function; PostgreSQL serializes the call within a backend, no shared mutable state, no locks/async/threads.

#### Acceptance Criteria
- [ ] `theodb_rs` crate compiles under pgrx `=0.16.1`, feature `pg17`.
- [ ] `cargo pgrx schema` emits `theodb.embed(content text, model text DEFAULT NULL) RETURNS vector`.
- [ ] Pass: lint — `cargo clippy` zero warnings on the new crate.
- [ ] Pass: size — `src/lib.rs` ≤ 500 lines.

#### DoD (Definition of Done)
- [ ] All tasks completed.
- [ ] Builder stage compiles the crate (`docker build --target theodb-rs-builder`).
- [ ] `cargo clippy` clean.
- [ ] File-size budget respected.

---

## Phase 1: embed in Rust — HTTP + SSRF + typed errors

**Objective:** replace the stub body with a real POST to the embeddings endpoint, at security + error parity with the plpython3u version.

### T1.1 — Implement `theodb.embed` HTTP call with SSRF guard + 22023 typed errors

#### Objective
Read the `theodb.embedding_*` GUCs, POST the content to the endpoint via the chosen HTTP crate, parse the embedding into a `vector`, and reject bad input (non-http(s), unset endpoint, redirect) with SQLSTATE 22023 — matching `sql/30:18-46`.

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — implements the real `embed`: GUC reads (`theodb.embedding_endpoint/model/api_key`), an SSRF-guarded POST (Content-Type + Authorization + timeout, no redirects), JSON parse of the embedding, and typed-error mapping to SQLSTATE 22023.
2. **Why it is necessary now** — this is the core parity surface: the function is useless (and unsafe) without the real HTTP + SSRF behavior, and Phase 3's oracle asserts exactly this. Doing it after the skeleton compiles isolates HTTP-correctness from build-correctness. Cites D2 + blueprint Corner 4 T2 + `sql/30:37-46`.

#### Evidence
- plpython3u baseline to match: `sql/30-theodb-embed.sql:18-30` (GUC reads), `:37-39` (SSRF http(s)-only), `:37-46` (error → 22023). 
- HTTP crate API: minreq `github.com/neonmoe/minreq` (POST/header/timeout); ureq `github.com/algesten/ureq` (redirect config) — D2.
- Error mapping: Blueprint §"T2" lines 98-99 (`PgSqlErrorCode` → 22023).

#### Files to edit
```
theodb_rs/Cargo.toml — add the HTTP dep: minreq with features `https-native` (OpenSSL — NOT rustls, per deps-audit) + `json-using-serde`, pinned; OR ureq fallback; serde_json (or minreq's json) for parsing
theodb_rs/src/lib.rs — replace stub embed body with: read GUCs, validate endpoint (http(s)-only, else 22023), POST (no redirect, timeout), parse embedding, return vector; map all failures to ereport SQLSTATE 22023
theodb_rs/src/embed.rs (NEW, optional) — extract the HTTP+SSRF helper if lib.rs would exceed budget (SRP)
```

#### Deep file dependency analysis
- `Cargo.toml` gains exactly one HTTP dep (parsimony rung 4) + a JSON parser. `lib.rs` `embed` body is rewritten; its public signature is unchanged (Phase 0 contract holds). If `lib.rs` approaches 500 LoC, split the HTTP/SSRF logic into `src/embed.rs` (cite architecture.md SRP). No downstream Rust file depends on internals; the SQL surface is stable.

#### Deep Dives
- **GUCs:** read with pgrx (`Spi::get_one` on `current_setting('theodb.embedding_endpoint', true)`) or a registered `GucSetting` — use `current_setting` to match plpython3u semantics exactly (NULL/unset → 22023 "endpoint not configured").
- **SSRF guard (invariant from Baseline):** endpoint MUST start with `http://` or `https://`; otherwise `ereport(ERROR, errcode 22023)`. Disable redirects on the client (D2 — if minreq cannot, use ureq). No following 3xx.
- **Error mapping:** every failure (unset GUC, bad scheme, connect error, non-2xx, JSON parse error, dim mismatch) → SQLSTATE 22023 with a clear message (error-handling.md: typed + contextual, fail-fast). Parity: `sql/30` raises 22023 on these.
- **Vector parse:** response `data[0].embedding` → `Vec<f32>` → `vector`. Assert non-empty.

#### Pseudo-code / Signatures
```rust
#[pg_extern]
fn embed(content: &str, model: default!(Option<&str>, "NULL")) -> Vec<f32> {
    let endpoint = guc("theodb.embedding_endpoint")
        .ok_or_else(|| err22023("theodb.embedding_endpoint is not set"))?;
    if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
        ereport_22023!("embedding endpoint must be http(s)://");   // SSRF parity (sql/30:37-39)
    }
    let body = json!({ "input": content, "model": model.or(guc("theodb.embedding_model")) });
    let resp = http_post(&endpoint, body)        // Content-Type + Authorization + timeout + NO redirect
        .map_err(|e| err22023(&format!("embedding request failed: {e}")))?;
    let v = parse_embedding(resp).map_err(|e| err22023(&format!("bad embedding response: {e}")))?;
    if v.is_empty() { ereport_22023!("embedding response had no vector"); }
    v
}
// Example: endpoint="ftp://x" → ERROR SQLSTATE 22023 "embedding endpoint must be http(s)://"
```

#### Tasks
1. Confirm the HTTP crate's POST/header/timeout/no-redirect API (D2); pick minreq or ureq; pin in Cargo.toml.
2. Implement GUC reads (same names as `sql/30`).
3. Implement SSRF guard (http(s)-only, no redirects) → 22023.
4. Implement POST + JSON parse → `vector`; map every failure to 22023.
5. (If needed) extract `src/embed.rs` to respect the 500-LoC budget.

#### TDD
```
RED:     #[pg_test] test_embed_rejects_non_http — endpoint='ftp://x' → ERROR SQLSTATE 22023
RED:     #[pg_test] test_embed_rejects_unset_endpoint — no GUC → ERROR SQLSTATE 22023
RED:     #[pg_test] test_embed_no_redirect — endpoint returning 302 → does NOT follow → 22023 (or documented behavior)
RED:     #[pg_test] test_embed_endpoint_4xx_maps_to_22023 (EC-3) — stub returns 400/413 → ERROR SQLSTATE 22023 with a clear message (the non-2xx path MUST cover 4xx, not only 5xx/connect)
GREEN:   Implement embed body so all #[pg_test] pass
REFACTOR: Extract src/embed.rs if lib.rs > ~300 LoC
VERIFY:  cargo pgrx test (in the builder stage)
```

> **EC-4 (DOCUMENT):** `embed` returns whatever dimension the endpoint produces — `pgvector::Vector` is dimension-flexible. The parity oracle pins 384 only because `tools/embedding_server.py` is a 384-dim model; a real endpoint with a different model returns a different dim, which is correct (the function does not hard-code 384). Note this in the embed doc/benchmark report.

#### Concurrency tests (only when applicable)
(none — single-threaded) — embed is a per-call synchronous function; no shared mutable state, no locks/async/threads in the Rust impl. PostgreSQL serializes the function call within a backend.

#### Failure-scenario note
External I/O (the embeddings HTTP endpoint) — see `## Failure scenarios` for the timeout/5xx/redirect rows; the `#[pg_test]` above + the Python oracle exercise them.

#### Acceptance Criteria
- [ ] Endpoint scheme validation rejects non-http(s) → SQLSTATE 22023 (parity `sql/30:37-39`), verified by `cargo pgrx test` (test_embed_rejects_non_http) exiting 0.
- [ ] `theodb.embed('x')` with `theodb.embedding_endpoint` unset raises SQLSTATE 22023, verified by `cargo pgrx test` (test_embed_rejects_unset_endpoint) exiting 0.
- [ ] No redirects followed (SSRF parity) — confirmed by `#[pg_test]` or documented crate behavior.
- [ ] Successful POST returns a non-empty `vector`.
- [ ] Pass: lint — `cargo clippy` zero warnings.
- [ ] Pass: size — each changed Rust file ≤ 500 lines.

#### DoD (Definition of Done)
- [ ] `cargo pgrx test` green for the SSRF/error `#[pg_test]`s.
- [ ] `cargo clippy` clean.
- [ ] HTTP crate API (no-redirect) confirmed + recorded (Q1).
- [ ] File-size budget respected.

---

## Phase 2: remove from sql/30 + Docker wiring

**Objective:** stop the SQL `theodb` from defining `embed`, and build+install `theodb_rs` in the image.

### T2.1 — Remove `theodb.embed` from sql/30 and add the Docker builder stage + CREATE EXTENSION

#### Objective
Delete the `CREATE OR REPLACE FUNCTION theodb.embed ... $$;` body (and its `REVOKE` line that targets it) from `sql/30`, keeping `CREATE SCHEMA IF NOT EXISTS theodb`; add a `theodb-rs-builder` Docker stage + COPY artifacts + `CREATE EXTENSION theodb_rs` at init.

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — removes the plpython3u `theodb.embed` definition from `sql/30` (so only `theodb_rs` defines it — no clash), and wires the crate into the image (second builder stage mirroring scale-builder; COPY `.so`/`.control`/`.sql`; `CREATE EXTENSION theodb_rs`).
2. **Why it is necessary now** — until `sql/30` stops defining `embed` AND the extension is installed, the image either clashes (two definitions) or has no Rust embed at all; both block Phase 3's parity run against the image. Cites D1 + D3 + blueprint Recommendation 3 + Baseline (`sql/30:12-80`, `Dockerfile:10-28,52-53`).

#### Evidence
- `sql/30` structure: `sql/30-theodb-embed.sql:12` (`CREATE SCHEMA`), `:14-76` (the function body), `:80` (`REVOKE ALL ON FUNCTION theodb.embed`). Removing `:14-80`'s function + revoke, keeping `:12`.
- Build pattern: `Dockerfile:10-28` (scale-builder), `:52-53` (vectorscale COPY), `:79` (CREATE EXTENSION at init).

#### Files to edit
```
sql/30-theodb-embed.sql — remove CREATE OR REPLACE FUNCTION theodb.embed(...)$$; and the REVOKE ... FROM PUBLIC line; KEEP CREATE SCHEMA IF NOT EXISTS theodb (+ header comment). If nothing but the schema remains, the file becomes a thin schema-ensure (acceptable) — or fold the CREATE SCHEMA elsewhere if cleaner (KISS).
Dockerfile — add stage `theodb-rs-builder` (FROM the same Rust base as scale-builder: apt build-essential postgresql-server-dev-17 libssl-dev pkg-config clang; rustup; cargo install --locked cargo-pgrx 0.16.1; cargo pgrx init --pg17; COPY theodb_rs/ ; cargo pgrx install --release --features pg17); in runtime stage COPY --from=theodb-rs-builder the theodb_rs.so + theodb_rs.control + theodb_rs--*.sql into the PG lib/extension dirs; add CREATE EXTENSION theodb_rs (CASCADE) to the init SQL alongside the existing extension creation
```

#### Deep file dependency analysis
- `sql/30` (Baseline row): today's only definer of `theodb.embed`. After edit, the SCHEMA stays (other SQL qualifies `theodb.*`); the function moves to `theodb_rs`. Risk (Baseline invariant): callers MUST still resolve `theodb.embed` — satisfied because `theodb_rs` installs it in the same schema before user SQL runs.
- `Dockerfile` (Baseline row): scale-builder + vectorscale COPY MUST keep working — the new stage is additive; the new COPY mirrors lines 52-53. `CREATE EXTENSION theodb_rs` runs at init (idempotent on fresh DB).

#### Deep Dives
- **Invariant:** `CREATE SCHEMA IF NOT EXISTS theodb` MUST remain reachable before any `theodb.*` reference (extension install order: `theodb_rs` creates the schema via its own SQL or relies on the kept `CREATE SCHEMA`). Confirm install order in the integration test.
- **Edge case:** fresh DB init must `CREATE EXTENSION theodb_rs` AFTER `vector` (control `requires='vector'` + CASCADE handles it).
- **Backward compat:** the `REVOKE ALL ... FROM PUBLIC` posture (least privilege) must be preserved — replicate it for the Rust function (pgrx `#[pg_extern]` is granted to PUBLIC by default; add a `REVOKE` in the extension's install SQL or a post-install step).

#### Tasks
1. Edit `sql/30`: remove the function body + its REVOKE; keep `CREATE SCHEMA` (+ comment).
2. Add the `theodb-rs-builder` Docker stage (mirror scale-builder).
3. COPY `theodb_rs` artifacts into the runtime image.
4. Add `CREATE EXTENSION theodb_rs` (CASCADE) at init.
5. Re-apply the `REVOKE ALL ON FUNCTION theodb.embed(text,text) FROM PUBLIC` for the Rust function (least-privilege parity).

#### TDD
```
RED:     test_image_builds — `docker build .` fails until the builder stage + COPY are correct
RED:     test_no_duplicate_embed — fresh DB: CREATE EXTENSION theodb_rs + theodb both succeed (no duplicate-definition error)
RED:     test_embed_revoked_from_public — a non-superuser role cannot EXECUTE theodb.embed (parity sql/30:80)
GREEN:   Edit sql/30 + Dockerfile so all pass
REFACTOR: None expected
VERIFY:  docker build -t theo-db:m17 . && docker run ... (init succeeds; \df theodb.embed shows the function)
```

#### Concurrency tests (only when applicable)

(none — single-threaded) — the embed function is a synchronous per-call SQL function; PostgreSQL serializes the call within a backend, no shared mutable state, no locks/async/threads.

#### Acceptance Criteria
- [ ] `sql/30` no longer defines `theodb.embed`; `CREATE SCHEMA IF NOT EXISTS theodb` preserved.
- [ ] Image builds with the `theodb-rs-builder` stage; runtime has no Rust toolchain.
- [ ] Fresh DB init installs `theodb_rs` + `theodb` with no duplicate-definition error.
- [ ] `theodb.embed` is REVOKEd from PUBLIC (least-privilege parity).
- [ ] Pass: lint — Dockerfile hadolint clean (if available) / no obvious anti-patterns.

#### DoD (Definition of Done)
- [ ] `docker build -t theo-db:m17 .` succeeds.
- [ ] Container init creates both extensions; `\df theodb.embed` shows the function.
- [ ] CHANGELOG `[Unreleased]` updated.

---

## Phase 3: parity — Python oracle + #[pg_test]

**Objective:** prove the Rust `theodb.embed` behaves identically to the plpython3u one.

### T3.1 — Run the existing parity oracle against the rebuilt image (Rust embed)

#### Objective
Run `benchmarks/tests/test_embed_sql.py` UNCHANGED against `theo-db:m17`; all 8 tests pass (384-dim non-zero vector, semantic check, typed 22023 errors) served by the Rust function.

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — points the existing Python e2e suite at the rebuilt image and confirms green; the suite is the cross-language parity proof (blueprint Corner 1).
2. **Why it is necessary now** — this is the Goal's metric. Without the oracle green against the Rust impl, parity is unproven and the rewrite is not "done" (ADR 0006 incremental-with-parity). Doing it after the image builds is the only point where the Rust embed is callable end-to-end. Cites D4 + `testing.md` + `benchmarks/tests/test_embed_sql.py:79-82,85+`.

#### Evidence
- Oracle: `benchmarks/tests/test_embed_sql.py` (188 LoC, 8 `test_` functions) — `:79-82` (len==384 + not-all-zeros), `:85+` (semantic), typed-error tests.
- Stub: `tools/embedding_server.py:1-9` (real 384-dim BAAI/bge-small model, deterministic).

#### Files to edit
```
benchmarks/tests/test_embed_sql.py — NO logic change (oracle is fixed). Only touch if the connection fixture needs the m17 image tag / --add-host for the stub (a fixture/env tweak, not an assertion change).
```

#### Deep file dependency analysis
- The oracle (Baseline row) is intentionally unchanged — rewriting it would lose the independent cross-language check. Only the harness wiring (image tag, `--add-host=host.docker.internal:host-gateway`, `PGHOST/PGPORT`) may change, and only if required to reach the rebuilt container + stub.

#### Deep Dives
- **Invariant:** the 8 assertions must pass with ZERO changes to their logic. A change to an assertion would mean the contract changed — forbidden (public API stable).
- **Edge cases:** the typed-error tests (endpoint unset / non-http → 22023) must pass against the Rust mapping from Phase 1.

#### Tasks
1. Start `tools/embedding_server.py` on the host.
2. `docker run` `theo-db:m17` with `--add-host=host.docker.internal:host-gateway`, set the `theodb.embedding_endpoint` GUC to the stub.
3. `python3 -m pytest benchmarks/tests/test_embed_sql.py -v` against the container.
4. Confirm 8/8 green; capture output as evidence.

#### TDD
```
RED:     (the suite itself is RED until the Rust embed works end-to-end against the image)
RED:     test_embed_empty_content_matches_plpython (EC-2) — capture the plpython3u behavior for theodb.embed('') BEFORE Phase 2 removes it (vector vs 22023), then assert the Rust impl matches the SAME behavior (parity — do NOT invent new behavior for empty input)
GREEN:   8/8 test_embed_sql.py tests pass against theo-db:m17 + the empty-content parity test green
REFACTOR: None (oracle frozen)
VERIFY:  python3 -m pytest benchmarks/tests/test_embed_sql.py -v
```

#### Concurrency tests (only when applicable)

(none — single-threaded) — the embed function is a synchronous per-call SQL function; PostgreSQL serializes the call within a backend, no shared mutable state, no locks/async/threads.

#### Acceptance Criteria
- [ ] `benchmarks/tests/test_embed_sql.py` 8/8 green against `theo-db:m17` (Rust embed).
- [ ] No assertion logic changed (only fixture/env wiring if needed).
- [ ] The 22023 typed-error tests pass against the Rust mapping, verified by `python3 -m pytest benchmarks/tests/test_embed_sql.py -k error` exiting 0.

#### DoD (Definition of Done)
- [ ] Full `test_embed_sql.py` green; output captured.
- [ ] `cargo pgrx test` green (the Phase 1 `#[pg_test]` runs in CI/builder).

---

## Phase 4: latency benchmark (Rust vs plpython3u)

**Objective:** reproducible measured evidence — the CTO requirement.

### T4.1 — Benchmark embed latency Rust vs plpython3u and write the report

#### Objective
Measure N `SELECT theodb.embed('…')` calls against the same `tools/embedding_server.py` stub for the Rust impl and the plpython3u impl, ≥3 runs each, report mean±std → `docs/benchmarks/m17-embed-rust-vs-plpython.md`, NO perf claim beyond the numbers.

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — runs a reproducible micro-bench (same stub, same N, ≥3 runs) for both implementations and commits a report with methodology + mean±std + the honest I/O-bound caveat.
2. **Why it is necessary now** — the CTO mandate is "DEVE TER DADOS EM BENCHMARK"; D4 makes the benchmark a gate. It runs after parity (Phase 3) because benchmarking a non-equivalent function would be meaningless. Cites D4 + ADR 0002 + public-copy.md + blueprint T3 lines 108-113.

#### Evidence
- Benchmark spec: Blueprint §"T3" lines 108-113 (N≈200 calls, same stub, mean±std, ≥3 runs, no perf claim, I/O-bound caveat).
- public-copy: `.claude/rules/public-copy.md § 4` (comparative claim needs reproducible artifact) — the report IS the artifact; the prose states no claim.

#### Files to edit
```
benchmarks/bench_embed.py (NEW) — harness: connect to a container, time N theodb.embed calls, ≥3 runs, compute mean±std; parametrized over impl (Rust image vs a plpython3u image/sql) against the SAME stub; emit a markdown table
docs/benchmarks/m17-embed-rust-vs-plpython.md (NEW) — methodology (hardware, N, runs, stub, PG version), results table (mean±std for Rust vs plpython3u), explicit "embed is I/O-bound; this documents no regression, not a speedup" note
CHANGELOG.md — note the benchmark + the Rust embed rewrite under [Unreleased]
```

#### Deep file dependency analysis
- `bench_embed.py` is NEW and standalone (reuses the stub + a psycopg connection like the oracle). It does NOT import the oracle's assertions. The report is documentation. For the plpython3u baseline, run against the pre-M17 image (e.g. `theo-db:m16`) or a temporarily-kept plpython3u definition; record exactly which baseline was used (reproducibility).

#### Deep Dives
- **Methodology (PhD-rigor, analysis-golden-rule.md style):** report N, runs, mean±std, PG version, hardware, stub model — every number has units + method. ≥3 runs. The stub is deterministic (same vector), so latency is the only variable.
- **Invariant (honesty):** NO claim beyond the measured numbers; explicitly state the I/O-bound caveat (the stub dominates wall-clock). No "Nx faster" anywhere (public-copy.md).
- **Edge case:** if the Rust path is measurably slower (e.g., cold connection), report it honestly and investigate; a regression is a finding, not something to hide.

#### Pseudo-code / Signatures
```python
def bench(conn, n=200, runs=3) -> dict:
    samples = []
    for _ in range(runs):
        t0 = time.perf_counter()
        for _ in range(n):
            conn.execute("SELECT theodb.embed('benchmark text')")
        samples.append((time.perf_counter() - t0) / n)   # per-call seconds
    return {"mean": statistics.mean(samples), "std": statistics.pstdev(samples), "n": n, "runs": runs}
# report: rust = bench(rust_conn); py = bench(py_conn); write markdown table; NO claim beyond numbers
```

#### Tasks
1. Write `benchmarks/bench_embed.py` (N calls, ≥3 runs, mean±std, both impls, same stub).
2. Run the Rust impl (`theo-db:m17`) and the plpython3u baseline (record which image/definition).
3. Write `docs/benchmarks/m17-embed-rust-vs-plpython.md` (methodology + table + I/O-bound caveat, no claim).
4. Update CHANGELOG `[Unreleased]`.

#### TDD
```
RED:     test_bench_reports_mean_std — bench_embed.bench() returns a dict with mean/std/n/runs for a stub conn (unit test with a fake conn, no real DB needed)
GREEN:   Implement bench() so the test passes
REFACTOR: None expected
VERIFY:  python3 -m pytest benchmarks/tests/test_bench_embed.py -v  (unit) + manual run producing the report
```

#### Concurrency tests (only when applicable)
(none — single-threaded) — benchmark issues calls serially by design (measuring per-call latency, not concurrency).

#### Acceptance Criteria
- [ ] `docs/benchmarks/m17-embed-rust-vs-plpython.md` exists with methodology + mean±std for both impls (≥3 runs, N documented).
- [ ] No perf claim in prose beyond the measured numbers; the I/O-bound caveat is stated.
- [ ] `bench_embed.py` is reproducible (documented command).
- [ ] Pass: lint — `ruff` clean on `bench_embed.py`.

#### DoD (Definition of Done)
- [ ] Benchmark report committed; numbers present.
- [ ] CHANGELOG updated.
- [ ] No public-copy violation (`hooks/public-copy-lint.sh` clean on the report — it's under `docs/benchmarks/`, technical-direct).

---

## Phase 5: deps-audit + Integration Validation

**Objective:** CVE/license gate on the HTTP crate, then validate the full chain.

### T5.1 — /deps-audit the HTTP crate + record license

#### Objective
Run `/deps-audit` on the chosen HTTP crate (minreq or ureq) **and the `pgvector` crate (D5)** + their transitive deps (CVE), and record the licenses (ISC/MIT/Apache) in the deps gate.

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — audits the two new dependencies (HTTP crate + `pgvector` crate from D5) for CVEs and confirms their licenses are D1-permissive (ISC/MIT/Apache), recording them.
2. **Why it is necessary now** — Rule 9 + D1: a new dep enters the Apache distribution only after a CVE + license check; this is the `cycle-plan` deps-audit gate. Cites blueprint Recommendation 6 + `deps-audit-golden-rule.md`.

#### Evidence
- D1 license gate: `theo-db/CLAUDE.md` rule 2 (only Apache/MIT/BSD/PostgreSQL/ISC). minreq=ISC, ureq=MIT/Apache (blueprint Corner 2).
- Deps gate: `.claude/rules/deps-audit-golden-rule.md` (CVE severity → verdict).

#### Files to edit
```
theodb_rs/Cargo.toml — the dep is already pinned (Phase 1); this task verifies it
theodb_rs/Cargo.lock (NEW) — committed for reproducibility (pinned transitive deps)
CHANGELOG.md — record the new dependency + license under [Unreleased]
```

#### Deep file dependency analysis
- `Cargo.lock` is NEW + committed (reproducible build, like pgvectorscale's pinned build). The audit reads it. No code change.

#### Tasks
1. Run `cargo audit` (or `/deps-audit`) on `theodb_rs`.
2. Confirm no HIGH/CRITICAL CVE on the HTTP crate AND the `pgvector` crate (D5) (else block per golden rule).
3. Record the licenses (HTTP: ISC/MIT; `pgvector`: MIT) in CHANGELOG / deps note.
4. Commit `Cargo.lock`.

#### TDD
```
RED:     (audit fails the gate if a HIGH/CRITICAL CVE is present)
GREEN:   cargo audit clean; license recorded
REFACTOR: None
VERIFY:  cargo audit  (in the builder) — zero HIGH/CRITICAL
```

#### Concurrency tests (only when applicable)

(none — single-threaded) — the embed function is a synchronous per-call SQL function; PostgreSQL serializes the call within a backend, no shared mutable state, no locks/async/threads.

#### Acceptance Criteria
- [ ] No HIGH/CRITICAL CVE on the HTTP crate, the `pgvector` crate (D5), or their transitive deps.
- [ ] Licenses (HTTP ISC/MIT; `pgvector` MIT) recorded — D1 compliant.
- [ ] `Cargo.lock` committed.

#### DoD (Definition of Done)
- [ ] `cargo audit` clean.
- [ ] License recorded in CHANGELOG.

---

## Coverage Matrix

| # | Gap / Requirement (ROADMAP-v2 M17 DoD + blueprint Rec) | Task(s) | Resolution |
|---|---|---|---|
| 1 | Create the `theodb_rs` pgrx crate (pgrx=0.16.1, pg17) with `#[pg_schema] mod theodb` + `#[pg_extern] fn embed` (Rec 1) | T0.1 | Crate skeleton + generated `theodb.embed` SQL |
| 2 | `embed` HTTP via minimal audited crate + SSRF + 22023 typed errors (Rec 2) | T1.1 | Real POST + SSRF guard + error mapping at parity with sql/30 |
| 3 | Remove `theodb.embed` from sql/30 + Docker builder stage + CREATE EXTENSION (Rec 3) | T2.1 | sql/30 edited; second builder stage; extension installed |
| 4 | Parity: `test_embed_sql.py` green + a `#[pg_test]` (Rec 4) | T1.1 (#[pg_test]), T3.1 (oracle) | 8/8 oracle green against Rust image + Rust SSRF/error test |
| 5 | Latency benchmark Rust vs plpython3u → docs/benchmarks, no claim (Rec 5) | T4.1 | Reproducible harness + report with mean±std + I/O-bound caveat |
| 6 | `/deps-audit` on the HTTP crate + license (Rec 6) | T5.1 | CVE clean + ISC/MIT recorded + Cargo.lock pinned |
| 7 | Coexistence (no duplicate `theodb.embed`) (D1 / EC-2) | T2.1 | Separate `theodb_rs` ext; sql/30 no longer defines embed |
| 8 | Least-privilege parity (REVOKE from PUBLIC) | T2.1 | REVOKE re-applied for the Rust function |
| 9 | `vector` return-type binding pinned (edge-case EC-1) | T0.1, T1.1, T5.1 (D5) | `pgvector` crate (MIT) `Vector` type → native SQL `vector`; audited |
| 10 | empty-content + 4xx parity (edge-cases EC-2, EC-3) | T3.1, T1.1 | Empty-content matches plpython3u baseline; 4xx → 22023 |

**Coverage: 10/10 gaps covered (100%)**

## Global Definition of Done

- [ ] All phases completed.
- [ ] All tests passing — `python3 -m pytest benchmarks/tests/test_embed_sql.py` green + `cargo pgrx test` green.
- [ ] Zero lint warnings — `cargo clippy` + `ruff` on new Python.
- [ ] File-size budget respected (every Rust/Python file ≤ 500 lines).
- [ ] CHANGELOG.md updated under `[Unreleased]` (Unbreakable Rule 6).
- [ ] Backward compatibility preserved — `theodb.embed(text,text)` signature + SQLSTATE 22023 contract unchanged (public API).
- [ ] Benchmark report committed to `docs/benchmarks/m17-embed-rust-vs-plpython.md` with measured numbers + no perf claim.
- [ ] `/deps-audit` clean on the HTTP crate; license D1-compliant; `Cargo.lock` committed.
- [ ] **Runtime-metric proof** — `theodb.embed` served by `theodb_rs` is observed returning a real 384-dim vector in the integration workload (the oracle), not just compiled.
- [ ] **Plan archived** — after `/review` returns `READY_TO_MERGE` AND the PR is merged, move this plan to `knowledge-base/plans/completed/`.

## Failure scenarios (external I/O — the embeddings HTTP endpoint)

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| embeddings endpoint (HTTP, via `theodb.embedding_endpoint` GUC) | endpoint unset / NULL GUC | `test_embed_sql.py` unsets the GUC | ERROR SQLSTATE 22023 "endpoint not configured" (no crash, no partial) |
| embeddings endpoint (HTTP) | non-http(s) scheme (SSRF attempt, e.g. `ftp://`, `file://`) | `#[pg_test]` + oracle set a non-http endpoint | ERROR SQLSTATE 22023; request never sent |
| embeddings endpoint (HTTP) | redirect (3xx) to an internal address (SSRF-via-redirect) | `#[pg_test]` with a stub returning 302 | redirect NOT followed → 22023 (D2 — minreq no-redirect or ureq fallback) |
| embeddings endpoint (HTTP) | connect error / timeout | point the GUC at an unreachable host with a short timeout | ERROR SQLSTATE 22023 within the timeout; no hang |
| embeddings endpoint (HTTP) | 4xx (400/413 — bad/oversized input, token limit) | `#[pg_test]` + stub returns 400/413 (EC-3) | ERROR SQLSTATE 22023 with a clear message; the non-2xx path covers 4xx, not only 5xx |
| embeddings endpoint (HTTP) | malformed / empty response body | stub returns non-JSON or empty `data` | ERROR SQLSTATE 22023 "bad embedding response"; no empty vector returned |

## Final Phase: Integration Validation (MANDATORY)

> Runs AFTER all phases. The plan is NOT done until this chain passes.

**Objective:** validate the Rust `theodb.embed` works in a real workload against the rebuilt image.

### Execution
```
docker build -t theo-db:m17 .                                   # builds both extensions (D3)
docker run ... theo-db:m17 (with --add-host + GUCs to the stub) # init creates theodb + theodb_rs
python3 -m pytest benchmarks/tests/test_embed_sql.py -v         # parity oracle 8/8 (the Goal metric)
cargo pgrx test (in builder)                                    # Rust SSRF/error #[pg_test]
python3 -m pytest benchmarks/tests/test_bench_embed.py -v       # benchmark harness unit test
cargo clippy --all-targets                                      # zero warnings
ruff check benchmarks/                                          # zero warnings
cargo audit                                                     # zero HIGH/CRITICAL
```

### Acceptance Criteria
- [ ] `test_embed_sql.py` 8/8 green against `theo-db:m17` (Rust embed) — the Goal metric.
- [ ] `cargo pgrx test` green (SSRF reject + error mapping).
- [ ] The benchmark report `docs/benchmarks/m17-embed-rust-vs-plpython.md` contains a mean±std results table over ≥3 runs + the I/O-bound caveat, verified by `grep -Eq 'mean|std' docs/benchmarks/m17-embed-rust-vs-plpython.md` exiting 0.
- [ ] `cargo clippy` + `ruff` clean; `cargo audit` clean.
- [ ] Every `## Failure scenarios` row is exercised (22023 typed errors, no redirect, no hang, no empty vector), verified by `python3 -m pytest benchmarks/tests/test_embed_sql.py -v` exiting 0.
- [ ] Both extensions install on a fresh DB (no duplicate `theodb.embed`).

### If Validation Fails
1. Separate plan-caused failures from pre-existing.
2. Fix all plan-caused failures before declaring complete.
3. Re-run the chain.
4. Log pre-existing issues in the PR description (do not block on them).
