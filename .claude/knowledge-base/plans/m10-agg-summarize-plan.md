---
slug: m10-agg-summarize
milestone_id: M10
created_at: 2026-06-28
goal: Ship ai.agg_summarize aggregate that summarizes many rows into one summary, closing docs/features 11
---

# Plan: `ai.agg_summarize` — aggregate summarization (feature 11)

> **Version 1.0** — Close `docs/features/11-sumarizacao-conteudo.md`'s aggregate path: the scalar
> `ai.summarize(content)` already ships (M7-S3); spec 11 also asks for **aggregate** summarization
> (many rows → one summary). This slice adds a PostgreSQL aggregate `ai.agg_summarize(text)` built on the
> existing private `ai._chat` helper — mirroring the M7-S3 `ai.*` pattern (SECURITY INVOKER, REVOKE FROM
> PUBLIC, deterministic-stub test + a real-OpenAI evidence test). No new dependency.

## Goal

> Ship `ai.agg_summarize(text)` — a SQL aggregate that summarizes a set of rows into one summary via
> `ai._chat` — measured by an integration test where the aggregate over N rows returns a single non-empty
> summary from the deterministic chat stub, plus a real-OpenAI evidence run.

## Context

`docs/features/11-sumarizacao-conteudo.md` documents both scalar and aggregate summarization as the
TheoDB target. M7-S3 shipped the **scalar** `ai.summarize(content, model)` (a thin SQL wrapper over the
private `ai._chat(prompt, system, model)` HTTP helper) in `sql/50-theodb-ai.sql`, tested via the
deterministic OpenAI-compatible stub `tools/chat_server.py` + a real-OpenAI opt-in test
(`benchmarks/tests/test_ai_sql.py`). The **aggregate** path (collapse many rows into one summary) was
deferred (M7-S3 CHANGELOG: "modos array/cursor são follow-up"). This slice delivers it as a first-class
PostgreSQL aggregate, reusing `ai._chat` (Rule 9 — no reinvention, no new dependency).

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `sql/50-theodb-ai.sql` | ~165 | M7-S3 (2026-06-28) | the 5 scalar `ai.*` + private `ai._chat`; baked via initdb.d | `ai._chat` signature + the 5 scalar fns unchanged; new objects are additive + idempotent |
| `benchmarks/tests/test_ai_sql.py` | (exists) | M7-S3 (2026-06-28) | contract tests vs the chat stub + real-OpenAI opt-in | existing tests stay green; an agg test is appended |
| `smoke.sh` | (exists) | — | presence/privilege smoke | add an `ai.agg_summarize` presence check |
| `docs/sql-ai-functions.md` | (exists) | M7-S3 | the ai.* doc | add an `ai.agg_summarize` section |
| `CHANGELOG.md` | (exists) | — | public contract | `[Unreleased]` gets the M10 entry |

Every file in any `#### Files to edit` below appears in this table.

### Current callers / dependents

- **Symbol:** `ai._chat(prompt, system, model)` (private, plpython3u) in `sql/50-theodb-ai.sql`.
  - **Callers:** the 5 scalar wrappers (`ai.generate`/`ai.if`/`ai.analyze_sentiment`/`ai.summarize`/`ai.rank`). The new `ai._agg_summ_final` becomes one more caller.
  - **External:** baked into `theo-db:dev` via initdb.d; reached over HTTP to `theodb.llm_endpoint`. Adding an aggregate is additive — no existing object changes.
- Enumerated via `grep -nE 'ai\._chat|ai\.summarize|CREATE (AGGREGATE|OR REPLACE FUNCTION)' sql/50-theodb-ai.sql`.

### Domain glossary

