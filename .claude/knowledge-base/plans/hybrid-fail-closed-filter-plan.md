---
slug: hybrid-fail-closed-filter
milestone_id: M120
created_at: 2026-07-20
goal: Add a fail-closed structured filter (col/op/value, quote_identifier + quote_literal + operator allowlist) to ai.hybrid_search_rrf so an un-allowlisted operator raises SQLSTATE 22023 and a subquery value cannot execute.
---

# Plan: M120 — fail-closed structured filter for `ai.hybrid_search_rrf`

## Goal

Add a **fail-closed structured `filter`** (`[{col, op, value}]`) to `ai.hybrid_search_rrf` — composed with
`quote_identifier(col)` + `quote_literal(value)` + a fixed **operator allowlist** — so that (a) an un-allowlisted
operator/shape raises **SQLSTATE 22023** (proven by an A/B in-PG negative test) and (b) a subquery/SQL-fragment
value is quoted-as-literal and **cannot execute**, while a valid structured filter returns the **same rows** as
the equivalent safe `filter_sql`.

Single metric: **the negative test rejects a bad operator with SQLSTATE 22023 AND the structured-positive query
returns the identical id-set to the equivalent `filter_sql` query** (A/B in-PG).

## Context

`hybrid.rs::run_rrf` (L146) composes `filter_sql` as raw `%5$s` into both legs' `WHERE ... AND (%5$s)`; the L147
guard is syntactic (rejects `;`/comments) — NOT a parser. The module docstring (L8-11) + code (L144) both name
the structured-filter API as the fail-closed follow-up. council-security F1 (backlog) flagged this as a latent
BLOCKER if `ai.hybrid_search_rrf` is exposed multi-tenant. Blueprint:
`.claude/knowledge-base/discoveries/blueprints/hybrid-fail-closed-filter-blueprint.md`.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | Role today | Change |
|---|---|---|
| `theodb_rs/src/hybrid.rs` | `run_rrf` (raw `filter_sql` → `%5$s`, L146-151) + `run_rrf_json` (config parse, L270-306) | Add a structured-filter compose fn (allowlist + quote_identifier + quote_literal); `run_rrf_json` parses a new `filter` JSON key and builds the safe predicate string passed as `filter_sql` to `run_rrf`. |

