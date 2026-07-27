#!/usr/bin/env bash
set -euo pipefail

HOST="${PGHOST:-localhost}"
PORT="${PGPORT:-5432}"
USER="${PGUSER:-postgres}"
PGPASSWORD="${PGPASSWORD:-postgres}"
export PGPASSWORD

# --- PostgreSQL client resolution -------------------------------------------------------------
# This smoke deliberately connects from OUTSIDE the database container, over the PUBLISHED port —
# that is the path a real user takes, and precisely what a `docker exec` test would never exercise.
# So it needs psql/pg_isready on the caller, and the CI runner does not ship them.
#
# That gap stayed invisible for as long as these jobs died earlier, on the fixed-port collision:
# the script was never reached. Fixing the port surfaced it as `pg_isready: command not found`
# (exit 127) across all four jobs that call this script.
#
# Installing a host package is not an option (no root on the runner), and switching to `docker exec`
# would silently change WHAT is asserted. Instead, fall back to a containerized client sharing the
# host network namespace: `localhost:$PORT` there resolves to the very same published port, so every
# assertion below keeps its exact meaning. Costs ~1s per invocation, and only when the host lacks
# the client — a developer with psql installed pays nothing.
if ! command -v pg_isready >/dev/null 2>&1 || ! command -v psql >/dev/null 2>&1; then
  SMOKE_CLIENT_IMAGE="${SMOKE_CLIENT_IMAGE:-postgres:18-bookworm}"
  command -v docker >/dev/null 2>&1 || {
    echo "ERROR: this smoke needs either the PostgreSQL client (psql/pg_isready) or docker to run a" >&2
    echo "       containerized one; neither is available. Install postgresql-client and retry." >&2
    exit 2
  }
  echo "note: no host PostgreSQL client — using $SMOKE_CLIENT_IMAGE over the host network namespace"
  # `-i` keeps stdin open for the heredocs below; `-e PGPASSWORD` forwards the exported value.
  pg_isready() { docker run --rm --network host -e PGPASSWORD "$SMOKE_CLIENT_IMAGE" pg_isready "$@"; }
  psql()       { docker run --rm -i --network host -e PGPASSWORD "$SMOKE_CLIENT_IMAGE" psql "$@"; }
fi

for i in $(seq 1 10); do
  pg_isready -h "$HOST" -p "$PORT" -U "$USER" -q && break
  sleep 1
done
pg_isready -h "$HOST" -p "$PORT" -U "$USER" -q

# M15: TheoDB ships as an installable extension — assert it is present (every surface below comes from it).
THEODB_VER=$(psql -h "$HOST" -p "$PORT" -U "$USER" -t -A -q -v ON_ERROR_STOP=1 <<'SQL'
CREATE EXTENSION IF NOT EXISTS theodb CASCADE;
SELECT extversion FROM pg_extension WHERE extname='theodb';
SQL
)
if [ -z "$THEODB_VER" ]; then
  echo "THEODB SMOKE FAILED: extension 'theodb' not installed (expected CREATE EXTENSION theodb to work)" >&2
  exit 1
fi
echo "theodb: extension installed (v$THEODB_VER) OK"

psql -h "$HOST" -p "$PORT" -U "$USER" -v ON_ERROR_STOP=1 <<'SQL'
CREATE EXTENSION IF NOT EXISTS vector;
SELECT '[1,2,3]'::vector <=> '[4,5,6]'::vector;
SQL