- **`ai._chat`** — the single private HTTP round-trip to a configurable OpenAI-compatible chat endpoint (SSRF-guarded, fail-fast typed errors, REVOKE FROM PUBLIC).
- **PostgreSQL aggregate** — `CREATE AGGREGATE name(argtype) (sfunc, stype, finalfunc)`: `sfunc(state,row)` accumulates per row; `finalfunc(state)` produces the result once.
- **chat stub** — `tools/chat_server.py`, a deterministic OpenAI-compatible server; a `summarize` system prompt → `"A concise summary: " + <canned>` (zero external cost in CI).
- **SECURITY INVOKER** — the wrappers run as the caller; the caller needs EXECUTE on `ai._chat` too.

### Architecture boundaries affected

Per `rules/architecture.md`: the change is entirely within the **AI SQL surface** (`sql/50-theodb-ai.sql`)
— an interface-layer capability over the `ai._chat` adapter. No new layer, no new dependency; the
aggregate is composed from the existing helper (DIP: domain prompt → adapter HTTP).

## Prior Art & Related Work

- **Internal (the pattern to mirror):** `sql/50-theodb-ai.sql` `ai.summarize` (the scalar summarizer this aggregates) + `ai._chat` (the HTTP helper) + the REVOKE/SECURITY-INVOKER discipline; `benchmarks/tests/test_ai_sql.py` (stub + real-OpenAI test pattern); `tools/chat_server.py` (`summarize` branch).
- **Internal (discovery):** `knowledge-base/discoveries/blueprints/alloydb-vector-ai-implementation-blueprint.md` (the `ai.*`/`google_ml_integration`-style surface TheoDB mirrors) + the `m7-*` blueprints.
- **External:** PostgreSQL `CREATE AGGREGATE` docs (`https://www.postgresql.org/docs/current/sql-createaggregate.html`) — sfunc/stype/finalfunc contract.
- **Reference:** `.claude/knowledge-base/references/` (PostgreSQL docs mirror, when present).

## Objective

- [ ] `ai.agg_summarize(text)` aggregate: `sfunc` accumulates row texts (newline-joined, NULL-skipping), `finalfunc` calls `ai._chat` once on the (bounded) accumulation.
- [ ] Idempotent DDL (`DROP AGGREGATE IF EXISTS` before `CREATE AGGREGATE`) so re-applying `sql/50` works; baked via initdb.d.
- [ ] `REVOKE ALL ... FROM PUBLIC` on the aggregate + its two support functions (parity with the scalar `ai.*`).
- [ ] Empty/all-NULL input → `NULL` summary (no LLM call); prompt bounded to a documented char cap (cost/token safety).
- [ ] Integration test (chat stub): aggregate over N rows → one non-empty summary; + real-OpenAI evidence run; + smoke presence.

## ADRs

### D1 — Implement as a native PostgreSQL aggregate over `ai._chat` (reuse, don't reinvent)

**Decision:** Ship `ai.agg_summarize(text)` as a `CREATE AGGREGATE` with a pure-SQL `sfunc` (newline-join,
NULL-skipping) accumulating into a `text` state and a `finalfunc` that calls the existing `ai._chat` once.

**Rationale:** Rule 9 / parsimony rung 4 — `ai._chat` already encapsulates the HTTP/SSRF/parse/fail-fast
contract; the aggregate is pure composition. A native aggregate is the idiomatic SQL surface for
"collapse many rows into one" (works in `GROUP BY`, `SELECT ai.agg_summarize(col) FROM t`).

**Alternatives considered:** *A set-returning/array scalar fn `ai.summarize_rows(text[])`* — rejected:
not composable with `GROUP BY`; the aggregate is the SQL-native shape spec 11 implies. *Reimplement the
HTTP in the finalfunc* — rejected: duplicates `ai._chat` (Rule 9 violation). *plpython3u finalfunc* —
rejected: a pure-SQL finalfunc (CASE over NULL + `ai._chat`) is simpler (KISS) and needs no Python.

**Consequences:** the aggregate inherits `ai._chat`'s typed errors + SECURITY INVOKER grant requirement
(caller needs EXECUTE on `ai._chat`).

### D2 — Concat-with-cap for MVP; map-reduce deferred (YAGNI)

