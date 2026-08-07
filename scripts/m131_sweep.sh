#!/bin/bash
# M131 (#135) — EXPLAIN sweep over the 43 ClickBench queries with the columnar-aggregate pushdown ON.
#
# Produces the artifact behind `wiki/benchmarks/m131-columnar-agg-accelerated.md`: per-query EXPLAIN latency, whether
# the query hung (the #135 defect was an EXPLAIN-deparse infinite recursion that `statement_timeout` cannot
# interrupt), and whether the `theodb_columnar_agg` CustomScan engaged.
#
# Pre-req: a TheoDB PG with the ClickBench `hits` table loaded `USING theodb_columnar` (see
# `benchmarks/run_m128_clickbench.py`, which fetches ClickBench's CC-BY-NC-SA queries.sql at runtime — never vendored).
#
# Usage:
#   PSQL_BIN=/path/to/psql PGPORT=28900 \
#   QUERIES=benchmarks/clickbench/theodb/queries.sql \
#   OUT=benchmarks/artifacts/m131-explain-sweep.json \
#   bash scripts/m131_sweep.sh
#
# Exits non-zero if ANY query still hangs (a #135 regression gate).
set -uo pipefail

PSQL_BIN="${PSQL_BIN:-psql}"
PGPORT="${PGPORT:-28900}"; PGHOST="${PGHOST:-localhost}"
PGUSER="${PGUSER:-postgres}"; PGDATABASE="${PGDATABASE:-postgres}"
QUERIES="${QUERIES:-benchmarks/clickbench/theodb/queries.sql}"
OUT="${OUT:-benchmarks/artifacts/m131-explain-sweep.json}"
TIMEOUT_S="${TIMEOUT_S:-20}"
BOX="${BOX:-self-hosted (NOT canonical hardware)}"

PSQL="$PSQL_BIN -p $PGPORT -h $PGHOST -U $PGUSER -d $PGDATABASE -tAX"
[ -f "$QUERIES" ] || { echo "FATAL: queries file not found: $QUERIES"; exit 1; }

i=0; hung=0; cs=0; maxms=0; first=1
printf '{"box":"%s","build":"theodb_rs post-M131 fix","protocol":"EXPLAIN only (planning + plan printing), pushdown ON, %ss per-query timeout","queries":[' \
  "$BOX" "$TIMEOUT_S" > "$OUT"

while IFS= read -r q; do
  [ -z "$q" ] && continue
  i=$((i+1))
  qc=$(echo "$q" | sed 's/;[[:space:]]*$//')
  start=$(date +%s%N)
  out=$(timeout "$TIMEOUT_S" bash -c "$PSQL -c \"SET theodb.enable_columnar_agg=on; SET statement_timeout=0; EXPLAIN $qc;\"" 2>&1); rc=$?
  end=$(date +%s%N); ms=$(( (end-start)/1000000 ))
  h=0; [ $rc -eq 124 ] && { h=1; hung=$((hung+1)); ms=-1; }
  c=0; echo "$out" | grep -q "theodb_columnar_agg" && { c=1; cs=$((cs+1)); }
  [ $ms -gt $maxms ] && maxms=$ms
  [ $first -eq 0 ] && printf ',' >> "$OUT"; first=0
  printf '{"q":%d,"explain_ms":%d,"hung":%d,"customscan":%d}' "$i" "$ms" "$h" "$c" >> "$OUT"
done < "$QUERIES"

printf '],"total":%d,"hung":%d,"customscan_engaged":%d,"max_explain_ms":%d}\n' "$i" "$hung" "$cs" "$maxms" >> "$OUT"
echo "total=$i hung=$hung customscan_engaged=$cs max_explain_ms=$maxms  -> $OUT"
[ "$hung" -eq 0 ] || { echo "FAIL: $hung query(ies) still hang in EXPLAIN (#135 regression)"; exit 1; }
