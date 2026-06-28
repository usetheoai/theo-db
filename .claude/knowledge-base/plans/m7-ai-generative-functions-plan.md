---
slug: m7-ai-generative-functions
created_at: 2026-06-28
goal: Ship SQL generative-AI functions (ai.generate/if/rank/analyze_sentiment/summarize) over a configurable LLM endpoint
---

# Plan: Generative-AI SQL functions (`ai.*`) over a configurable model — M7-S3

> **Version 1.0** — Ship the M7 "IA generativa em SQL" capability: five scalar `ai.*` functions
> (`ai.generate`, `ai.if`, `ai.analyze_sentiment`, `ai.summarize`, `ai.rank`) that call a **configurable
> OpenAI-compatible chat-completions endpoint** (local or cloud), mirroring AlloyDB's `google_ml_integration`
> / `ai.generate` and extending the proven M2 `theodb.embed` pattern (GUCs + `plpython3u` + fail-fast +
> SSRF hardening). One private `ai._chat()` HTTP helper is the single source of truth; each public function
> is a thin task-specific wrapper. Model-agnostic = the lever over AlloyDB's Gemini lock-in (CLAUDE.md
> TheoDB rule 1). Array/cursor "accelerated" modes are an explicit follow-up (YAGNI — scalar is the DoD MVP).

## Goal

> Enable TheoDB users to call generative-AI over their data in SQL (`ai.generate`/`ai.if`/`ai.analyze_sentiment`/`ai.summarize`/`ai.rank`) against a configurable model endpoint, measured by per-function contract integration tests passing against a real OpenAI-compatible endpoint.

## Context

ROADMAP `### M7` DoD-4 requires "Funções de IA generativa em SQL — `ai.generate`, `ai.if`, `ai.analyze_sentiment`,
`ai.summarize`/`ai.agg_summarize` — sobre modelo configurável (local/remoto)… cada função com teste de
contrato." This is **M7-S3**. The SOTA anchor (ADR `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md`)
is AlloyDB, which ships `ai.generate` + the `google_ml_integration` extension calling a managed Gemini/Vertex
endpoint; TheoDB matches the capability with a **configurable** OpenAI-compatible endpoint (any local or cloud
model) — the model-agnostic win over AlloyDB's lock-in (CLAUDE.md TheoDB rule 1). The implementation extends
the already-shipped `theodb.embed` (M2) pattern from the embeddings endpoint to the chat-completions endpoint —
same GUC mechanism, same `plpython3u`, same SSRF/no-redirect/fail-fast discipline. Target API specs:
`docs/features/07-funcoes-ia-sql.md` (`ai.generate`/`ai.if`/`ai.rank`), `docs/features/10-analise-sentimento.md`
(`ai.analyze_sentiment`), `docs/features/11-sumarizacao-conteudo.md` (`ai.summarize`). The earlier M7-S1
already created the `ai` schema (`sql/40-theodb-hybrid.sql`), which these functions join.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `sql/30-theodb-embed.sql` | 89 | `ba98af3` (2026-06-27) | M2 `theodb.embed` — the configurable-endpoint pattern this slice mirrors | Unchanged; read-only reference for the pattern |
| `sql/40-theodb-hybrid.sql` | 102 | `d00e330` (2026-06-28) | M7-S1 — created the `ai` schema | `ai` schema + `ai.hybrid_search_rrf` intact; new functions are additive |
| `sql/50-theodb-ai.sql` (NEW) | 0 | — | (to be created) the five generative `ai.*` functions + private `ai._chat` helper | — |
| `Dockerfile` | 70 | `5f7acb1` (2026-06-28) | Builds image; copies `sql/*.sql` to initdb.d | Existing `COPY sql/30` + `sql/40` lines stay; add `COPY sql/50` |
| `smoke.sh` | 45 | `d00e330` (2026-06-28) | Engine + pgvector + hybrid smoke | Existing checks stay; an `ai.*` presence check is additive (no network in smoke) |
| `tools/embedding_server.py` | 94 | `129aa7f` (2026-06-27) | Local fastembed server (embeddings) for the M2 test | Pattern reference; a tiny chat stub server may be added for offline contract tests |
| `tools/chat_server.py` (NEW) | 0 | — | (to be created) minimal OpenAI-compatible `/v1/chat/completions` stub for deterministic offline contract tests | — |
| `benchmarks/tests/test_ai_sql.py` (NEW) | 0 | — | (to be created) per-function contract tests (offline stub + real-OpenAI opt-in) | — |
| `benchmarks/tests/test_embed_sql.py` | 188 | `ba98af3` (2026-06-27) | M2 embed contract test — the test pattern to mirror | Stays green; new file mirrors its server-fixture shape |
| `.github/workflows/ci.yml` | (exists) | — | CI | Existing jobs stay; add an `ai-sql` job (offline stub — no external API in CI) |
| `docs/features/07-funcoes-ia-sql.md` | (exists) | — | Target API spec | Add an "implemented surface" note |
| `docs/sql-embeddings.md` | (exists) | — | M2 embeddings doc | Sibling new doc `docs/sql-ai-functions.md` documents the GUCs + functions |
| `docs/sql-ai-functions.md` (NEW) | 0 | — | (to be created) GUC config + per-function usage + security notes | — |
| `CHANGELOG.md` | (exists) | — | Public contract | `[Unreleased]` gets the M7-S3 entry |

Every file in any `#### Files to edit` below appears in this table.

### Current callers / dependents