**Decision:** The `finalfunc` concatenates the accumulated rows and truncates to a fixed documented char
cap (`12000`) before the single `ai._chat` call. No map-reduce, no per-chunk recursion, no GUC knob.

**Rationale:** Bounding the prompt is real correctness (unbounded text → token-limit 400s + cost) — not
YAGNI. Map-reduce (chunk → summarize → summarize-of-summaries) is genuine future work with no current
demand (YAGNI); a fixed cap is the KISS MVP. A GUC config knob is the speculative-config anti-pattern
(`parsimony-ladder.md`) — a documented constant suffices until a second concrete need appears.

**Alternatives considered:** *No cap* — rejected: unbounded cost/failure on large groups. *GUC
`theodb.agg_summarize_max_chars`* — rejected: YAGNI config knob. *map-reduce now* — rejected: no demand;
deferred honestly (documented limit).

**Consequences:** very large groups are truncated (documented); the summary reflects the first ~12000
chars. Honest limitation, not a silent one.

### D3 — Two test layers (deterministic stub default + real-OpenAI opt-in)

**Decision:** Reuse the M7-S3 test pattern: an OFFLINE deterministic-stub test (CI, zero cost) asserts the
SQL→HTTP→parse contract; a REAL opt-in test (`-k real`, gated on `THEODB_LLM_ENDPOINT`+`OPENAI_API_KEY`
from the gitignored `.env`) asserts shape (non-empty, derived from input), skipping cleanly otherwise.

**Rationale:** mirrors the shipped `ai.*` test discipline (honest evidence, no silent green, no cost in
CI). The user mandate requires real functional evidence — the opt-in real run provides it.

**Alternatives considered:** *Stub-only* — rejected: no real evidence (the mandate forbids it). *Real-only*
— rejected: non-deterministic + costs money in CI.

**Consequences:** CI stays deterministic; a real run is recorded as evidence in the implementation log.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Large groups exceed the LLM token limit | Medium | `finalfunc` caps the prompt at 12000 chars (D2); documented limitation; map-reduce deferred | AI |
| Aggregate makes one LLM call per group — cost/latency scales with group count | Medium | VOLATILE + documented; the caller controls grouping; no perf claim without benchmark (Rule 5) | AI |
| Re-applying `sql/50` fails (no CREATE OR REPLACE AGGREGATE) | Low | `DROP AGGREGATE IF EXISTS` before `CREATE AGGREGATE` (idempotent); initdb is fresh anyway | AI |
| Granting `ai._chat` to PUBLIC to "fix" a permission error re-opens outbound HTTP | Low | Keep REVOKE FROM PUBLIC; doc the dual-grant requirement (as the scalar `ai.*` already do) | AI |

## Unresolved Questions

- Q1 — Support a `model` override arg (`ai.agg_summarize(text, text)`)? Resolved: no for MVP — aggregate extra args are per-row; the default model (GUC `theodb.llm_model`) suffices; model-override deferred (YAGNI), documented.
- Q2 — Map-reduce for very large groups? Resolved at plan time: deferred (D2) — fixed cap MVP; map-reduce is future work with no current demand.

## Dependencies

M10 adds **no new dependency** (Unbreakable Rule 9). It composes the shipped `ai._chat` (plpython3u, in
the image since M7-S3) into a native PostgreSQL aggregate.

| Dependency | Version | Role | License | Status / CVE |
|---|---|---|---|---|
| `plpython3u` + `ai._chat` | shipped (M7-S3) | the HTTP helper the aggregate calls | PostgreSQL License | already shipped; no change |
| `psycopg2` (test, dev-only) | as in requirements.txt | test DB client | LGPL | already a dev dep |

No CVE audit delta: zero new declared dependencies.

## Dependency Graph

```
Phase 1 (aggregate DDL + tests) ──▶ Phase 2 (doc + real-OpenAI evidence + CHANGELOG)
```

## Phase 1: `ai.agg_summarize` aggregate + tests

**Objective:** Ship the aggregate with a deterministic-stub integration test + smoke presence.

