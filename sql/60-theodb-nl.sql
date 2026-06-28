-- TheoDB M7-S4 — Safe natural-language -> SQL with anti-prompt-injection guardrails.
-- The hard problem is NOT generating SQL (one ai._chat call) — it is making execution SAFE when the user
-- question is adversarial ("ignore instructions; DROP TABLE users"). The defense does NOT trust the LLM —
-- it is layered (blueprint m7-nl-to-sql-safe). Two deterministic guards are load-bearing:
--   L2  static validation (single statement, SELECT/WITH-only, banned-function denylist)        — generate-time
--   L4  PARSER-GRADE relation allowlist via `EXPLAIN (FORMAT JSON)` — enumerates EVERY relation  — generate-time
--       the planner resolves (comma-joins, quoted idents, subqueries, CTE base rels); regex lexing is NOT
--       used for the allowlist (it missed comma-joins / quoted identifiers — review BLOCKER).
--   L3  read-only sandbox execution (transaction_read_only + statement_timeout -> SQLSTATE 25006 on writes) — execute-time
-- Plus L1 prompt constraint (hardening). Generate-vs-execute split: ai.nl_to_sql returns validated SQL;
-- ai.nl_query executes it in the read-only sandbox. EXPLAIN (no ANALYZE) PLANS but does not EXECUTE the
-- query, so enumerating relations is side-effect-free (no data read, no function call).
-- Deployment hardening (recommended): also run ai.nl_query under a least-privilege read-only role (the
-- read-only txn does not block role-gated read funcs like pg_read_file/pg_stat_file — the L2 denylist + the
-- EXPLAIN allowlist cover those, and a restricted role is the belt-and-suspenders backstop).
-- Idempotent: safe to re-run / load from docker-entrypoint-initdb.d.

CREATE EXTENSION IF NOT EXISTS plpython3u;
CREATE SCHEMA IF NOT EXISTS ai;

-- ai.nl_to_sql — generate (via ai._chat) + statically validate to ONE read-only SELECT over the allowlist.
-- L2 (denylist) + L4 (EXPLAIN-based relation allowlist). Returns the validated SQL, or raises 22023 on any
-- violation (never returns unsafe SQL). Does NOT execute the query.
CREATE OR REPLACE FUNCTION ai.nl_to_sql(question text, allowed_relations text[], model text DEFAULT NULL)
RETURNS text
LANGUAGE plpython3u
AS $$
import re, json

if question is None or not question.strip():
    plpy.error("ai.nl_to_sql: question must not be empty", sqlstate="22023")
if not allowed_relations:
    plpy.error("ai.nl_to_sql: allowed_relations must be a non-empty list", sqlstate="22023")

# Allowlist normalization: accept either bare ('documents') or schema-qualified ('public.documents').
allowed = set()
for r in allowed_relations:
    if r and r.strip():
        allowed.add(r.strip().lower())
if not allowed:
    plpy.error("ai.nl_to_sql: allowed_relations must be a non-empty list", sqlstate="22023")

# L1 — constrain the model. (Hardening; the deterministic guards below do not trust this.)
system = (
    "You translate a question into exactly ONE read-only PostgreSQL SELECT query. "
    "You may reference ONLY these relations: " + ", ".join(sorted(allowed)) + ". "
    "Output ONLY the SQL — no prose, no markdown, no trailing semicolon. "
    "Use SELECT or WITH only. Never modify data."
)
raw = plpy.execute(
    plpy.prepare("SELECT ai._chat($1, $2, $3) AS v", ["text", "text", "text"]),
    [question, system, model],
)[0]["v"] or ""

sql = raw.strip()
if sql.startswith("```"):
    sql = re.sub(r"^```[a-zA-Z]*\n?", "", sql)
    sql = re.sub(r"\n?```$", "", sql).strip()

# Comment-stripped, lowercased copy for the L2 textual checks.
nocom = re.sub(r"--[^\n]*", " ", sql)
nocom = re.sub(r"/\*.*?\*/", " ", nocom, flags=re.S)
low = nocom.lower().strip()

# L2(a) — single statement (no ';' except an optional trailing one).
if ";" in low.rstrip().rstrip(";"):
    plpy.error("ai.nl_to_sql: multiple statements are not allowed", sqlstate="22023")
# L2(b) — read query only.
if not re.match(r"^\s*(select|with)\b", low):
    plpy.error("ai.nl_to_sql: only SELECT/WITH queries are allowed (got: %s)" % sql[:60], sqlstate="22023")
# L2(c) — banned tokens / dangerous functions. read-only (L3) blocks writes but NOT role-gated read funcs;
# the EXPLAIN allowlist (L4) catches relation reads but NOT no-FROM functions — so deny the file/exfil family
# here (incl. the pg_ls_* / pg_stat_file siblings the first cut missed).
BANNED = re.compile(
    r"\b(drop|insert|update|delete|alter|truncate|grant|revoke|create|copy|merge|reindex|vacuum|"
    r"pg_read_file|pg_read_binary_file|pg_stat_file|pg_ls_dir|pg_ls_waldir|pg_ls_logdir|pg_ls_tmpdir|"
    r"pg_ls_archive_statusdir|pg_ls_|lo_import|lo_export|lo_get|lo_put|dblink|pg_sleep|set_config|"
    r"current_setting|pg_terminate_backend|pg_cancel_backend|pg_read_server_files)\b", re.I)
