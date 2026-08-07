#!/usr/bin/env bash
# M99 D2 — honest columnar-vs-heap benchmark for the theodb_columnar TAM. Emits a JSON blob (stdout) consumed
# into wiki/benchmarks/m99-columnar-tam.md e benchmarks/artifacts/m99-columnar-tam.json. MEASURES: (1) on-disk size columnar vs heap (the compression
# win — deterministic, reproducible); (2) full-scan aggregate + GROUP BY wall time, N runs, mean ± stddev.
# HONEST CEILING: M99 has no projection/skip/vectorization pushdown (that is M100) — a plain seqscan decodes ALL
# columns of ALL chunk groups, so scan time is expected to be at PARITY or SLOWER than heap; the measured win is
# on-disk size. No superiority claim (M73/M97 discipline). Run on the build droplet.
set -euo pipefail
PGINST="${PGINST:-$HOME/.pgrx/17.10/pgrx-install}"
export PATH="$PGINST/bin:$PATH"
DATA=/tmp/bench_tmp
PORT=59716
DB=postgres
N="${N:-1000000}"
RUNS="${RUNS:-5}"

cleanup() { pg_ctl -D "$DATA" -m immediate stop -w >/dev/null 2>&1 || true; rm -rf "$DATA"; }
trap cleanup EXIT
rm -rf "$DATA"
initdb -D "$DATA" -U theo >/dev/null 2>&1
{ echo "port=$PORT"; echo "shared_buffers=1GB"; echo "work_mem=256MB"; echo "max_parallel_workers_per_gather=0"; } >> "$DATA/postgresql.conf"
pg_ctl -D "$DATA" -l "$DATA/log" start -w >/dev/null
q() { psql -X -q -p "$PORT" -U theo -d "$DB" -tAc "$1"; }

q "CREATE EXTENSION theodb_rs;" >/dev/null
# Analytical shape: a monotonic id, a low-cardinality category (compressible), a measure, a bool flag.
GEN="SELECT g AS id, 'cat_' || (g % 10) AS category, (g * 1.5)::float8 AS measure, (g % 2 = 0) AS flag FROM generate_series(1, $N) g"
q "CREATE TABLE t_col (id int, category text, measure float8, flag bool) USING theodb_columnar;" >/dev/null
q "CREATE TABLE t_heap (id int, category text, measure float8, flag bool);" >/dev/null
q "INSERT INTO t_col $GEN;" >/dev/null
q "INSERT INTO t_heap $GEN;" >/dev/null
q "SELECT count(*) FROM t_col;" >/dev/null   # ensure materialized/flushed
q "VACUUM ANALYZE t_heap;" >/dev/null

SIZE_COL=$(q "SELECT pg_relation_size('t_col');")
SIZE_HEAP=$(q "SELECT pg_relation_size('t_heap');")

# Time a query N runs (1 warm-up discarded), print mean and stddev in ms via EXPLAIN ANALYZE execution time.
time_query() {
    local tbl="$1" sql="$2"; local times=()
    q "EXPLAIN ANALYZE $sql" >/dev/null   # warm-up
    for _ in $(seq 1 "$RUNS"); do
        local ms
        ms=$(psql -X -q -p "$PORT" -U theo -d "$DB" -tAc "EXPLAIN (ANALYZE, TIMING OFF, BUFFERS OFF) $sql" 2>/dev/null | grep -oE 'Execution Time: [0-9.]+' | grep -oE '[0-9.]+')
        times+=("$ms")
    done
    printf '%s\n' "${times[@]}" | awk '{s+=$1; a[NR]=$1} END {m=s/NR; for(i=1;i<=NR;i++){d+=(a[i]-m)^2} print m"|"sqrt(d/NR)}'
}

AGG_SQL="SELECT count(*), sum(measure), avg(measure) FROM %s"
GRP_SQL="SELECT category, sum(measure) FROM %s GROUP BY category"

AGG_COL=$(time_query t_col "$(printf "$AGG_SQL" t_col)")
AGG_HEAP=$(time_query t_heap "$(printf "$AGG_SQL" t_heap)")
GRP_COL=$(time_query t_col "$(printf "$GRP_SQL" t_col)")
GRP_HEAP=$(time_query t_heap "$(printf "$GRP_SQL" t_heap)")

# Correctness cross-check: aggregates must be identical columnar vs heap.
EQ=$(q "SELECT (SELECT count(*)||'/'||round(sum(measure)::numeric,2) FROM t_col) = (SELECT count(*)||'/'||round(sum(measure)::numeric,2) FROM t_heap);")

am=${AGG_COL%%|*}; as=${AGG_COL##*|}; hm=${AGG_HEAP%%|*}; hs=${AGG_HEAP##*|}
gcm=${GRP_COL%%|*}; gcs=${GRP_COL##*|}; ghm=${GRP_HEAP%%|*}; ghs=${GRP_HEAP##*|}

cat <<JSON
{
  "milestone": "M99",
  "benchmark": "columnar-vs-heap scan (theodb_columnar TAM)",
  "hardware": "$(nproc) vCPU, $(free -g | awk '/Mem/{print $2}')GB RAM (DigitalOcean droplet)",
  "pg_version": "17.10 (pgrx-managed)",
  "rows": $N,
  "runs_per_query": $RUNS,
  "result_equivalence_col_eq_heap": "$EQ",
  "on_disk_bytes": { "columnar": $SIZE_COL, "heap": $SIZE_HEAP, "compression_ratio_heap_over_col": $(awk "BEGIN{printf \"%.2f\", $SIZE_HEAP/$SIZE_COL}") },
  "scan_ms_mean_stddev": {
    "full_aggregate": { "columnar_mean": $am, "columnar_stddev": $as, "heap_mean": $hm, "heap_stddev": $hs },
    "group_by_category": { "columnar_mean": $gcm, "columnar_stddev": $gcs, "heap_mean": $ghm, "heap_stddev": $ghs }
  },
  "honest_ceiling": "M99 seqscan decodes ALL columns of ALL chunk groups (no projection/skip/vectorization pushdown — that is M100). The measured win is ON-DISK SIZE (compression); scan wall-time is expected at parity-or-slower vs heap. NOT a superiority claim (M73/M97)."
}
JSON