### T1.1 — Aggregate DDL (`sfunc` + `finalfunc` + `CREATE AGGREGATE` + REVOKE) + stub test

#### Objective
Add the aggregate and its two support functions to `sql/50-theodb-ai.sql`, REVOKE FROM PUBLIC, and prove it via the deterministic chat stub.

#### Why this step (action + reasoning — ReAct discipline)

1. **What this step does** — appends to `sql/50-theodb-ai.sql`: `ai._agg_summ_accum(text,text)` (pure-SQL
   newline-join, NULL-skipping, IMMUTABLE), `ai._agg_summ_final(text)` (pure-SQL: NULL→NULL else
   `ai._chat(left(state,12000), '<summarize system prompt>', NULL)`, VOLATILE), `DROP AGGREGATE IF EXISTS`
   + `CREATE AGGREGATE ai.agg_summarize(text)`, and `REVOKE ALL ... FROM PUBLIC` on all three. Adds an
   integration test that runs the aggregate over a seeded N-row table against the chat stub.

2. **Why it is necessary now** — it is the feature: the SQL-native "many rows → one summary" surface of
   spec 11, composed from the existing `ai._chat` (parsimony), proven by deterministic evidence.

#### Evidence
- Scalar pattern + helper: `sql/50-theodb-ai.sql` (`ai.summarize` line ~134; `ai._chat` line ~16; REVOKE block ~160).
- Test + stub pattern: `benchmarks/tests/test_ai_sql.py` (chat_server fixture); `tools/chat_server.py` (`summarize` branch → `"A concise summary: " + canned`).
- Aggregate contract: PostgreSQL `CREATE AGGREGATE` docs.

#### Files to edit
```
sql/50-theodb-ai.sql — add ai._agg_summ_accum, ai._agg_summ_final, ai.agg_summarize aggregate + REVOKE
benchmarks/tests/test_ai_sql.py — RED stub test for ai.agg_summarize
```

#### Deep file dependency analysis
- `sql/50-theodb-ai.sql` (Baseline row, invariant: scalar fns + `ai._chat` unchanged): additive objects + a REVOKE line; idempotent DROP-before-CREATE for the aggregate.
- `test_ai_sql.py` (Baseline row, invariant: existing tests green): appends one test using the existing `chat_server` + `conn` fixtures.

#### Deep Dives
- **sfunc:** `CASE WHEN item IS NULL THEN state WHEN state IS NULL THEN item ELSE state||E'\n'||item END` (IMMUTABLE, pure).
- **finalfunc:** `CASE WHEN state IS NULL THEN NULL ELSE ai._chat(left(state,12000),'Summarize the following collected texts into a single concise summary (1-3 sentences).',NULL) END` (VOLATILE). System prompt contains "summarize" so the stub's summarize branch fires.
- **Edge cases:** empty input → finalfunc gets NULL state → returns NULL (no LLM call). All-NULL rows → state stays NULL → NULL. Single row → summarizes that row.
- **Idempotency:** `DROP AGGREGATE IF EXISTS ai.agg_summarize(text);` precedes `CREATE AGGREGATE`.

#### Pseudo-code / Signatures
```sql
CREATE OR REPLACE FUNCTION ai._agg_summ_accum(state text, item text) RETURNS text LANGUAGE sql IMMUTABLE AS $$
  SELECT CASE WHEN item IS NULL THEN state WHEN state IS NULL THEN item ELSE state || E'\n' || item END $$;
CREATE OR REPLACE FUNCTION ai._agg_summ_final(state text) RETURNS text LANGUAGE sql VOLATILE AS $$
  SELECT CASE WHEN state IS NULL THEN NULL
    ELSE ai._chat(left(state, 12000), 'Summarize the following collected texts into a single concise summary (1-3 sentences).', NULL) END $$;
DROP AGGREGATE IF EXISTS ai.agg_summarize(text);
CREATE AGGREGATE ai.agg_summarize(text) (sfunc = ai._agg_summ_accum, stype = text, finalfunc = ai._agg_summ_final);
REVOKE ALL ON FUNCTION ai._agg_summ_accum(text,text), ai._agg_summ_final(text), ai.agg_summarize(text) FROM PUBLIC;
```

