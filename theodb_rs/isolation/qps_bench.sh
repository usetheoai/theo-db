#!/usr/bin/env bash
# M147 — micro-benchmark de QPS do scan IVF, para provar que o refactor não regride latência > 5% (DoD).
# Roda as mesmas queries do A/B REP vezes contra o índice v5 (o caminho storage-separado mais comum) e reporta
# o tempo total (mean de RUNS execuções). Rodar no binário baseline e no novo; comparar o mean.
#
# Uso: qps_bench.sh <rótulo>   — imprime "QPS_BENCH <rótulo> mean_ms=<n> runs=<r>".
set -uo pipefail
LABEL="${1:?falta o rótulo (baseline|novo)}"
PGINST="${PGINST:-$HOME/.pgrx/18.4/pgrx-install}"
export PATH="$PGINST/bin:$PATH"
DATA=$(mktemp -d /tmp/qps.XXXXXX); PORT="${PORT:-$(( 40000 + RANDOM % 20000 ))}"
REP="${REP:-200}"   # queries por run
RUNS="${RUNS:-5}"   # runs para o mean±ruído

cleanup() { pg_ctl -D "$DATA" -m immediate stop -w >/dev/null 2>&1 || true; rm -rf "$DATA"; }
trap cleanup EXIT
initdb -D "$DATA" -U theo >/dev/null 2>&1 || { echo "QPS_FAIL initdb"; exit 2; }
{ echo "port=$PORT"; echo "shared_preload_libraries='theodb_rs'"; echo "autovacuum=off"; } >> "$DATA/postgresql.conf"
pg_ctl -D "$DATA" -l "$DATA/log" start -w >/dev/null || { echo "QPS_FAIL start"; exit 2; }
q() { psql -X -q -p "$PORT" -U theo -d postgres -tAc "$1" 2>&1; }

q "CREATE EXTENSION theodb_rs CASCADE;" >/dev/null
# mesmo dataset determinístico do A/B
q "CREATE TABLE t (id int, e vector(8));" >/dev/null
q "INSERT INTO t SELECT g, ('[' || string_agg((((g*2654435761 + d*2246822519 + 1013904223) % 10007) % 1000)::numeric::text, ',' ORDER BY d) || ']')::vector(8)
   FROM generate_series(1,2000) g, LATERAL generate_series(1,8) d GROUP BY g;" >/dev/null
q "CREATE INDEX t_idx ON t USING theodb_ivfflat (e) WITH (lists=16, pq_subspaces=4, aq_threshold=1500, separate_storage=1);" >/dev/null

# um arquivo SQL com REP queries (5 vetores fixos, repetidos), sob enable_seqscan=off.
SQLF="$DATA/bench.sql"
{
  echo "SET enable_seqscan=off; SET enable_indexscan=on;"
  for ((i=0; i<REP; i++)); do
    case $((i % 5)) in
      0) v="[10,20,30,40,50,60,70,80]";;
      1) v="[5,5,5,5,5,5,5,5]";;
      2) v="[99,1,99,1,99,1,99,1]";;
      3) v="[50,50,50,50,50,50,50,50]";;
      4) v="[1,2,3,4,5,6,7,8]";;
    esac
    echo "SELECT id FROM t ORDER BY e <-> '$v'::vector LIMIT 10;"
  done
} > "$SQLF"

# warm-up (1 run descartado) + RUNS medidos; mean em ms.
psql -X -q -p "$PORT" -U theo -d postgres -f "$SQLF" >/dev/null 2>&1
TOTAL=0
for ((r=0; r<RUNS; r++)); do
  T0=$(date +%s%N)
  psql -X -q -p "$PORT" -U theo -d postgres -f "$SQLF" >/dev/null 2>&1
  T1=$(date +%s%N)
  MS=$(( (T1 - T0) / 1000000 ))
  TOTAL=$(( TOTAL + MS ))
done
MEAN=$(( TOTAL / RUNS ))
echo "QPS_BENCH $LABEL mean_ms=$MEAN runs=$RUNS reps=$REP (menor=melhor)"
