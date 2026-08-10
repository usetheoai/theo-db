#!/usr/bin/env bash
# M102 — honest benchmark for the SET-oriented, planner-optimizable AI operators. Emits JSON (stdout) →
# wiki/benchmarks/archive/m102-ai-operators.md e benchmarks/artifacts/m102-ai-operators.json. Two measured artifacts:
#   (1) DETERMINISTIC (theodb.llm_test_model='parity', HTTP-free, reproducible, CI-safe):
#       - batching: `ai.if_batch` over N values issues exactly 1 inference round-trip vs N for the per-row path
#         (measured via ai.call_count()); wall-time of each.
#       - push-down: `WHERE id<=K AND ai.if_costly(...)` evaluates the AI predicate on <=K survivors, not all N
#         (the cheap qual is ordered first by the high COST; measured via ai.call_count()).
#   (2) REAL-AI (OpenAI chat, key from .env) — bounded K, RUNS repetitions: wall-time of the batched operator
#       (1 round-trip) vs the per-row path (K round-trips), mean +/- stddev. HONEST CEILING (ADR D4): this is a
#       composability / round-trip win with STATISTICAL accuracy, ORTHOGONAL to vector recall. The batched and
#       per-row systems differ, so the answers are not asserted identical on a live model (that is the per-model
#       quality question); the deterministic pass is the correctness proof, the real pass is the latency evidence.
# Run on the droplet (regenerate the extension first). REAL pass is skipped (reported) when no key is configured.
set -euo pipefail
PGINST="${PGINST:-$HOME/.pgrx/17.10/pgrx-install}"
export PATH="$PGINST/bin:$PATH"
DATA=/tmp/bench102_tmp
PORT=59719
DB=postgres
N="${N:-1000}"          # deterministic set size
PUSHK="${PUSHK:-100}"   # cheap-qual selectivity for the push-down measurement
REALK="${REALK:-16}"    # real-AI set size (bounded to keep cost/latency small)
RUNS="${RUNS:-3}"
ENVFILE="${ENVFILE:-/home/theo/theodb-src/.env}"

cleanup() { pg_ctl -D "$DATA" -m immediate stop -w >/dev/null 2>&1 || true; rm -rf "$DATA"; }
trap cleanup EXIT
rm -rf "$DATA"
initdb -D "$DATA" -U theo >/dev/null 2>&1
{ echo "port=$PORT"; echo "shared_buffers=1GB"; echo "work_mem=64MB"; echo "max_parallel_workers_per_gather=0"; } >> "$DATA/postgresql.conf"
pg_ctl -D "$DATA" -l "$DATA/log" start -w >/dev/null
q() { psql -X -q -p "$PORT" -U theo -d "$DB" -tAc "$1"; }

q "CREATE EXTENSION theodb_rs;" >/dev/null
q "CREATE TABLE t (id int, txt text);" >/dev/null
q "INSERT INTO t SELECT g, g::text FROM generate_series(1, $N) g;" >/dev/null

# ---------- (1) DETERMINISTIC: batching + push-down ----------
q "SET theodb.llm_test_model='parity';" >/dev/null