#### Tasks
1. Append the two functions + aggregate + REVOKE to `sql/50-theodb-ai.sql`.
2. Rebuild `theo-db:dev` (bakes updated sql/50 via initdb.d); fresh container.
3. Append `test_agg_summarize_over_rows` (stub) to `test_ai_sql.py`.

#### TDD
```
RED:     test_agg_summarize_over_rows() [integration] — seed a 3-row table; SELECT ai.agg_summarize(content) FROM t; assert result is non-empty and starts with "A concise summary:" (stub summarize branch). MUST fail before the aggregate exists (undefined function).
GREEN:   Implement the aggregate so the test passes against the rebuilt container.
REFACTOR: keep the finalfunc pure-SQL; no plpython needed. "None expected" otherwise.
VERIFY:  PG*=... pytest -m integration benchmarks/tests/test_ai_sql.py -k agg_summarize -q
```

#### Concurrency tests

**Concurrency posture: (none — single-threaded)** — the aggregate accumulates rows sequentially within one query; no shared mutable state, no locks/async in our code (PostgreSQL executes the aggregate serially per group).

#### Acceptance Criteria
- [ ] `ai.agg_summarize` exists + is REVOKED from PUBLIC — `docker exec <c> psql -U postgres -tAc "SELECT proname FROM pg_proc WHERE proname='agg_summarize'"` prints `agg_summarize` AND `has_function_privilege('public','ai.agg_summarize(text)','execute')` returns `f`.
- [ ] Stub test passes — `PGHOST=... pytest -m integration benchmarks/tests/test_ai_sql.py -k agg_summarize -q` exits `0`.
- [ ] Empty input → NULL — `psql -tAc "SELECT ai.agg_summarize(c) FROM (SELECT NULL::text c WHERE false) z"` returns empty (NULL), no error.
- [ ] Existing ai.* tests still green — `PGHOST=... pytest -m integration benchmarks/tests/test_ai_sql.py -k 'not real' -q` exits `0`.
- [ ] Pass: re-applying sql/50 is idempotent — `psql -f sql/50-theodb-ai.sql` exits `0` twice in a row.

#### DoD
- [ ] All tasks completed and validated — every Acceptance Criteria above exits `0`.
- [ ] Aggregate green on the rebuilt image; no regression to scalar ai.* — `pytest -m integration benchmarks/tests/test_ai_sql.py -k 'not real' -q` exits `0`.
- [ ] REVOKE FROM PUBLIC verified — `has_function_privilege` returns `f`.
- [ ] CHANGELOG `[Unreleased]` updated — `grep -c 'agg_summarize' CHANGELOG.md` returns `> 0`.
- [ ] File-size budget respected — changed files `wc -l` < `500`.

## Phase 2: Doc + real-OpenAI evidence + CHANGELOG

**Objective:** Document the aggregate, capture real-OpenAI evidence, log the CHANGELOG entry.

### T2.1 — `docs/sql-ai-functions.md` section + real-OpenAI evidence + smoke + CHANGELOG

#### Objective
Add the doc section, run a real-OpenAI evidence test, add the smoke presence check + CHANGELOG entry.

#### Why this step (action + reasoning — ReAct discipline)

1. **What this step does** — adds an `ai.agg_summarize` section to `docs/sql-ai-functions.md` (usage +
   GROUP BY example + cap/limit + dual-grant note), runs the opt-in real-OpenAI test and records the
   output in the implementation log, adds an `ai.agg_summarize` presence line to `smoke.sh`, and the
   CHANGELOG `[Unreleased]` entry.

2. **Why it is necessary now** — the mandate requires real functional evidence (not just stub); the doc
   makes the surface usable; the smoke guards presence in the baked image.