m = BANNED.search(low)
if m:
    plpy.error("ai.nl_to_sql: banned token '%s' in generated SQL" % m.group(1), sqlstate="22023")
if re.search(r"\bdo\b\s*\$\$|\bcall\b", low):
    plpy.error("ai.nl_to_sql: procedural blocks are not allowed", sqlstate="22023")

# L4 — PARSER-GRADE relation allowlist. Ask PostgreSQL's planner to enumerate every relation the query
# touches (comma-joins, quoted identifiers, subqueries, CTE base relations all included), via EXPLAIN
# (FORMAT JSON) which PLANS but does NOT execute. Then assert each planned base relation is allowlisted.
try:
    plan_rows = plpy.execute("EXPLAIN (FORMAT JSON, VERBOSE false) " + sql)
except Exception as e:  # noqa: BLE001 — any planner error (unknown relation/syntax) => reject, fail-closed
    plpy.error("ai.nl_to_sql: query did not plan (rejected): %s" % str(e)[:120], sqlstate="22023")

plan_json = plan_rows[0][list(plan_rows[0].keys())[0]]
if isinstance(plan_json, str):
    plan_json = json.loads(plan_json)

def _collect_relations(node, acc):
    if isinstance(node, dict):
        name = node.get("Relation Name")
        if name:
            schema = (node.get("Schema") or "public").lower()
            acc.add((schema, name.lower()))
        for v in node.values():
            _collect_relations(v, acc)
    elif isinstance(node, list):
        for v in node:
            _collect_relations(v, acc)

rels = set()
_collect_relations(plan_json, rels)
for schema, name in rels:
    qualified = schema + "." + name
    # accept schema-qualified match, OR a bare allowlist entry only when the relation is in public
    if qualified not in allowed and not (name in allowed and schema == "public"):
        plpy.error("ai.nl_to_sql: relation '%s' is not in the allowlist" % qualified, sqlstate="22023")

return sql
$$;

-- ai.nl_query — validate (via ai.nl_to_sql) then EXECUTE in the read-only sandbox (L3). Returns jsonb rows.
-- Any write that reaches execution raises SQLSTATE 25006 (read_only_sql_transaction); the DB is never mutated.
-- The transaction_read_only / statement_timeout GUCs are SET LOCAL then RESTORED, so the function never
-- leaks read-only state into the caller's surrounding transaction (review MEDIUM).
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
    -- runaway query is aborted by statement_timeout (57014). Deterministic, independent of the LLM.
    -- CONTRACT (fail-safe by design): this marks the CURRENT transaction read-only and does NOT (cannot)
    -- restore it — PostgreSQL forbids returning to read-write after a query has run (SQLSTATE 25001). In the
    -- normal single-statement usage (`SELECT ai.nl_query(...)`) the transaction ends immediately, so there is
    -- no leak. If you call it inside an explicit multi-statement transaction, that transaction stays read-only
    -- (a subsequent write raises 25006 — restrictive, never permissive). Call it in its own transaction, or
    -- last. statement_timeout is likewise SET LOCAL for the duration.
    PERFORM set_config('transaction_read_only', 'on', true);
    PERFORM set_config('statement_timeout', '5000', true);

    -- Outer LIMIT on the wrapper (not injected inside the subquery) so a generated SELECT that already ends
    -- in LIMIT does not produce a `LIMIT n LIMIT m` syntax error (review LOW).
    EXECUTE format(
        'SELECT coalesce(jsonb_agg(row_to_json(t)), ''[]''::jsonb) FROM (%s) t LIMIT %s',
        validated, max_rows
    ) INTO result;

    RETURN result;
END;
$$;

-- Least-privilege. Both functions make an outbound LLM call (via ai._chat) and ai.nl_query executes dynamic
-- SQL — NOT granted to PUBLIC. NOTE (SECURITY INVOKER, like ai._chat): a caller needs EXECUTE on
-- ai.nl_query, ai.nl_to_sql AND ai._chat together — grant all three; never grant ai._chat to PUBLIC to
-- "fix" a permission error (that re-opens outbound HTTP to every role). Recommended deployment hardening:
-- run ai.nl_query under a dedicated least-privilege read-only role with SELECT only on the safe views.
REVOKE ALL ON FUNCTION ai.nl_to_sql(text, text[], text) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai.nl_query(text, text[], text, int) FROM PUBLIC;

COMMENT ON FUNCTION ai.nl_to_sql(text, text[], text) IS
  'Translate a natural-language question into ONE validated read-only SELECT over allowed_relations, via the '
  'configurable model (ai._chat). Defense: L2 static validation (single statement, SELECT/WITH-only, '
  'banned-function denylist) + L4 parser-grade relation allowlist (EXPLAIN enumerates every planned relation '
  '— comma-joins/quoted idents/subqueries/CTEs included). Fail-fast 22023. Does NOT execute. Not granted to PUBLIC.';
COMMENT ON FUNCTION ai.nl_query(text, text[], text, int) IS
  'Generate+validate (ai.nl_to_sql) then execute the SELECT in a PostgreSQL-native read-only sandbox '
  '(transaction_read_only + statement_timeout -> SQLSTATE 25006 on any write; GUCs restored after). Returns '
  'jsonb rows. The database is never mutated by an adversarial question (defense in depth). Not granted to PUBLIC.';
