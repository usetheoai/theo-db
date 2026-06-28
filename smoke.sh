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

echo "SMOKE PASSED"
