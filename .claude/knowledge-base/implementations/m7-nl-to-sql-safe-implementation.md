# Implementation Summary — M7-S4 Safe NL→SQL

**Slug:** m7-nl-to-sql-safe · **Date:** 2026-06-28 · **Plan:** SHIPPABLE_WITH_CAVEATS 88.8

## Delivered (sql/60-theodb-nl.sql)
- `ai.nl_to_sql(question, allowed_relations, model)` → validated read-only SELECT (L1 prompt + L2 static validation + L4 allowlist; fail-fast 22023).
- `ai.nl_query(question, allowed_relations, model, max_rows)` → jsonb rows, executed in the L3 PG-native read-only sandbox (`transaction_read_only`+`statement_timeout` → 25006 on writes).
- Both REVOKE'd from PUBLIC; baked via initdb.d.

## Security evidence (anti-prompt-injection — the gate)
`benchmarks/tests/test_nl_sql.py` — 13 tests green. The stub *complies* with each injection so the GUARDS are what's proven:
- DROP (`__NLINJECT_DROP__`) → 22023, `secret` table intact.
- write/UPDATE (`__NLINJECT_WRITE__`) → 22023, `documents` unmodified.
- multi-statement / `pg_read_file` exfil / non-allowlisted relation (pg_authid) → 22023.
- L3 read-only sandbox proven independently: a write under `set_config('transaction_read_only','on',true)` raises 25006, row unchanged.
- empty allowlist / max_rows<=0 → 22023; both functions non-PUBLIC.

## Real evidence (no mock)
Real OpenAI (gpt-4o-mini, key from gitignored .env), 2026-06-28: `ai.nl_query('how many rows are in documents', ARRAY['documents'])` → `[{"count": 2}]` — the model generated SQL, it passed L1/L2/L4, executed in the L3 sandbox, returned the correct count.

## No regression
S3 ai.* (18 offline) green after the stub change; 69 unit; ruff + vulture clean.

## Deferred (honest)
Full AlloyDB `theodb_ai_nl` config/template/value-index surface (ADR D4 — YAGNI). Recommended deployment hardening: run `ai.nl_query` under a least-privilege read-only role (read-only txn does not block role-gated read funcs; covered by L2 denylist).

## Review hardening (cycle-review NEEDS_FIXES → resolved)
- **BLOCKER (read-exfil bypass):** L4 regex allowlist was bypassable by comma-join (`FROM documents, secret`) and quoted identifiers (`FROM "secret"`). **Fixed:** replaced the regex with a **parser-grade allowlist via `EXPLAIN (FORMAT JSON)`** — the planner enumerates every relation (comma-join/quoted/subquery/CTE base rels); each is checked against `allowed_relations`. Confirmed: comma-join + quoted exfil now rejected (22023).
- **HIGH (denylist gaps):** added `pg_stat_file`, `pg_ls_waldir/logdir/tmpdir/archive_statusdir`, `pg_ls_*` prefix, `lo_get/put`, `pg_read_server_files` to the L2 denylist (no-FROM exfil funcs EXPLAIN can't see as relations).
- **HIGH (L3 untested end-to-end):** added `test_nl_query_func_write_blocked_by_readonly_sandbox` — `SELECT nextval('s')` passes the L2 keyword denylist and is stopped ONLY by L3 (25006), sequence unadvanced. Proves L3 is load-bearing on the real `ai.nl_query` path.
- **MEDIUM (SET LOCAL leak):** PostgreSQL forbids restoring transaction_read_only to read-write after a query (25001), so the GUC cannot be restored mid-txn. Resolved honestly: documented the fail-safe contract (call in its own transaction; in an explicit txn it stays read-only — restrictive, never permissive) + a test asserting that fail-safe behavior.
- **LOW (double-LIMIT):** wrap as `FROM (%s) t LIMIT n` (outer) so a generated SELECT ending in LIMIT doesn't 42601.
- Real OpenAI re-verified through the EXPLAIN-allowlist path: `ai.nl_query('how many rows are in documents', ARRAY['documents'])` → `[{"count": 3}]`. Full suite: 22 NL + 18 S3 (no regression) + 69 unit green.