- **Symbol:** `theodb.embed(content, model)` in `sql/30-theodb-embed.sql`
  - **Callers:** loaded at initdb; `benchmarks/tests/test_embed_sql.py`; `ai.hybrid_search_rrf` (vector leg).
  - **External:** no. Unchanged by this slice — `ai.*` generative functions are a separate surface using a separate GUC namespace (`theodb.llm_*`).
- **Symbol:** `ai` schema (created in `sql/40-theodb-hybrid.sql`)
  - **Callers:** `ai.hybrid_search_rrf` (M7-S1); harness `db.hybrid_rrf_docs`.
  - **External:** no. The new `ai.generate/if/rank/analyze_sentiment/summarize` + `ai._chat` are additive members of this schema; no existing member changes.
- **Symbol:** `ai._chat` (NEW private helper) — sole caller of the chat-completions endpoint; called only by the five public `ai.*` functions in the same file. No external caller.

Enumerated via `grep -rln 'theodb.embed\|CREATE SCHEMA IF NOT EXISTS ai\|ai\\.' --include='*.sql' --include='*.py' sql/ benchmarks/`.

### Domain glossary

- **chat-completions endpoint** — an OpenAI-compatible HTTP endpoint accepting `{model, messages:[{role,content}]}` and returning `choices[0].message.content`; the generative twin of the embeddings endpoint.
- **GUC** — PostgreSQL Grand Unified Configuration setting; here `theodb.llm_endpoint`/`theodb.llm_model`/`theodb.llm_api_key` (session- or role-settable), mirroring the M2 `theodb.embedding_*` GUCs.
- **`ai._chat`** — the single private function that performs the HTTP round-trip + response parsing; the DRY source of truth for all five public functions.
- **system prompt** — a task-shaping instruction prepended per function (e.g. sentiment → "reply with exactly one of: positive/negative/neutral") that constrains the model's output so the SQL function can parse it deterministically.
- **SSRF hardening** — refusing non-http(s) endpoints and disabling redirects so a session-set GUC cannot make the server fetch internal/metadata URLs (inherited from `theodb.embed`).

### Architecture boundaries affected

Per `rules/architecture.md`: the `ai.*` functions are an **infrastructure/adapter** surface inside the database
image (same layer as `theodb.embed`), making server-side outbound HTTP. No inner layer imports them. The new
GUC namespace (`theodb.llm_*`) is parallel to the embeddings namespace — no coupling. The chat stub server
(`tools/chat_server.py`) and tests are **dev-only tooling** outside product layering. DIP: the five public
functions depend on the single `ai._chat` helper (one HTTP boundary), not each on its own HTTP code.

## Prior Art & Related Work

- **Internal pattern (the design source):** `sql/30-theodb-embed.sql:1-89` — the proven configurable-endpoint
  function (GUC resolution, SSRF guard, no-redirect opener, typed fail-fast errors, REVOKE-from-PUBLIC). This
  slice extends that exact pattern from embeddings to chat-completions.
- **Internal blueprint:** `.claude/knowledge-base/discoveries/blueprints/alloydb-vector-ai-implementation-blueprint.md`
  — the AlloyDB `google_ml_integration` / `ai.generate` SOTA anchor.
- **Target specs:** `docs/features/07-funcoes-ia-sql.md` (`ai.generate`/`ai.if`/`ai.rank` signatures + modes),
  `docs/features/10-analise-sentimento.md` (`ai.analyze_sentiment` → positive/negative/neutral),
  `docs/features/11-sumarizacao-conteudo.md` (`ai.summarize`).
- **External literature:** OpenAI Chat Completions API
  (`https://platform.openai.com/docs/api-reference/chat`) — the `{model, messages}` → `choices[].message.content`
  contract the configurable endpoint implements.
- **Why no new DISCOVER cycle:** the prior art is already in-hand (the `theodb.embed` pattern + the AlloyDB
  blueprint + the target specs). There is no permissive OSS repo implementing `ai.generate` to investigate —
  it is a managed-cloud feature. Per `cycle-plan` pre-conditions ("known prior art → DISCOVER not required"),
  a fresh discovery would be investigation theatre (KISS).

## Objective

- [ ] A private `ai._chat(prompt, system, model)` helper performs the configurable chat-completions call (GUCs `theodb.llm_*`), with SSRF guard + no-redirect + typed fail-fast errors (mirrors `theodb.embed`).
- [ ] `ai.generate(prompt, model)` → text (raw completion).
- [ ] `ai.if(prompt, model)` → boolean (yes/no classification, parsed deterministically).
- [ ] `ai.analyze_sentiment(content, model)` → text ∈ {positive, negative, neutral}.
- [ ] `ai.summarize(content, model)` → text (concise summary).
- [ ] `ai.rank(prompt, model)` → real (numeric score parsed from the model output).
- [ ] Every function has a contract test: deterministic offline (stub server) for CI + an opt-in real-OpenAI test (key from gitignored `.env`).
- [ ] All five functions baked into the image (initdb.d) and `REVOKE`d from PUBLIC (outbound HTTP, like `theodb.embed`).

## ADRs

### D1 — One private `ai._chat` helper; five thin public wrappers (DRY + parsimony)

**Decision:** A single `ai._chat(prompt text, system text, model text) RETURNS text` (plpython3u) does the HTTP
round-trip + parsing. The five public functions each build a task-specific `system` prompt and post-process
`ai._chat`'s output (parse boolean / float / normalize sentiment label).

