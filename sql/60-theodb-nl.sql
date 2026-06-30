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

-- ai.nl_query — L3 read-only sandbox execution (M19 ADR-F: stays plpgsql — transaction-control + dynamic
-- EXECUTE are inherently SQL operations; the M18 precedent kept the chunked import PROCEDURE plpgsql). It
-- calls `ai.nl_to_sql` (now the Rust theodb_rs validator) at the SQL level — which is REQUIRED so nl_to_sql's
-- L4 EXPLAIN-over-SPI runs in a clean context (a nested Rust→Rust call breaks the planner call). The
-- anti-injection core (L1/L2/L4) is 100% Rust; this thin wrapper only adds the read-only execution.
-- A write reaching execution raises SQLSTATE 25006; the database is never mutated. Late-bound (plpgsql) so
-- it creates cleanly before theodb_rs installs ai.nl_to_sql.
CREATE OR REPLACE FUNCTION ai.nl_query(question text, allowed_relations text[],
                                       model text DEFAULT NULL, max_rows int DEFAULT 100)
RETURNS jsonb
LANGUAGE plpgsql
AS $$
DECLARE
    validated text;
    result jsonb;
BEGIN
    IF max_rows IS NULL OR max_rows <= 0 THEN
        RAISE EXCEPTION 'ai.nl_query: max_rows must be > 0 (got %)', max_rows USING ERRCODE = '22023';
    END IF;

    -- L1 + L2 + L4 (generate + static validation + EXPLAIN-based relation allowlist); 22023 on any violation.
    validated := ai.nl_to_sql(question, allowed_relations, model);

    -- L3 — PostgreSQL-native read-only sandbox (SET LOCAL, transaction-scoped). A write raises 25006; a
    -- runaway query is aborted by statement_timeout (57014). Fail-safe by design (not restored — PG forbids
    -- RW-after-query, 25001; the normal single-statement SELECT ai.nl_query(...) ends the txn immediately).
    PERFORM set_config('transaction_read_only', 'on', true);
    PERFORM set_config('statement_timeout', '5000', true);

    -- Outer LIMIT on the wrapper (not injected inside the subquery) so a generated `… LIMIT n` does not
    -- produce a `LIMIT n LIMIT m` syntax error.
    EXECUTE format(
        'SELECT coalesce(jsonb_agg(row_to_json(t)), ''[]''::jsonb) FROM (%s) t LIMIT %s',
        validated, max_rows
    ) INTO result;

    RETURN result;
END;
$$;

REVOKE ALL ON FUNCTION ai.nl_query(text, text[], text, int) FROM PUBLIC;

COMMENT ON FUNCTION ai.nl_query(text, text[], text, int) IS
  'Generate+validate (ai.nl_to_sql, Rust) then execute the SELECT in a PostgreSQL-native read-only sandbox '
  '(transaction_read_only + statement_timeout -> SQLSTATE 25006 on any write). Returns jsonb rows. The '
  'database is never mutated by an adversarial question (defense in depth). Not granted to PUBLIC.';
