# Review — M16 unified-vector-relational

**Slug:** unified-vector-relational · **Milestone:** M16 · **Date:** 2026-06-29
**Verdict:** READY_TO_MERGE (after addressing 2 HIGH)
**Plan:** `.claude/knowledge-base/plans/unified-vector-relational-plan.md` (plan-confidence SHIPPABLE_WITH_CAVEATS 86.0)
**Implementation:** `.claude/knowledge-base/implementations/unified-vector-relational-implementation.md`
**Code-quality:** PASS (no languages enabled → NOOP)
**Commits reviewed:** `9a100d2` (impl) · `7e77470` (review fixes)

## Process

3 specialist agents in parallel:
1. **cross-validation + architecture** → READY_TO_MERGE (2 MEDIUM, 1 LOW).
2. **test-audit + wiring** → **NEEDS_FIXES (2 HIGH)** — the binding verdict.
3. **SQL/extension + security** → READY_TO_MERGE (3 INFO only).

All three confirmed the cycle-review hard gates pass. The 2 HIGH from the test-auditor were fixed + re-verified
before this verdict.

## Hard gates (cycle-review) — all PASS

| Gate | Status | Evidence |
|---|---|---|
| Tests passing on branch | PASS | `test_unified.py` 11/11 + `test_extension_install.py` 9/9 + `smoke.sh` PASSED vs theo-db:m16; ruff clean |
| No secrets committed | PASS | diff grep clean (agent 3) |
| No direct commit to main | PASS | branch develop |
| No Co-Authored-By trailer | PASS | `9a100d2`/`7e77470` trailers empty (agent 3) |
| CHANGELOG updated | PASS | `[Unreleased] § Added` (M16) |
| SQL injection (import_pinecone) | PASS | regclass + `%I` + bound `USING` params; hostile-identifier test green (agent 3) |
| Error handling (typed, fail-fast) | PASS | malformed → SQLSTATE 22023; full rollback, no partial insert (agent 3) |
| Least privilege | PASS | `REVOKE ALL … FROM PUBLIC`; SECURITY INVOKER (agent 3) |
| Extension-safety (sql/80) | PASS | no top-level tx / no CREATE EXTENSION (agent 3) |
| No unbenchmarked perf claim (ADR 0005 / public-copy) | PASS | new docs carry no speed claim; demo is simplicity/consistency (agents 1+3) |

## Findings & resolution

| # | Sev | Finding (agent) | Resolution | Verify |
|---|---|---|---|---|
| 1 | **HIGH** | The "+AI" third of the unification moat was untested — `test_unified_query_*` covered only vector+relational; Goal/Coverage/impl-log claim AI (agents 1+2) | added `test_unified_query_with_ai_leg`: runs the FULL unified SQL (vector JOIN relational WHERE + `ai.summarize`) via the deterministic chat stub; asserts the AI leg routes ("A concise summary") | test green (11/11) |
| 2 | **HIGH** | Plan-declared runnable-SQL migration-doc test downgraded to a string grep; migrate doc had `vector(1536)` vs 3-dim values (would fail if copied) (agents 1+2) | replaced with `test_migrate_doc_runnable_sql_executes` (runs the guide's SQL blocks against the container); fixed the doc to a consistent toy `vector(3)` + a note to use the real dim | test green; doc consistent |
| 3 | MEDIUM | over-filtering xfail guard could mask permanent non-coverage (agent 2) | the test now PASSES deterministically (far cluster + `enable_seqscan=off` reproduces over-filtering); the xfail remains only as an honest fallback, not the normal path | test passes (not xfail) |
| 4 | LOW | `safe_identifiers` thin; `dim_mismatch` asserts base `DataException`; observability is introspection not a counter | accepted — the mechanism (`%I`/regclass) is correct, errors are typed, and an introspectable extension function is the right observability shape for SQL (no runtime counter needed) | — |
| 5 | INFO | import_pinecone atomicity comment conservative; `CREATE SCHEMA IF NOT EXISTS` redundant; new object in 1.0 base vs upgrade-chain (pre-1.0 accepted) | no change needed (documented; pre-1.0 per plan Drawbacks) | — |

## ROADMAP M16 DoD — met (honestly)

| DoD | Status | Evidence |
|---|---|---|
| Query unificada canônica + e2e | ✅ | `docs/quickstart.md` § Unified query; `test_unified_query_returns_correct_joined_rows` + `_with_ai_leg` (all 3 legs) |
| Filtered vector search (over-filtering + EXPLAIN) | ✅ | over-filtering PROVEN (not trivial) + Index-Scan EXPLAIN test |
| Migração do Pinecone (import + teste) + guia | ✅ | `theodb.import_pinecone` + 4 tests; `docs/migrate-from-pinecone.md` (SQL executed by test) |
| Demo honesta 1-vs-2 | ✅ | `docs/unification-1-vs-2-systems.md`; no-perf-claim test enforces it |
| Sem dep nova / sem claim de performance | ✅ | native jsonb; public-copy clean |

## Verdict

**READY_TO_MERGE.** M16 makes the unification moat (ADR 0005) demonstrable end-to-end — validated against the
rebuilt `theo-db:m16`: 11/11 unified tests (all 3 legs incl. AI via stub), the over-filtering recall fix
genuinely proven, the Pinecone import injection-safe + typed-error-tested, and the migration guide's SQL
executed (not grepped). No BLOCKER; the 2 HIGH coverage-honesty gaps were fixed and re-verified; all hard
gates green. Next: `/release` (when the human chooses) — would cut v0.15.0 + flip ROADMAP M16.