# M7-S1: hybrid search (FTS + vector + RRF) golden assertion — the doc matched by BOTH legs ranks first.
TOP=$(psql -h "$HOST" -p "$PORT" -U "$USER" -t -A -q -v ON_ERROR_STOP=1 <<'SQL'
DROP TABLE IF EXISTS smoke_docs;
CREATE TABLE smoke_docs (
  doc_id text PRIMARY KEY,
  content text,
  text_tsv tsvector GENERATED ALWAYS AS (to_tsvector('english', coalesce(content,''))) STORED,
  embedding vector(3)
);
CREATE INDEX smoke_docs_gin ON smoke_docs USING gin (text_tsv);
INSERT INTO smoke_docs(doc_id, content, embedding) VALUES
  ('d1','database systems','[1,0,0]'),   -- FTS 'database' AND near query vector  -> both legs
  ('d2','database tuning','[0,1,0]'),     -- FTS 'database' only
  ('d3','cooking recipes','[1,0,0]');     -- vector only
SELECT id FROM ai.hybrid_search_rrf(
  tbl => 'smoke_docs'::regclass, id_col => 'doc_id', content_tsv_col => 'text_tsv',
  vector_col => 'embedding', query_text => 'database', query_vector => '[1,0,0]'::vector,
  result_limit => 1);
SQL
)
if [ "$TOP" != "d1" ]; then
  echo "HYBRID SMOKE FAILED: expected top result 'd1' (both legs), got '$TOP'" >&2
  exit 1
fi
echo "hybrid: ai.hybrid_search_rrf top result = $TOP (both-legs doc) OK"

# M7-S3: generative ai.* functions present + locked down (NO network — presence + privilege only).
AI_CHECK=$(psql -h "$HOST" -p "$PORT" -U "$USER" -t -A -q -v ON_ERROR_STOP=1 <<'SQL'
SELECT
  (SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
     WHERE n.nspname='ai' AND p.proname IN ('generate','if','analyze_sentiment','summarize','rank'))::text
  || ':' ||
  -- none of the outbound-HTTP functions may be executable by PUBLIC (least-privilege)
  (SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
     WHERE n.nspname='ai' AND p.proname IN ('generate','if','analyze_sentiment','summarize','rank','_chat')
       AND has_function_privilege('public', p.oid, 'execute'))::text;
SQL
)
if [ "$AI_CHECK" != "5:0" ]; then
  echo "AI SMOKE FAILED: expected '5:0' (5 functions present, 0 PUBLIC-executable), got '$AI_CHECK'" >&2
  exit 1
fi
echo "ai: 5 generative ai.* functions present, 0 executable by PUBLIC OK"

# M10: ai.agg_summarize aggregate present + locked down (NO network — presence + privilege only).
AGG_CHECK=$(psql -h "$HOST" -p "$PORT" -U "$USER" -t -A -q -v ON_ERROR_STOP=1 <<'SQL'
SELECT
  -- aggregate present (prokind='a') AND its finalfunc is VOLATILE (the real "LLM call re-runs" guarantee;
  -- the aggregate itself is provolatile='i' like every PG aggregate, so we assert the finalfunc instead)
  (SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
     WHERE n.nspname='ai' AND p.proname='agg_summarize' AND p.prokind='a')::text
  || ':' ||
  (SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
     WHERE n.nspname='ai' AND p.proname='_agg_summ_final' AND p.provolatile='v')::text
  || ':' ||
  (SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
     WHERE n.nspname='ai' AND p.proname IN ('agg_summarize','_agg_summ_accum','_agg_summ_final')
       AND has_function_privilege('public', p.oid, 'execute'))::text;
SQL
)
if [ "$AGG_CHECK" != "1:1:0" ]; then
  echo "AGG SMOKE FAILED: expected '1:1:0' (aggregate present, finalfunc VOLATILE, 0 PUBLIC-executable), got '$AGG_CHECK'" >&2
  exit 1
fi
echo "ai: ai.agg_summarize aggregate present (finalfunc VOLATILE), 0 executable by PUBLIC OK"

# M11: ai.generate_batch present + locked down (NO network — presence + privilege only).
BATCH_CHECK=$(psql -h "$HOST" -p "$PORT" -U "$USER" -t -A -q -v ON_ERROR_STOP=1 <<'SQL'
SELECT
  (SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
     WHERE n.nspname='ai' AND p.proname='generate_batch')::text
  || ':' ||
  (SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
     WHERE n.nspname='ai' AND p.proname='generate_batch'
       AND has_function_privilege('public', p.oid, 'execute'))::text;
SQL
)
if [ "$BATCH_CHECK" != "1:0" ]; then
  echo "BATCH SMOKE FAILED: expected '1:0' (ai.generate_batch present, 0 PUBLIC-executable), got '$BATCH_CHECK'" >&2
  exit 1
