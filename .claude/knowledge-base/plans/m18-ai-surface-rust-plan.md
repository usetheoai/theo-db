---
slug: m18-ai-surface-rust
milestone_id: M18
created_at: 2026-06-30
goal: Rewrite the 5 plpython3u ai.* generative functions to Rust/pgrx at proven parity, measured by the full ai suite (`benchmarks/tests/test_ai_sql.py` 36/36 green against the rebuilt image) plus a chat Rust-vs-plpython3u latency benchmark, with zero plpython3u remaining in the AI layer.
---

# Plan: M18 — `ai.*` generative surface plpython3u → Rust/pgrx

## Goal

> "Port `ai._chat` + `ai.if` + `ai.analyze_sentiment` + `ai.rank` + `ai.generate_batch` from plpython3u to
> Rust/pgrx at proven parity, measured by (a) `benchmarks/tests/test_ai_sql.py` 36/36 green against the rebuilt
> image AND (b) `docs/benchmarks/m18-ai-rust-vs-plpython.md` showing the Rust `ai._chat` at no-regression vs
> the plpython3u baseline (mean±std, ≥3 runs), with **zero `LANGUAGE plpython3u` left in `sql/50`**."

## Context

ROADMAP-v2 M18 (depends on M17, done). The audit-remediation slice (v0.17.0) already added a bounded
recoverable-class retry to the plpython3u `ai._chat`; M18 moves the whole generative surface to Rust, reusing
the M17 HTTP client (`theodb_rs/src/embed.rs`). The behavioral spec is our own plpython3u source (the port is
internal — blueprint `.claude/knowledge-base/discoveries/blueprints/m18-ai-surface-rust-blueprint.md`); the
parity gate is the 36-test oracle + the deterministic stub `tools/chat_server.py`. The aggregate
`ai.agg_summarize` is SQL-based (NOT plpython3u) — no pgrx aggregate is needed (Rule 9 — reuse what exists).

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `theodb_rs/src/embed.rs` | ~250 | `6ca19a7` | Domain: `run`/`run_batch` + the HTTP client (`post_json`, retry, SSRF) | the HTTP core is extracted to `http.rs`; `run`/`run_batch` behavior unchanged; embed 31-test oracle stays green |
| `theodb_rs/src/http.rs` (NEW) | 0 | — | shared HTTP client: `post_json` (noun-neutral) + `is_recoverable_status` + `backoff` + `MAX_RETRIES` + `truncate` | reused by embed + chat (DRY) |
| `theodb_rs/src/chat.rs` (NEW) | 0 | — | Domain: `chat(prompt,system,model)` + `resolve_chat_cfg` + 4 task parsers (if/sentiment/rank/batch) | byte-identical system prompts; parse parity; typed 22023/38000 |
| `theodb_rs/src/pg.rs` | ~43 | `6ca19a7` | glue: err_input/err_external/warn/guc | reused as-is; errcontext parameterized (ADR-C) |
| `theodb_rs/src/lib.rs` | ~175 | `6ca19a7` | api-surface: `#[pg_extern]` + `extension_sql!` wrappers + #[pg_test] | add `chat` module + the `ai.*` externs + `extension_sql!` creating `ai._chat`/`ai.if`/`ai.analyze_sentiment`/`ai.rank`/`ai.generate_batch` into the `ai` schema (REVOKE); embed wrappers unchanged |
| `sql/50-theodb-ai.sql` | ~341 | audit-rem | the AI surface (5 plpython3u + the SQL keepers) | REMOVE the 5 plpython3u defs; change `ai.generate`/`ai.summarize`/`ai._agg_summ_final` from LANGUAGE sql → plpgsql (late-bound to the now-theodb_rs `ai._chat`); keep the aggregate + accum |
| `sql/theodb--1.1--1.2.sql` (NEW) | 0 | — | retirement migration: conditional DROP of the 5 plpython3u `ai.*` (only when plpython3u AND not theodb_rs-owned) | mirror the M17 `theodb--1.0--1.1.sql` guard |
| `theodb.control` | 5 | v0.17.0 | umbrella control, `default_version='1.1'` | bump `default_version` to `1.2` |
| `benchmarks/tests/test_ai_sql.py` | — | M11 | the 36-test parity oracle | stays green UNCHANGED (the contract) |
| `benchmarks/bench_chat.py` (NEW) | 0 | — | chat Rust-vs-plpython3u latency harness | — |
| `docs/benchmarks/m18-ai-rust-vs-plpython.md` (NEW) | 0 | — | benchmark report (no-regression) | — |
| `tools/chat_server.py` | ~250 | M11 | deterministic chat stub (routes on system prompts) | UNCHANGED — the Rust prompts must match its routing |
| `CHANGELOG.md` | — | — | public contract | `[Unreleased]` updated |

### Current callers / dependents