**Rationale:** DRY — one HTTP/SSRF/error code path, not five copies (parsimony ladder rung 5-6). Mirrors how
`theodb.embed` centralizes the embeddings call. Adding a sixth function later is a thin wrapper, not new HTTP code.

**Alternatives considered:** Five independent functions each with their own urllib code — rejected (DRY violation,
5× the SSRF surface to audit). A generic `ai.call(json)` exposing raw request/response — rejected (leaks the
provider contract to users; the typed wrappers are the ergonomic + safe surface).

**Consequences:** `ai._chat` is private (`REVOKE FROM PUBLIC`, name underscore-prefixed); the public functions
are the contract. Output parsing lives in each wrapper (its own concern).

### D2 — Configurable OpenAI-compatible chat endpoint via `theodb.llm_*` GUCs (model-agnostic)

**Decision:** Endpoint/model/api-key come from `theodb.llm_endpoint`/`theodb.llm_model`/`theodb.llm_api_key`
GUCs (session/role-settable), separate from the M2 `theodb.embedding_*` namespace. Payload is the standard
chat-completions shape `{model, messages:[{role:system},{role:user}]}`.

**Rationale:** Matches AlloyDB's capability while staying model-agnostic (any local/cloud OpenAI-compatible
endpoint) — the explicit win over AlloyDB's Gemini lock-in (CLAUDE.md rule 1). Separate GUC namespace because a
deployment may use different endpoints/models for embeddings vs generation.

**Alternatives considered:** Reuse the `theodb.embedding_*` GUCs — rejected (embeddings and chat are different
endpoints/models; conflating them blocks mixed deployments). Ship a model in the image — rejected (D1/M2
decision: TheoDB ships no model). Hardcode OpenAI — rejected (kills model-agnosticism, the whole point).

**Consequences:** Operators set two GUC groups. The test suite sets `theodb.llm_*` per session. SSRF guard +
no-redirect inherited from the `theodb.embed` pattern.

### D3 — Deterministic offline contract tests (stub server) for CI; real-OpenAI test opt-in

**Decision:** A tiny `tools/chat_server.py` OpenAI-compatible stub returns canned, rule-shaped responses so
each function's PARSING contract is tested deterministically offline in CI (no external API, no cost, no
flakiness). A separate real-OpenAI test runs only when `THEODB_LLM_*`/`OPENAI_API_KEY` are configured (opt-in,
skips cleanly otherwise — no silent green).

**Rationale:** `rules/testing.md` §6 — no network/non-determinism in the CI lane. The stub proves the
SQL↔HTTP↔parse contract; the opt-in real test proves genuine end-to-end against OpenAI (the "real evidence,
no mock" bar from M2). LLM output is non-deterministic, so the real test asserts *shape/contract* (e.g.
sentiment ∈ the 3 labels, `ai.if` returns a boolean), never an exact string.

