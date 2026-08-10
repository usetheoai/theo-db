#!/bin/bash
# M159 — ClickHouse side of the same-box ClickBench gap measurement (council-benchmark MEDIUM: make it reproducible).
# Loads the SAME systematic 1M `hits_sample.tsv` the TheoDB harness used into a daemon-free `clickhouse local --path`
# MergeTree, then runs the 43 ClickBench (clickhouse-dialect) queries 3× each and writes {q, ch_hot_s=min} per line.
#   ch_hot_s is ClickHouse SERVER-SIDE exec time (`--time`) — NOT a client round-trip; TheoDB's harness times the
#   psycopg2 round-trip. This asymmetry ADDS fixed overhead to TheoDB only, so the measured ratio is CONSERVATIVE
#   (overstates the gap, never flatters TheoDB). Documented in wiki/benchmarks/m159-clickhouse-gap-verdict.md.
# Usage: m159_clickhouse_run.sh <clickhouse-binary> <sample.tsv> <ch_create.sql> <ch_queries.sql> <out.jsonl> [ch-path]
set -u
CH="${1:?clickhouse binary}"; SAMPLE="${2:?sample.tsv}"; CREATE="${3:?create.sql}"; QUERIES="${4:?queries.sql}"
OUT="${5:?out.jsonl}"; CHP="${6:-/tmp/ch-data}"
mkdir -p "$CHP"
"$CH" local --path="$CHP" --multiquery < "$CREATE"                                    # CREATE OR REPLACE TABLE hits (MergeTree)
"$CH" local --path="$CHP" --query "INSERT INTO hits FROM INFILE '$SAMPLE' FORMAT TSV;"
echo "loaded: $("$CH" local --path="$CHP" --query 'SELECT count() FROM hits;')"
: > "$OUT"
i=0
while IFS= read -r q; do
  [ -z "$q" ] && continue
  qn=${q%;}
  best=""
  for r in 1 2 3; do
    t=$("$CH" local --path="$CHP" --time --query "$qn FORMAT Null" 2>&1 >/dev/null | tail -1)
    case "$t" in ''|*[!0-9.]*) t=-1;; esac
    if [ -z "$best" ] || awk "BEGIN{exit !($t<$best)}"; then best=$t; fi   # hot = min of 3
  done
  echo "{\"q\":$i,\"ch_hot_s\":$best}" >> "$OUT"
  i=$((i+1))
done < "$QUERIES"
echo "wrote $OUT ($i queries)"
