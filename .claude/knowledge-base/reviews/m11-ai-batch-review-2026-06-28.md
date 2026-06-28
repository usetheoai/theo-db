# Review — M11 `ai.generate_batch` (accelerated batch AI, feature 08)

**Slug:** m11-ai-batch · **Milestone:** M11 · **Date:** 2026-06-28
**Verdict:** READY_TO_MERGE (after fixes)
**Plan:** `.claude/knowledge-base/plans/m11-ai-batch-plan.md` (plan-confidence **SHIPPABLE 90.4**)
**Discovery:** satisfied by existing AI blueprints (`alloydb-vector-ai-implementation`, `m7-*`); the batch-design decision (one packed request vs N calls) is plan ADR D1.
**Code-quality:** `.claude/knowledge-base/audits/m11-ai-batch-code-quality-2026-06-28.md` (**PASS** 100)
**Implementation:** `knowledge-base/implementations/m11-ai-batch-implementation.md` (real-OpenAI evidence `['Paris','4','cold']`)
**Commits:** `caa3c1f` (impl) · `6972fbd` (review fixes)

## Process

4 specialist agents in parallel (test-auditor · cross-validation · security+architecture · batch-semantics
correctness), all with live verification against container `m11-it`. Tally: cross-validation + security+arch
= READY_TO_MERGE; test-auditor + batch-correctness = NEEDS_FIXES. All actionable findings fixed +
re-verified live (rebuilt image). No BLOCKER.

## ROADMAP M11 DoD — met (honestly)

| DoD | Status | Evidence |
|---|---|---|
| ≥ 1 batched `ai.*` fn; REVOKE FROM PUBLIC | ✅ | `ai.generate_batch(text[],text)`; `has_function_privilege('public',…)=f` |
| round-trip reduction measured + N answers in order | ✅ (DoD reworded — finding #6) | batch → stub `/count` delta **1**; 3 scalar → **3**; answers in order |
| doc with example | ✅ | `docs/sql-ai-functions.md` § Accelerated batch |

## Findings & resolution

| # | Sev | Finding | Resolution | Verify |
|---|---|---|---|---|
| 1 | HIGH (correctness ×2) | `str(x)` silently coerced non-string JSON elements (`4`→"4", `{"a":1}`→"{'a': 1}", `true`→"True") — silent corruption (Rule 8) | Fail-fast `22023` on any non-string element; `__nonstr__` stub seam + `test_generate_batch_non_string_element_raises_typed` | test PASS (live) |
| 2 | HIGH (test) | invalid-JSON / prose-reply path (the core "valid JSON" half) untested | Added `test_generate_batch_invalid_json_raises_typed` via the `__malformed__` seam (asserts 22023 + "valid JSON") | test PASS |
| 3 | MEDIUM (correctness) | stub over-counted when a prompt embedded a `N. …` line → false `22023` on a legit batch | Stub now sizes the JSON array from the declared N (`exactly N strings`), not by counting user lines; `test_generate_batch_embedded_numbered_line_still_one_batch` | test PASS |
| 4 | MEDIUM (test) | markdown-fence strip untested + fragile on single-line fence | Replaced with a robust regex strip (single + multi-line); `__fenced__` seam + `test_generate_batch_strips_json_fence` | test PASS |
| 5 | MEDIUM (test) | threadsafe test a weak mutation-killer; comment overclaimed | Raised K to 300; comment softened to a reliability test (lock guards the read-modify-write; GIL ≠ atomic) | test PASS |
| 6 | LOW (cross-val) | ROADMAP M11 DoD said "same result as N scalar calls" (not the shipped mechanism) | Reworded to the measured round-trip-reduction + N-answers-in-order contract | ROADMAP diff |
| 7 | LOW (security) | stale GRANT example (omitted `generate_batch` + `agg_summarize`) | Refreshed the example GRANT list | sql diff |
| 8 | LOW/INFO (security) | no `COMMENT ON FUNCTION`; single-line fence | Added `COMMENT ON FUNCTION ai.generate_batch`; regex strip handles single-line fence | sql diff |
| 9 | INFO (security/correctness) | in-band numbering weakens N-alignment under crafted multi-line prompts (model could miscount) | Documented best-effort limitation (comment + doc): use scalar `ai.generate` for a guaranteed per-item result; `len==N` defends count | doc/comment |

### Release-sequencing note (cross-val HIGH — human action, not a code defect)

`[Unreleased]` now carries **both M10 and M11** (M10 was never released; last tag v0.9.0). `cycle-release`'s
single-flip invariant flips exactly ONE milestone per release. **At release time the human MUST either cut
two releases (M10 then M11, each flipping its own checkbox + writing its roadmap-runs file) OR consciously
bundle and manually flip both checkboxes with two run-file entries.** Not an M11 code issue — a release
decision recorded here so it is not missed.

## Hard gates (cycle-review)

| Gate | Status |
|---|---|
| Tests passing on branch | PASS — 33 offline ai tests + 1 real-OpenAI green; ruff clean |
| No secrets committed | PASS — `sk-proj` staged = 0; `.env` gitignored |
| No direct commit to `main` | PASS — develop |
| No Co-Authored-By trailer | PASS |
| CHANGELOG updated | PASS — `[Unreleased] § Added` M11 entry |
| No unbenchmarked perf claim | PASS — acceleration = measured round-trip count; `faster than|outperforms` = 0 in doc |
| No new dependency (Rule 9) | PASS — composes `ai._chat`; stdlib only |
| Backward compatibility | PASS — scalar `ai.*` + `ai._chat` + `ai.agg_summarize` + existing stub branches unchanged |

## Verdict

**READY_TO_MERGE.** M11 closes feature 08's accelerated path: `ai.generate_batch(text[])` answers N
prompts in ONE `ai._chat` round-trip (no new dependency), with the acceleration **measured** (batch=+1 vs
N scalar=+N requests) and real gpt-4o-mini evidence (`['Paris','4','cold']`). The two HIGH findings (silent
non-string coercion; untested invalid-JSON path) and all MEDIUM/LOW items are fixed and re-verified live
(rebuilt image; 33 offline + 1 real test green; strict element validation, faithful stub, robust fence
strip). The M10+M11 release-sequencing is flagged for the human at `/release` time.