- `ai._chat(text,text,text)` — called by `ai.generate`/`ai.summarize` (sql/50, LANGUAGE sql), `ai._agg_summ_final` (sql/50), and (M19) `ai.nl_to_sql`/`nl_query` (sql/60, plpgsql via SPI). **The ported `ai._chat` MUST remain a SQL-callable function named `ai._chat(text,text,text) RETURNS text`** so every caller keeps working.
- The 4 task wrappers (`if`/`analyze_sentiment`/`rank`/`generate_batch`) call `ai._chat` via SPI today; in Rust they call the domain `chat()` directly (ADR-B) while the public `ai._chat` SQL wrapper still exists for SQL callers.
- `ai.generate`/`ai.summarize`/`ai._agg_summ_final` are LANGUAGE sql whose bodies reference `ai._chat`; once `ai._chat` moves to `theodb_rs` (created AFTER `theodb` via CASCADE), a LANGUAGE sql body would fail at CREATE (bodies are validated when `check_function_bodies` is on). They MUST become plpgsql (late-bound; bodies not validated at create) — ADR-D.
- The plpython3u `ai.*` shipped through v0.17.0 as `theodb` members → existing installs own them → retirement migration on 1.1→1.2 (ADR-E), mirroring M17's embed retirement.

### Domain glossary

