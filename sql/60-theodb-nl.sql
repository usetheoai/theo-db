-- TheoDB M7-S4 / M19 — Safe natural-language -> SQL with anti-prompt-injection guardrails.
-- The hard problem is NOT generating SQL (one ai._chat call) — it is making execution SAFE when the user
-- question is adversarial. The layered defense does NOT trust the LLM (blueprint m7-nl-to-sql-safe):
--   L1 prompt constraint · L2 static validation (single statement, SELECT/WITH-only, banned-function
--   denylist) · L4 PARSER-GRADE relation allowlist via `EXPLAIN (FORMAT JSON)` (enumerates EVERY planned
--   relation — comma-joins/quoted idents/subqueries/CTEs) · L3 read-only sandbox execution (SQLSTATE 25006).
--
-- M19 (ROADMAP-v2): `ai.nl_to_sql` (the LAST plpython3u) AND `ai.nl_query` are now implemented in Rust by the
-- `theodb_rs` extension (theodb_rs/src/nl.rs) — NOT here. With this, the `theodb` surface is plpython3u-free
-- and `plpython3u` leaves `theodb.control` requires. L4 still delegates relation enumeration to PostgreSQL's
-- planner (EXPLAIN) — a Rust SQL parser would diverge from the planner and reopen the comma-join/quoted-ident
-- vulnerability (the original review BLOCKER). The M12 config layer (sql/61) calls `ai.nl_query` by name.
-- Idempotent: safe to re-run / load from docker-entrypoint-initdb.d.

-- dep (vector) declared in theodb.control `requires` (M15) — not created here
CREATE SCHEMA IF NOT EXISTS ai;