#### Evidence
- Doc format: `docs/sql-ai-functions.md` (existing ai.* sections).
- Real-test pattern: `benchmarks/tests/test_ai_sql.py` `-k real` (gated on `.env`).
- Smoke pattern: `smoke.sh` (existing ai.* presence checks).

#### Files to edit
```
docs/sql-ai-functions.md — ai.agg_summarize section (usage, GROUP BY, cap, dual-grant)
benchmarks/tests/test_ai_sql.py — real-OpenAI agg_summarize evidence test (-k real, skips cleanly)
smoke.sh — ai.agg_summarize presence check
CHANGELOG.md — [Unreleased] M10 entry
```

#### Deep file dependency analysis
- `docs/sql-ai-functions.md` (Baseline row): additive section.
- `smoke.sh` (Baseline row): additive presence check (same shape as scalar ai.* checks).
- `test_ai_sql.py`: appends one `-k real` test (skips without `.env`).

#### Deep Dives
- **Real evidence honesty:** the real test asserts shape (non-empty summary derived from the seeded rows), never exact text (LLM non-determinism); the actual model output is pasted into the implementation log as evidence.

#### Tasks
1. Write the doc section.
2. Run the real-OpenAI test; record output in the implementation log.
3. Add the smoke check + CHANGELOG entry.

#### TDD
```
RED:     real test absent / doc section absent.
GREEN:   `grep -c agg_summarize docs/sql-ai-functions.md smoke.sh` > 0; real test present (skips cleanly without .env).
REFACTOR: none expected.
VERIFY:  grep -c 'agg_summarize' docs/sql-ai-functions.md smoke.sh && pytest -m integration benchmarks/tests/test_ai_sql.py -k 'agg_summarize and real' -q
```

#### Concurrency tests

**Concurrency posture: (none — single-threaded)** — docs + a sequential test; no concurrent state.

#### Acceptance Criteria
- [ ] Doc section present — `grep -ci 'agg_summarize' docs/sql-ai-functions.md` returns `> 0`.
- [ ] Smoke presence check present — `grep -c 'agg_summarize' smoke.sh` returns `> 0`.
- [ ] Real-OpenAI evidence recorded — implementation log contains the real model summary (or a documented clean skip if `.env` absent) — `grep -ci 'agg_summarize' knowledge-base/implementations/m10-agg-summarize-implementation.md` returns `> 0`.
- [ ] No unbenchmarked perf claim — `grep -ciE 'faster than|outperforms' docs/sql-ai-functions.md` returns `0`.
- [ ] Pass: size — changed files `wc -l` < `500`.

#### DoD
- [ ] All tasks completed and validated — every Acceptance Criteria above exits `0`.
- [ ] Doc + smoke + real-evidence committed — `grep -c agg_summarize docs/sql-ai-functions.md smoke.sh` returns `> 0`.
- [ ] CHANGELOG `[Unreleased]` updated — `grep -c 'agg_summarize' CHANGELOG.md` returns `> 0`.
- [ ] File-size budget respected — changed files `wc -l` < `500`.

## Coverage Matrix

| # | Gap / Requirement | Task(s) | Resolution |
|---|---|---|---|
| 1 | feature 11 aggregate summarization shipped | T1.1, T2.1 | `ai.agg_summarize(text)` aggregate over `ai._chat` |
| 2 | reuse `ai._chat` (Rule 9, no new dep) | T1.1 | finalfunc composes the shipped helper (per ADR D1) |
| 3 | REVOKE FROM PUBLIC (security parity) | T1.1 | REVOKE on aggregate + 2 support fns |
| 4 | empty/NULL input safe | T1.1 | NULL state → NULL summary, no LLM call |
| 5 | prompt bounded (cost/token safety) | T1.1 | `left(state,12000)` cap (per ADR D2) |
| 6 | real functional evidence (not stub-only) | T2.1 | real-OpenAI opt-in test + logged output (per ADR D3) |
| 7 | idempotent DDL | T1.1 | DROP AGGREGATE IF EXISTS before CREATE |
| 8 | no regression to scalar ai.* | T1.1 | existing ai.* tests green |