- **chat client** — one POST to `theodb.llm_endpoint` with `{model, messages:[{role,content}]}`, parsing `choices[0].message.content` (vs embed's `data[].embedding`).
- **late-bound** — a plpgsql function body is NOT validated against referenced functions at CREATE time (only at first call), unlike LANGUAGE sql; this is what lets a `theodb` function reference a `theodb_rs` function created later.
- **byte-identical system prompt** — the 5 plpython3u system/user prompt strings; the stub `chat_server.py` routes replies by matching substrings of them, so any drift breaks every offline test.
- **retirement migration** — the `ALTER EXTENSION theodb UPDATE` step that conditionally DROPs the legacy plpython3u `ai.*` so `theodb_rs` can own them without a duplicate-definition clash.

### Architecture boundaries affected

Per `.claude/rules/architecture.md`: the 3-boundary layering (M17 blueprint ADR-1) extends — `http.rs` + `chat.rs` are domain (portable logic), `pg.rs` is glue, `lib.rs` is api-surface. The chat retry/SSRF live in the infrastructure adapter (`http.rs`) — `error-handling.md` recoverable-class discipline. No new dependency-direction violation; no new crate (reuse `minreq`/`serde_json`/`pgrx`).

## Prior Art & Related Work

- **Internal blueprint:** `.claude/knowledge-base/discoveries/blueprints/m18-ai-surface-rust-blueprint.md` (the M18 port map — patterns P1–P5, ADR seeds).
- **M17 reference implementation:** `theodb_rs/src/embed.rs` (the HTTP client + retry + SSRF this port generalizes) + `docs/benchmarks/m17-embed-rust-vs-plpython.md` (the no-regression benchmark pattern).
- **Behavioral spec to preserve:** `sql/50-theodb-ai.sql:16-296` (the 5 plpython3u bodies — verbatim system prompts + parse + error specs in the blueprint).
- **Parity oracle:** `benchmarks/tests/test_ai_sql.py` (36 tests) + `tools/chat_server.py`.
- No external/AGPL code consulted — internal port (Rule 9).

## Objective

- [ ] `ai._chat`, `ai.if`, `ai.analyze_sentiment`, `ai.rank`, `ai.generate_batch` are Rust/pgrx (served by `theodb_rs`), at parity (the 36-test oracle green, UNCHANGED).
- [ ] The shared HTTP client is extracted to `http.rs` and reused by embed + chat (DRY); the embed 31-test oracle stays green.
- [ ] `REVOKE … FROM PUBLIC` + SSRF/no-redirect/fail-fast/api_key-no-leak preserved on every ported `ai.*`.
- [ ] Zero `LANGUAGE plpython3u` remains in `sql/50`; the `ai.generate`/`summarize`/aggregate keepers still work (late-bound).
- [ ] The 1.1→1.2 upgrade conditionally retires the plpython3u `ai.*`; no duplicate-definition clash on upgrade + add theodb_rs; `default_version`=1.2.
- [ ] `docs/benchmarks/m18-ai-rust-vs-plpython.md` shows the Rust chat path at no-regression vs plpython3u (mean±std, ≥3 runs).

## ADRs

### D1 — `chat.rs` domain module mirroring `embed.rs`; wrappers call `chat()` directly
**Decision:** add `theodb_rs::chat::chat(prompt, system, model) -> String` (one POST, parse `choices[0].message.content`) + 4 parsers (`parse_bool`/`parse_sentiment`/`parse_rank`/`parse_batch`); the `#[pg_extern]`s for `ai.if`/`analyze_sentiment`/`rank`/`generate_batch` call `chat()` directly, not via SPI.
**Rationale:** mirrors M17's `embed.rs` layering (P1); calling `chat()` directly avoids the SPI round-trip the plpython3u version paid (the wrappers did `plpy.prepare("SELECT ai._chat(...)")`), at identical behavior (same prompt + parse). Parity preserved; the stub round-trip-count tests still hold (batch = ONE `chat()` call).
**Alternatives considered:** (a) wrappers call the SQL `ai._chat` via SPI from Rust — rejected: needless SPI hop + couples to the SQL wrapper; (b) keep wrappers in plpython3u — rejected: that is the thing M18 removes.
**Consequences:** the public `ai._chat` SQL wrapper still exists (SQL callers + M19 nl), AND the Rust wrappers share the same `chat()`.

### D2 — Generalize the HTTP core into `http.rs` (DRY); reuse the embed client
**Decision:** extract `post_json(fn_name, endpoint, payload, api_key) -> Value` + `is_recoverable_status` + `backoff` + `MAX_RETRIES` + `truncate` from `embed.rs` into `theodb_rs/src/http.rs` (noun-neutral message `"{fn_name}: endpoint call failed: …"`); both `embed.rs` and `chat.rs` use it.
**Rationale:** the retry policy ({429,502,503}+connect/timeout, MAX_RETRIES=2, backoff) is IDENTICAL between embed and the plpython3u `ai._chat` (`error-handling.md §2`); duplicating it is the inverse of parsimony (Rule 9). The only embed-specific string ("embedding endpoint") becomes noun-neutral — the oracles assert only the `"call failed"` substring, so both stay green.
**Alternatives considered:** (a) a separate chat `post_json` — rejected: duplicates the SSRF/retry core; (b) leave it in embed.rs and have chat import it — rejected: embed.rs is the embed domain, not a shared-client home (SRP).
**Consequences:** one HTTP client; a change to retry/SSRF lands in one place; embed.rs shrinks to its domain.

### D3 — Byte-identical system prompts + verbatim parse parity
**Decision:** the 5 system/user prompts are copied verbatim from the plpython3u source; the parsers replicate the exact logic (first-token regex for bool/sentiment, first-float search for rank, markdown-fence-strip + JSON-array for batch incl. JSON-null→SQL-NULL).
**Rationale:** the stub routes on the system prompts; the oracle asserts exact error substrings + SQLSTATE. Drift breaks parity. This is the load-bearing parity contract (P3/P4).
**Alternatives considered:** (a) "improve" the prompts — rejected: breaks the stub + changes model behavior (not a port); (b) looser parsing — rejected: the oracle pins the exact error classes.
**Consequences:** parse-parity is testable against the 36-test oracle with no test changes.

### D4 — `ai.generate`/`summarize`/`_agg_summ_final`: LANGUAGE sql → plpgsql (late-bound)
**Decision:** change these three theodb keepers from LANGUAGE sql to LANGUAGE plpgsql (same VOLATILE, same body `RETURN ai._chat(...)`), so their bodies are not validated against `ai._chat` at CREATE (it now lives in theodb_rs, created after theodb).
**Rationale:** LANGUAGE sql bodies are validated at CREATE (`check_function_bodies` on) → would fail referencing the not-yet-created `theodb_rs` `ai._chat`; plpgsql is late-bound. Minimal change (3 thin wrappers), behavior-identical. The aggregate references `_agg_summ_accum`/`_agg_summ_final` (both still exist at CREATE AGGREGATE) — unaffected.
**Alternatives considered:** (a) move generate/summarize/aggregate into theodb_rs too — rejected: larger surface move, and the aggregate + accum would have to move; (b) keep LANGUAGE sql — rejected: breaks CREATE ordering across extensions.
**Consequences:** `_agg_summ_final` stays VOLATILE (oracle asserts 'v'); generate/summarize behavior unchanged.

### D5 — Conditional retirement DROP in 1.1→1.2; bump default_version to 1.2
**Decision:** `theodb--1.1--1.2.sql` DROPs each of the 5 plpython3u `ai.*` ONLY when it exists AND is `LANGUAGE plpython3u` AND is NOT a `theodb_rs` member (pg_proc.prolang + pg_depend, deptype 'e'); bump `theodb.control default_version` to 1.2.
**Rationale:** the plpython3u `ai.*` shipped through v0.17.0 as theodb members; adding the theodb_rs `ai.*` would clash. Mirrors the M17 `theodb--1.0--1.1.sql` retirement idiom (proven). Fresh 1.2 installs no-op (sql/50 no longer defines them).
**Alternatives considered:** (a) no-op migration — rejected: real clash for existing installs; (b) unconditional DROP — rejected: fails when theodb_rs owns them.
**Consequences:** existing v0.x installs upgrade then add theodb_rs without a clash; fresh installs at 1.2 never had the plpython3u `ai.*`.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Extracting the HTTP core to `http.rs` could regress the embed path | High | The embed 31-test oracle (test_embed_sql + failure_scenarios + embed_batch) MUST stay green post-extraction; run it in Integration Validation | maintainers |
| System-prompt drift breaks the stub routing → every offline ai test fails | High | Copy the 5 prompts verbatim; a `#[pg_test]`/grep asserts the Rust constants equal the documented strings; the 36-test oracle catches drift | maintainers |
| Cross-extension CREATE ordering (theodb sql callers vs theodb_rs ai._chat) | High | D4 (plpgsql late-bound); Integration Validation runs `test_extension_install.py` (fresh CREATE) | maintainers |
| Retirement DROP wrong → drops theodb_rs's ai.* OR fails upgrade | High | Guard on prolang=plpython3u AND NOT theodb_rs-member; test BOTH the upgrade path and the fresh path (real ALTER EXTENSION UPDATE) | maintainers |
| `ai.rank` returns `real` (f32) — Python float (f64) → real cast parity | Medium | pgrx `_ai_rank` returns `f32`; parse to f64 then cast; oracle asserts the [0,1] stub value 0.8 | maintainers |
| generate_batch JSON-null → SQL NULL element contract differs from embed_batch (which rejects NULL) | Medium | `parse_batch` preserves JSON null as `None`; the oracle has no null-element-out test but the plpython3u did preserve — keep parity | maintainers |

## Unresolved Questions

- Q1 — Does `_agg_summ_final` need to become plpgsql too (it's LANGUAGE sql calling ai._chat)? (Resolved at D4: yes — it is one of the three keepers changed to plpgsql; the aggregate definition itself is unaffected.)
- Q2 — Keep the plpython3u `ai._chat` as `ai._chat_py` for the benchmark (like M17's `theodb.embed_py`)? (Resolved at Phase 3: yes — created in the benchmark setup only, not shipped, so the Rust-vs-plpython3u comparison is apples-to-apples in one container.)

## Dependencies

(none — NO new dependency. The chat client reuses `minreq`/`serde_json`/`pgrx` already in `theodb_rs/Cargo.toml`; the retry/backoff is stdlib (reused from embed); JSON parse is `serde_json`; the keepers + retirement migration are pure SQL/plpgsql. `/deps-audit` has no new declared dep to scan.)

## Dependency Graph

```
Phase 0 (http.rs extraction) ──▶ Phase 1 (chat.rs + Rust ai.* wrappers) ──┐
                                                                          ├──▶ Phase 3 (benchmark)
Phase 2 (theodb side: plpgsql keepers + retirement migration) ────────────┤
                                                                          └──▶ Phase 4 (Integration Validation)
```
Phase 0 before Phase 1 (chat depends on the shared client). Phase 2 is independent of 0/1 (different files: sql/50, theodb--1.1--1.2.sql, theodb.control) but its runtime correctness needs Phase 1's ai._chat present (validated together in Phase 4). Phase 3 + 4 run last.

---

## Phase 0: Extract the shared HTTP client to `http.rs`

### T0.1 — Generalize `post_json` + retry into `theodb_rs/src/http.rs`

#### Objective
Move the HTTP send + bounded retry + SSRF + parse core out of `embed.rs` into a noun-neutral `http.rs`, reused by embed (now) and chat (Phase 1), with the embed oracle staying green.

#### Why this step (action + reasoning)
1. **What:** create `theodb_rs/src/http.rs` with `pub(crate) fn post_json(fn_name, endpoint, payload, api_key) -> serde_json::Value` + `is_recoverable_status` + `backoff` + `MAX_RETRIES` + `truncate`; message noun-neutral (`"{fn_name}: endpoint call failed: …"`). Re-point `embed.rs` to `crate::http`.
2. **Why now:** chat needs the identical client (D2); extracting first keeps DRY and isolates the risk to one verified-green step. Cites blueprint P2 + embed.rs.

#### Files to edit
```
theodb_rs/src/http.rs (NEW) — post_json + is_recoverable_status + backoff + MAX_RETRIES + truncate (moved from embed.rs, noun-neutral)
theodb_rs/src/embed.rs — remove the moved items; `use crate::http::{post_json, truncate}`; run/run_batch call crate::http::post_json
theodb_rs/src/lib.rs — add `mod http;`
```

#### Deep file dependency analysis
- `embed.rs` currently owns `post_json`/`is_recoverable_status`/`backoff`/`MAX_RETRIES`/`truncate`. Moving them to `http.rs` and importing keeps `run`/`run_batch` behavior identical. The only message change: `"embedding endpoint call failed"` → `"endpoint call failed"` — the embed failure oracle asserts only the `"call failed"` substring, so it stays green.
- Downstream: chat.rs (Phase 1) imports `crate::http::post_json`.

#### TDD
```
RED:    (embed regression) the existing 31-test embed oracle must stay green after the move — run test_embed_sql.py + test_embed_failure_scenarios.py + test_embed_batch.py against the rebuilt image
GREEN:  extract http.rs; re-point embed.rs
VERIFY: cargo clippy -- -D warnings ; embed oracle 31/31 green vs the rebuilt image
```

#### Concurrency tests (only)
(none — single-threaded) — synchronous HTTP, no shared mutable state.

#### Failure-scenario note
External HTTP — covered in `## Failure scenarios` (the embed failure oracle re-run proves the moved client preserves connect/timeout/5xx/non-JSON → 38000 + retry).

#### Acceptance Criteria
- [ ] `theodb_rs/src/http.rs` exists with the shared client; `embed.rs` imports it (no duplicated HTTP/retry code) — verified by `grep -q 'use crate::http' theodb_rs/src/embed.rs` and absence of a second `minreq::post` retry loop in embed.rs.
- [ ] The embed oracle stays green UNCHANGED — asserted by `pytest benchmarks/tests/test_embed_sql.py benchmarks/tests/test_embed_failure_scenarios.py benchmarks/tests/test_embed_batch.py` (exit 0) against the rebuilt image.
- [ ] Quality gates pass — `cargo clippy --features pg17 -- -D warnings` exits 0.

#### DoD
- [ ] `http.rs` is the single HTTP client; embed oracle 31/31 green; clippy clean.

---

## Phase 1: `chat.rs` domain + the 5 Rust `ai.*` functions

### T1.1 — `chat()` + 4 parsers + `#[pg_extern]`s + `extension_sql!` wrappers

#### Objective
Implement the chat client + the 4 task parsers in Rust at parity, expose `ai._chat`/`ai.if`/`ai.analyze_sentiment`/`ai.rank`/`ai.generate_batch` via `theodb_rs` into the `ai` schema (REVOKE).

#### Why this step (action + reasoning)
1. **What:** `theodb_rs/src/chat.rs` — `resolve_chat_cfg` (llm_* GUCs, http(s) SSRF guard), `chat(prompt,system,model) -> String` (build `{model,messages}`, `crate::http::post_json`, parse `choices[0].message.content`, empty→38000), and `parse_bool`/`parse_sentiment`/`parse_rank`/`parse_batch` (verbatim logic). `lib.rs` — `#[pg_extern]` `_chat`/`_ai_if`/`_ai_sentiment`/`_ai_rank`/`_generate_batch_text` + `extension_sql!` creating the public `ai.*` (REVOKE FROM PUBLIC).
2. **Why now:** the core of M18; depends on Phase 0. Cites blueprint P3/P4 + sql/50 verbatim specs + D1/D3.

#### Files to edit
```
theodb_rs/src/chat.rs (NEW) — resolve_chat_cfg + chat() + parse_bool/parse_sentiment/parse_rank/parse_batch (byte-identical prompts; typed 22023/38000)
theodb_rs/src/lib.rs — mod chat; #[pg_extern] _chat (-> String), _ai_if (-> bool), _ai_sentiment (-> String), _ai_rank (-> f32), _generate_batch_text (Vec<Option<String>> -> Vec<Option<String>>); extension_sql! creating ai._chat/ai.if/ai.analyze_sentiment/ai.rank/ai.generate_batch into schema ai + REVOKE
theodb_rs/src/pg.rs — parameterize the err errcontext (ADR-C) so ai.* errors don't report 'theodb.embed' context
```

#### Deep Dives
- **System prompts (verbatim, D3):** `ai.if`=`"Answer with exactly one word: yes or no."`; `ai.analyze_sentiment`=`"Classify the sentiment of the text. Reply with exactly one of: positive, negative, neutral."`; `ai.rank`=`"Reply with a single number between 0 and 1 and nothing else."`; `ai.generate_batch` user=`"\n".join("%d. %s"%(i+1,p))`, system=`"You are given %d numbered items. Respond with ONLY a JSON array of exactly %d strings — the answer to each item, in order. No prose, no markdown."%(n,n)`.
- **Parsers (verbatim):** bool first-token `[^a-z0-9]+` split {yes,true,1,y,t}/{no,false,0,n,f}; sentiment first-token `[^a-z]+` split {positive,negative,neutral}; rank first `-?\d+(?:\.\d+)?` float, no clamp; batch fence-strip (`^```[A-Za-z0-9_-]*\s*` + `\s*```$`) then serde_json array, len==n, JSON null→SQL NULL, non-string→22023.
- **Errors:** preserve substrings — `"must not be NULL"`, `"llm_endpoint is not set"`, `"http(s)"`, `"call failed"`, `"empty completion"`, `"unexpected chat response shape"`, `"unparseable boolean"`, `"did not return a known label"`, `"did not return a number"`, `"valid JSON"`, `"non-string"`, `"expected a JSON array of N items"` + correct SQLSTATE (22023 input/parse, 38000 endpoint).
- **`ai._chat` stays SQL-callable** (`ai._chat(text,text,text) RETURNS text`) — wrapper over `theodb_rs._chat` (D1/P5).

#### Pseudo-code / Signatures
```rust
// chat.rs
pub(crate) fn chat(prompt: Option<&str>, system: Option<&str>, model: Option<&str>) -> String {
    let prompt = prompt.unwrap_or_else(|| err_input("ai._chat: prompt must not be NULL"));
    let (endpoint, mdl, api_key) = resolve_chat_cfg(model);
    let mut messages = Vec::new();
    if let Some(s) = system.filter(|s| !s.is_empty()) { messages.push(json!({"role":"system","content":s})); }
    messages.push(json!({"role":"user","content":prompt}));
    let payload = json!({"model": mdl, "messages": messages}).to_string();
    let body = crate::http::post_json("ai._chat", &endpoint, payload, api_key.as_deref());
    let content = body.get("choices").and_then(|c| c.get(0)).and_then(|m| m.get("message"))
        .and_then(|m| m.get("content")).and_then(|c| c.as_str())
        .unwrap_or_else(|| err_external(&format!("ai._chat: unexpected chat response shape: {}", crate::http::truncate(&body.to_string(),200))));
    if content.is_empty() { err_external("ai._chat: endpoint returned an empty completion"); }
    content.to_string()
}
```

#### Tasks
1. `resolve_chat_cfg` (llm_* GUCs + http(s) guard) + `chat()`.
2. `parse_bool`/`parse_sentiment`/`parse_rank`/`parse_batch` (verbatim).
3. `lib.rs` externs + `extension_sql!` (ai._chat + 4 wrappers, REVOKE).
4. `pg.rs` errcontext parameterization (ADR-C).
5. `#[pg_test]` input guards (NULL prompt → 22023; prompt-table parsers reject malformed → 22023) that run offline.

#### TDD
```
RED:    #[pg_test(error="must not be NULL")] chat_null_prompt — SELECT ai._chat(NULL,...) -> 22023
RED:    (parity, py) test_ai_sql.py 36/36 against the rebuilt image — generate/if/sentiment/rank/summarize/agg/batch + all malformed/SSRF/empty/shape/REVOKE cases
GREEN:  implement chat.rs + wrappers
REFACTOR: share resolve_*/error helpers cleanly
VERIFY: cargo pgrx-built image ; python3 -m pytest benchmarks/tests/test_ai_sql.py -v  (36/36)
```

#### Concurrency tests (only)
(none — single-threaded) — each call is one synchronous request; no shared state.

#### Failure-scenario note
External HTTP — covered in `## Failure scenarios` (chat endpoint 5xx/timeout/non-JSON/bad-shape/empty; SSRF redirect; api_key no-leak).

#### Acceptance Criteria
- [ ] All 5 `ai.*` are Rust (theodb_rs) and the 36-test oracle is green UNCHANGED — asserted by `pytest benchmarks/tests/test_ai_sql.py` (exit 0) against the rebuilt image.
- [ ] `ai._chat` remains SQL-callable as `ai._chat(text,text,text)` — verified by `psql -c "SELECT to_regprocedure('ai._chat(text,text,text)') IS NOT NULL"` true.
- [ ] Security preserved — every `ai.*` is `REVOKE … FROM PUBLIC` (asserted by `test_ai_sql.py::test_ai_functions_not_executable_by_public`); SSRF http(s)-only + no-redirect + api_key never in error (asserted by the SSRF + connection-refused oracle cases).
- [ ] Quality gates pass — `cargo clippy --features pg17 -- -D warnings` exits 0; every changed file ≤ 500 lines (`wc -l`).

#### DoD
- [ ] `cargo pgrx test` input guards + `test_ai_sql.py` 36/36 green; clippy clean.

---

## Phase 2: theodb side — plpgsql keepers + retirement migration

### T2.1 — Remove plpython3u from sql/50, late-bind the keepers, add the 1.1→1.2 migration

#### Objective
Remove the 5 plpython3u definitions; make the SQL keepers late-bound; retire the legacy plpython3u `ai.*` on upgrade; bump default_version.

#### Why this step (action + reasoning)
1. **What:** delete the 5 plpython3u `CREATE OR REPLACE FUNCTION` blocks from sql/50; change `ai.generate`/`ai.summarize`/`ai._agg_summ_final` to LANGUAGE plpgsql (late-bound); write `sql/theodb--1.1--1.2.sql` (conditional DROP of the 5 plpython3u `ai.*`); bump `theodb.control default_version` to 1.2.
2. **Why now:** completes "zero plpython3u in sql/50" + the existing-install upgrade path. Cites D4/D5 + the M17 retirement precedent.

#### Files to edit
```
sql/50-theodb-ai.sql — REMOVE ai._chat/ai.if/ai.analyze_sentiment/ai.rank/ai.generate_batch (plpython3u); change ai.generate/ai.summarize/ai._agg_summ_final to LANGUAGE plpgsql (same VOLATILE, body RETURN ai._chat(...)); keep ai._agg_summ_accum + the aggregate; keep CREATE SCHEMA ai + the REVOKEs/COMMENTs for the keepers
sql/theodb--1.1--1.2.sql (NEW) — DO block: for each of the 5 ai.* names, DROP if plpython3u AND not a theodb_rs member
theodb.control — default_version '1.1' -> '1.2'
benchmarks/tests/test_ai_retirement.py (NEW) — real upgrade path: v0.x-shaped install (plpython3u ai.* as theodb members) -> ALTER EXTENSION theodb UPDATE TO '1.2' -> dropped -> CREATE EXTENSION theodb_rs no clash; + fresh-install no-op
```

#### Deep Dives
- **Late-bind (D4):** `ai.generate`/`summarize` → `CREATE FUNCTION ... LANGUAGE plpgsql VOLATILE AS $$ BEGIN RETURN ai._chat(...); END $$;` (body not validated at create). `_agg_summ_final` → plpgsql VOLATILE wrapping the existing CASE.
- **Retirement guard (D5):** per name in {_chat, if, analyze_sentiment, rank, generate_batch}: `DROP FUNCTION IF EXISTS ai.<name>(<args>)` only when `prolang=plpython3u AND NOT theodb_rs-member` (pg_proc + pg_depend deptype 'e').
- **Invariant:** fresh 1.2 install → no plpython3u ai.* → DROP no-ops; theodb_rs creates the Rust ai.*.

#### TDD
```
RED:    test_ai_retirement.py::test_upgrade_drops_plpython_ai_then_theodb_rs_installs_clean — fresh DB, CREATE EXTENSION theodb VERSION '1.1', seed plpython3u ai._chat+wrappers as theodb members, ALTER EXTENSION theodb UPDATE TO '1.2' -> all dropped -> CREATE EXTENSION theodb_rs no clash
RED:    test_ai_retirement.py::test_owned_ai_preserved — theodb_rs-owned ai.* never dropped by the guard
RED:    test_extension_install.py (existing, extended) — fresh CREATE both extensions: ai.* present, extversion theodb = 1.2
GREEN:  remove plpython3u, late-bind keepers, write the migration, bump version
VERIFY: python3 -m pytest benchmarks/tests/test_ai_retirement.py benchmarks/tests/test_extension_install.py -v
```

#### Concurrency tests (only)
(none — single-threaded) — DDL migration.

#### Acceptance Criteria
- [ ] Zero `LANGUAGE plpython3u` in `sql/50` — verified by `grep -c 'LANGUAGE plpython3u' sql/50-theodb-ai.sql` equals 0.
- [ ] The keepers (`ai.generate`/`summarize`/`agg_summarize`) still work — asserted by `pytest benchmarks/tests/test_ai_sql.py -k "generate or summarize or agg"` (exit 0).
- [ ] Existing-install upgrade drops the legacy plpython3u `ai.*` then theodb_rs installs clean — asserted by `pytest benchmarks/tests/test_ai_retirement.py` (exit 0).
- [ ] `theodb.control` `default_version='1.2'` and fresh install lands there — verified by `grep -q "default_version = '1.2'" theodb.control` and `test_extension_install.py` asserting extversion 1.2.

#### DoD
- [ ] sql/50 plpython3u-free; retirement + install tests green; CHANGELOG notes the migration.

---

## Phase 3: benchmark (chat Rust vs plpython3u)

### T3.1 — `bench_chat.py` + report (no-regression)

#### Objective
Measure per-call latency of the Rust chat path vs the plpython3u baseline in one container, honestly framed (I/O-bound → no-regression).

#### Why this step (action + reasoning)
1. **What:** `benchmarks/bench_chat.py` (mirror `bench_embed.py`): keep the plpython3u `ai._chat` as `ai._chat_py` in the bench setup; measure N serial `ai.generate` (Rust) vs the plpython3u path against the same chat stub; report mean±std (≥3 runs).
2. **Why now:** CTO requirement (measurement-first, ADR 0002). Cites blueprint § Benchmark + M17's bench_embed pattern.

#### Files to edit
```
benchmarks/bench_chat.py (NEW) — bench(conn, n, runs, func) for the chat path; creates ai._chat_py (plpython3u) for the comparison
docs/benchmarks/m18-ai-rust-vs-plpython.md (NEW) — the no-regression report (mean±std, >=3 runs, methodology, honest I/O-bound framing)
```

#### TDD
```
RED:    bench harness produces a numeric mean/std for both arms (smoke: a short run returns finite numbers)
GREEN:  implement bench_chat.py + write the report from a real run vs the rebuilt image
VERIFY: python3 benchmarks/bench_chat.py --endpoint http://host.docker.internal:PORT/v1/chat/completions --report docs/benchmarks/m18-ai-rust-vs-plpython.md
```

#### Concurrency tests (only)
(none — single-threaded) — serial latency measurement.

#### Acceptance Criteria
- [ ] `docs/benchmarks/m18-ai-rust-vs-plpython.md` shows the Rust chat path mean±std (≥3 runs) vs plpython3u with an honest no-regression framing — produced by `python3 benchmarks/bench_chat.py ... --report ...` (per `public-copy.md` — no unbenchmarked claim).

#### DoD
- [ ] Benchmark report committed; numbers from a real run vs the rebuilt image.

---

## Coverage Matrix

| # | M18 DoD / requirement | Task(s) | Resolution |
|---|---|---|---|
| 1 | `ai._chat` Rust, parity | T1.1 | chat.rs + ai._chat wrapper; oracle green |
| 2 | `ai.if` Rust, parity | T1.1 | parse_bool; oracle green |
| 3 | `ai.analyze_sentiment` Rust, parity | T1.1 | parse_sentiment; oracle green |
| 4 | `ai.rank` Rust, parity | T1.1 | parse_rank (real); oracle green |
| 5 | `ai.generate_batch` Rust, parity | T1.1 | parse_batch (fence+JSON, N-in/N-out, null preserve); oracle green |
| 6 | DRY HTTP client (no regression on embed) | T0.1 | http.rs extraction; embed 31-test oracle green |
| 7 | REVOKE + SSRF/no-redirect/fail-fast/api_key-no-leak preserved | T1.1 | reused http.rs SSRF + REVOKE wrappers; oracle SSRF/REVOKE cases |
| 8 | Zero plpython3u in the AI layer (sql/50) | T2.1 | remove 5 defs; grep == 0 |
| 9 | SQL keepers still work (late-bound) | T2.1 | plpgsql generate/summarize/_agg_summ_final; oracle keeper cases |
| 10 | Existing-install upgrade (retire plpython3u ai.*) | T2.1 | theodb--1.1--1.2.sql conditional DROP + default_version 1.2 |
| 11 | Benchmark (chat Rust vs plpython3u) | T3.1 | bench_chat.py + report (no-regression) |

**Coverage: 11/11 requirements covered (100%).**

## Global Definition of Done

- [ ] All phases complete.
- [ ] `benchmarks/tests/test_ai_sql.py` 36/36 green vs the rebuilt image (UNCHANGED test file).
- [ ] The embed oracle (test_embed_sql + failure_scenarios + embed_batch) green UNCHANGED (no regression from the http.rs extraction).
- [ ] `test_extension_install.py` green (fresh CREATE both extensions; extversion theodb = 1.2; ai.* present); `test_ai_retirement.py` green (upgrade path).
- [ ] `cargo clippy --features pg17 -- -D warnings` clean; `ruff` clean.
- [ ] Zero `LANGUAGE plpython3u` in `sql/50`.
- [ ] `docs/benchmarks/m18-ai-rust-vs-plpython.md` present (no-regression, mean±std, ≥3 runs).
- [ ] CHANGELOG `[Unreleased]` updated.
- [ ] Backward compatibility: `ai.*` signatures/return types/volatility unchanged; `ai._chat` SQL-callable; the aggregate volatility ('v' finalfunc) preserved.
- [ ] File-size budget ≤ 500 lines per changed file.

## Failure scenarios (external I/O — chat endpoint)

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| chat endpoint | transient 503 then 200 | (retry policy reused from embed; covered by embed retry oracle + ai._chat path) | retry ≤2 → success |
| chat endpoint | connection refused / unreachable | port 1 (test_ai_sql.py connection-refused cases) | fail-fast 38000 "call failed" after the cap |
| chat endpoint | non-JSON 200 / bad shape | stub `__BADSHAPE__` | 38000 "unexpected chat response shape" (no retry) |
| chat endpoint | empty completion | stub `__EMPTY__` | 38000 "endpoint returned an empty completion" |
| chat endpoint | SSRF redirect to internal host | http(s) guard + max_redirects(0) | refused; SSRF case green; api_key never in error |
| chat endpoint | malformed task output | stub `__MALFORMED__` / `__wronglen__` / `__nonstr__` | 22023 typed parse errors (if/rank/sentiment/batch) |

## Final Phase: Integration Validation (MANDATORY)

> Runs after Phases 0–3. NOT done until the full chain + benchmark pass.

### Execution
```
docker build -t theo-db:m18 .
docker run -d --add-host=host.docker.internal:host-gateway ... theo-db:m18
# Parity + no-regression:
python3 -m pytest benchmarks/tests/test_ai_sql.py \
  benchmarks/tests/test_embed_sql.py benchmarks/tests/test_embed_failure_scenarios.py benchmarks/tests/test_embed_batch.py \
  benchmarks/tests/test_extension_install.py benchmarks/tests/test_ai_retirement.py -v
cargo clippy --features pg17 -- -D warnings ; ruff check benchmarks/
# Benchmark (CTO data):
python3 benchmarks/bench_chat.py ... --report docs/benchmarks/m18-ai-rust-vs-plpython.md
```

### Acceptance Criteria
- [ ] `test_ai_sql.py` 36/36 + the embed oracle + install + retirement all green vs `theo-db:m18`.
- [ ] `cargo clippy -- -D warnings` + `ruff` clean.
- [ ] Benchmark report shows the no-regression chat numbers (mean±std, ≥3 runs) — `python3 benchmarks/bench_chat.py ... --report docs/benchmarks/m18-ai-rust-vs-plpython.md`.
- [ ] Zero `LANGUAGE plpython3u` in `sql/50` — `grep -c 'LANGUAGE plpython3u' sql/50-theodb-ai.sql` is 0.

### If Validation Fails
1. Separate plan-caused from pre-existing failures.
2. Fix all plan-caused failures before declaring complete.
3. Re-run the chain.
