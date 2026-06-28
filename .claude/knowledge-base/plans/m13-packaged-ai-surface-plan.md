---
slug: m13-packaged-ai-surface
milestone_id: M13
created_at: 2026-06-28
goal: Ship the literal ai.hybrid_search(jsonb) API + a theodb_ml model registry over existing capabilities, closing docs/features 06/07 surface
---

# Plan: native packaged AI surface — `ai.hybrid_search()` JSON + `theodb_ml` registry (features 06/07)

> **Version 1.0** — features 06 (hybrid search) and 07 (AI SQL functions) shipped their CAPABILITIES
> (`ai.hybrid_search_rrf(...)` with explicit args; the `ai.*` scalar/agg/batch fns over `theodb.llm_*`
> session GUCs). The specs also document a literal packaged API surface: `ai.hybrid_search()` taking a JSON
> config, and a `theodb_ml` model registry (`create_model`). This slice ships those surfaces over the
> existing capabilities — a thin JSON wrapper for hybrid search, and a `theodb_ml` registry of named
> `(endpoint, model)` configs that bridges to the unchanged `ai._chat` via session GUCs. **Honesty (ADR
> D2): API keys are NEVER persisted in the registry — they stay session GUCs; persisting keys in a table
> would be a security regression.** What is sugar vs. new capability is stated plainly.

## Goal

> Ship `ai.hybrid_search(config jsonb)` (literal spec-06 JSON API over `ai.hybrid_search_rrf`) and the
> `theodb_ml` model registry (`create_model`/`apply_model` over named endpoint+model configs, keys stay
> session GUCs), measured by an integration test where the JSON API returns the same rows as
> `ai.hybrid_search_rrf` AND a registered model applied via `theodb_ml.apply_model` drives an `ai.generate`
> call, plus a real-OpenAI evidence run.

## Context

