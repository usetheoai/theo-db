# Review — M7-S3 Generative-AI SQL functions (`ai.*`)

**Slug:** m7-ai-generative-functions
**Date:** 2026-06-28
**Verdict:** READY_TO_MERGE (after fixes)
**Plan:** `.claude/knowledge-base/plans/m7-ai-generative-functions-plan.md` (SHIPPABLE_WITH_CAVEATS 87.6)
**Code-quality:** `.claude/knowledge-base/audits/m7-ai-generative-functions-code-quality-2026-06-28.md` (PASS)
**Implementation:** `knowledge-base/implementations/m7-ai-generative-functions-implementation.md`
**Commits:** `e2abef2` (impl) · `87e3499` (review fixes)

## Process

5 specialist agents in parallel (security · correctness · test-auditor · cross-validation · architecture).
Initial tally: security + architecture READY; correctness + test + cross-validation NEEDS_FIXES (2 HIGH +
several MEDIUM). All addressed and re-verified live (offline 18 + real-OpenAI 1).

## Findings & resolution

| # | Sev | Finding | Resolution | Verify |
|---|---|---|---|---|
| 1 | HIGH | `ai.generate`/`ai.summarize` declared `LANGUAGE sql STABLE` — wrong for a non-deterministic side-effecting HTTP call; planner could fold/hoist calls, contradicting "per row" | Changed to `VOLATILE` (matches `theodb.embed` + the 3 plpython3u siblings) | `provolatile='v'` on all six functions; suite green |
| 2 | HIGH | CHANGELOG claimed "polaridade verificada" with no recorded evidence | Wrote `…-implementation.md` recording the real-OpenAI run (positive/negative observed 2026-06-28); CHANGELOG points to it | impl summary committed |
| 3 | MEDIUM | `ai.if` `startswith("no")` → "not sure"/"nope" silently return False (D4 fail-fast bypass); `ai.analyze_sentiment` "positively awful" → positive | First-token match (regex split) for both; unparseable → typed `22023` | `test_if_*`, sentiment tests green |
| 4 | HIGH/MED (test) | Plan claimed empty-completion→38000, NULL-prompt→22023, bad-shape→38000 "tested" but no test existed; generate/summarize asserted only non-empty (routing unproven) | Added `__EMPTY__`/`__BADSHAPE__`/`__NEUTRAL__`/`__NO__` stub seams + 6 tests; generate/summarize now assert system-prompt routing; negatives assert message substrings | 18 offline tests green |
| 5 | MEDIUM | SECURITY INVOKER: granting only the wrapper is insufficient (caller needs EXECUTE on `ai._chat`); comment implied wrapper-only grant → operator might grant `_chat` to PUBLIC (re-opens SSRF) | Corrected the comment to require granting `ai._chat` + wrapper together + explicit "never grant `_chat` to PUBLIC" | comment in `sql/50` |
| 6 | LOW | Stale "não implementadas" banner in `docs/features/07` | Banner updated to reflect the shipped scalar surface | doc diff |
| 7 | MEDIUM (arch) | DRY: `ai._chat` duplicates `theodb.embed` SSRF/transport knowledge | Tracked as a follow-up slice (extract `theodb._http_post_json` before the 3rd HTTP fn; touches `sql/30`) — Rule-of-3 tolerates the 2nd copy; not blocking | impl summary § follow-ups |
| 8 | LOW | `ai.rank` no clamp (out-of-range allowed) | Accepted per plan Q2 (model-defined range); documented | docs |

Non-issues confirmed by reviewers: SSRF guard (http(s)-only + no-redirect) on par with `theodb.embed`;
api_key never interpolated into any error string (the `str(body)[:200]` echo is the *response*, key lives only
in the request header); injection-safe (`plpy.prepare` typed params + `json.dumps`); stub is dev-only (not in
the image); D1 single-HTTP-source-of-truth honored.

## Hard gates (cycle-review)

| Gate | Status |
|---|---|
| Tests passing on branch | PASS — 69 unit + 44 integration (1 real-OpenAI skip when env unset) |
| No secrets committed | PASS — staged `sk-proj` matches = 0; `.env` gitignored; api_key never in errors |
| No direct commit to `main` | PASS — develop |
| No authorship trailer (user policy) | PASS |
| CHANGELOG updated | PASS — `[Unreleased]` M7-S3, points at recorded real-OpenAI evidence |
| No unbenchmarked perf claim | PASS — no performance number claimed (rule 5) |

## Verdict

READY_TO_MERGE. Both HIGH findings fixed and re-verified live; all MEDIUM/LOW addressed or honestly tracked.
The slice ships five generative `ai.*` functions over a configurable model (correct volatility, fail-fast
parsing, SSRF-guarded, least-privilege) with real OpenAI end-to-end evidence recorded. Follow-up (non-blocking):
extract the shared `theodb._http_post_json` helper before the next HTTP-calling function (DRY of security
knowledge across `theodb.embed` + `ai._chat`). M7 stays open until S2 (BM25 permissive) + S4 (NL→SQL) land.
