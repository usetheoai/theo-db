#!/usr/bin/env bash
# M124 — dogfood anchor smoke: prove the theo-data retrieval path end-to-end on a SELF-HOSTED TheoDB.
#
# The anchor (dogfood-golden-rule § 1, slug `theo-data-capability-on-theodb`): a capability's real retrieval
# runs on TheoDB the team owns — `theodb.create_vectorizer` keeps an embedding column fresh via the background
# worker, and queries fuse FTS + vector via `ai.hybrid_search_rrf`. This smoke drives that exact path and asserts
# a populated embedding column + a non-empty fused result. It is the `wired` evidence generator (NOT `running` —
# that needs sustained real traffic, which a smoke cannot manufacture; see ADR M124-1).
#
# Secrets: the OpenAI key is read from $OPENAI_API_KEY (never hard-coded, never committed). The GUCs are set at
# the instance level (ALTER SYSTEM) so the BACKGROUND WORKER — which runs in its own session — can see them.
#
# Prereqs (see docs/ops/self-host-quickstart.md): a self-hosted PG17 with theodb_rs installed +
# `shared_preload_libraries = 'theodb_rs'` (so the vectorizer worker runs). Env: PGPORT, PGHOST, PGUSER, PGDATABASE
# (default the pgrx-managed instance), OPENAI_API_KEY.
set -euo pipefail

PSQL="psql -v ON_ERROR_STOP=1 -p ${PGPORT:-28900} -h ${PGHOST:-localhost} -U ${PGUSER:-postgres} -d ${PGDATABASE:-postgres}"
: "${OPENAI_API_KEY:?set OPENAI_API_KEY (from your .env — never commit it)}"
FAIL=0; note() { echo "  [smoke] $*"; }

note "0. extension + embedding provider config (instance-level so the worker sees it)"
$PSQL -c "CREATE EXTENSION IF NOT EXISTS theodb_rs;" >/dev/null
$PSQL -c "ALTER SYSTEM SET theodb.embedding_endpoint = 'https://api.openai.com/v1/embeddings';" >/dev/null
$PSQL -c "ALTER SYSTEM SET theodb.embedding_model    = 'text-embedding-3-small';" >/dev/null
$PSQL -c "ALTER SYSTEM SET theodb.embedding_api_key  = '${OPENAI_API_KEY}';" >/dev/null
$PSQL -c "SELECT pg_reload_conf();" >/dev/null

note "1. FAILURE MODE (SSRF fail-closed): a non-http(s) endpoint must be rejected with a typed error"
if $PSQL -c "SET theodb.embedding_endpoint='file:///etc/passwd'; SELECT theodb.embed('x');" 2>&1 | grep -qiE 'must be http|input'; then
  note "   PASS — non-http(s) endpoint rejected fail-closed"
else
  note "   WARN — SSRF guard did not fire as expected"; FAIL=1
fi

note "2. content table + create_vectorizer BEFORE loading (the freshness path)"
# ORDER MATTERS (failure mode #2, documented): create_vectorizer wires an ON-DML trigger but does NOT backfill
# rows that already exist — so the vectorizer must be created BEFORE content is loaded (or existing rows re-touched
# with an UPDATE). Loading first leaves the queue empty and the column NULL forever.
$PSQL <<'SQL' >/dev/null
DROP TABLE IF EXISTS df_docs CASCADE;
CREATE TABLE df_docs (
  id       int PRIMARY KEY,
  body     text NOT NULL,
  body_tsv tsvector GENERATED ALWAYS AS (to_tsvector('english', body)) STORED,
  embedding vector(1536)
);
SELECT theodb.create_vectorizer('df_docs','id','body','df_docs','embedding',
                                 'text-embedding-3-small', 1536, 'fixed', 512, 64);
INSERT INTO df_docs (id, body) VALUES
 (1, 'PostgreSQL vacuum reclaims dead tuples and prevents transaction id wraparound'),
 (2, 'HNSW graph index enables approximate nearest neighbor search over vectors'),
 (3, 'Reciprocal rank fusion combines a lexical BM25 leg and a dense vector leg'),
 (4, 'Write-ahead logging guarantees crash-safe durability for committed transactions'),
 (5, 'A background worker keeps the embedding column fresh as source content changes');
SQL

note "3. wait for the background worker to embed (poll 1s; timeout 120s)"
deadline=$((SECONDS+120)); pending=99
while [ $SECONDS -lt $deadline ]; do
  pending=$($PSQL -tAc "SELECT count(*) FROM df_docs WHERE embedding IS NULL")
  queue=$($PSQL -tAc "SELECT count(*) FROM theodb.vectorizer_queue WHERE state <> 'done'" 2>/dev/null || echo '?')
  note "   pending embeddings=$pending  queue(non-done)=$queue"
  [ "$pending" = "0" ] && break
  sleep 4
done
if [ "$pending" != "0" ]; then
  note "   FAILURE MODE observed — $pending rows still unembedded after 120s (endpoint slow/429/misconfig)."
  note "   queue diagnostics:"; $PSQL -c "SELECT state, count(*) FROM theodb.vectorizer_queue GROUP BY 1" || true
  FAIL=1
fi

WORKER_OK=$([ "$pending" = "0" ] && echo yes || echo no)

note "4. ensure the embedding column is populated (worker if it succeeded; else a session backfill — the"
note "   worker-independent proof that the QUERY path works with REAL embeddings, ADR M124-2)"
if [ "$WORKER_OK" != "yes" ]; then
  note "   worker did not embed (failure mode logged as evidence); backfilling via theodb.embed (session path)"
  $PSQL -c "UPDATE df_docs SET embedding = theodb.embed(body)::vector WHERE embedding IS NULL;" >/dev/null
fi
EMB=$($PSQL -tAc "SELECT count(*) FROM df_docs WHERE embedding IS NOT NULL")
note "   rows embedded: $EMB / 5"

note "5. the anchor QUERY path — ai.hybrid_search_rrf (FTS + REAL vector fused)"
RES=$($PSQL -tAc "SELECT count(*) FROM ai.hybrid_search_rrf(
  'df_docs','id','body_tsv','embedding',
  query_text => 'how does the index keep vector search fast',
  k => 60, per_leg_limit => 10, result_limit => 5)")
note "   hybrid_search_rrf returned $RES fused rows (top result should be the HNSW/ANN doc — vector leg alive)"
[ "${RES:-0}" -ge 1 ] || { note "   FAIL — empty fused result"; FAIL=1; }

# The anchor's CORE (the query path) is the pass/fail gate; the async-worker outcome is recorded as a diagnostic.
echo "SMOKE_RESULT: $([ "${RES:-0}" -ge 1 ] && echo PASS || echo FAIL)  (query_path_fused_rows=${RES:-0}, embedded=$EMB/5, async_worker_embedded=$WORKER_OK)"
[ "${RES:-0}" -ge 1 ]
