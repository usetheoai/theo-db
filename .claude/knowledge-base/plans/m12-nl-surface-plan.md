---
slug: m12-nl-surface
milestone_id: M12
created_at: 2026-06-28
goal: Ship the theodb_ai_nl config/template/value-index surface over the M7-S4 NL gate, closing docs/features 12 core
---

# Plan: `theodb_ai_nl` config / templates / value-index surface (feature 12)

> **Version 1.0** — The M7-S4 MVP shipped the *safe* NL→SQL gate (`ai.nl_to_sql`/`ai.nl_query`: L1 prompt +
> L2 static validation + L4 parser-grade EXPLAIN allowlist + L3 read-only sandbox). Spec 12 also asks for a
> **configuration surface**: persisted schema context, a prompt-template registry, and a categorical
> value-index. This slice ships those three core capabilities in schema `ai`, **reusing the M7-S4 gate
> verbatim** — the config only ENRICHES the prompt and supplies the allowed-relation list; it NEVER relaxes
> the deterministic L2/L4/L3 validation. The full 58-function AlloyDB `theodb_ai_nl` extension is the target;
> we ship the core in `ai` (documented divergence — ADR D1, same posture as M7-S4).

## Goal

> Ship the `theodb_ai_nl` core config surface (`ai.nl_config` + `ai.nl_templates` + `ai.nl_value_index` +
> `ai.nl_query_cfg`) over the unchanged M7-S4 gate, measured by an integration test where a registered config
> drives a NL→SQL query AND an adversarial question through the config is still blocked (DB intact), plus a
> real-OpenAI evidence run.

## Context

