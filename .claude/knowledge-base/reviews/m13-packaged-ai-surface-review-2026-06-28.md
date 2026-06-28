# Review — M13 native packaged AI surface (features 06/07)

**Slug:** m13-packaged-ai-surface · **Milestone:** M13 · **Date:** 2026-06-28
**Verdict:** READY_TO_MERGE (after fixes)
**Plan:** `.claude/knowledge-base/plans/m13-packaged-ai-surface-plan.md` (plan-confidence **SHIPPABLE 95.2**)
**Discovery:** satisfied by `m7-hybrid-search-rrf-blueprint.md` + `alloydb-vector-ai-implementation-blueprint.md`.
**Code-quality:** `.claude/knowledge-base/audits/m13-packaged-ai-surface-code-quality-2026-06-28.md` (**PASS** 100)
**Implementation:** `knowledge-base/implementations/m13-packaged-ai-surface-implementation.md` (real evidence `'ok'`)
**Commits:** `156433d` (impl) · `89c6d76` (review fixes)

## Process

4 specialist agents (security · cross-validation · architecture+parsimony · test-auditor), all with live
verification against container `m13-it` (test-auditor re-run after a transient rate-limit). Tally: security +
cross-validation + architecture + test-auditor = READY_TO_MERGE. All actionable findings fixed + re-verified
live. No BLOCKER/HIGH.

## ROADMAP M13 DoD — met (honestly)

| DoD | Status | Evidence |
|---|---|---|
| `ai.hybrid_search()` JSON OR ADR | ✅ | `ai.hybrid_search(jsonb)` thin wrapper (ADR D1) + parity test (identical rows to rrf) |
| `theodb_ml` `create_model` OR ADR | ✅ | `theodb_ml` registry (create/drop/list/apply) + bridge to unchanged `ai._chat` |
| REVOKE + doc + honest sugar-vs-capability | ✅ | all fns non-public; doc § Packaged surface; ADR D1 (sugar) / D2 (capability + security divergence) |

## Findings & resolution

| # | Sev | Finding | Resolution | Verify |
|---|---|---|---|---|
| 1 | MEDIUM (arch) | `apply_model` left a stale `theodb.llm_model` GUC: apply(A,model) then apply(B,NULL) kept A's model → B's endpoint used with A's model | `apply_model` now ALWAYS `set_config('theodb.llm_model', COALESCE(model_name,''))` (`''` → `ai._chat` falls back to `'default'`); regression test `test_theodb_ml_apply_model_resets_stale_model_guc` | test PASS (live) |
| 2 | MEDIUM (test) | registry tests shared mutable `theodb_ml.models` state (worked by unique ids, not by design) | autouse `_clean_registry` fixture (DELETE before+after each test) — independence guaranteed | 9 tests PASS |
| 3 | LOW (test) | dead `_set_endpoint` helper left over from the test split (would trip `/code-quality` D1) | removed | vulture clean |
| 4 | LOW (test) | `create_model` upsert + empty-id guard unasserted | added `test_theodb_ml_create_model_upsert_updates` + `_empty_id_raises` | tests PASS |
| 5 | LOW (arch/cross-val) | spec-06/07 divergence (no literal `CALL`/`model_id =>`/key storage) under-documented | added honest delivered-notes to `docs/features/06` + `07` (D2 no-key-persistence; GUC bridge) | doc diff |
| 6 | LOW (cross-val) | plan said registry tests append to `test_ai_sql.py`; impl split to `test_theodb_ml.py` (file-size budget) | documented in impl log + here; no coverage gap | both files green |
| 7 | INFO (security) | adversarial probes (hostile `table`/`*_col`, `file://` endpoint, key leak) | all failed safely live; `ai._chat` re-validates scheme; `%I`/regclass quoting holds | security agent live |

## Hard gates (cycle-review)

| Gate | Status |
|---|---|
| Tests passing on branch | PASS — 9 theodb_ml + 2 hybrid-parity + 1 real green; 103 offline suite green; ruff + vulture clean |
| No secrets committed | PASS — `sk-proj` staged = 0; `.env` gitignored |
| No direct commit to `main` | PASS — develop |
| No Co-Authored-By trailer | PASS |
| CHANGELOG updated | PASS — `[Unreleased] § Added` M13 entry |
| No unbenchmarked perf claim | PASS — `ai.hybrid_search` labeled sugar; no perf claim |
| No new dependency (Rule 9) | PASS — composes existing capabilities |
| Existing capabilities unchanged | PASS — `sql/50` (`ai._chat`) absent from diff; `sql/40` additive only (`ai.hybrid_search_rrf` body untouched) |
| **API keys never persisted (security)** | PASS — `theodb_ml.models` has 0 `%key%` columns (live + test); keys stay session GUCs (ADR D2) |

## Verdict

**READY_TO_MERGE.** M13 ships the literal spec-06/07 packaged surface — `ai.hybrid_search(jsonb)` (thin
wrapper, parity-tested honest sugar) and the `theodb_ml` registry (`create_model`/`apply_model`) — over the
**unchanged** `ai.hybrid_search_rrf` + `ai._chat` (no new dependency). The security posture is verified live:
**API keys are never persisted** (no registry column; keys stay session GUCs — ADR D2), the SSRF scheme guard
holds at both create and call time, and the JSON wrapper cannot inject (regclass + `%I` quoting). The one
MEDIUM (stale-model GUC) and all test-hygiene LOWs are fixed and re-verified live; real gpt-4o-mini evidence
(`apply_model` → `ai.generate` → `'ok'`) is captured.
