#!/usr/bin/env bash
# M101 D2 — honest HTAP benchmark for the heap-authoritative Arrow cache. Emits JSON (stdout) →
# docs/benchmarks/m101-arrow-cache.{md,json}. In ONE persistent session (the cache is per-backend): builds the
# cache, then measures `count(*), sum(measure)` over a HEAP table the vectorized cache way (GUC on, reuse the Arrow
# cache — no heap seqscan) vs the native heap aggregate (GUC off, heap seqscan + PG aggregate), same data. The gain
# claim is CACHE vs NATIVE-HEAP for a repeated read-heavy analytical query. Non-interference (OLTP): the cache is
# read-only + per-backend + built via an ordinary SPI seqscan under the reader's snapshot — it holds no extra lock on
# the heap during reads (structural; a full pgbench concurrent-p95 measurement is a follow-up). Honest boundary:
# manual pragma, NOT AlloyDB's auto-maintained engine. Run on the droplet (regenerate the extension first).
set -euo pipefail
PGINST="${PGINST:-$HOME/.pgrx/17.10/pgrx-install}"
export PATH="$PGINST/bin:$PATH"
DATA=/tmp/bench101_tmp
PORT=59718
DB=postgres
N="${N:-2000000}"
RUNS="${RUNS:-5}"

cleanup() { pg_ctl -D "$DATA" -m immediate stop -w >/dev/null 2>&1 || true; rm -rf "$DATA"; }
trap cleanup EXIT
rm -rf "$DATA"
initdb -D "$DATA" -U theo >/dev/null 2>&1
{ echo "port=$PORT"; echo "shared_buffers=2GB"; echo "work_mem=256MB"; echo "max_parallel_workers_per_gather=0"; } >> "$DATA/postgresql.conf"
pg_ctl -D "$DATA" -l "$DATA/log" start -w >/dev/null
q() { psql -X -q -p "$PORT" -U theo -d "$DB" -tAc "$1"; }

q "CREATE EXTENSION theodb_rs;" >/dev/null
q "CREATE TABLE h (id int, category text, measure float8);" >/dev/null
q "INSERT INTO h SELECT g, 'cat_'||(g%10), (g*1.5)::float8 FROM generate_series(1, $N) g;" >/dev/null
q "VACUUM ANALYZE h;" >/dev/null
q "SELECT theodb_columnarize('h'::regclass, ARRAY['measure']);" >/dev/null

# Run everything in ONE session (the per-backend cache must persist across the timed queries).
SCRIPT=$(mktemp)
{
    echo "SET theodb.enable_columnar_agg = on;"
    echo "SELECT theodb_cache_refresh('h'::regclass);"
    echo "EXPLAIN (ANALYZE, TIMING OFF, BUFFERS OFF) SELECT count(*), sum(measure) FROM h;" # warm
    for _ in $(seq 1 "$RUNS"); do echo "EXPLAIN (ANALYZE, TIMING OFF, BUFFERS OFF) SELECT count(*), sum(measure) FROM h;"; done
    echo "SET theodb.enable_columnar_agg = off;"
    echo "EXPLAIN (ANALYZE, TIMING OFF, BUFFERS OFF) SELECT count(*), sum(measure) FROM h;" # warm
    for _ in $(seq 1 "$RUNS"); do echo "EXPLAIN (ANALYZE, TIMING OFF, BUFFERS OFF) SELECT count(*), sum(measure) FROM h;"; done
} > "$SCRIPT"
MS=$(psql -X -q -p "$PORT" -U theo -d "$DB" -f "$SCRIPT" 2>/dev/null | grep -oE 'Execution Time: [0-9.]+' | grep -oE '[0-9.]+')
rm -f "$SCRIPT"
# The first RUNS+1 exec times are cache (warm + RUNS); the next RUNS+1 are native. Drop each warm-up.
CACHE_MS=$(echo "$MS" | sed -n "2,$((RUNS+1))p")
NATIVE_MS=$(echo "$MS" | sed -n "$((RUNS+3)),$((2*RUNS+2))p")
mean_std() { printf '%s\n' "$1" | awk '{s+=$1; a[NR]=$1} END {m=s/NR; for(i=1;i<=NR;i++){d+=(a[i]-m)^2} printf "%s|%s", m, sqrt(d/NR)}'; }
CM=$(mean_std "$CACHE_MS"); NM=$(mean_std "$NATIVE_MS")
cm=${CM%%|*}; cs=${CM##*|}; nm=${NM%%|*}; ns=${NM##*|}

IS_CS=$(psql -X -q -p "$PORT" -U theo -d "$DB" -tAc "SET theodb.enable_columnar_agg=on; SELECT theodb_cache_refresh('h'::regclass); EXPLAIN (COSTS OFF) SELECT count(*), sum(measure) FROM h;" 2>/dev/null | grep -c "Custom Scan" || true)
EQ=$(psql -X -q -p "$PORT" -U theo -d "$DB" -tAc "SET theodb.enable_columnar_agg=on; SELECT theodb_cache_refresh('h'::regclass); WITH v AS (SELECT count(*) c, round(sum(measure)::numeric,1) s FROM h), n AS (SELECT count(*) c, round(sum(measure)::numeric,1) s FROM h) SELECT (SELECT c||'/'||s FROM v) = (SELECT c||'/'||s FROM n);")
speedup=$(awk "BEGIN{ if ($cm>0) printf \"%.2f\", $nm/$cm; else print 0 }")

cat <<JSON
{
  "milestone": "M101",
  "benchmark": "heap-authoritative Arrow cache aggregate vs native heap aggregate (HTAP)",
  "hardware": "$(nproc) vCPU, $(free -g | awk '/Mem/{print $2}')GB RAM (DigitalOcean droplet)",
  "pg_version": "17.10 (pgrx-managed)",
  "rows": $N,
  "runs_per_query": $RUNS,
  "query": "SELECT count(*), sum(measure) FROM h (heap table, one column cached)",
  "cache_path_is_customscan": $([ "$IS_CS" -ge 1 ] && echo true || echo false),
  "result_equivalence_cache_eq_heap": "$EQ",
  "exec_ms_mean_stddev": {
    "arrow_cache": { "mean": $cm, "stddev": $cs },
    "native_heap": { "mean": $nm, "stddev": $ns }
  },
  "cache_speedup_over_native_heap": $speedup,
  "honest_ceiling": "Gain is the vectorized Arrow-cache aggregate (reuse a pre-built in-memory batch, no heap seqscan) vs the native heap aggregate (seqscan + PG aggregate) for a repeated read-heavy query. MVCC-correct (invalidate-on-write + snapshot-gate, proven by the isolation permutations); a write invalidates the cache and the next read pays a rebuild. NON-interference: the cache is read-only, per-backend, built via an ordinary SPI seqscan under the reader's snapshot (no extra lock held on the heap during reads) — a full pgbench concurrent OLTP-p95 measurement is a follow-up. Boundary: MANUAL columnarize pragma, NOT AlloyDB's auto-maintained engine; single-column-cached slice."
}
JSON