**Alternatives considered:** Only real-OpenAI tests — rejected (non-deterministic, costs money, flaky in CI).
Only stub — rejected (never proves the real provider works; M2 set the "real, no mock" precedent). Mock at the
Python layer inside plpython3u — rejected (can't, the function runs in the DB; the stub server is the honest seam).

**Consequences:** CI runs the offline stub job; the real-OpenAI test is a documented opt-in for local/manual
validation using the gitignored `.env` key.

### D4 — Deterministic output parsing with fail-fast on unparseable model output

**Decision:** `ai.if` parses a leading yes/true/1 vs no/false/0 (case-insensitive, system-prompt-constrained);
`ai.rank` parses the first float in the output; `ai.analyze_sentiment` normalizes to one of {positive,
negative, neutral}. If the (constrained) output cannot be parsed, raise a typed error (`22023`) — never guess.

**Rationale:** Error handling (Rule 8) — a model that ignores the format instruction must fail loud, not return
a silent wrong boolean. The system prompt minimizes this, but the parser is the last line of defense.

**Alternatives considered:** Best-effort fuzzy parse with a default (e.g. neutral on failure) — rejected (silent
wrong answer; Rule 8 fail-fast). Return raw text for all — rejected (the typed return is the function's contract).

**Consequences:** Tested via the stub returning a malformed body → assert the typed error.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Synchronous blocking HTTP per row → a `SELECT ai.generate(col) FROM big_table` is slow/expensive | Medium | Document "scalar = synchronous, one round-trip per row; batch large jobs" (mirrors `theodb.embed` doc); array/cursor accelerated modes are a documented follow-up | Docs |
| LLM output non-determinism breaks naive parsing (`ai.if`/`ai.rank`/sentiment) | Medium | Output-constraining system prompts (D4) + fail-fast typed error on unparseable output + tests assert contract/shape not exact text | DB |
| `theodb.llm_api_key` GUC visible to `SHOW`/`log_statement` (same caveat as `theodb.embed`) | Medium | Document: set per-session out of band, never in logged DDL; REVOKE function from PUBLIC; key never echoed in error messages | Security |
| Server-side outbound HTTP from the DB = SSRF surface if endpoint GUC is attacker-set | Medium | Inherit `theodb.embed` SSRF guard (http(s)-only, no redirects); REVOKE functions from PUBLIC (only granted roles can trigger outbound HTTP) | Security |
| Cost/quota: real OpenAI calls cost money; a runaway query bills the account | Low | CI uses the offline stub (zero external calls); real test is opt-in + small; doc warns about per-row cost | Bench |

## Unresolved Questions

- Q1 — Does `ai.summarize` need a length/word-count parameter? Resolved at plan time: MVP is a single concise-summary system prompt; a `max_words` param is a documented follow-up (YAGNI).
- Q2 — `ai.rank` output range — normalized 0..1 or model-defined? Resolved: the system prompt asks for a float 0..1; the parser returns whatever float the model emits (real), documented as "model-defined unless the prompt constrains it".
- Q3 — Should `ai.if` accept a model that returns prose ("Yes, because…")? Resolved: the system prompt demands a bare yes/no; the parser reads the leading token; non-conforming output → typed error (D4).

## Dependencies

M7-S3 adds **no new runtime dependency** (Unbreakable Rule 9). `plpython3u` (already shipped in M2 for
`theodb.embed`), Python stdlib `urllib`/`json` (no third-party HTTP lib), and PostgreSQL itself. The chat stub
server uses only the Python stdlib `http.server` (dev-only).

| Dependency | Version | Role | License | Status / CVE |
|---|---|---|---|---|
| `plpython3u` | bundled (PG 17) | runs the `ai.*` functions | PostgreSQL License | shipped in M2; no new dep |
| Python stdlib (`urllib`, `json`, `http.server`) | 3.x (bundled) | HTTP call + stub server | PSF | no new dep |
| `psycopg2-binary` (test, dev-only) | as in `benchmarks/requirements.txt` | DB client for contract tests | LGPL (dev-only) | already a dev dep |

No CVE audit delta: zero new declared dependencies.

## Edge Cases & MUST-FIX (from /edge-case-plan)

| # | Edge case / risk | MUST-FIX (which task) | Acceptance |
|---|---|---|---|
| E1 | Model ignores format instruction (returns prose for `ai.if`/`ai.rank`) | T1.1 D4 parser fail-fast; T2.1 `test_*_malformed_raises_typed` (stub `malformed` mode → `22023`) | malformed → typed `22023`, never silent wrong value |
| E2 | `theodb.llm_api_key` leaks in an error message | T1.1 — api_key never interpolated into any `plpy.error` string (mirrors `sql/30-theodb-embed.sql`) | grep the function body: no `api_key` in error text |
| E3 | SSRF via attacker-set endpoint GUC (`file://`, redirect to metadata) | T1.1 — http(s)-only guard + no-redirect opener (inherited from embed) | `test` with `file://` → `22023`; redirects disabled |
| E4 | Empty/NULL content or empty model completion | T1.1 — NULL content → `22023`; empty `choices[0].message.content` → `38000` | both typed, tested |
| E5 | Per-row cost/latency on a big-table `SELECT ai.generate(col)` | docs (T3.1) — synchronous one-round-trip-per-row caveat + batch guidance; array/cursor accelerated mode is a documented follow-up (YAGNI) | doc states the caveat explicitly |
| E6 | CI must not call the real LLM (cost/flake) | T2.1/T3.1 — CI runs the offline deterministic stub only; real-OpenAI test is opt-in, skips cleanly when env unset | CI `ai-sql` job makes zero external calls |

## Dependency Graph

```
Phase 1 (ai._chat + 5 public fns SQL) ──▶ Phase 2 (stub server + contract tests) ──▶ Phase 3 (smoke + CI + docs)
```

Sequential: Phase 2 tests the functions Phase 1 creates; Phase 3 wires the smoke/CI/docs over both. Final
Phase (Integration Validation) last.

---

## Phase 1: `ai._chat` helper + five generative `ai.*` functions (SQL)

**Objective:** Create the private chat helper + five public generative functions, baked into the image, REVOKEd from PUBLIC.

### T1.1 — `sql/50-theodb-ai.sql`: `ai._chat` + `ai.generate`/`ai.if`/`ai.analyze_sentiment`/`ai.summarize`/`ai.rank`

#### Objective
Add an idempotent SQL file defining the private `ai._chat` helper and the five public functions, and copy it into the image via initdb.d.

#### Why this step (action + reasoning — ReAct discipline)

1. **What this step does** — creates `sql/50-theodb-ai.sql` with `ai._chat` (plpython3u HTTP call to the
   configurable chat endpoint, SSRF-guarded, no-redirect, fail-fast) + five thin public wrappers that set a
   task system prompt and parse the output; adds the `COPY sql/50-...` line to the `Dockerfile`.

2. **Why it is necessary now** — these functions ARE the M7-S3 deliverable; nothing else can be tested or
   smoked until they exist. The design (one helper + wrappers) is fixed by ADR D1/D2 and mirrors the proven
   `theodb.embed` (`sql/30-theodb-embed.sql:1-89`).

#### Evidence
- Pattern source: `sql/30-theodb-embed.sql:11-89` (GUC resolution, SSRF guard `:38-40`, no-redirect opener `:52-56`, typed errors `:28,32,62`, REVOKE `:81`).
- `ai` schema already exists: `sql/40-theodb-hybrid.sql:12` (`CREATE SCHEMA IF NOT EXISTS ai`).
- Target signatures: `docs/features/07-funcoes-ia-sql.md` §10 (`ai.if`→BOOLEAN), §22 (`ai.generate`→TEXT), §28 (`ai.rank`→REAL); `docs/features/10-analise-sentimento.md` §11 (positive/negative/neutral); `docs/features/11-sumarizacao-conteudo.md`.
- Chat API contract: OpenAI Chat Completions (`{model,messages}`→`choices[0].message.content`).

#### Files to edit
```
sql/50-theodb-ai.sql — (NEW) ai._chat + 5 public functions (idempotent), REVOKE FROM PUBLIC
Dockerfile — add COPY sql/50-theodb-ai.sql /docker-entrypoint-initdb.d/50-theodb-ai.sql
```

#### Deep file dependency analysis
- `sql/50-theodb-ai.sql` (NEW): joins the existing `ai` schema (`sql/40`, Baseline row); mirrors the idempotent header + `CREATE OR REPLACE FUNCTION` pattern of `sql/30`. No downstream SQL depends on it yet; tests (Phase 2) + smoke (Phase 3) call it.
- `Dockerfile` (Baseline row, invariant: keep `COPY sql/30` + `sql/40`): one additive `COPY` line, ordered after 40 (idempotent at initdb).

#### Deep Dives
- **`ai._chat(prompt text, system text DEFAULT NULL, model text DEFAULT NULL) RETURNS text`** (plpython3u):
  resolve `theodb.llm_endpoint` (required, else `22023`), SSRF-guard (http(s) only), `theodb.llm_model`,
  `theodb.llm_api_key`; POST `{model, messages:[{role:system,content:system?},{role:user,content:prompt}]}`;
  no-redirect opener, timeout 30s; on URLError/OSError/ValueError → `38000`; parse `choices[0].message.content`
  → on KeyError/IndexError → `38000`; return the content string.
- **`ai.generate(prompt, model)`** → `ai._chat(prompt, NULL, model)` (raw).
- **`ai.if(prompt, model)`** → system="Answer with exactly one word: yes or no."; parse leading token:
  yes/true/1→true, no/false/0→false, else `22023`.
- **`ai.analyze_sentiment(content, model)`** → system="Classify sentiment. Reply with exactly one of:
  positive, negative, neutral."; lower-strip output; if ∈ set return it, else `22023`.
- **`ai.summarize(content, model)`** → system="Summarize the text concisely in 1-2 sentences."; return text.
- **`ai.rank(prompt, model)`** → system="Reply with a single number between 0 and 1."; regex first float;
  none → `22023`; return as real.
- **Invariants:** NULL content/prompt → `22023` (mirrors embed). `ai._chat` private (REVOKE). All public
  functions REVOKE FROM PUBLIC (outbound HTTP). api_key never in any error string.
- **Edge cases:** model returns empty content → `38000`; model returns prose for `ai.if`/`ai.rank` → typed `22023`.

#### Pseudo-code / Signatures
```pseudocode
function ai._chat(prompt, system=NULL, model=NULL) returns text  -- plpython3u
  endpoint = current_setting('theodb.llm_endpoint', true)
  if not endpoint: raise 22023 'theodb.llm_endpoint not set'
  if not endpoint.startswith(('http://','https://')): raise 22023 'http(s) only'
  msgs = ([{role:system, content:system}] if system else []) + [{role:user, content:prompt}]
  body = POST(endpoint, {model: model or theodb.llm_model or 'default', messages: msgs},
              bearer=theodb.llm_api_key, no_redirect, timeout=30)   -- URLError/OSError/ValueError -> 38000
  return body['choices'][0]['message']['content']                   -- KeyError/IndexError -> 38000

function ai.if(prompt, model=NULL) returns boolean
  out = lower(strip(ai._chat(prompt, 'Answer with exactly one word: yes or no.', model)))
  if out starts yes/true/1: return true
  if out starts no/false/0: return false
  raise 22023 'ai.if: unparseable boolean: ' || left(out,50)

# Example (stub returns "yes"): ai.if('Is the sky blue?') -> true
# Example (sentiment stub "positive"): ai.analyze_sentiment('great movie') -> 'positive'
```

#### Tasks
1. Write `sql/50-theodb-ai.sql`: idempotent header, `ai._chat` (private), 5 public functions, REVOKEs, COMMENTs.
2. Add `COPY sql/50-theodb-ai.sql /docker-entrypoint-initdb.d/50-theodb-ai.sql` to `Dockerfile`.

#### TDD
```
RED:     (defined in Phase 2 — the contract tests in test_ai_sql.py drive this file; they fail until sql/50 exists)
GREEN:   Implement sql/50-theodb-ai.sql so the Phase 2 contract tests pass against the stub server.
REFACTOR: Extract the output-parse helpers in plpython only if it improves clarity; else "None expected".
VERIFY:  docker build -t theo-db:dev . && (start container) && cd benchmarks && pytest -m integration tests/test_ai_sql.py -q
```

#### Concurrency tests

**Concurrency posture: (none — single-threaded)** — each function is a single read-only statement performing one blocking HTTP round-trip; no shared mutable state, no locks, no async, no transaction-spanning mutation.

#### Acceptance Criteria
- [ ] After a fresh container init, all six functions exist — `docker exec … psql -c "\df ai.*"` lists `ai._chat`, `ai.generate`, `ai.if`, `ai.analyze_sentiment`, `ai.summarize`, `ai.rank`.
- [ ] `ai._chat` and all public functions are `REVOKE`d from PUBLIC — `psql -c "\dp"` / `has_function_privilege('public', …, 'execute')` returns false.
- [ ] Endpoint unset → `ai.generate('x')` raises SQLSTATE `22023` — `pytest -m integration -k endpoint_unset` exits `0`.
- [ ] Pass: size — `sql/50-theodb-ai.sql` `wc -l` returns `< 500`.

#### DoD
- [ ] All tasks completed and validated
- [ ] Functions load from initdb.d on a fresh container — `psql -c "\df ai.generate"` shows the row
- [ ] CHANGELOG `[Unreleased]` updated
- [ ] File-size budget respected (per `architecture.md`)

---

## Phase 2: Stub server + per-function contract tests

**Objective:** Prove each function's SQL↔HTTP↔parse contract deterministically offline (stub) + an opt-in real-OpenAI end-to-end test.

### T2.1 — `tools/chat_server.py` stub + `benchmarks/tests/test_ai_sql.py`

#### Objective
Add a minimal OpenAI-compatible chat-completions stub server and contract tests: one per function (offline, deterministic) + parsing/negative cases + an opt-in real-OpenAI test.

#### Why this step (action + reasoning — ReAct discipline)

1. **What this step does** — creates `tools/chat_server.py` (stdlib `http.server`, returns a canned
   `choices[0].message.content` shaped by the request so each function's parse path is exercised) and
   `benchmarks/tests/test_ai_sql.py` mirroring the `test_embed_sql.py` server-fixture pattern; adds the
   real-OpenAI opt-in test gated on env.

2. **Why it is necessary now** — `rules/testing.md` requires every business rule (here: each function's
   output parsing + the typed-error guards) to have a deterministic test; the stub is the honest seam that
   makes the in-DB HTTP call testable offline (D3). The real-OpenAI test gives the "real, no mock" evidence
   M2 established.

#### Evidence
- Test/server-fixture pattern: `benchmarks/tests/test_embed_sql.py:1-40` (free-port + subprocess server + `host.docker.internal` wiring) + `tools/embedding_server.py:1-94`.
- The functions under test: `sql/50-theodb-ai.sql` (T1.1).
- `.env` real key (opt-in real test, D3/D4): `OPENAI_API_KEY` + chat endpoint `https://api.openai.com/v1/chat/completions`, model e.g. `gpt-4o-mini`.

#### Files to edit
```
tools/chat_server.py — (NEW) stdlib http.server OpenAI-compatible /v1/chat/completions stub (deterministic, request-shaped)
benchmarks/tests/test_ai_sql.py — (NEW) per-function contract tests (offline stub) + parsing/negative cases + opt-in real-OpenAI test
```

#### Deep file dependency analysis
- `tools/chat_server.py` (NEW): stdlib only; mirrors `tools/embedding_server.py` shape (argparse host/port). Imported/spawned by the test fixture, never by production.
- `test_ai_sql.py` (NEW): `integration` marker; reuses the `_dsn()`/`host.docker.internal` wiring from `test_embed_sql.py`. The real-OpenAI test skips when env unset.

#### Deep Dives
- **Stub shaping:** the stub inspects the user message / system prompt and returns a canned content that lets
  each parser succeed: for the sentiment system prompt → "positive"; for the yes/no prompt → "yes"; for the
  rank prompt → "0.8"; for generate/summarize → a fixed sentence. A `?mode=malformed` query (or a magic
  prompt token) makes it return prose so the typed-error path (D4) is testable.
- **Determinism:** the stub is rule-based, no randomness → CI deterministic (testing.md §6).
- **Real-OpenAI test (opt-in):** when `THEODB_LLM_ENDPOINT`/`OPENAI_API_KEY` set, run `ai.analyze_sentiment`
  over a clearly-positive and clearly-negative text and assert the label ∈ {positive,negative,neutral} and
  matches polarity; assert shape, not exact string (LLM non-determinism). Skips cleanly otherwise.

#### Pseudo-code / Signatures
```pseudocode
# stub: POST /v1/chat/completions {model, messages} ->
#   content = decide_from(messages[system], messages[user])  # rule-based
#   200 {choices:[{message:{role:'assistant', content}}]}
# test (offline): set theodb.llm_endpoint -> stub; assert each function's typed return + parse
# test (real, opt-in): set theodb.llm_endpoint=OpenAI; assert sentiment polarity + shape
```

#### Tasks
1. Write `tools/chat_server.py` (deterministic OpenAI-compatible stub, malformed mode).
2. Write `test_ai_sql.py`: 1 happy test per function (offline), `ai.if`/`ai.rank`/sentiment parse + negative (malformed→`22023`), endpoint-unset→`22023`, opt-in real-OpenAI sentiment polarity test.

#### TDD
```
RED:     test_generate_returns_text() — stub returns a sentence; ai.generate(...) returns it. Fails before sql/50.
RED:     test_if_parses_boolean() — stub "yes" → true; "no" variant → false.
RED:     test_analyze_sentiment_in_label_set() — stub "positive" → 'positive'; assert ∈ {positive,negative,neutral}.
RED:     test_summarize_returns_text() — stub summary → non-empty text.
RED:     test_rank_parses_float() — stub "0.8" → 0.8::real.
RED:     test_if_malformed_raises_typed() — stub prose → SQLSTATE 22023 (D4 fail-fast).
RED:     test_endpoint_unset_raises_typed() — no theodb.llm_endpoint → 22023.
RED:     test_real_openai_sentiment_polarity() [opt-in] — skips unless OPENAI_API_KEY set; else asserts polarity + label-set shape.
GREEN:   Implement chat_server.py + (with sql/50 from T1.1) make all pass.
REFACTOR: dedupe the per-function test scaffolding via a small helper; else "None expected".
VERIFY:  cd benchmarks && pytest -m integration tests/test_ai_sql.py -q
```

#### Concurrency tests

**Concurrency posture: (none — single-threaded)** — the stub server handles one request at a time in the test; the functions are single-statement; no shared mutable state under concurrency.

#### Acceptance Criteria
- [ ] Offline contract suite green — `cd benchmarks && pytest -m integration tests/test_ai_sql.py -k 'not real' -q` exits `0` (7 tests).
- [ ] Negative path: malformed model output → SQLSTATE `22023` asserted (`-k malformed` green).
- [ ] Real-OpenAI test skips cleanly when `OPENAI_API_KEY` unset (no silent pass) and, when set, asserts sentiment polarity + label-set membership.
- [ ] Pass: lint — `cd benchmarks && ruff check tests/test_ai_sql.py` exits `0`.
- [ ] Pass: size — `tools/chat_server.py` and `test_ai_sql.py` `wc -l` each return `< 500`.

#### DoD
- [ ] All tasks completed and validated
- [ ] Offline suite green — `cd benchmarks && pytest -m integration tests/test_ai_sql.py -k 'not real' -q`
- [ ] Real-OpenAI test verified manually with `.env` key (evidence in implementation summary)
- [ ] Zero lint warnings
- [ ] CHANGELOG `[Unreleased]` updated

---

## Phase 3: Smoke + CI + docs

**Objective:** Surface the functions in the smoke (presence, no network), gate the offline contract suite in CI, and document the GUCs + functions.

### T3.1 — Smoke presence check + `ai-sql` CI job + `docs/sql-ai-functions.md`

#### Objective
Add an `ai.*` presence assertion to `smoke.sh` (no network), add a CI job running the offline contract suite, and document configuration + usage + security.

#### Why this step (action + reasoning — ReAct discipline)

1. **What this step does** — appends a presence check (the six functions exist + are non-PUBLIC) to
   `smoke.sh`; adds an `ai-sql` CI job building the image + running the offline stub contract tests; writes
   `docs/sql-ai-functions.md` (GUC setup, per-function usage, the synchronous/cost + api-key-in-logs caveats)
   and an implemented-surface note in the spec.

2. **Why it is necessary now** — the wiring triad: smoke = runtime presence caller, CI = integration gate,
   docs = the observable contract for users. The smoke must NOT call the network (no endpoint in CI base), so
   it only asserts the functions exist + are locked down.

#### Evidence
- Smoke pattern: `smoke.sh:1-45` (existing vector + hybrid checks).
- CI job pattern: `.github/workflows/ci.yml` (`hybrid-search` job added in M7-S1 is the template).
- Doc sibling: `docs/sql-embeddings.md` (M2 doc — same shape for the GUC + function reference).
- api-key-in-logs caveat source: `sql/30-theodb-embed.sql:83-89` COMMENT.

#### Files to edit
```
smoke.sh — append: assert ai.generate/if/analyze_sentiment/summarize/rank exist + not executable by PUBLIC (no network)
.github/workflows/ci.yml — add ai-sql job (build image + run offline stub contract suite) with timeout-minutes
docs/sql-ai-functions.md — (NEW) GUC config + per-function usage + synchronous/cost + api-key-in-logs security notes
docs/features/07-funcoes-ia-sql.md — implemented-surface note pointing at the shipped ai.* functions
CHANGELOG.md — [Unreleased] M7-S3 entry
```

#### Deep file dependency analysis
- `smoke.sh` (Baseline row, invariant: existing checks green): additive presence block; uses the existing `psql` harness; NO network call (asserts existence + privilege only).
- `.github/workflows/ci.yml` (invariant: existing jobs stay): additive `ai-sql` job; reuses buildx cache; spawns the stub server (offline); `timeout-minutes` per convention.
- `docs/sql-ai-functions.md` (NEW) + spec note: additive docs.

#### Deep Dives
- **Smoke presence check:** `SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname='ai' AND p.proname IN ('generate','if','analyze_sentiment','summarize','rank')` = 5; assert each is not PUBLIC-executable. No HTTP.
- **CI job:** build image → start container (`host.docker.internal` gateway) → run `pytest -k 'not real'` (stub server spawned by the fixture). No external API, deterministic.
- **Doc honesty:** state plainly that scalar functions are synchronous (one round-trip per row) and that array/cursor accelerated modes are a follow-up; document the api-key-in-logs caveat.

#### Tasks
1. Append the presence + privilege assertion to `smoke.sh`.
2. Add the `ai-sql` CI job (offline stub suite) with `timeout-minutes`.
3. Write `docs/sql-ai-functions.md` + the spec implemented-surface note; add the CHANGELOG entry.

#### TDD
```
RED:     smoke ai.* presence assertion fails against an image WITHOUT sql/50 (count != 5).
GREEN:   With sql/50 baked, `bash smoke.sh` prints the ai.* presence line + SMOKE PASSED.
REFACTOR: factor the presence SQL into a heredoc; else "None expected".
VERIFY:  docker build -t theo-db:dev . && PGPORT=<p> bash smoke.sh
```

#### Concurrency tests

**Concurrency posture: (none — single-threaded)** — smoke/CI/doc changes are sequential shell + YAML + markdown; no concurrent state.

#### Acceptance Criteria
- [ ] `bash smoke.sh` against a fresh `theo-db:dev` prints the `ai.*` presence line (5 functions, non-PUBLIC) + `SMOKE PASSED`; a missing function exits non-zero.
- [ ] CI `ai-sql` job parses + runs the offline suite — `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"` parses; job has `timeout-minutes`.
- [ ] `docs/sql-ai-functions.md` exists documenting the `theodb.llm_*` GUCs, each function's signature, and the synchronous-cost + api-key-in-logs caveats.
- [ ] Pass: size — changed files `wc -l` within budget.

#### DoD
- [ ] All tasks completed and validated
- [ ] Smoke green; CI job parses and runs locally-validated steps
- [ ] Docs committed
- [ ] CHANGELOG `[Unreleased]` updated
- [ ] File-size budget respected

---

## Coverage Matrix

| # | Gap / Requirement (ROADMAP M7 DoD-4 + specs) | Task(s) | Resolution |
|---|---|---|---|
| 1 | `ai.generate` over a configurable model | T1.1 | `ai.generate` → `ai._chat` |
| 2 | `ai.if` (conditional classification) | T1.1, T2.1 | boolean parse + contract test |
| 3 | `ai.analyze_sentiment` (positive/negative/neutral) | T1.1, T2.1 | label-set parse + contract test |
| 4 | `ai.summarize` (content summarization) | T1.1, T2.1 | text return + contract test |
| 5 | `ai.rank` (numeric scoring) | T1.1, T2.1 | float parse + contract test |
| 6 | Configurable local/remote model (model-agnostic) | T1.1 | `theodb.llm_*` GUCs (D2) |
| 7 | Per-function contract test | T2.1 | offline stub (deterministic) + opt-in real-OpenAI |
| 8 | Security: outbound HTTP locked down + no secret leak | T1.1, T3.1 | SSRF guard + REVOKE PUBLIC + api-key-not-in-errors + doc caveat |
| 9 | End-to-end runtime evidence (smoke + CI) | T3.1 | presence smoke + `ai-sql` CI job |
| 10 | No new dependency / no AGPL | T1.1 | plpython3u + stdlib only |

**Coverage: 10/10 gaps covered (100%)**

## Global Definition of Done

- [ ] All phases completed
- [ ] All tests passing — `cd benchmarks && pytest -m integration tests/test_ai_sql.py -k 'not real' -q` green (offline) + real-OpenAI verified manually
- [ ] Zero lint warnings — `cd benchmarks && ruff check tests/test_ai_sql.py`
- [ ] File-size budget respected (per `rules/architecture.md`)
- [ ] CHANGELOG.md updated under `[Unreleased]` (Unbreakable Rule 6)
- [ ] Backward compatibility preserved — `theodb.embed`, `ai.hybrid_search_rrf`, `VectorDB` unchanged
- [ ] Plan-specific: 6 functions load from initdb.d, all REVOKEd from PUBLIC; each function has a passing contract test
- [ ] Runtime-metric proof — the offline contract suite observes each function returning its typed result against the stub (not just compiling); real-OpenAI sentiment polarity observed in manual validation
- [ ] Plan archived after `/review` READY_TO_MERGE + PR merge

## Failure scenarios (external I/O)

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| chat-completions endpoint (HTTP, `theodb.llm_endpoint`) | endpoint GUC unset | call `ai.generate('x')` with no GUC set | typed error SQLSTATE `22023` (fail-fast, no silent NULL) — `test_endpoint_unset_raises_typed` |
| chat-completions endpoint (HTTP) | connection refused / timeout / 5xx | point GUC at a closed port (stub not started) | typed error SQLSTATE `38000` (URLError/OSError) — asserted |
| chat-completions endpoint (HTTP) | 200 with malformed / non-conforming body | stub `malformed` mode returns prose for `ai.if`/`ai.rank` | typed error SQLSTATE `22023` (D4 — unparseable output, never a silent wrong boolean/float) |
| chat-completions endpoint (HTTP) | SSRF attempt (non-http scheme / redirect to internal) | set GUC to `file://…` | typed error `22023` (http(s)-only guard); redirects disabled in the opener |

## Final Phase: Integration Validation (MANDATORY)

**Objective:** Validate the generative functions end-to-end against a real container (offline stub) + a real OpenAI call.

### Execution
```
docker build -t theo-db:dev .
docker run -d --name m7s3-it --add-host=host.docker.internal:host-gateway -e POSTGRES_PASSWORD=postgres -p <port>:5432 theo-db:dev
PGPORT=<port> bash smoke.sh                                       # ai.* presence + non-PUBLIC
cd benchmarks && pip install -r requirements.txt
PGHOST=localhost PGPORT=<port> PGUSER=postgres PGPASSWORD=postgres PGDATABASE=postgres \
  pytest -m integration tests/test_ai_sql.py -k 'not real' -q     # offline stub contract suite
# real-OpenAI (manual, key from gitignored .env):
THEODB_LLM_ENDPOINT=https://api.openai.com/v1/chat/completions THEODB_LLM_MODEL=gpt-4o-mini \
  OPENAI_API_KEY=$(grep ^OPENAI_API_KEY .env | cut -d= -f2) \
  pytest -m integration tests/test_ai_sql.py -k real -q
ruff check tests/test_ai_sql.py
```

### Acceptance Criteria
- [ ] All offline contract tests green (per-function happy + parse + negative + endpoint-unset)
- [ ] Real-OpenAI sentiment polarity test passes when the `.env` key is supplied (shape/contract asserted, not exact text)
- [ ] Zero lint warnings
- [ ] Runtime-metric proof — each function observed returning its typed result against the stub; real sentiment polarity observed
- [ ] Failure scenarios green — endpoint-unset (`22023`), connection-refused (`38000`), malformed-output (`22023`), SSRF (`22023`) all observed
