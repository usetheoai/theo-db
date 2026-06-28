# Review — M10 `ai.agg_summarize` (aggregate summarization, feature 11)

**Slug:** m10-agg-summarize · **Milestone:** M10 · **Date:** 2026-06-28
**Verdict:** READY_TO_MERGE (after fixes)
**Plan:** `.claude/knowledge-base/plans/m10-agg-summarize-plan.md` (plan-confidence **SHIPPABLE 96.0**)
**Discovery:** satisfied by existing AI blueprints (`alloydb-vector-ai-implementation`, `m7-*`); the aggregate-design decision (concat-with-cap vs map-reduce) is captured in plan ADR D2.
**Code-quality:** `.claude/knowledge-base/audits/m10-agg-summarize-code-quality-2026-06-28.md` (**PASS** 100)
**Implementation:** `knowledge-base/implementations/m10-agg-summarize-implementation.md` (real-OpenAI evidence)
**Commits:** `ba7052b` (impl) · `5cb790e` (review fixes)

## Process

4 specialist agents in parallel (test-auditor · cross-validation · security+architecture · PostgreSQL
domain), all with live verification against container `m10-it`. Tally: security+arch + PG-domain =
READY_TO_MERGE; test-auditor + cross-validation = NEEDS_FIXES. All actionable findings fixed + re-verified
live (rebuilt image). No BLOCKER.

## ROADMAP M10 DoD — met (honestly)

| DoD | Status | Evidence |
|---|---|---|
| `ai.agg_summarize` aggregate created; LLM call VOLATILE in finalfunc; REVOKE FROM PUBLIC | ✅ | `CREATE AGGREGATE`; finalfunc `provolatile='v'`; all 3 objects `has_function_privilege('public',…)=f` (DoD wording corrected — see finding #1) |
| stub test (N rows → one summary) green + real evidence recorded | ✅ | `test_agg_summarize_over_rows` PASS; real gpt-4o-mini summary of 3 incident notes logged |
| doc with example | ✅ | `docs/sql-ai-functions.md` § Aggregate summarization |

## Findings & resolution

| # | Sev | Finding | Resolution | Verify |
|---|---|---|---|---|
| 1 | HIGH (cross-val) → re-scoped | Aggregate shipped `provolatile='i'` while DoD/doc/comment claimed VOLATILE | **Empirically probed**: PG gives EVERY aggregate `i`/`s`, never `v` (live: 155 `i` / 8 `s` / 0 `v`; `string_agg`/`sum` are `i`; no syntax for VOLATILE aggregate). The real anti-cache guarantee is the **VOLATILE finalfunc** (re-run per query; aggregates aren't constant-folded). Reverted accum to honest `IMMUTABLE`; corrected test/smoke/doc/comment/ROADMAP-DoD to assert/state the finalfunc-VOLATILE guarantee | `_agg_summ_final='v'`, `agg_summarize='i'`; `test_agg_summarize_finalfunc_is_volatile` PASS |
| 2 | MEDIUM (test) | PUBLIC-revoke regression test didn't cover the 3 new objects | Extended `test_ai_functions_not_executable_by_public` to include `agg_summarize` + `_agg_summ_accum` + `_agg_summ_final` | test PASS; AGG smoke `…:0` |
| 3 | MEDIUM (test) + LOW (domain) | mixed NULL/non-NULL accum branch + all-empty untested; empty-string not skipped | accum now skips `NULL OR ''`; added `test_agg_summarize_skips_null_and_empty_rows` (mixed → summary; all-empty → NULL, no LLM call) | test PASS |
| 4 | MEDIUM (domain) | summary order-dependence (indeterminate input order) undocumented | documented in the SQL header comment, `COMMENT ON AGGREGATE`, and the doc: pin with `ai.agg_summarize(x ORDER BY <key>)` | comment/doc diff |
| 5 | LOW (test) | no aggregate-path negative test | added `test_agg_summarize_propagates_empty_completion_typed` (`__EMPTY__` → 38000 through the finalfunc) | test PASS |
| 6 | LOW (security) | per-group cost not commented on the aggregate | `COMMENT ON AGGREGATE` states one synchronous LLM call per group; cost scales with group count | `\dd`/comment |
| 7 | INFO (security/domain) | parallel-safety; idempotency; SSRF inheritance; dual-grant; NULL→NULL | confirmed correct (no `combinefunc` + PARALLEL UNSAFE → no partial LLM fan-out; DROP-before-CREATE; inherits `ai._chat` SSRF + typed errors) | live probes |

## Hard gates (cycle-review)

| Gate | Status |
|---|---|
| Tests passing on branch | PASS — 23 offline ai tests + 1 real-OpenAI green; lint (ruff) clean |
| No secrets committed | PASS — `sk-proj` staged = 0; `.env` gitignored |
| No direct commit to `main` | PASS — develop |
| No Co-Authored-By trailer | PASS |
| CHANGELOG updated | PASS — `[Unreleased] § Added` M10 entry |
| No unbenchmarked perf claim | PASS — cost documented per-group; no speed claim |
| No new dependency (Rule 9) | PASS — composes shipped `ai._chat`; image deps unchanged |
| Backward compatibility | PASS — 5 scalar `ai.*` + `ai._chat` unchanged; aggregate is additive |

## Verdict

**READY_TO_MERGE.** M10 closes feature 11's aggregate path: `ai.agg_summarize(text)` collapses many rows
into one summary via the shipped `ai._chat` (no new dependency), with real gpt-4o-mini evidence. The one
HIGH (aggregate volatility) was resolved honestly after an empirical probe proved no PostgreSQL aggregate
can be VOLATILE — the guarantee is the VOLATILE finalfunc, now asserted by test + smoke. All MEDIUM/LOW
findings fixed and re-verified live (rebuilt image, 23 + 1 tests green, REVOKE/volatility/NULL-safety/
order-doc all confirmed).