`docs/features/06-busca-hibrida.md` documents `ai.hybrid_search(...)` (JSON-config entry); we shipped the
capability as `ai.hybrid_search_rrf(tbl, id_col, content_tsv_col, vector_col, query_text, query_vector, k,
per_leg_limit, result_limit)` (M7-S1, `sql/40`). `docs/features/07-funcoes-ia-sql.md` documents a
`theodb_ml` extension with `create_model` registering models referenced by the `ai.*` functions; we shipped
the `ai.*` functions over `theodb.llm_endpoint`/`llm_model`/`llm_api_key` session GUCs (M7-S3, `sql/50`).
M13 closes the literal-surface gap: a thin JSON wrapper + a model registry, both composed over the existing
capabilities (Rule 9), with the API-key persistence deliberately diverged for security (ADR D2).

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `sql/40-theodb-hybrid.sql` | ~110 | M7-S1 (2026-06-28) | `ai.hybrid_search_rrf` (RRF fusion) | existing fn UNCHANGED; `ai.hybrid_search` wrapper is additive |
| `sql/50-theodb-ai.sql` | ~290 | M11 | `ai._chat` + the `ai.*` fns over `theodb.llm_*` GUCs | UNCHANGED — `theodb_ml.apply_model` only SETs the GUCs `ai._chat` already reads |
| `sql/70-theodb-ml.sql` (NEW) | 0 | — | (to be created) `theodb_ml` model registry | — |
| `Dockerfile` | (exists) | — | COPYs sql/*.sql to initdb.d in order | add `COPY sql/70-...` after sql/61 |
| `benchmarks/tests/test_integration.py` | ~470 | M11 | hybrid/index integration tests | existing tests green; hybrid_search JSON tests appended |
| `benchmarks/tests/test_ai_sql.py` | ~400 | M12 | ai.* contract tests (chat stub) | existing tests green; theodb_ml tests appended |
| `smoke.sh` | ~139 | M12 | presence/privilege smoke | add `ai.hybrid_search` + `theodb_ml` presence checks |
| `docs/sql-ai-functions.md` | ~246 | M12 | the ai.* doc | add packaged-surface sections |
| `CHANGELOG.md` | (exists) | — | public contract | `[Unreleased]` gets the M13 entry |

Every file in any `#### Files to edit` below appears in this table.

### Current callers / dependents

- **Symbol:** `ai.hybrid_search_rrf(...)` (`sql/40`) — called by the M7-S1 hybrid tests. The new `ai.hybrid_search(jsonb)` becomes a caller (thin wrapper). The existing fn is unchanged.
- **Symbol:** `ai._chat` reads `theodb.llm_endpoint`/`llm_model`/`llm_api_key` via `current_setting(..., true)` (`sql/50:25-39`). `theodb_ml.apply_model` SETs `theodb.llm_endpoint`/`llm_model` for the session — it does NOT modify `ai._chat`.
- Enumerated via `grep -nE 'ai\.hybrid_search_rrf|current_setting|theodb\.llm_' sql/40-theodb-hybrid.sql sql/50-theodb-ai.sql`.

### Domain glossary

- **`ai.hybrid_search(jsonb)`** — literal spec-06 JSON API; parses a config object and delegates to `ai.hybrid_search_rrf`.
- **`theodb_ml` registry** — a schema with `models(model_id, endpoint, model_name)` + `create_model`/`apply_model`; named endpoint+model configs. **No api_key column** — keys stay session GUCs (ADR D2).
- **`apply_model(model_id)`** — SETs the session GUCs (`theodb.llm_endpoint`/`llm_model`) from a registry row, bridging the registry to the unchanged `ai._chat` (the API key is set separately, per session).
- **sugar vs capability** — `ai.hybrid_search` is ergonomic sugar over an existing fn; the `theodb_ml` registry is a real new capability (named multi-endpoint/model configs) minus key persistence.

### Architecture boundaries affected

Per `rules/architecture.md`: `ai.hybrid_search` is an interface-layer convenience over `ai.hybrid_search_rrf`;
`theodb_ml` is a new registry schema that bridges to the unchanged `ai._chat` adapter via session GUCs. No
new dependency; no modification to `ai._chat` or `ai.hybrid_search_rrf`.

## Prior Art & Related Work

- **Internal (capabilities to wrap):** `sql/40-theodb-hybrid.sql` (`ai.hybrid_search_rrf`); `sql/50-theodb-ai.sql` (`ai._chat` + GUC reads); `benchmarks/tests/test_integration.py` (hybrid test pattern) + `test_ai_sql.py` (chat-stub pattern).
- **Internal (discovery):** `knowledge-base/discoveries/blueprints/m7-hybrid-search-rrf-blueprint.md` + `knowledge-base/discoveries/blueprints/alloydb-vector-ai-implementation-blueprint.md` (the `ai.hybrid_search` / `theodb_ml` surfaces mirrored).
- **External:** AlloyDB `google_ml_integration` `create_model` + `ai.hybrid_search` reference — `docs/features/06-busca-hibrida.md`, `docs/features/07-funcoes-ia-sql.md`; PostgreSQL `jsonb` operators (`->>`, `?`) + `set_config`.
- **Reference:** `.claude/knowledge-base/references/` (when present).

## Objective

- [ ] `ai.hybrid_search(config jsonb)` — parses table/id_col/content_tsv_col/vector_col/query_text/query_vector/k/per_leg_limit/result_limit and delegates to `ai.hybrid_search_rrf`; returns the SAME `TABLE(id text, score real)`; fail-fast 22023 on missing required keys.
- [ ] `theodb_ml.models(model_id, endpoint, model_name)` table + `theodb_ml.create_model`/`drop_model`/`list_models`/`apply_model`. **No api_key column.**
- [ ] `theodb_ml.create_model` validates the endpoint is `http(s)://` (SSRF-consistent with `ai._chat`); `apply_model` SETs the session GUCs (bridges to the unchanged `ai._chat`).
- [ ] `REVOKE ALL ... FROM PUBLIC` on every new function (parity with `ai.*`).
- [ ] `ai.hybrid_search_rrf` + `ai._chat` UNCHANGED (the wrappers compose them).
- [ ] Real-OpenAI evidence: a registered model applied via `theodb_ml.apply_model` drives a real `ai.generate` call.

## ADRs

### D1 — `ai.hybrid_search(jsonb)` is a thin wrapper (literal surface; honest sugar)

**Decision:** Ship `ai.hybrid_search(config jsonb)` as a thin wrapper that parses the JSON config and delegates
to the unchanged `ai.hybrid_search_rrf`. Returns the identical `TABLE(id text, score real)`.

**Rationale:** Rule 9 — reuse the RRF fusion fn; the wrapper only adds the spec-06 JSON calling convention.
It is ergonomic sugar (a JSON entry for callers that build config dynamically), honestly labeled as such — it
adds no fusion capability. Cheap, testable, gives the literal surface the spec documents.

**Alternatives considered:** *Skip it (the explicit-arg fn suffices)* — rejected: the spec documents the JSON
API; closing the surface is the M13 DoD. *A new fusion implementation* — rejected: there is one RRF
definition (M7-S1 ADR); a wrapper must not fork it.

**Consequences:** two entry points to one fusion (explicit args + JSON); the wrapper is sugar, documented.

### D2 — `theodb_ml` registry stores endpoint+model ONLY; API keys stay session GUCs (security divergence)

**Decision:** `theodb_ml.models` has columns `(model_id, endpoint, model_name)` — **no `api_key` column**.
`theodb_ml.apply_model(model_id)` SETs `theodb.llm_endpoint`/`theodb.llm_model` for the session; the API key
is set separately by the caller (`SET theodb.llm_api_key = ...`, per session). The literal AlloyDB
`create_model` (which stores credentials) is deliberately diverged.

**Rationale:** persisting API keys in a table is a security regression — the table lands in `pg_dump`, logical
replication, base backups, and `SELECT`-by-any-grantee. The shipped posture (M7-S3) keeps the key a session
GUC, set out of band, never in logged DDL (the `ai._chat` COMMENT states this). The registry delivers the
real capability (named multi-endpoint/model configs, per-call selection via `apply_model`) WITHOUT that
regression. Bridging via session GUCs means `ai._chat` is reused UNCHANGED (its SSRF guard re-validates the
endpoint anyway).

**Alternatives considered:** *Store the key in the registry (literal AlloyDB parity)* — rejected: security
regression (persisted credentials). *Store an encrypted key / secret reference* — rejected: YAGNI + a secrets
manager is out of scope; the session-GUC path already solves it. *Modify `ai._chat` to resolve a model_id* —
rejected: touches the HTTP/SSRF adapter; the GUC bridge keeps it unchanged.

**Consequences:** the registry is endpoint+model only; the key is supplied per session (documented). This is
a documented divergence from the literal spec, justified by security.

### D3 — `apply_model` bridges via `set_config` (no change to `ai._chat`)

**Decision:** `apply_model` does `PERFORM set_config('theodb.llm_endpoint', endpoint, false)` +
`set_config('theodb.llm_model', model_name, false)` (session-level), then `ai.*` calls use the applied model.

**Rationale:** wires the registry to the existing GUC-based `ai._chat` without modifying it (KISS, Rule 9).
The endpoint was http(s)-validated at `create_model`; `ai._chat` re-validates anyway (defense in depth).

**Alternatives considered:** *Per-call model_id arg threaded through every `ai.*`* — rejected: large
surface change for no capability gain over the session bridge.

**Consequences:** model selection is session-scoped (apply once, then call `ai.*`); documented.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Operators expect literal AlloyDB `create_model` with key storage | Medium | ADR D2 + doc: keys are session GUCs by design (security); `apply_model` bridges; documented plainly | AI |
| `ai.hybrid_search` JSON wrapper diverges from `ai.hybrid_search_rrf` behavior | Medium | wrapper delegates verbatim; a parity test asserts identical rows for the same config | AI |
| `apply_model` endpoint could be SSRF | Low | `create_model` validates `http(s)://`; `ai._chat` re-validates (SSRF guard) at call time | AI |
| Registry / wrapper re-opens an injection surface | Low | no dynamic user SQL; `ai.hybrid_search` passes typed args to the existing fn; `theodb_ml` is plain table DML | AI |

## Unresolved Questions

- Q1 — Should `ai.*` take a `model_id` arg resolving the registry per call? Resolved: no — session bridge via `apply_model` (D3); per-call arg is YAGNI.
- Q2 — Encrypted key storage in the registry? Resolved at plan time: no — keys stay session GUCs (D2); a secrets manager is out of scope.

## Dependencies

M13 adds **no new dependency** (Unbreakable Rule 9). It composes `ai.hybrid_search_rrf` + the GUC-based
`ai._chat`; new objects are SQL/plpgsql + one registry table.

| Dependency | Version | Role | License | Status / CVE |
|---|---|---|---|---|
| `ai.hybrid_search_rrf` (`sql/40`) | shipped (M7-S1) | the RRF fusion the JSON wrapper delegates to | PostgreSQL License | already shipped; UNCHANGED |
| `ai._chat` + `theodb.llm_*` GUCs (`sql/50`) | shipped (M7-S3) | the chat path `apply_model` bridges to | PostgreSQL License | already shipped; UNCHANGED |
| `psycopg2` (test, dev-only) | as in requirements.txt | test DB client | LGPL | already a dev dep |

No CVE audit delta: zero new declared dependencies.

## Dependency Graph

```
Phase 1 (ai.hybrid_search wrapper + theodb_ml registry + Dockerfile COPY) ──▶ Phase 2 (tests + real evidence + smoke + doc + CHANGELOG)
```

## Phase 1: `ai.hybrid_search(jsonb)` + `theodb_ml` registry

**Objective:** Ship both literal surfaces over the existing capabilities; wire into the image.

### T1.1 — `ai.hybrid_search` wrapper (sql/40) + `theodb_ml` registry (sql/70) + REVOKE + Dockerfile COPY

#### Objective
Add the JSON hybrid wrapper, the theodb_ml registry (table + 4 fns), REVOKE from PUBLIC, and bake sql/70.

#### Why this step (action + reasoning — ReAct discipline)

1. **What this step does** — appends `ai.hybrid_search(config jsonb)` to `sql/40` (parses config → delegates
   to `ai.hybrid_search_rrf`); creates `sql/70-theodb-ml.sql` with `theodb_ml.models` + `create_model`
   (http(s)-validated)/`drop_model`/`list_models`/`apply_model` (session-GUC bridge); `REVOKE ... FROM
   PUBLIC`; adds the `COPY sql/70-...` to the Dockerfile after sql/61.

2. **Why it is necessary now** — these are the two literal packaged surfaces of specs 06/07; both compose
   existing capabilities (parsimony) and need to be in the baked image for the tests + smoke.

#### Evidence
- RRF signature: `sql/40-theodb-hybrid.sql:27-38` (`ai.hybrid_search_rrf(tbl regclass, id_col, content_tsv_col, vector_col, query_text, query_vector, k, per_leg_limit, result_limit) RETURNS TABLE(id text, score real)`).
- GUC reads: `sql/50-theodb-ai.sql:25-39` (`current_setting('theodb.llm_endpoint', true)` etc.).
- Dockerfile order: `Dockerfile:77` (`COPY sql/61-...`) — sql/70 COPY goes after.

#### Files to edit
```
sql/40-theodb-hybrid.sql — add ai.hybrid_search(jsonb) wrapper + REVOKE
sql/70-theodb-ml.sql (NEW) — theodb_ml schema + models table + create_model/drop_model/list_models/apply_model + REVOKE
Dockerfile — COPY sql/70-theodb-ml.sql to initdb.d (after sql/61)
```

#### Deep file dependency analysis
- `sql/40` (Baseline row, invariant: `ai.hybrid_search_rrf` unchanged): additive wrapper + a REVOKE line.
- `sql/70` (NEW): `CREATE SCHEMA IF NOT EXISTS theodb_ml`; table + 4 fns; depends only on stdlib SQL.
- `Dockerfile` (Baseline row): one additive COPY.

#### Deep Dives
- **`ai.hybrid_search(jsonb)`:** require keys `table`,`id_col`,`content_tsv_col`,`vector_col` (else 22023); `RETURN QUERY SELECT * FROM ai.hybrid_search_rrf((config->>'table')::regclass, config->>'id_col', config->>'content_tsv_col', config->>'vector_col', config->>'query_text', CASE WHEN config ? 'query_vector' THEN (config->>'query_vector')::vector END, coalesce((config->>'k')::int,60), coalesce((config->>'per_leg_limit')::int,20), coalesce((config->>'result_limit')::int,5))`. STABLE.
- **`theodb_ml.create_model(p_id, p_endpoint, p_model_name DEFAULT NULL)`:** validate `p_endpoint ~ '^https?://'` (else 22023); upsert into models. **No key.**
- **`theodb_ml.apply_model(p_id)`:** load row (22023 if not found); `PERFORM set_config('theodb.llm_endpoint', endpoint, false)`; if model_name not null `PERFORM set_config('theodb.llm_model', model_name, false)`; returns void.
- **Edge cases:** missing required hybrid keys → 22023; bad endpoint scheme → 22023; apply unknown model → 22023; drop unknown model → 22023.

#### Pseudo-code / Signatures
```sql
CREATE OR REPLACE FUNCTION ai.hybrid_search(config jsonb) RETURNS TABLE(id text, score real) LANGUAGE plpgsql STABLE AS $$ ... delegate ... $$;
CREATE TABLE IF NOT EXISTS theodb_ml.models (model_id text PRIMARY KEY, endpoint text NOT NULL, model_name text, created_at timestamptz DEFAULT now());
CREATE OR REPLACE FUNCTION theodb_ml.create_model(p_id text, p_endpoint text, p_model_name text DEFAULT NULL) RETURNS void ...;  -- http(s) validate + upsert
CREATE OR REPLACE FUNCTION theodb_ml.apply_model(p_id text) RETURNS void ...;  -- set_config session GUCs
REVOKE ALL ON FUNCTION ai.hybrid_search(jsonb), theodb_ml.create_model(text,text,text), theodb_ml.apply_model(text), ... FROM PUBLIC;
```

#### Tasks
1. Append `ai.hybrid_search(jsonb)` + REVOKE to `sql/40`.
2. Write `sql/70-theodb-ml.sql` (schema + table + 4 fns + REVOKE + COMMENTs).
3. Add the Dockerfile COPY after sql/61.
4. Rebuild `theo-db:dev`; fresh container.

#### TDD
```
RED:     test_theodb_ml_create_apply_drives_generate() + test_hybrid_search_json_matches_rrf() [integration] — create_model + apply_model -> ai.generate works against the stub; ai.hybrid_search(jsonb) returns the same rows as ai.hybrid_search_rrf for the same config. MUST fail before the new objects exist.
GREEN:   Implement sql/40 wrapper + sql/70 registry so both pass on the rebuilt image.
REFACTOR: factor shared upsert if it helps; else "None expected".
VERIFY:  PG*=... pytest -m integration -k 'hybrid_search_json or theodb_ml' -q
```

#### Concurrency tests

**Concurrency posture: (none — single-threaded)** — DDL + per-call functions; no shared mutable state, no locks/async in our code (PostgreSQL serializes the statements; session GUCs are per-connection).

#### Acceptance Criteria
- [ ] `ai.hybrid_search` + `theodb_ml` objects exist + REVOKED — `psql -tAc "SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE (n.nspname='ai' AND p.proname='hybrid_search') OR (n.nspname='theodb_ml' AND p.proname IN ('create_model','drop_model','list_models','apply_model'))"` returns `5`; none PUBLIC-executable.
- [ ] **No api_key column in the registry (security)** — `psql -tAc "SELECT count(*) FROM information_schema.columns WHERE table_schema='theodb_ml' AND table_name='models' AND column_name ILIKE '%key%'"` returns `0`.
- [ ] JSON hybrid parity — `PGHOST=... pytest -m integration tests/test_integration.py -k 'hybrid_search_json' -q` exits `0` (same rows as rrf).
- [ ] registry drives ai.generate — `pytest -m integration tests/test_ai_sql.py -k 'theodb_ml' -q` exits `0`.
- [ ] idempotent re-apply — `psql -f sql/70-theodb-ml.sql` exits `0` twice.

#### DoD
- [ ] All tasks completed and validated — every Acceptance Criteria above exits `0`.
- [ ] Both surfaces green on the rebuilt image; REVOKE verified — none PUBLIC-executable.
- [ ] CHANGELOG `[Unreleased]` updated — `grep -c 'hybrid_search\|theodb_ml' CHANGELOG.md` returns `> 0`.
- [ ] File-size budget respected — changed files `wc -l` < `500`.

## Phase 2: Tests + real evidence + smoke + doc + CHANGELOG

**Objective:** Prove parity + the registry bridge + key-not-persisted, capture real evidence, document the honest divergence.

### T2.1 — Tests + real-OpenAI evidence + smoke + doc + CHANGELOG + ADR divergence note

#### Objective
Add the parity + registry tests, run a real-OpenAI evidence test, add smoke + doc (incl. the security divergence) + CHANGELOG.

#### Why this step (action + reasoning — ReAct discipline)

1. **What this step does** — adds tests (JSON↔rrf parity, create_model+apply_model→ai.generate, key-not-in-registry,
   bad-endpoint/unknown-model 22023, revoke); runs an opt-in real-OpenAI test (apply a registered real model →
   `ai.generate`) and logs it; adds smoke presence checks; documents the packaged surface + the D2 security
   divergence in `docs/sql-ai-functions.md`; CHANGELOG entry.

2. **Why it is necessary now** — the mandate requires real functional evidence; the doc must state plainly
   what is sugar (hybrid_search) vs capability (registry) and why keys are not persisted (D2).

#### Evidence
- Parity target: `ai.hybrid_search_rrf` rows (`test_integration.py` hybrid tests).
- Registry bridge: `ai._chat` GUC reads (`sql/50`); chat stub (`tools/chat_server.py`).
- Doc format: `docs/sql-ai-functions.md` existing sections.

#### Files to edit
```
benchmarks/tests/test_integration.py — ai.hybrid_search JSON parity tests
benchmarks/tests/test_ai_sql.py — theodb_ml create/apply/drop + key-not-persisted + real-OpenAI tests
smoke.sh — ai.hybrid_search + theodb_ml presence/privilege checks
docs/sql-ai-functions.md — packaged surface section + D2 security divergence
CHANGELOG.md — [Unreleased] M13 entry
```

#### Deep file dependency analysis
- `test_integration.py` (Baseline row): appends JSON-parity tests using the existing hybrid seed/fixtures.
- `test_ai_sql.py` (Baseline row): appends theodb_ml tests using the `conn`/`chat_server` fixtures.
- `smoke.sh`/`docs` (Baseline rows): additive.

#### Deep Dives
- **Parity test:** seed a documents-shaped table; call `ai.hybrid_search_rrf(...)` and `ai.hybrid_search('{...}'::jsonb)` with the same config (explicit `query_vector` for determinism, no stub needed); assert identical `(id, score)` rows.
- **Registry bridge test:** `theodb_ml.create_model('m', '<stub-url>', 'stub-chat')`; `theodb_ml.apply_model('m')`; `SET theodb.llm_api_key='x'`; `ai.generate('hi')` → non-empty (uses the applied endpoint). Assert `theodb.llm_endpoint` GUC == the stub url after apply.
- **Key-not-persisted test:** assert no `%key%` column in `theodb_ml.models`.

#### Tasks
1. Add parity + registry tests.
2. Run the real-OpenAI test (apply a registered real model → ai.generate); record output in the impl log.
3. Add smoke checks + doc (with D2 divergence) + CHANGELOG.

#### TDD
```
RED:     parity/registry tests fail before the objects exist; real test skips cleanly without .env.
GREEN:   tests pass on the rebuilt image; `grep -c 'hybrid_search\|theodb_ml' docs/sql-ai-functions.md smoke.sh` > 0.
REFACTOR: none expected.
VERIFY:  grep -c 'theodb_ml' docs/sql-ai-functions.md smoke.sh && pytest -m integration -k 'theodb_ml and real' -q
```

#### Concurrency tests

**Concurrency posture: (none — single-threaded)** — sequential tests + docs; no concurrent product state.

#### Acceptance Criteria
- [ ] JSON↔rrf parity proven — `PGHOST=... pytest -m integration tests/test_integration.py -k 'hybrid_search_json' -q` exits `0`.
- [ ] Registry bridge drives ai.generate + GUC applied — `pytest -m integration tests/test_ai_sql.py -k 'theodb_ml' -q` exits `0`.
- [ ] Key-not-persisted asserted — the `%key%`-column test passes (count 0).
- [ ] Real-OpenAI evidence recorded — `grep -ci 'theodb_ml\|apply_model' knowledge-base/implementations/m13-packaged-ai-surface-implementation.md` returns `> 0`.
- [ ] Doc states the D2 security divergence — `grep -ci 'session GUC\|never persist\|not persisted' docs/sql-ai-functions.md` returns `> 0`.
- [ ] No unbenchmarked perf claim — `grep -ciE 'faster than|outperforms' docs/sql-ai-functions.md` returns `0`.
- [ ] Pass: size — changed files `wc -l` < `500`.

#### DoD
- [ ] All tasks completed and validated — every Acceptance Criteria above exits `0`.
- [ ] Parity + registry green; key-not-persisted asserted; REVOKE verified.
- [ ] CHANGELOG `[Unreleased]` updated — `grep -c 'theodb_ml\|hybrid_search' CHANGELOG.md` returns `> 0`.
- [ ] File-size budget respected — changed files `wc -l` < `500`.

## Coverage Matrix

| # | Gap / Requirement | Task(s) | Resolution |
|---|---|---|---|
| 1 | feature 06 literal `ai.hybrid_search()` JSON API | T1.1, T2.1 | thin wrapper over `ai.hybrid_search_rrf` (ADR D1) + parity test |
| 2 | feature 07 `theodb_ml` registry (`create_model`) | T1.1, T2.1 | `theodb_ml.models` + `create_model`/`drop_model`/`list_models` |
| 3 | registry usable by `ai.*` | T1.1, T2.1 | `apply_model` session-GUC bridge to unchanged `ai._chat` (ADR D3) |
| 4 | **API keys never persisted (security)** | T1.1, T2.1 | no `api_key` column (ADR D2) + key-not-persisted test |
| 5 | REVOKE FROM PUBLIC (security parity) | T1.1 | REVOKE on every new fn |
| 6 | existing capabilities UNCHANGED | T1.1 | `ai.hybrid_search_rrf` + `ai._chat` not modified |
| 7 | real functional evidence | T2.1 | real-OpenAI apply_model→ai.generate + logged |
| 8 | honest sugar-vs-capability framing | T2.1 | doc + ADR D1/D2 state what is sugar vs new capability |

**Coverage: 8/8 gaps covered (100%)**

## Global Definition of Done

- [ ] All phases completed — every phase DoD above exits `0`.
- [ ] Packaged-surface suites green — `PGHOST=... pytest -m integration -k 'hybrid_search_json or theodb_ml' -q` exits `0` (and no regression: `-k 'hybrid or not real'`).
- [ ] All new fns REVOKED from PUBLIC — none PUBLIC-executable.
- [ ] **No api_key column in `theodb_ml.models`** — the `%key%`-column count is `0`.
- [ ] `ai.hybrid_search_rrf` + `ai._chat` UNCHANGED — `git diff` shows only additive wrapper/registry.
- [ ] Real-OpenAI evidence captured — impl log has the registered-model `ai.generate` output (or documented clean skip).
- [ ] File-size budget respected — changed files `wc -l` < `500` (per `rules/architecture.md`).
- [ ] CHANGELOG.md updated under `[Unreleased]` — `grep -c 'theodb_ml\|hybrid_search' CHANGELOG.md` returns `> 0` (Unbreakable Rule 6).
- [ ] No new dependency (Rule 9) — image + requirements unchanged.
- [ ] Plan archived after `/review` READY_TO_MERGE + PR merge.

## Failure scenarios (external I/O)

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| chat endpoint (`ai._chat`, HTTP) | endpoint unset after no apply_model | `ai.generate` with no GUC/registry applied | typed `22023` ("endpoint is not set") from `ai._chat` |
| `theodb_ml.create_model` | non-http(s) endpoint | `create_model('m','file:///etc/passwd')` | typed `22023` (scheme guard) |
| `theodb_ml.apply_model` | unknown model_id | `apply_model('nope')` | typed `22023` ("model not found") |
| embed endpoint (`ai.hybrid_search` → `theodb.embed`) | embed endpoint unset (query_vector NULL) | JSON config without query_vector + no embed GUC | typed error from `theodb.embed` (propagated) |
| `ai.hybrid_search` | missing required config key | `ai.hybrid_search('{}'::jsonb)` | typed `22023` (missing table/id_col/...) |

## Final Phase: Integration Validation (MANDATORY)

**Objective:** Validate both surfaces end-to-end against a freshly-baked container.

### Execution
```
docker build -t theo-db:dev .                                   # bake sql/70 + sql/40 wrapper
docker run -d --name m13-it --add-host=host.docker.internal:host-gateway \
  -e POSTGRES_PASSWORD=postgres -p <port>:5432 theo-db:dev      # wait healthy
cd benchmarks && pip install -r requirements.txt
PGHOST=localhost PGPORT=<port> PGUSER=postgres PGPASSWORD=postgres PGDATABASE=postgres \
  pytest -m integration -k 'hybrid_search_json or theodb_ml' -q
# real evidence (opt-in; loads .env):
THEODB_LLM_ENDPOINT=... OPENAI_API_KEY=... PG*=... \
  pytest -m integration tests/test_ai_sql.py -k 'theodb_ml and real' -q
psql -f sql/70-theodb-ml.sql   # idempotent re-apply exits 0
```

### Acceptance Criteria
- [ ] JSON hybrid parity green; registry create/apply drives ai.generate; key-not-persisted asserted — `PGHOST=... pytest -m integration -k 'hybrid_search_json or theodb_ml' -q` exits `0`.
- [ ] All new fns REVOKED from PUBLIC; bad-endpoint/unknown-model → 22023 — `psql -tAc "SELECT count(*) FROM information_schema.columns WHERE table_schema='theodb_ml' AND table_name='models' AND column_name ILIKE '%key%'"` returns `0`.
- [ ] Real-OpenAI evidence captured (or documented clean skip); existing capabilities unchanged — `git diff --name-only origin/main..HEAD | grep -cE '^sql/(40-theodb-hybrid|50-theodb-ai)\.sql$'` shows only additive changes (40 additive wrapper; 50 absent).
- [ ] No regression to hybrid/ai.* suites; idempotent re-apply of sql/70 — `PGHOST=... pytest -m integration -k 'hybrid or not real' -q` exits `0`.
