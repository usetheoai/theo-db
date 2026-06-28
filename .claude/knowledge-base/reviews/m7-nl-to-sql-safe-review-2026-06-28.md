# Review — M7-S4 Safe NL→SQL (anti-prompt-injection)

**Slug:** m7-nl-to-sql-safe
**Date:** 2026-06-28
**Verdict:** READY_TO_MERGE (after fixes)
**Plan:** `.claude/knowledge-base/plans/m7-nl-to-sql-safe-plan.md` (SHIPPABLE_WITH_CAVEATS 88.8)
**Discovery:** `.claude/knowledge-base/discoveries/blueprints/m7-nl-to-sql-safe-blueprint.md` (SHIPPABLE_WITH_CAVEATS 89)
**Code-quality:** `.claude/knowledge-base/audits/m7-nl-to-sql-safe-code-quality-2026-06-28.md` (PASS)
**Implementation:** `knowledge-base/implementations/m7-nl-to-sql-safe-implementation.md`
**Commits:** `fd68547` (impl) · `0d3bf92` (review fixes)

## Process

5 specialist agents in parallel (security/adversarial · test-auditor · cross-validation · architecture).
Initial tally: architecture READY; security + test + cross-validation NEEDS_FIXES — **2 independent BLOCKERs**
on read-exfiltration. All fixed + re-verified live (22 NL tests + real OpenAI).

## Findings & resolution

| # | Sev | Finding | Resolution | Verify |
|---|---|---|---|---|
| 1 | BLOCKER | L4 relation allowlist (regex `from/join`) bypassable for **read-exfil** by comma-join (`FROM documents, secret`) — only the first relation checked → `secret`/`pg_authid` readable (read-only doesn't block reads) | Replaced the regex with a **parser-grade allowlist via `EXPLAIN (FORMAT JSON)`** — the planner enumerates every relation it resolves; each checked vs `allowed_relations`. EXPLAIN plans but does not execute (side-effect-free) | `test_nl_to_sql_inject_comma_join_relation_rejected` + `test_nl_query_comma_join_exfil_blocked_no_leak` green (22023) |
| 2 | BLOCKER | Same L4 bypass via **quoted identifier** (`FROM "secret"`) — regex required `[a-zA-Z_]` after FROM → zero relations captured → allowlist no-op | Same EXPLAIN fix (the planner resolves quoted idents to the real relation) | `test_nl_to_sql_inject_quoted_identifier_rejected` green |
| 3 | HIGH | Denylist missed no-FROM exfil functions (`pg_stat_file`, `pg_ls_waldir/logdir/tmpdir/archive_statusdir`, `lo_get/put`) that EXPLAIN can't see as relations | Extended the L2 denylist (incl. `pg_ls_*` prefix + `pg_read_server_files`) | `test_nl_to_sql_inject_stat_file_rejected` green |
| 4 | HIGH | L3 (the declared load-bearing guard) never exercised end-to-end through `ai.nl_query` (write injections were caught at L2 first) | Added `test_nl_query_func_write_blocked_by_readonly_sandbox`: `SELECT nextval('s')` passes the L2 keyword denylist and is stopped ONLY by L3 (25006); sequence unadvanced | green (25006, `is_called=false`) |
| 5 | MEDIUM | `SET LOCAL transaction_read_only` leaked into the caller's transaction | PostgreSQL forbids restoring to read-write after a query ran (25001) — restore is impossible. Resolved honestly (Rule 3): documented the fail-safe contract (call in its own txn; in an explicit txn it stays read-only — restrictive, never permissive) + a test asserting that fail-safe behavior | `test_nl_query_readonly_is_failsafe_in_explicit_txn` green (a write after it → 25006) |
| 6 | LOW | Double-LIMIT syntax error when the generated SELECT already ends in LIMIT | Wrap as `FROM (%s) t LIMIT n` (outer LIMIT) | `test_nl_query_generated_limit_no_syntax_error` green |
| 7 | MEDIUM/LOW (test) | Missing: benign CTE path, empty question, the bypass vectors | Added benign-CTE, empty-question, and all bypass tests | suite = 22 tests green |

Reviewer-confirmed (no action): write/DDL integrity is double-blocked (L2 + L3/25006) — no DB-mutation payload
exists; api_key not leakable (L2 bans `current_setting`/`set_config`); SSRF inherited from `ai._chat`; DRY
(reuses `ai._chat` + one validation source `ai.nl_to_sql`); generate-vs-execute split clean; backward-compat
(S3 modes unchanged → 18 S3 tests green); REVOKE posture consistent; full `theodb_ai_nl` surface honestly
deferred (ADR D4).

## Hard gates (cycle-review)

| Gate | Status |
|---|---|
| Tests passing on branch | PASS — 22 NL + 18 S3 (no regression) + 69 unit |
| No secrets committed | PASS — `sk-proj` staged = 0; `.env` gitignored; api_key not leakable in SQL |
| No direct commit to `main` | PASS — develop |
| No authorship trailer (user policy) | PASS |
| CHANGELOG updated | PASS — `[Unreleased]` M7-S4 (hardened defense described) |
| No unbenchmarked perf claim | PASS — no perf claim (security slice) |
| Security gate (DoD) | PASS — read-exfil + write both deterministically blocked; "views parametrizadas seguras" enforced parser-grade |

## Verdict

READY_TO_MERGE. The ROADMAP M7-S4 DoD ("NL → SQL com guarda contra prompt-injection, views parametrizadas
seguras") is met with **functional security evidence**: the stub complies with every injection and each is
blocked — write/DDL by L2+L3 (25006), read-exfil (comma-join/quoted/non-allowlisted/`pg_stat_file`) by the
parser-grade EXPLAIN allowlist + extended denylist — with the database never mutated and `secret`/`pg_authid`
never read. Both BLOCKERs fixed and re-verified live; L3 proven end-to-end; real OpenAI re-verified through the
hardened path (`[{"count": 3}]`). Recommended deployment hardening (least-privilege read-only role) documented.
This is the **last M7 slice** — with S1 (released), S2/S3/S4 (READY_TO_MERGE on develop), M7's DoDs are complete.