`docs/features/12-linguagem-natural.md` documents the AlloyDB `theodb_ai_nl` extension (~58 functions:
`g_create_configuration`, `generate_schema_context`, `add_template`, `create_value_index`, …). M7-S4
shipped the security-critical core — the safe generate+execute gate (`sql/60-theodb-nl.sql`). The
*configuration* surface (register schema context / templates / value-index once, reuse across queries) was
deferred. This slice delivers the three core capabilities, composed over the existing gate (Rule 9): the
config is convenience + prompt-enrichment; every generated query still passes the identical L2 denylist + L4
EXPLAIN relation allowlist + L3 read-only sandbox. **Security is preserved by construction** because the
gate is reused unchanged.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `sql/60-theodb-nl.sql` | 179 | M7-S4 (2026-06-28) | the safe NL gate (`ai.nl_to_sql`/`ai.nl_query`) | **UNCHANGED** — the gate is reused, never modified or weakened |
| `sql/61-theodb-nl-config.sql` (NEW) | 0 | — | (to be created) config/template/value-index surface | — |
| `Dockerfile` | (exists) | — | COPYs sql/*.sql to initdb.d in order | add `COPY sql/61-...` AFTER sql/60 (depends on `ai.nl_query`) |
| `benchmarks/tests/test_nl_sql.py` | (exists) | M7-S4 | NL gate contract + injection tests | existing tests stay green; config tests appended |
| `smoke.sh` | ~121 | M11 | presence/privilege smoke | add `ai.nl_query_cfg` + tables presence check |
| `docs/sql-ai-functions.md` | ~213 | M11 | the ai.* doc | add a `theodb_ai_nl config surface` section |
| `CHANGELOG.md` | (exists) | — | public contract | `[Unreleased]` gets the M12 entry |

Every file in any `#### Files to edit` below appears in this table.

### Current callers / dependents

- **Symbol:** `ai.nl_to_sql(question, allowed_relations, model)` + `ai.nl_query(question, allowed_relations, model, max_rows)` (`sql/60`). The new `ai.nl_query_cfg` becomes a caller of `ai.nl_query` (no change to the gate).
- **Symbol:** the deterministic stub (`tools/chat_server.py`) nl branch keys on the system prompt `"read-only postgresql select"` → returns `SELECT count(*) AS n FROM documents` (benign) or an injection per `__NLINJECT_*__` seam. `ai.nl_query_cfg` enriches the *question* and calls `ai.nl_query` (same system prompt) → the stub behavior is unchanged.
- Enumerated via `grep -nE 'ai\.nl_(to_sql|query)|read-only postgresql select' sql/60-theodb-nl.sql tools/chat_server.py benchmarks/tests/test_nl_sql.py`.

### Domain glossary

- **nl_config** — a named row binding an app to its `allowed_relations`, persisted `schema_context`, an optional template, and a model. Lets the caller register once instead of passing `allowed_relations` per call.
- **template** — a named, enable-able block of domain instructions folded into the prompt CONTEXT (never the security system prompt — it cannot weaken the gate).
- **value-index** — for a categorical column, the set of its distinct allowed values, surfaced to the model as a hint ("status ∈ {active, churned}") so NL maps values correctly.
- **the gate (L1-L4)** — the M7-S4 deterministic defense; reused unchanged. The config enriches the prompt + supplies relations; the gate still validates the generated SQL.

### Architecture boundaries affected

Per `rules/architecture.md`: all new objects live in schema `ai` (interface-layer capability) over the
unchanged M7-S4 gate (the security adapter). No new dependency. The value-index auto-refresh is the only new
dynamic-SQL surface and is guarded (relation ∈ config allowlist + identifier-validated column + read-only).

## Prior Art & Related Work

- **Internal (the gate to reuse):** `sql/60-theodb-nl.sql` (`ai.nl_to_sql`/`ai.nl_query` — L1-L4); `benchmarks/tests/test_nl_sql.py` (gate + injection test pattern + `_setup` seed); `tools/chat_server.py` (`read-only postgresql select` + `__NLINJECT_*__` seams).
- **Internal (discovery):** `knowledge-base/discoveries/blueprints/m7-nl-to-sql-safe-blueprint.md` (the safe-NL design this builds on).
- **External:** AlloyDB `alloydb_ai_nl` reference (the `theodb_ai_nl` surface mirrored) — `docs/features/12-linguagem-natural.md`; PostgreSQL `format()`/`quote_ident` for safe dynamic SQL — `https://www.postgresql.org/docs/current/plpgsql-statements.html#PLPGSQL-QUOTE-LITERAL-EXAMPLE`.
- **Reference:** `.claude/knowledge-base/references/` (when present).

## Objective

- [ ] `ai.nl_config` / `ai.nl_templates` / `ai.nl_value_index` tables (idempotent `CREATE TABLE IF NOT EXISTS`).
- [ ] Management fns: `ai.nl_add_config`, `ai.nl_add_template`, `ai.nl_set_template_enabled`, `ai.nl_set_value_index`, `ai.nl_refresh_value_index` (guarded auto-populate).
- [ ] `ai.nl_query_cfg(question, config_id, max_rows)` — loads config, enriches the prompt (schema_context + enabled template + value-index hints), delegates to the UNCHANGED `ai.nl_query` with the config's allowed_relations.
- [ ] **Anti-injection gate preserved**: an adversarial question through `ai.nl_query_cfg` is still blocked (22023), DB intact — proven by a regression test.
- [ ] `REVOKE ALL ... FROM PUBLIC` on every new function (parity with the gate).
- [ ] `ai.nl_refresh_value_index` is injection-safe: relation must be ∈ the config's allowed_relations; column validated as a plain identifier; runs read-only; `quote_ident`.
- [ ] Real-OpenAI evidence: a registered config drives a real NL→SQL query returning rows.

## ADRs

### D1 — Ship the core surface in schema `ai`, reusing the M7-S4 gate (not a literal `theodb_ai_nl` extension)

**Decision:** Implement config / templates / value-index + `ai.nl_query_cfg` in schema `ai`, composing the
existing `ai.nl_query` gate. Do NOT reproduce the 58-function AlloyDB `theodb_ai_nl` extension.

**Rationale:** Rule 9 / parsimony — the security-critical gate already exists and is the hard part; the
config surface is convenience + prompt-enrichment. The three DoD capabilities (config, templates,
value-index) are the load-bearing core; the rest of the 58 functions (auto-template-from-history,
concept-types, generated-template review workflow) are YAGNI until demanded. Shipping in `ai` keeps one
coherent namespace (same posture M7-S4 took: "full theodb_ai_nl surface deferred").

**Alternatives considered:** *Build the full `theodb_ai_nl` extension* — rejected: ~58 functions, multi-week,
most YAGNI; over-engineering (KISS/YAGNI). *Modify `ai.nl_to_sql` to take schema_context/template params* —
rejected: touching the security gate risks weakening it; enrichment via the question keeps the gate
byte-identical. *A separate `theodb_ai_nl` schema mirroring AlloyDB names* — rejected: cosmetic divergence
from our `ai.*` convention with no functional gain (YAGNI).

**Consequences:** the literal AlloyDB function names are not provided (documented divergence); the capability
parity (register schema context + templates + value-index, NL query by config) is delivered + tested.

### D2 — Config enriches the PROMPT only; the deterministic gate is reused UNCHANGED

**Decision:** `ai.nl_query_cfg` builds an enrichment block (schema_context + enabled template's instructions
+ value-index hints) and PREPENDS it to the question, then calls the unchanged `ai.nl_query(enriched, config.allowed_relations, config.model, max_rows)`. `sql/60` is not modified.

**Rationale:** Security by construction — every generated query still passes L2 (denylist) + L4 (EXPLAIN
relation allowlist over the config's relations) + L3 (read-only sandbox). Because enrichment is prompt text,
it can only *influence* the LLM, never bypass the deterministic guards. This is the honest, safe way to add
configuration without re-opening the injection surface.

**Alternatives considered:** *Let the template replace the security system prompt* — rejected: that would let
config weaken L1; forbidden. *Trust the config's relations without re-validation* — rejected: `ai.nl_query`
re-validates every generated relation against the passed allowlist regardless.

**Consequences:** the template/schema_context cannot relax the gate (a feature, not a limit); the regression
test proves an injection through the config path is still blocked.

### D3 — `nl_refresh_value_index` dynamic SQL is hard-guarded (relation allowlist + identifier validation + read-only)

**Decision:** `ai.nl_refresh_value_index(config_id, relation, column_name, max_values)` auto-populates the
value-index by running `SELECT DISTINCT <col> FROM <relation> LIMIT n`, but ONLY after: (a) the relation is
exactly ∈ the config's `allowed_relations`; (b) `column_name` matches `^[A-Za-z_][A-Za-z0-9_]*$`; (c) the
relation resolves via `to_regclass`; the query runs with `transaction_read_only=on` and `quote_ident` on the
column. An explicit `ai.nl_set_value_index(...)` (operator supplies values, zero dynamic SQL) is also provided.

**Rationale:** auto-refresh-from-data is the real value-index feature, but dynamic SQL is an injection vector
— so it is guarded by the same allowlist discipline as the gate (relation must be operator-vetted in the
config). The explicit setter gives a zero-dynamic-SQL path for deterministic tests + operator control.

**Alternatives considered:** *No auto-refresh (explicit only)* — rejected: the spec's value-index is
data-derived; explicit-only is half the feature. *Unguarded dynamic SQL* — rejected: injection hole.

**Consequences:** refresh works only over relations the operator already allowlisted in the config (safe by
design); arbitrary-table reads are impossible.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Config surface re-opens the injection gate | High | D2 — config enriches the prompt only; the unchanged `ai.nl_query` gate (L2/L4/L3) runs on every query; regression test proves an injection through `nl_query_cfg` is blocked + DB intact | NL |
| `nl_refresh_value_index` dynamic SQL is an injection vector | High | D3 — relation ∈ config allowlist + identifier-validated column + `to_regclass` + read-only + `quote_ident`; test the guard rejects a non-allowlisted relation | NL |
| Schema-context / value-index hints leak sensitive data into the prompt | Medium | operator-controlled (they register the config); documented; value-index capped (`max_values`) | NL |
| Divergence from the literal AlloyDB `theodb_ai_nl` names | Low | documented (ADR D1 + doc); capability parity delivered + tested | NL |

## Unresolved Questions

- Q1 — Auto-generate templates from query history (`generate_templates`)? Resolved: no — YAGNI (D1); not in the DoD; deferred.
- Q2 — Concept-types / fragments (spec steps 23-29)? Resolved at plan time: deferred (YAGNI); the three core capabilities (config/templates/value-index) close the M12 DoD.

## Dependencies

M12 adds **no new dependency** (Unbreakable Rule 9). It composes the shipped M7-S4 gate + `ai._chat`; new
objects are plpython3u/plpgsql/SQL + three tables.

| Dependency | Version | Role | License | Status / CVE |
|---|---|---|---|---|
| `ai.nl_query`/`ai.nl_to_sql` + `ai._chat` | shipped (M7-S4/M7-S3) | the reused safe gate + HTTP helper | PostgreSQL License | already shipped; UNCHANGED |
| `psycopg2` (test, dev-only) | as in requirements.txt | test DB client | LGPL | already a dev dep |

No CVE audit delta: zero new declared dependencies.

## Dependency Graph

```
Phase 1 (tables + management fns + Dockerfile COPY) ──▶ Phase 2 (nl_query_cfg + tests + doc + real evidence + smoke + CHANGELOG)
```

## Phase 1: Config / template / value-index schema + management functions

**Objective:** Persist the config surface + safe management functions; wire the new SQL into the image.

### T1.1 — `sql/61` tables + management functions + REVOKE + Dockerfile COPY

#### Objective
Create the three tables + management functions (incl. the guarded value-index refresh), REVOKE from PUBLIC, and bake `sql/61` after `sql/60`.

#### Why this step (action + reasoning — ReAct discipline)

1. **What this step does** — creates `sql/61-theodb-nl-config.sql` with `ai.nl_config`/`ai.nl_templates`/
   `ai.nl_value_index` (idempotent), `ai.nl_add_config`/`ai.nl_add_template`/`ai.nl_set_template_enabled`/
   `ai.nl_set_value_index`/`ai.nl_refresh_value_index` (D3-guarded), `REVOKE ... FROM PUBLIC`; adds the
   `COPY sql/61-...` to the Dockerfile after sql/60.

2. **Why it is necessary now** — the persisted config surface is the foundation `ai.nl_query_cfg` (Phase 2)
   reads; the guarded refresh is the value-index feature; baking after sql/60 satisfies the gate dependency.

#### Evidence
- Gate to reuse: `sql/60-theodb-nl.sql` (`ai.nl_query` line 124).
- Dockerfile order: `Dockerfile` (`COPY sql/60-theodb-nl.sql` line 73) — sql/61 COPY goes after it.
- Safe dynamic SQL: PostgreSQL `format`/`quote_ident`/`to_regclass`.

#### Files to edit
```
sql/61-theodb-nl-config.sql (NEW) — 3 tables + 5 management fns + REVOKE + COMMENTs
Dockerfile — COPY sql/61-theodb-nl-config.sql to initdb.d (after sql/60)
```

#### Deep file dependency analysis
- `sql/61` (NEW): depends on schema `ai` (created in sql/30/60) — `CREATE SCHEMA IF NOT EXISTS ai` at the top for standalone idempotency.
- `Dockerfile` (Baseline row, invariant: existing COPYs unchanged): one additive COPY after line 73.

#### Deep Dives
- **Tables:** `ai.nl_config(config_id PK, allowed_relations text[] NOT NULL, schema_context text, template_id text, model text, created_at timestamptz default now())`; `ai.nl_templates(template_id PK, system_prompt text NOT NULL, enabled boolean NOT NULL default true)`; `ai.nl_value_index(config_id, relation, column_name, values text[] NOT NULL, refreshed_at timestamptz default now(), PRIMARY KEY(config_id, relation, column_name))`.
- **`nl_refresh_value_index` guard (D3):** load config; assert `relation = ANY(config.allowed_relations)` (else 22023); assert `column_name ~ '^[A-Za-z_][A-Za-z0-9_]*$'` (else 22023); `to_regclass(relation)` not null (else 22023); `set_config('transaction_read_only','on',true)`; `EXECUTE format('SELECT array_agg(v) FROM (SELECT DISTINCT %I::text AS v FROM %s WHERE %I IS NOT NULL ORDER BY 1 LIMIT %s) z', column_name, relation, column_name, max_values)`; upsert into `ai.nl_value_index`.
- **Edge cases:** config not found → 22023; max_values ≤ 0 → 22023; relation not allowlisted → 22023; bad column identifier → 22023.

#### Pseudo-code / Signatures
```sql
CREATE TABLE IF NOT EXISTS ai.nl_config (config_id text PRIMARY KEY, allowed_relations text[] NOT NULL,
  schema_context text, template_id text, model text, created_at timestamptz DEFAULT now());
-- ai.nl_templates, ai.nl_value_index (as above)
CREATE OR REPLACE FUNCTION ai.nl_add_config(p_id text, p_rels text[], p_ctx text DEFAULT NULL,
  p_template text DEFAULT NULL, p_model text DEFAULT NULL) RETURNS void ... -- upsert
CREATE OR REPLACE FUNCTION ai.nl_refresh_value_index(p_config text, p_relation text, p_column text,
  p_max int DEFAULT 50) RETURNS int ...  -- D3-guarded; returns count stored
REVOKE ALL ON FUNCTION ai.nl_add_config(...), ... FROM PUBLIC;
```

#### Tasks
1. Write `sql/61-theodb-nl-config.sql` (tables + 5 fns + REVOKE + COMMENTs).
2. Add the Dockerfile COPY after sql/60.
3. Rebuild `theo-db:dev`; fresh container.

#### TDD
```
RED:     test_nl_config_tables_and_revoke() [integration] — assert ai.nl_config/nl_templates/nl_value_index exist; ai.nl_add_config + ai.nl_add_template upsert; refresh guard rejects a non-allowlisted relation (22023); REVOKE FROM PUBLIC on every new fn. MUST fail before sql/61 exists.
GREEN:   Implement sql/61 + Dockerfile COPY so it passes on the rebuilt image.
REFACTOR: factor a shared upsert if it reduces dup; else "None expected".
VERIFY:  PG*=... pytest -m integration tests/test_nl_sql.py -k 'config or value_index' -q
```

#### Concurrency tests

**Concurrency posture: (none — single-threaded)** — DDL + per-call management functions; no shared mutable state, no locks/async in our code (PostgreSQL serializes the statements).

#### Acceptance Criteria
- [ ] Tables + fns exist + REVOKED — `psql -tAc "SELECT count(*) FROM pg_tables WHERE schemaname='ai' AND tablename IN ('nl_config','nl_templates','nl_value_index')"` returns `3`; `has_function_privilege('public','ai.nl_refresh_value_index(text,text,text,int)','execute')` returns `f`.
- [ ] refresh guard rejects a non-allowlisted relation — `pytest -m integration tests/test_nl_sql.py -k 'value_index_guard' -q` exits `0` (raises 22023).
- [ ] config + template upsert works — `pytest -m integration tests/test_nl_sql.py -k 'add_config' -q` exits `0`.
- [ ] idempotent re-apply — `psql -f sql/61-theodb-nl-config.sql` exits `0` twice.
- [ ] Pass: size — `sql/61` `wc -l` < `500`.

#### DoD
- [ ] All tasks completed and validated — every Acceptance Criteria above exits `0`.
- [ ] Tables/fns green on the rebuilt image; REVOKE verified — `has_function_privilege` returns `f`.
- [ ] CHANGELOG `[Unreleased]` updated — `grep -c 'nl_config\|theodb_ai_nl' CHANGELOG.md` returns `> 0`.
- [ ] File-size budget respected — changed files `wc -l` < `500`.

## Phase 2: `ai.nl_query_cfg` + tests + real evidence + doc

**Objective:** Config-aware NL query over the unchanged gate + prove anti-injection preserved + real evidence.

### T2.1 — `ai.nl_query_cfg` + config tests + anti-injection regression + smoke + doc + CHANGELOG

#### Objective
Add the config-aware query function, prove a config drives a benign query AND an injection through it is still blocked, capture real evidence, document.

#### Why this step (action + reasoning — ReAct discipline)

1. **What this step does** — adds `ai.nl_query_cfg(question, config_id, max_rows)` (loads config, builds the
   enrichment block from schema_context + enabled template + value-index, prepends to the question, calls the
   unchanged `ai.nl_query` with the config's allowed_relations); adds tests (config-driven benign query,
   anti-injection regression through the config path, value-index populated, config-not-found); a real-OpenAI
   evidence test; a smoke presence check; the doc section + CHANGELOG entry.

2. **Why it is necessary now** — it is the user-facing capability of the config surface, and the regression
   test is the proof that the convenience layer did NOT weaken the M7-S4 security gate (the highest risk).

#### Evidence
- Gate reuse: `ai.nl_query` (`sql/60` line 124).
- Stub nl behavior + injection seams: `tools/chat_server.py` (`read-only postgresql select`, `__NLINJECT_DROP__`).
- Test pattern + `_setup` seed: `benchmarks/tests/test_nl_sql.py`.

#### Files to edit
```
sql/61-theodb-nl-config.sql — add ai.nl_query_cfg + REVOKE + COMMENT
benchmarks/tests/test_nl_sql.py — config-driven query, anti-injection regression, value-index, config-not-found, real-OpenAI
smoke.sh — ai.nl_query_cfg + config tables presence/privilege check
docs/sql-ai-functions.md — theodb_ai_nl config surface section
CHANGELOG.md — [Unreleased] M12 entry
```

#### Deep file dependency analysis
- `sql/61` (from T1.1): append `ai.nl_query_cfg` (plpgsql) — reads the config tables, calls `ai.nl_query`.
- `test_nl_sql.py` (Baseline row): appends config tests using the existing `conn`/`chat_server`/`_setup` fixtures.
- `smoke.sh`/`docs` (Baseline rows): additive.

#### Deep Dives
- **`nl_query_cfg`:** load `ai.nl_config` by id (22023 if missing); build `enrichment = schema_context || template.system_prompt (if enabled) || value-index hints ("Column rel.col allowed values: …")`; `enriched := enrichment || E'\n\nQuestion: ' || question`; `RETURN ai.nl_query(enriched, cfg.allowed_relations, cfg.model, max_rows)`. The gate runs unchanged.
- **Anti-injection regression:** `nl_query_cfg('__NLINJECT_DROP__ …', 'cfg1')` → the stub returns `DROP TABLE …` → `ai.nl_query`→`ai.nl_to_sql` rejects (22023) → DB intact (assert the target table still exists + row count unchanged).
- **Edge cases:** config missing → 22023; empty question → 22023 (propagated from the gate); value-index hint formatting with no value-index → benign (no hint).

#### Pseudo-code / Signatures
```sql
CREATE OR REPLACE FUNCTION ai.nl_query_cfg(question text, config_id text, max_rows int DEFAULT 100)
RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE cfg ai.nl_config; tmpl text := ''; hints text := ''; enriched text;
BEGIN
  SELECT * INTO cfg FROM ai.nl_config WHERE config_id = $2;
  IF NOT FOUND THEN RAISE EXCEPTION 'ai.nl_query_cfg: config % not found', $2 USING ERRCODE='22023'; END IF;
  SELECT system_prompt INTO tmpl FROM ai.nl_templates WHERE template_id = cfg.template_id AND enabled;  -- optional
  SELECT string_agg(format('Column %s.%s allowed values: %s', relation, column_name, array_to_string(values, ', ')), E'\n')
    INTO hints FROM ai.nl_value_index WHERE config_id = $2;
  enriched := concat_ws(E'\n', cfg.schema_context, tmpl, hints, '', 'Question: ' || question);
  RETURN ai.nl_query(enriched, cfg.allowed_relations, cfg.model, max_rows);  -- UNCHANGED gate
END $$;
```

#### Tasks
1. Append `ai.nl_query_cfg` + REVOKE + COMMENT to `sql/61`.
2. Append config tests (benign, anti-injection regression, value-index, config-not-found) to `test_nl_sql.py`.
3. Rebuild; run; capture real-OpenAI evidence in the implementation log.
4. Add smoke check + doc section + CHANGELOG entry.

#### TDD
```
RED:     test_nl_query_cfg_benign_returns_rows() + test_nl_query_cfg_injection_blocked_db_intact() — register a config over 'documents'; benign question -> rows; __NLINJECT_DROP__ through the config -> 22023 AND documents table intact. MUST fail before ai.nl_query_cfg exists.
GREEN:   Implement ai.nl_query_cfg so both pass on the rebuilt image.
REFACTOR: none expected.
VERIFY:  PG*=... pytest -m integration tests/test_nl_sql.py -k 'nl_query_cfg' -q
```

#### Concurrency tests

**Concurrency posture: (none — single-threaded)** — the config-aware query is one sequential gate call; no shared mutable state, no locks/async.

#### Acceptance Criteria
- [ ] Config drives a benign query — `PGHOST=... pytest -m integration tests/test_nl_sql.py -k 'nl_query_cfg_benign' -q` exits `0` (returns rows).
- [ ] **Anti-injection preserved through the config** — `pytest -m integration tests/test_nl_sql.py -k 'nl_query_cfg_injection' -q` exits `0` (22023 + target table intact).
- [ ] `ai.nl_query_cfg` REVOKED from PUBLIC — `psql -tAc "SELECT has_function_privilege('public','ai.nl_query_cfg(text,text,int)','execute')"` returns `f`.
- [ ] config not found → 22023 — covered by `pytest ... -k 'nl_query_cfg' -q` exit `0`.
- [ ] Real-OpenAI evidence recorded — `grep -ci 'nl_query_cfg' knowledge-base/implementations/m12-nl-surface-implementation.md` returns `> 0`.
- [ ] Doc + smoke present — `grep -c 'nl_query_cfg' docs/sql-ai-functions.md smoke.sh` returns `> 0`.
- [ ] No regression to the M7-S4 gate — `pytest -m integration tests/test_nl_sql.py -k 'not real' -q` exits `0`.
- [ ] Pass: size — changed files `wc -l` < `500`.

#### DoD
- [ ] All tasks completed and validated — every Acceptance Criteria above exits `0`.
- [ ] `ai.nl_query_cfg` green; anti-injection regression green; REVOKE verified.
- [ ] CHANGELOG `[Unreleased]` updated — `grep -c 'nl_query_cfg\|nl_config' CHANGELOG.md` returns `> 0`.
- [ ] File-size budget respected — changed files `wc -l` < `500`.

## Coverage Matrix

| # | Gap / Requirement | Task(s) | Resolution |
|---|---|---|---|
| 1 | feature 12 config (table/GUC) | T1.1 | `ai.nl_config` table + `ai.nl_add_config` |
| 2 | feature 12 template registry | T1.1 | `ai.nl_templates` + add/enable/disable fns |
| 3 | feature 12 value-index (categorical) | T1.1 | `ai.nl_value_index` + `ai.nl_set_value_index` + guarded `ai.nl_refresh_value_index` |
| 4 | config-driven NL query | T2.1 | `ai.nl_query_cfg` over the unchanged gate |
| 5 | **anti-injection defense preserved** | T2.1 | gate reused unchanged (D2); regression test (injection blocked + DB intact) |
| 6 | refresh dynamic-SQL injection-safe | T1.1 | D3 guard (relation allowlist + identifier validation + read-only); guard test |
| 7 | REVOKE FROM PUBLIC (security parity) | T1.1, T2.1 | REVOKE on every new fn |
| 8 | real functional evidence (not stub-only) | T2.1 | real-OpenAI config-driven query + logged output |
| 9 | no regression to M7-S4 gate; no new dep | T2.1 | existing nl tests green; sql/60 unchanged; Rule 9 |

**Coverage: 9/9 gaps covered (100%)**

## Global Definition of Done

- [ ] All phases completed — every phase DoD above exits `0`.
- [ ] NL suite green — `PGHOST=... pytest -m integration tests/test_nl_sql.py -k 'not real' -q` exits `0` (gate + config, no regression).
- [ ] **Anti-injection preserved through the config path** — injection via `ai.nl_query_cfg` → 22023 + DB intact (regression test green).
- [ ] All new fns REVOKED from PUBLIC — `has_function_privilege('public', …, 'execute')` returns `f` for each.
- [ ] `sql/60` (the gate) UNCHANGED — `git diff --name-only` does not list `sql/60-theodb-nl.sql`.
- [ ] Real-OpenAI evidence captured — implementation log has the real config-driven query output (or documented clean skip).
- [ ] File-size budget respected — changed files `wc -l` < `500` (per `rules/architecture.md`).
- [ ] CHANGELOG.md updated under `[Unreleased]` — `grep -c 'nl_query_cfg\|nl_config' CHANGELOG.md` returns `> 0` (Unbreakable Rule 6).
- [ ] No new dependency (Rule 9) — image + requirements unchanged.
- [ ] Plan archived after `/review` READY_TO_MERGE + PR merge.

## Failure scenarios (external I/O)

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| chat endpoint (`ai._chat` via the gate) | endpoint unset | `nl_query_cfg` with no `theodb.llm_endpoint` | typed `22023` ("endpoint is not set") propagated from `ai._chat` |
| chat endpoint | adversarial reply (DROP/exfil/non-allowlisted relation) | `__NLINJECT_*__` seam through the config | gate rejects with `22023`; DB intact (the whole point of M12 preserving the gate) |
| config store | config_id not found | `nl_query_cfg('q','missing')` | typed `22023` ("config not found") |
| value-index refresh | non-allowlisted relation / bad column identifier | refresh over a relation not in the config | typed `22023` (D3 guard) — no dynamic read of an arbitrary table |
| relation | relation does not resolve | refresh over a non-existent (but allowlisted-typo) relation | `to_regclass` NULL → typed `22023` |

## Final Phase: Integration Validation (MANDATORY)

**Objective:** Validate the config surface end-to-end against a freshly-baked container.

### Execution
```
docker build -t theo-db:dev .                                   # bake sql/61
docker run -d --name m12-it --add-host=host.docker.internal:host-gateway \
  -e POSTGRES_PASSWORD=postgres -p <port>:5432 theo-db:dev      # wait healthy
cd benchmarks && pip install -r requirements.txt
PGHOST=localhost PGPORT=<port> PGUSER=postgres PGPASSWORD=postgres PGDATABASE=postgres \
  pytest -m integration tests/test_nl_sql.py -k 'not real' -q              # gate + config
# real evidence (opt-in; loads .env):
THEODB_LLM_ENDPOINT=... OPENAI_API_KEY=... PG*=... \
  pytest -m integration tests/test_nl_sql.py -k 'nl_query_cfg and real' -q
psql -f sql/61-theodb-nl-config.sql   # idempotent re-apply exits 0
```

### Acceptance Criteria
- [ ] Config-driven benign query green; injection through the config blocked + DB intact — `PGHOST=... pytest -m integration tests/test_nl_sql.py -k 'nl_query_cfg' -q` exits `0`.
- [ ] All new fns REVOKED from PUBLIC; refresh guard rejects non-allowlisted relation — `psql -tAc "SELECT has_function_privilege('public','ai.nl_query_cfg(text,text,int)','execute')"` returns `f`.
- [ ] Real-OpenAI evidence captured (or documented clean skip); sql/60 unchanged — `git diff --name-only origin/main..HEAD | grep -c '^sql/60-theodb-nl.sql$'` returns `0`.
- [ ] No regression to the M7-S4 gate; idempotent re-apply of sql/61 — `PGHOST=... pytest -m integration tests/test_nl_sql.py -k 'not real' -q` exits `0`.
