#!/usr/bin/env bash
set -euo pipefail

HOST="${PGHOST:-localhost}"
PORT="${PGPORT:-5432}"
USER="${PGUSER:-postgres}"
PGPASSWORD="${PGPASSWORD:-postgres}"
export PGPASSWORD

for i in $(seq 1 10); do
  pg_isready -h "$HOST" -p "$PORT" -U "$USER" -q && break
  sleep 1
done
pg_isready -h "$HOST" -p "$PORT" -U "$USER" -q

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

echo "SMOKE PASSED"