**Coverage: 8/8 gaps covered (100%)**

## Global Definition of Done

- [ ] All phases completed — every phase DoD above exits `0`.
- [ ] AI suite green — `PGHOST=... pytest -m integration benchmarks/tests/test_ai_sql.py -k 'not real' -q` exits `0` (no regression to scalar ai.*).
- [ ] `ai.agg_summarize` REVOKED from PUBLIC — `has_function_privilege('public','ai.agg_summarize(text)','execute')` returns `f`.
- [ ] Real-OpenAI evidence captured — implementation log has the real summary output (or documented clean skip).
- [ ] File-size budget respected — changed files `wc -l` < `500` (per `rules/architecture.md`).
- [ ] CHANGELOG.md updated under `[Unreleased]` — `grep -c 'agg_summarize' CHANGELOG.md` returns `> 0` (Unbreakable Rule 6).
- [ ] Backward compatibility preserved — the 5 scalar `ai.*` + `ai._chat` unchanged.
- [ ] No new dependency (Rule 9) — `requirements.txt` + image deps unchanged.
- [ ] Plan archived after `/review` READY_TO_MERGE + PR merge.

## Failure scenarios (external I/O)

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| chat endpoint (`ai._chat`, HTTP) | endpoint unset | call aggregate with no `theodb.llm_endpoint` | typed fail-fast `22023` ("endpoint is not set") — same as scalar ai.* |
| chat endpoint | endpoint down / call fails | point endpoint at a dead port (or stub down) | typed `38000` ("chat endpoint call failed") — propagated from `ai._chat` |
| chat endpoint | empty completion | stub returns empty (existing `__EMPTY__`-style path) | typed `38000` ("empty completion") — `ai._chat` guard |
| input | empty / all-NULL group | aggregate over zero/NULL rows | `NULL` summary, NO LLM call (finalfunc NULL-guard) |
| input | oversized group (> cap) | aggregate over a very long concatenation | prompt truncated to 12000 chars (documented); single `ai._chat` call succeeds |

## Final Phase: Integration Validation (MANDATORY)

**Objective:** Validate `ai.agg_summarize` end-to-end against a freshly-baked container.

### Execution
```
docker build -t theo-db:dev .                                   # bake updated sql/50
docker run -d --name m10-it --add-host=host.docker.internal:host-gateway \
  -e POSTGRES_PASSWORD=postgres -p <port>:5432 theo-db:dev      # wait healthy
cd benchmarks && pip install -r requirements.txt
PGHOST=localhost PGPORT=<port> PGUSER=postgres PGPASSWORD=postgres PGDATABASE=postgres \
  pytest -m integration tests/test_ai_sql.py -k 'agg_summarize and not real' -q     # stub
# real evidence (opt-in; loads .env):
THEODB_LLM_ENDPOINT=... OPENAI_API_KEY=... PG*=... \
  pytest -m integration tests/test_ai_sql.py -k 'agg_summarize and real' -q
psql -f sql/50-theodb-ai.sql   # idempotent re-apply exits 0
```

### Acceptance Criteria
- [ ] Aggregate green on the rebuilt image; summary non-empty + derived from rows — `PGHOST=... pytest -m integration tests/test_ai_sql.py -k 'agg_summarize and not real' -q` exits `0`.
- [ ] Empty/NULL input → NULL (no LLM call) + REVOKE FROM PUBLIC verified — `psql -tAc "SELECT has_function_privilege('public','ai.agg_summarize(text)','execute')"` returns `f`.
- [ ] Real-OpenAI evidence captured (or documented clean skip) — `grep -ci 'agg_summarize' knowledge-base/implementations/m10-agg-summarize-implementation.md` returns `> 0`.
- [ ] No regression to scalar ai.*; idempotent re-apply of sql/50 — `psql -f sql/50-theodb-ai.sql` exits `0` on a second consecutive run.