fi
echo "ai: ai.generate_batch present, 0 executable by PUBLIC OK"

# M7-S4: safe NL→SQL functions present + locked down (NO network — presence + privilege only).
NL_CHECK=$(psql -h "$HOST" -p "$PORT" -U "$USER" -t -A -q -v ON_ERROR_STOP=1 <<'SQL'
SELECT
  (SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
     WHERE n.nspname='ai' AND p.proname IN ('nl_to_sql','nl_query'))::text
  || ':' ||
  (SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
     WHERE n.nspname='ai' AND p.proname IN ('nl_to_sql','nl_query')
       AND has_function_privilege('public', p.oid, 'execute'))::text;
SQL
)
if [ "$NL_CHECK" != "2:0" ]; then
  echo "NL SMOKE FAILED: expected '2:0' (2 NL→SQL functions present, 0 PUBLIC-executable), got '$NL_CHECK'" >&2
  exit 1
fi
echo "ai: 2 safe NL→SQL functions (nl_to_sql/nl_query) present, 0 executable by PUBLIC OK"

# M12: theodb_ai_nl config surface present + locked down (NO network — presence + privilege only).
NLCFG_CHECK=$(psql -h "$HOST" -p "$PORT" -U "$USER" -t -A -q -v ON_ERROR_STOP=1 <<'SQL'
SELECT
  (SELECT count(*) FROM pg_tables WHERE schemaname='ai'
     AND tablename IN ('nl_config','nl_templates','nl_value_index'))::text
  || ':' ||
  (SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
     WHERE n.nspname='ai' AND p.proname IN ('nl_query_cfg','nl_add_config','nl_add_template',
       'nl_set_template_enabled','nl_set_value_index','nl_refresh_value_index'))::text
  || ':' ||
  (SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
     WHERE n.nspname='ai' AND p.proname IN ('nl_query_cfg','nl_add_config','nl_add_template',
       'nl_set_template_enabled','nl_set_value_index','nl_refresh_value_index')
       AND has_function_privilege('public', p.oid, 'execute'))::text;
SQL
)
if [ "$NLCFG_CHECK" != "3:6:0" ]; then
  echo "NL-CFG SMOKE FAILED: expected '3:6:0' (3 config tables, 6 fns present, 0 PUBLIC-executable), got '$NLCFG_CHECK'" >&2
  exit 1
fi
echo "ai: theodb_ai_nl config surface (3 tables + 6 fns incl nl_query_cfg) present, 0 executable by PUBLIC OK"

# M13: packaged surface — ai.hybrid_search(jsonb) + theodb_ml registry (presence/privilege + NO api_key column).
PKG_CHECK=$(psql -h "$HOST" -p "$PORT" -U "$USER" -t -A -q -v ON_ERROR_STOP=1 <<'SQL'
SELECT
  (SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
     WHERE (n.nspname='ai' AND p.proname='hybrid_search')
        OR (n.nspname='theodb_ml' AND p.proname IN ('create_model','drop_model','list_models','apply_model')))::text
  || ':' ||
  (SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
     WHERE ((n.nspname='ai' AND p.proname='hybrid_search')
         OR (n.nspname='theodb_ml' AND p.proname IN ('create_model','drop_model','list_models','apply_model')))
       AND has_function_privilege('public', p.oid, 'execute'))::text
  || ':' ||
  (SELECT count(*) FROM information_schema.columns
     WHERE table_schema='theodb_ml' AND table_name='models' AND column_name ILIKE '%key%')::text;
SQL
)
if [ "$PKG_CHECK" != "5:0:0" ]; then
  echo "PKG SMOKE FAILED: expected '5:0:0' (5 fns present, 0 PUBLIC-executable, 0 api_key columns), got '$PKG_CHECK'" >&2
  exit 1
fi
echo "ai: packaged surface (ai.hybrid_search + theodb_ml registry) present, 0 PUBLIC, 0 persisted keys OK"

echo "SMOKE PASSED"