### Current callers / dependents
- `run_rrf` ← `run_rrf_json` (hybrid.rs) ← `_ai_hybrid_search` `#[pg_extern]` (lib.rs/api.rs) ← SQL `ai.hybrid_search_rrf` / `ai.hybrid_search(jsonb)` (`sql/40-theodb-hybrid.sql`).
- The SQL wrapper passes a JSON config; a new `filter` key needs NO SQL-signature change (it's inside the jsonb).

### Domain glossary
- **RRF** — reciprocal-rank fusion of the vector + lexical legs.
- **filter_sql** — the raw caller-privilege boolean predicate (kept, opt-in).
- **structured filter** — the new `[{col,op,value}]` fail-closed predicate.
- **quote_identifier / quote_literal** — pgrx/pg_sys safe `%I` / `%L` quoting.

### Architecture boundaries affected
`hybrid.rs` is the AI-surface orchestration layer (application). No new boundary; the security posture mirrors `nl.rs` (fail-closed 22023). `rules/error-handling.md` (typed errors) + `rules/architecture.md` (SRP).

## Prior Art & Related Work
- Internal: `theodb_rs/src/nl.rs:8-135` (L2 fail-closed denylist / 22023 posture — reused). `backlog.md` M53 council-security F1.
- External pattern: pgvector/Qdrant/Weaviate expose structured filter DSLs (never raw SQL) — the standard parameterized-filter safety pattern.
- `pgrx-0.19` `spi::quote_identifier`.

## Dependencies

(none — **no new dependency**. `pgrx::spi::quote_identifier` + pgrx/pg_sys literal quoting + serde_json, all already linked. Parsimony rung 2/4. `/deps-audit` PASS by construction.)

## Objective
Compose an injection-safe structured predicate from `[{col,op,value}]` and wire it into the hybrid config, keeping `filter_sql` as an opt-in documented caller-privilege escape hatch.

## ADRs

- **ADR-1 — Structured filter over hardening the raw blacklist.** Chosen: add a structured `filter` path. Rejected: extend the `filter_sql` blacklist — a blacklist can never be complete for raw caller SQL (council-security F1); the structured path is the only fail-closed option.
- **ADR-2 — Keep `filter_sql` opt-in, drop the "injection-safe" implication.** Chosen: retain `filter_sql` for backward-compat, COMMENT/doc names it raw caller-privilege; structured `filter` is the recommended path for untrusted/multi-tenant callers. Rejected: hard-remove `filter_sql` (breaks callers).
- **ADR-3 — `quote_literal` the value, not new SPI binds.** Chosen: `quote_identifier(col) <allowlisted_op> quote_literal(value)` into the existing `%5$s`. Rejected: thread `$7+` binds through the fixed `$1..$6` template — larger change, identical safety (`quote_literal` is already injection-safe, the `%L` equivalent).

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Structured filter is less expressive than raw SQL (no subqueries/functions). | MEDIUM | That is the point (fail-closed); `filter_sql` opt-in covers advanced raw needs at caller-privilege. | impl |
| An operator allowlist that is too narrow blocks legitimate filters. | LOW | Cover the common set (`= < > <= >= <> IN &&`); extend via a one-line allowlist addition + a test if a real need appears. | impl |
| `quote_literal` for a numeric value must not force a string type mismatch. | MEDIUM | Emit bare numeric for JSON numbers (safe — pure digits), `quote_literal` only for strings; test both. | impl |

## Unresolved Questions
- Exact pgrx 0.19 literal-quoting helper (`quote_literal` vs `pg_sys::quote_literal_cstr`) — resolved at impl by checking the pgrx API; `pg_sys::quote_literal_cstr` is the guaranteed fallback.

## Dependency Graph
```
Phase 1 (structured-filter compose + allowlist + unit) ──▶ Phase 2 (wire into run_rrf_json) ──▶ Phase 3 (A/B in-PG) ──▶ Integration Validation
```

## Phase 1: Structured filter compose

### T1.1 — `compose_structured_filter(conditions) -> Result<String, String>`

#### Objective
A pure fn: given `&[{col, op, value}]`, return a safe predicate string (`(quote_identifier(col) op literal AND ...)`), or an Err (mapped to 22023) when any operator is not in the allowlist or the shape is invalid.

#### Why this step (action + reasoning)
Action: build the safe predicate with quote_identifier + quote_literal + operator allowlist. Reasoning: this is the fail-closed core (blueprint Corner 4); isolating it as a pure fn makes the allowlist + quoting unit-testable without PG, and mirrors `nl.rs`'s fail-closed posture (Prior Art).

#### Evidence
`hybrid.rs:146` (filter compose point), `nl.rs:8-135` (fail-closed pattern), pgrx `spi::quote_identifier`.

#### Files to edit
- `theodb_rs/src/hybrid.rs` (new `compose_structured_filter` + `OP_ALLOWLIST` const; < 120 LoC delta).

#### Deep file dependency analysis
Uses `pgrx::spi::quote_identifier` (linked) + literal quoting. Returns `Result<String,String>`; the caller (`run_rrf_json`) maps Err → `err_input` (22023), same as the existing guards.

#### TDD
- RED: `structured_filter_rejects_unlisted_op` — op `;DROP` / `= (SELECT` / `LIKE` (if not allowlisted) → Err.
- RED: `structured_filter_quotes_identifier_and_literal` — `{col:"na me", op:"=", value:"x'; --"}` → the emitted string has the identifier double-quoted and the value single-quoted-escaped (no break-out); assert the exact safe string.
- RED: `structured_filter_numeric_value_bare` — `{col:"cat", op:"=", value:1}` → `... = 1` (bare numeric, no quotes).

#### Concurrency tests (only when applicable)
(none — single-threaded pure fn.)

#### Acceptance Criteria
- Every op outside `OP_ALLOWLIST` → Err. Identifiers quoted, string values literal-quoted, numerics bare. Empty conditions → `"true"`.

#### DoD
- Unit tests green (these are pure-Rust, no PG — run via the standalone example harness OR `cargo build` + a small example, per the project's no-cargo-pgrx-test convention).

## Phase 2: Wire into `run_rrf_json`

### T2.1 — parse `filter` config key + pass composed predicate to `run_rrf`

#### Objective
`run_rrf_json` parses an optional `filter` (JSON array); if present, compose the safe predicate and pass it as `filter_sql` to `run_rrf`. `filter` and `filter_sql` are mutually exclusive (both set → 22023).

#### Why this step (action + reasoning)
Action: add `get` for `filter`, call `compose_structured_filter`, thread the result. Reasoning: `run_rrf_json` (L288) already parses `filter_sql`; the structured path slots in beside it (SRP — the compose fn does the security, the parser does the wiring).

#### Evidence
`hybrid.rs:270-306` (run_rrf_json config parse + run_rrf call).

#### Files to edit
- `theodb_rs/src/hybrid.rs` (run_rrf_json: parse `filter`, mutual-exclusion guard, compose + pass).

#### Deep file dependency analysis
`get_str`/`cfg.get` already used; `filter` is `cfg.get("filter").and_then(|v| v.as_array())`. Mutual-exclusion + the composed string reuse the existing `filter_sql` param of `run_rrf` (no signature change).

#### TDD
- RED (A/B in-PG, Phase 3): `hybrid_structured_filter_rejects_bad_op` (config with a bad op → 22023).
- RED: `hybrid_filter_and_filter_sql_mutually_exclusive` (both set → 22023).

#### Concurrency tests (only when applicable)
(none — single-threaded.)

#### Acceptance Criteria
- Config `{filter:[{col:"cat",op:"=",value:1}]}` returns the same ids as `{filter_sql:"cat = 1"}`. Both set → 22023.

#### DoD
- `cargo build` green; the A/B in-PG (Phase 3) proves the behavior.

## Phase 3: A/B in-PG validation

### T3.1 — install + SQL A/B (positive parity + negative fail-closed)

#### Objective
Build+install; a hybrid table + `ai.hybrid_search_rrf` with a structured `filter`; assert (a) parity with the safe `filter_sql`, (b) a bad operator → 22023, (c) a subquery value quoted-as-literal returns nothing (does not execute).

#### Why this step (action + reasoning)
Action: run the security assertions in a real PG (the project convention — `cargo pgrx test` doesn't link on the droplet). Reasoning: the whole value of M120 is the fail-closed behavior in-PG; only an installed-extension A/B proves it (Rule 5 — evidence, not opinion).

#### Evidence
`docs/packaging/license-audit.md` (droplet pgrx PG); the M118 A/B convention.

#### Files to edit
- `docs/security/m120-fail-closed-filter.md` (NEW — the A/B evidence + the fail-closed contract).

#### TDD
- The A/B SQL script IS the test: positive parity (structured == filter_sql id-set) + negative (bad op / mutual-exclusion → 22023 SQLSTATE) + subquery-value-returns-nothing. Committed as reproducible evidence.

#### Failure scenarios (external I/O)
(none — SPI-only, no external network. The hybrid embed leg is guarded separately; M120 touches only the relational filter composition.)

#### Acceptance Criteria
- A/B shows: structured parity ✅, bad op → 22023 ✅, subquery value → 0 rows (not executed) ✅.

#### DoD
- `docs/security/m120-fail-closed-filter.md` committed with the SQL + outputs; no overclaim.

## Coverage Matrix

| DoD requirement | Task(s) |
|---|---|
| Structured filter (allowlist + quote_identifier + quote_literal) | T1.1 |
| Fail-closed: un-allowlisted op → 22023 | T1.1, T2.1 |
| Subquery value cannot execute (quoted literal) | T1.1, T3.1 |
| filter_sql kept opt-in (mutual-exclusion) | T2.1 |
| MEASURED in-PG: parity + negative 22023 | T3.1 |

100% — every DoD item maps to ≥1 task.

## Global Definition of Done
- [ ] Unit tests (compose fn) green.
- [ ] `cargo build` green; A/B in-PG evidence committed.
- [ ] Fail-closed proven (22023 on bad op; subquery value returns nothing).
- [ ] `filter_sql` COMMENT/doc drops the "injection-safe" implication (ADR-2).
- [ ] CHANGELOG `[Unreleased]` updated.
- [ ] No new dependency. File-size budget respected.
- [ ] Plan archived after `/review` READY_TO_MERGE + PR merged.

## Failure scenarios (when I/O external)
(none — no external I/O; SPI-only relational filter composition.)

## Final Phase: Integration Validation (MANDATORY)

### Execution
1. Provision/reuse droplet; `cargo pgrx install`.
2. `cargo build` (unit compose tests via example) green.
3. A/B in-PG: structured parity + bad-op 22023 + subquery-value-0-rows.

### Acceptance Criteria
- All three A/B assertions pass; no regression to the existing `filter_sql` path.

### If Validation Fails
- Bad op not rejected → the allowlist/compose is wrong; fix T1.1 before proceeding.
- Structured parity fails → the quoting/composition differs from the equivalent filter_sql; reconcile.
