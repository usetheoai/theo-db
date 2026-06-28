# Review — M12 `theodb_ai_nl` config surface (feature 12)

**Slug:** m12-nl-surface · **Milestone:** M12 · **Date:** 2026-06-28
**Verdict:** READY_TO_MERGE (after fixes)
**Plan:** `.claude/knowledge-base/plans/m12-nl-surface-plan.md` (plan-confidence **SHIPPABLE 97.6**)
**Discovery:** satisfied by `m7-nl-to-sql-safe-blueprint.md` (the safe-NL design this builds on).
**Code-quality:** `.claude/knowledge-base/audits/m12-nl-surface-code-quality-2026-06-28.md` (**PASS** 100)
**Implementation:** `knowledge-base/implementations/m12-nl-surface-implementation.md` (real evidence `[{"count": 3}]`)
**Commits:** `a8820e2` (impl) · `793f491` (review fixes)

## Process

4 specialist agents in parallel (security · test-auditor · cross-validation · architecture+parsimony), all
with live verification against container `m12-it`. Tally: security + cross-validation + architecture =
READY_TO_MERGE; test-auditor = NEEDS_FIXES. All actionable findings fixed + re-verified live. No BLOCKER.

## ROADMAP M12 DoD — met (honestly)

| DoD | Status | Evidence |
|---|---|---|
| config (table/GUC) + template registry + value-index | ✅ | `ai.nl_config`/`nl_templates`/`nl_value_index` + 5 mgmt fns |
| anti-injection of the MVP preserved (regression) | ✅ | gate (`sql/60`) byte-unchanged; injection via `ai.nl_query_cfg` → 22023 + DB intact; L4-wiring proven (see #1) |
| tests (generate + execute) green + real OpenAI evidence | ✅ | 34 offline + 1 real; real `[{"count": 3}]` |
| doc | ✅ | `docs/sql-ai-functions.md` § theodb_ai_nl config surface |

## Findings & resolution

| # | Sev | Finding | Resolution | Verify |
|---|---|---|---|---|
| 1 | HIGH (test) | the injection test hit L2 (keyword denylist), independent of `allowed_relations` — it did NOT prove the config's unique job (forwarding `cfg.allowed_relations` to the gate's L4) | Added `test_nl_query_cfg_relation_exfil_blocked_by_l4`: a comma-join exfil of `secret` (NOT in cfg1's allowlist) through `ai.nl_query_cfg` → 22023 (L4) + `secret` intact | test PASS (live) |
| 2 | MEDIUM (arch) | plan ADR D3 claimed a `transaction_read_only=on` guard the code omits | **Honest doc-to-code alignment**: the refresh is a FIXED-SHAPE read (`SELECT DISTINCT %I FROM %s::regclass` — no user SQL), so no read-only GUC is needed (it would also block the function's own index upsert). Corrected ADR D3 / Objective / Drawbacks / Coverage wording to match the code | plan diff |
| 3 | MEDIUM (test) | disabled-template exclusion + `nl_set_template_enabled` not-found untested | Added `test_nl_set_template_enabled_disables_and_not_found` | test PASS |
| 4 | MEDIUM (test) | explicit `ai.nl_set_value_index` path untested | Added `test_nl_set_value_index_explicit_and_guards` (upsert + NULL guard + unknown-config guard) | test PASS |
| 5 | LOW (arch) | `nl_set_value_index` allowed orphan rows (no config check) | Added a `config not found → 22023` check (fail-fast, parity with refresh) | test PASS |
| 6 | LOW (test) | `nl_query_cfg` empty-question + refresh `max<=0` untested | Added `test_nl_query_cfg_empty_question_raises` + `test_nl_refresh_value_index_rejects_nonpositive_max` | test PASS |
| 7 | LOW (cross-val) | smoke asserted 0-public among the 6 fns but not that they EXIST | smoke now asserts `3:6:0` (3 tables, 6 fns present, 0 public) | smoke live `3:6:0` |
| 8 | INFO (cross-val) | ROADMAP M12 "Entregáveis" named `sql/60` | corrected to `sql/61` (gate reused unchanged) | ROADMAP diff |
| 9 | INFO (security/arch) | refresh case-sensitive allowlist match (stricter than the gate); no FK on template_id | accepted — fail-closed / graceful-degrade; documented (KISS) | — |

## Hard gates (cycle-review)

| Gate | Status |
|---|---|
| Tests passing on branch | PASS — 34 offline nl tests + 1 real green; code-quality PASS |
| No secrets committed | PASS — `sk-proj` staged = 0; `.env` gitignored |
| No direct commit to `main` | PASS — develop |
| No Co-Authored-By trailer | PASS |
| CHANGELOG updated | PASS — `[Unreleased] § Added` M12 entry |
| No unbenchmarked perf claim | PASS — no perf claim made |
| No new dependency (Rule 9) | PASS — composes the M7-S4 gate; image unchanged |
| Security gate preserved | PASS — `sql/60` byte-unchanged (`git diff` count 0); injection via config blocked + DB intact; L4 wiring proven |

## Verdict

**READY_TO_MERGE.** M12 delivers feature 12's config surface (config + templates + value-index +
`ai.nl_query_cfg`) over the **unchanged** M7-S4 anti-injection gate. The security claim is verified live and
adversarially: the gate is byte-unchanged, an injection through the config path is rejected (22023) with the
DB intact, and — after the HIGH fix — a test proves the config-supplied `allowed_relations` reaches the gate's
L4 relation allowlist (a non-allowlisted exfil is blocked). The value-index auto-refresh is injection-proof
(allowlist + identifier validation + fixed-shape read). All review findings fixed and re-verified live (34 +
1 tests green). Honest divergence from the literal 58-function AlloyDB extension is documented in 5 places.
