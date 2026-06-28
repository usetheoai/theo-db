# Implementation — M10 `ai.agg_summarize` (aggregate summarization, feature 11)

**Slug:** m10-agg-summarize · **Milestone:** M10 · **Date:** 2026-06-28
**Plan:** `.claude/knowledge-base/plans/m10-agg-summarize-plan.md` (plan-confidence SHIPPABLE 96.0)

## What shipped

A native PostgreSQL aggregate `ai.agg_summarize(text)` that collapses many rows into one summary by
composing the existing private `ai._chat` HTTP helper (Rule 9 — no new dependency):

- `ai._agg_summ_accum(state text, item text)` — pure-SQL, IMMUTABLE, newline-join, NULL-skipping.
- `ai._agg_summ_final(state text)` — pure-SQL, VOLATILE: `NULL → NULL` (empty/all-NULL group → no LLM
  call); else `ai._chat(left(state,12000), '<summarize system prompt>', NULL)` (prompt bounded for
  cost/token safety; map-reduce deferred — ADR D2).
- `CREATE AGGREGATE ai.agg_summarize(text)` (sfunc/stype/finalfunc); `DROP AGGREGATE IF EXISTS` first
  (idempotent re-apply of `sql/50`).
- `REVOKE ALL ... FROM PUBLIC` on the aggregate + both support functions (parity with the scalar `ai.*`).

Files: `sql/50-theodb-ai.sql` (DDL), `benchmarks/tests/test_ai_sql.py` (3 tests), `docs/sql-ai-functions.md`
(doc), `smoke.sh` (presence), `CHANGELOG.md`.

## Evidence (real, no mock)

### Deterministic stub (CI, zero cost)
- `test_agg_summarize_over_rows` — aggregate over a 3-row table → one non-empty summary routed through the
  summarize system prompt (`"A concise summary: …"`). PASS.
- `test_agg_summarize_empty_and_null_input_is_null` — empty group and all-NULL rows → `NULL`, no LLM call. PASS.
- No regression: full `ai.*` offline suite **20 passed**.

### Real OpenAI (opt-in, gpt-4o-mini, key from gitignored `.env`)
- `test_real_openai_agg_summarize_shape` — **1 passed**.
- Captured real output, aggregating 3 incident notes:

  > **Input rows:** (1) "The deployment failed because the database ran out of disk space." (2) "A second
  > outage was caused by an expired TLS certificate." (3) "The team added monitoring alerts for disk usage
  > and certificate expiry."
  >
  > **`ai.agg_summarize` (gpt-4o-mini) →** "The deployment encountered failures due to insufficient disk
  > space in the database and an expired TLS certificate. To address these issues, the team implemented
  > monitoring alerts for both disk usage and certificate expiration."

  A genuine, coherent single summary of the collected rows — the aggregate works end-to-end against the
  real model. (Shape asserted, never exact text — LLM non-determinism.)

## Gates

- REVOKE FROM PUBLIC verified: `has_function_privilege('public','ai.agg_summarize(text)','execute')` → `f`.
- Idempotent re-apply of `sql/50-theodb-ai.sql`: applied twice, exit 0 both times.
- Baked into `theo-db:dev` via initdb.d (image rebuilt; `pg_proc` count for `agg_summarize` = 1).
- No new dependency (Rule 9); the 5 scalar `ai.*` + `ai._chat` unchanged (backward compatible).
- No unbenchmarked perf claim (Rule 5); cost/latency documented, no speed claim.

## Known limits (honest)

- Prompt bounded to 12000 chars (very large groups are truncated — documented; map-reduce deferred, ADR D2).
- Default model only (no per-call model override for the aggregate — YAGNI, ADR Q1).
