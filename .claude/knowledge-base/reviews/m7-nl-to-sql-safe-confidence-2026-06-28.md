# Discover-Confidence — m7-nl-to-sql-safe

**Date:** 2026-06-28
**Verdict:** SHIPPABLE_WITH_CAVEATS
**Blueprint:** .claude/knowledge-base/discoveries/blueprints/m7-nl-to-sql-safe-blueprint.md

## Key finding (the S4 security contract)
4-layer never-trust-the-LLM defense, load-bearing = **PG-native read-only sandbox** (`SET TRANSACTION READ ONLY` → SQLSTATE 25006 on any write, sourced verbatim from postgresql.org). Honest residual-risk: read-only does NOT block `COPY ... TO PROGRAM` / `pg_read_file` / `lo_*` / `dblink` (role-gated, not write-gated) → restricted role + static banned-function validation + relation allowlist required. Generate-vs-execute split (`ai.nl_to_sql` returns validated SQL; `ai.nl_query` executes only in the sandbox). SOTA anchor: AlloyDB get_sql/execute_nl_query + "parameterized secure views"; OWASP LLM01 (prompt-layer insufficient → fail safe by construction).

## Caveat
AlloyDB exact page on docs.cloud.google.com (301 off-allowlist) — posture confirmed via cloud.google.com search. No load-bearing security guarantee is UNVERIFIED (all from postgresql.org).
