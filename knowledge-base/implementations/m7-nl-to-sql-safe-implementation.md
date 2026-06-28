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