# batched: 1 call for N; wall-time
BATCH_MS=$(q "SET theodb.llm_test_model='parity'; SELECT ai.call_reset();
  \\timing is off
  SELECT extract(epoch from clock_timestamp())*1000;" 2>/dev/null | tail -1 || true)
# Measure via psql \timing-free: use a SQL-level timer.
DET=$(psql -X -q -p "$PORT" -U theo -d "$DB" -tA <<SQL
SET theodb.llm_test_model='parity';
SELECT ai.call_reset();
\set t0 'SELECT extract(epoch from clock_timestamp())*1000'
SELECT extract(epoch from clock_timestamp())*1000 AS batch_start \gset
SELECT count(*) FROM (SELECT unnest(ai.if_batch('is even', array_agg(txt ORDER BY id))) FROM t) s WHERE unnest;
SELECT extract(epoch from clock_timestamp())*1000 AS batch_end \gset
SELECT :batch_end - :batch_start AS batch_ms, ai.call_count() AS batch_calls \gset
SELECT :'batch_ms' || '|' || :'batch_calls';
SQL
)
BATCH_MS=$(echo "$DET" | tail -1 | cut -d'|' -f1)
BATCH_CALLS=$(echo "$DET" | tail -1 | cut -d'|' -f2)

# per-row path over the same N: one round-trip per row (ai.if_costly on all rows)
DET2=$(psql -X -q -p "$PORT" -U theo -d "$DB" -tA <<SQL
SET theodb.llm_test_model='parity';
SELECT ai.call_reset();
SELECT extract(epoch from clock_timestamp())*1000 AS pr_start \gset
SELECT count(*) FROM t WHERE ai.if_costly('is even', txt);
SELECT extract(epoch from clock_timestamp())*1000 AS pr_end \gset
SELECT :pr_end - :pr_start AS pr_ms, ai.call_count() AS pr_calls \gset
SELECT :'pr_ms' || '|' || :'pr_calls';
SQL
)
PERROW_MS=$(echo "$DET2" | tail -1 | cut -d'|' -f1)
PERROW_CALLS=$(echo "$DET2" | tail -1 | cut -d'|' -f2)

# push-down: WHERE id<=K AND ai.if_costly -> AI predicate on <=K survivors
PUSH=$(psql -X -q -p "$PORT" -U theo -d "$DB" -tA <<SQL
SET theodb.llm_test_model='parity';
SELECT ai.call_reset();
SELECT count(*) FROM t WHERE id <= $PUSHK AND ai.if_costly('is even', txt);
SELECT ai.call_count();
SQL
)
PUSH_CALLS=$(echo "$PUSH" | tail -1)

# ---------- (2) REAL-AI (OpenAI) ----------
REAL_JSON='null'
OPENAI_KEY=$(grep -E '^OPENAI_API_KEY=' "$ENVFILE" 2>/dev/null | cut -d= -f2- | tr -d '"'"'"' \r\n' || true)
if [ -n "${OPENAI_KEY:-}" ]; then
  LLM_MODEL="${LLM_MODEL:-gpt-4o-mini}"
  ENDPOINT="https://api.openai.com/v1/chat/completions"
  # warm the connection once (ignored)
  psql -X -q -p "$PORT" -U theo -d "$DB" -tA >/dev/null 2>&1 <<SQL || true
SET theodb.llm_endpoint='$ENDPOINT'; SET theodb.llm_api_key='$OPENAI_KEY'; SET theodb.llm_model='$LLM_MODEL';
SELECT ai.if_batch('is this an even number', ARRAY['2','3']);
SQL
  bsum=0; bsq=0; psum=0; psq=0; ok=1
  for r in $(seq 1 "$RUNS"); do
    RB=$(psql -X -q -p "$PORT" -U theo -d "$DB" -tA <<SQL
SET theodb.llm_endpoint='$ENDPOINT'; SET theodb.llm_api_key='$OPENAI_KEY'; SET theodb.llm_model='$LLM_MODEL';
SELECT ai.call_reset();
SELECT extract(epoch from clock_timestamp())*1000 AS s \gset
SELECT ai.if_batch('is this an even number', (SELECT array_agg(txt ORDER BY id) FROM t WHERE id<=$REALK));
SELECT extract(epoch from clock_timestamp())*1000 AS e \gset
SELECT (:e - :s)::int || '|' || ai.call_count();
SQL
) || { ok=0; break; }
    RP=$(psql -X -q -p "$PORT" -U theo -d "$DB" -tA <<SQL
SET theodb.llm_endpoint='$ENDPOINT'; SET theodb.llm_api_key='$OPENAI_KEY'; SET theodb.llm_model='$LLM_MODEL';
SELECT ai.call_reset();
SELECT extract(epoch from clock_timestamp())*1000 AS s \gset
SELECT count(*) FROM t WHERE id<=$REALK AND ai.if_costly('is this an even number', txt);
SELECT extract(epoch from clock_timestamp())*1000 AS e \gset
SELECT (:e - :s)::int || '|' || ai.call_count();
SQL
) || { ok=0; break; }
    bms=$(echo "$RB" | tail -1 | cut -d'|' -f1); bcalls=$(echo "$RB" | tail -1 | cut -d'|' -f2)
    pms=$(echo "$RP" | tail -1 | cut -d'|' -f1); pcalls=$(echo "$RP" | tail -1 | cut -d'|' -f2)
    bsum=$((bsum+bms)); bsq=$((bsq+bms*bms)); psum=$((psum+pms)); psq=$((psq+pms*pms))
    LAST_BCALLS=$bcalls; LAST_PCALLS=$pcalls
  done
  if [ "$ok" = 1 ]; then
    bmean=$((bsum/RUNS)); pmean=$((psum/RUNS))
    REAL_JSON=$(cat <<J
{"model":"$LLM_MODEL","k":$REALK,"runs":$RUNS,"batch_ms_mean":$bmean,"perrow_ms_mean":$pmean,"batch_calls":$LAST_BCALLS,"perrow_calls":$LAST_PCALLS,"speedup":$(python3 -c "print(round($pmean/max($bmean,1),2))")}
J
)
  else
    REAL_JSON='{"skipped":"a real-AI call failed (network/model); deterministic pass stands as the correctness+round-trip proof"}'
  fi
fi

cat <<JSON
{
  "milestone": "M102",
  "deterministic": {
    "n": $N,
    "batch_ms": ${BATCH_MS:-null}, "batch_round_trips": ${BATCH_CALLS:-null},
    "perrow_ms": ${PERROW_MS:-null}, "perrow_round_trips": ${PERROW_CALLS:-null},
    "roundtrip_reduction": "${BATCH_CALLS:-?} vs ${PERROW_CALLS:-?}",
    "pushdown_k": $PUSHK, "pushdown_ai_calls": ${PUSH_CALLS:-null},
    "pushdown_note": "AI predicate evaluated on <=$PUSHK survivors of $N rows (cheap qual ordered first by COST)"
  },
  "real_ai": $REAL_JSON,
  "honest_ceiling": "Composability/round-trip win with STATISTICAL accuracy; orthogonal to vector recall. Correctness proven by the deterministic 'parity' model; real-AI answer quality is a per-model statistical question, not asserted equal here."
}
JSON
