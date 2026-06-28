# Code-Quality Audit — m7-nl-to-sql-safe

**Date:** 2026-06-28 · **Verdict:** PASS (0 hard caps, 0 soft caps)

`run_code_quality.py m7-nl-to-sql-safe` → PASS. Independently verified: vulture + ruff clean on
`theodb_bench`/`tests/test_nl_sql.py`/`tools/chat_server.py`; imports resolve (13 NL + 18 S3 + 69 unit pass);
`bash -n smoke.sh` clean; SQL validated functionally (`ai.nl_to_sql`/`ai.nl_query` load from initdb.d; 13
security tests + real OpenAI green).

## Wiring triad
- `ai.nl_to_sql`/`ai.nl_query` — callers: smoke presence + `test_nl_sql.py` + real OpenAI; observable: 22023/25006 typed errors + jsonb rows + DB-intact assertions.
- `tools/chat_server.py` NL/injection modes — caller: test fixture; observable: deterministic injection payloads → guards catch them.

PASS → proceed to /review.
