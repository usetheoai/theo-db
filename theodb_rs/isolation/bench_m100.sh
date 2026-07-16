#!/usr/bin/env bash
# M100 D2 — honest OLAP benchmark for the DataFusion vectorized CustomScan aggregate. Emits JSON (stdout) →
# docs/benchmarks/m100-datafusion-executor.{md,json}. MEASURES a `count(*), sum(measure)` aggregate over a WIDE
# columnar table three ways on the SAME data: (1) VECTORIZED — theodb.enable_columnar_agg=on (the M100 CustomScan
# decodes only the projected column into Arrow + runs a DataFusion aggregate); (2) M99 SEQSCAN — GUC off (the M99
# row-at-a-time TAM scan: decode all columns + form heap tuples + PG aggregate); (3) HEAP — a row-store control.
# The honest gain claim is VECTORIZED vs the M99 SEQSCAN on the same columnar data (projection + no form/deform).
# vs HEAP is context only (heap is a different storage; no superiority claim vs AlloyDB in-core — M73/M97). Run on
# the droplet; regenerate the extension first (cargo pgrx install).
set -euo pipefail
PGINST="${PGINST:-$HOME/.pgrx/17.10/pgrx-install}"
export PATH="$PGINST/bin:$PATH"
DATA=/tmp/bench100_tmp
PORT=59717
DB=postgres
N="${N:-5000000}"
RUNS="${RUNS:-5}"

cleanup() { pg_ctl -D "$DATA" -m immediate stop -w >/dev/null 2>&1 || true; rm -rf "$DATA"; }
trap cleanup EXIT
rm -rf "$DATA"
initdb -D "$DATA" -U theo >/dev/null 2>&1
{ echo "port=$PORT"; echo "shared_buffers=2GB"; echo "work_mem=256MB"; echo "max_parallel_workers_per_gather=0"; } >> "$DATA/postgresql.conf"
pg_ctl -D "$DATA" -l "$DATA/log" start -w >/dev/null
q() { psql -X -q -p "$PORT" -U theo -d "$DB" -tAc "$1"; }

q "CREATE EXTENSION theodb_rs;" >/dev/null
GEN="SELECT g AS id, 'cat_' || (g % 10) AS category, (g % 7) AS bucket, (g % 2 = 0) AS flag, (g * 1.5)::float8 AS measure FROM generate_series(1, $N) g"
q "CREATE TABLE t_col (id int, category text, bucket int, flag bool, measure float8) USING theodb_columnar;" >/dev/null
q "CREATE TABLE t_heap (id int, category text, bucket int, flag bool, measure float8);" >/dev/null
q "INSERT INTO t_col $GEN;" >/dev/null
q "INSERT INTO t_heap $GEN;" >/dev/null
q "SELECT count(*) FROM t_col;" >/dev/null   # ensure the columnar stripes are flushed
q "VACUUM ANALYZE t_heap;" >/dev/null

AGG="SELECT count(*), sum(measure) FROM %s"

time_query() {
    local setup="$1" sql="$2"; local times=()
    q "$setup; EXPLAIN ANALYZE $sql" >/dev/null   # warm-up
    for _ in $(seq 1 "$RUNS"); do
        local ms
        ms=$(psql -X -q -p "$PORT" -U theo -d "$DB" -tAc "$setup; EXPLAIN (ANALYZE, TIMING OFF, BUFFERS OFF) $sql" 2>/dev/null | grep -oE 'Execution Time: [0-9.]+' | grep -oE '[0-9.]+')
        times+=("$ms")
    done
    printf '%s\n' "${times[@]}" | awk '{s+=$1; a[NR]=$1} END {m=s/NR; for(i=1;i<=NR;i++){d+=(a[i]-m)^2} print m"|"sqrt(d/NR)}'
}

VEC=$(time_query "SET theodb.enable_columnar_agg=on" "$(printf "$AGG" t_col)")
SEQ=$(time_query "SET theodb.enable_columnar_agg=off" "$(printf "$AGG" t_col)")
HEAP=$(time_query "SET theodb.enable_columnar_agg=off" "$(printf "$AGG" t_heap)")

# Confirm the vectorized path is actually the CustomScan (else the number is meaningless).
IS_CS=$(psql -X -q -p "$PORT" -U theo -d "$DB" -tAc "SET theodb.enable_columnar_agg=on; EXPLAIN (COSTS OFF) $(printf "$AGG" t_col)" 2>/dev/null | grep -c "Custom Scan" || true)
# Result-equivalence: vectorized aggregate == heap aggregate.
EQ=$(q "SET theodb.enable_columnar_agg=on; SELECT (SELECT count(*)||'/'||round(sum(measure)::numeric,1) FROM t_col) = (SELECT count(*)||'/'||round(sum(measure)::numeric,1) FROM t_heap);")

vm=${VEC%%|*}; vs=${VEC##*|}; sm=${SEQ%%|*}; ss=${SEQ##*|}; hm=${HEAP%%|*}; hs=${HEAP##*|}
speedup=$(awk "BEGIN{ if ($vm>0) printf \"%.2f\", $sm/$vm; else print 0 }")

cat <<JSON
{
  "milestone": "M100",
  "benchmark": "DataFusion vectorized CustomScan aggregate vs M99 seqscan vs heap",
  "hardware": "$(nproc) vCPU, $(free -g | awk '/Mem/{print $2}')GB RAM (DigitalOcean droplet)",
  "pg_version": "17.10 (pgrx-managed)",
  "rows": $N,
  "runs_per_query": $RUNS,
  "query": "SELECT count(*), sum(measure) FROM <table>",
  "vectorized_path_is_customscan": $([ "$IS_CS" -ge 1 ] && echo true || echo false),
  "result_equivalence_vec_eq_heap": "$EQ",
  "exec_ms_mean_stddev": {
    "vectorized_columnar": { "mean": $vm, "stddev": $vs },
    "m99_seqscan_columnar": { "mean": $sm, "stddev": $ss },
    "heap": { "mean": $hm, "stddev": $hs }
  },
  "vectorized_speedup_over_m99_seqscan": $speedup,
  "honest_ceiling": "The gain claim is VECTORIZED vs the M99 row-at-a-time TAM seqscan on the SAME columnar data (projection pushdown + no heap-tuple form/deform + Arrow aggregate). vs HEAP is context only (different storage). NOT a superiority claim vs AlloyDB's in-core engine (M73/M97). Slice-1 covers count(*)/sum(float8) without GROUP BY/WHERE; those + GROUP BY pushdown widen the gain in later slices."
}
JSON
