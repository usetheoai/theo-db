#!/usr/bin/env bash
# M103 — honest COLUMN-PRUNING benchmark for the co-resident vector+columnar substrate. Emits JSON (stdout) →
# docs/benchmarks/m103-vector-columnar.{md,json}. The co-resident filtered top-k (theodb.vindex_knn_columnar)
# decodes ONLY the 4 index columns (tid, part_id, label, vec) — the analytical columns are NEVER decoded. The
# cleanest measurable proof of pruning: the knn latency is INVARIANT to the analytical-column width. We build two
# co-resident indexes on the SAME vectors — one with a NARROW payload (1 float8), one WIDE (W float8 columns) —
# and show the knn latency is ~equal (payload width does not touch the vector scan), plus the on-disk column-size
# pruning ratio. HONEST CEILING (ADR D4): recall is EQUAL by construction (proven by the byte-identity pg_test),
# NOT a claim; NO QPS-vs-ScaNN claim (the M73/M74 paradigm ceiling is untouched). Run on the droplet (install first).
set -euo pipefail
PGINST="${PGINST:-$HOME/.pgrx/17.10/pgrx-install}"
export PATH="$PGINST/bin:$PATH"
DATA=/tmp/bench103_tmp
PORT=59720
DB=postgres
N="${N:-50000}"
DIM="${DIM:-8}"
WIDE="${WIDE:-16}"     # analytical columns in the WIDE index
RUNS="${RUNS:-5}"

cleanup() { pg_ctl -D "$DATA" -m immediate stop -w >/dev/null 2>&1 || true; rm -rf "$DATA"; }
trap cleanup EXIT
rm -rf "$DATA"
initdb -D "$DATA" -U theo >/dev/null 2>&1
{ echo "port=$PORT"; echo "shared_buffers=1GB"; echo "work_mem=128MB"; echo "max_parallel_workers_per_gather=0"; } >> "$DATA/postgresql.conf"
pg_ctl -D "$DATA" -l "$DATA/log" start -w >/dev/null
q() { psql -X -q -p "$PORT" -U theo -d "$DB" -tAc "$1"; }

q "CREATE EXTENSION theodb_rs;" >/dev/null

# a random-ish but deterministic vector column of DIM dims + a scalar label
VECEXPR="theodb.f32vec_to_bytea(ARRAY(SELECT ((g*7+d*13) % 100)::float4 FROM generate_series(1,$DIM) d))"
q "CREATE TABLE src AS
   SELECT g AS tid, (g % 4) AS label, $VECEXPR AS vec FROM generate_series(1,$N) g;" >/dev/null
q "CREATE TABLE parts AS
   SELECT a.ord AS tid, a.part_id FROM (SELECT array_agg(vec ORDER BY tid) v FROM src) s,
   LATERAL unnest(theodb.vindex_assign(s.v, 64)) WITH ORDINALITY AS a(part_id, ord);" >/dev/null

# NARROW index: 1 analytical column
q "CREATE TABLE idx_narrow (tid int8, part_id int4, label int4, vec bytea, p0 float8) USING theodb_columnar;" >/dev/null
q "INSERT INTO idx_narrow SELECT s.tid, p.part_id, s.label, s.vec, (s.tid*1.5)::float8
   FROM src s JOIN parts p USING (tid);" >/dev/null

# WIDE index: WIDE analytical columns (same vec/label/part_id)
WCOLS=$(for i in $(seq 0 $((WIDE-1))); do echo -n "p$i float8, "; done)
WVALS=$(for i in $(seq 0 $((WIDE-1))); do echo -n ", (s.tid*$i.0)::float8"; done)
q "CREATE TABLE idx_wide (tid int8, part_id int4, label int4, vec bytea, ${WCOLS%, }) USING theodb_columnar;" >/dev/null
q "INSERT INTO idx_wide SELECT s.tid, p.part_id, s.label, s.vec ${WVALS}
   FROM src s JOIN parts p USING (tid);" >/dev/null

QUERY="ARRAY(SELECT ((37*7+d*13)%100)::float4 FROM generate_series(1,$DIM) d)::float4[]"

bench_knn() {
  local tbl="$1"; local sum=0
  for r in $(seq 1 "$RUNS"); do
    local ms
    ms=$(psql -X -q -p "$PORT" -U theo -d "$DB" -tA <<SQL
SELECT extract(epoch from clock_timestamp())*1000 AS s \gset
SELECT count(*) FROM theodb.vindex_knn_columnar('$tbl'::regclass, $QUERY, 10, 64, 0);
SELECT extract(epoch from clock_timestamp())*1000 AS e \gset
SELECT round((:e - :s)::numeric, 2);
SQL
)
    ms=$(echo "$ms" | tail -1)
    sum=$(python3 -c "print($sum + $ms)")
  done
  python3 -c "print(round($sum/$RUNS, 2))"
}

NARROW_MS=$(bench_knn idx_narrow)
WIDE_MS=$(bench_knn idx_wide)

# on-disk column-size pruning ratio: the knn touches only (tid,part_id,label,vec); payload cols are the rest.
TOTAL_WIDE=$(q "SELECT pg_total_relation_size('idx_wide');")
TOTAL_NARROW=$(q "SELECT pg_total_relation_size('idx_narrow');")

# composed filtered-knn + analytical aggregation (one plan)
COMPOSE_MS=$(psql -X -q -p "$PORT" -U theo -d "$DB" -tA <<SQL
SELECT extract(epoch from clock_timestamp())*1000 AS s \gset
SELECT avg(i.p0) FROM theodb.vindex_knn_columnar('idx_narrow'::regclass, $QUERY, 10, 64, 0) knn JOIN idx_narrow i USING(tid);
SELECT extract(epoch from clock_timestamp())*1000 AS e \gset
SELECT round((:e - :s)::numeric, 2);
SQL
)
COMPOSE_MS=$(echo "$COMPOSE_MS" | tail -1)

RATIO=$(python3 -c "print(round($WIDE_MS/max($NARROW_MS,0.01), 3))")

cat <<JSON
{
  "milestone": "M103",
  "n": $N, "dim": $DIM, "wide_analytical_cols": $WIDE, "runs": $RUNS,
  "column_pruning": {
    "knn_ms_narrow_payload": $NARROW_MS,
    "knn_ms_wide_payload": $WIDE_MS,
    "wide_over_narrow_ratio": $RATIO,
    "on_disk_bytes_wide": $TOTAL_WIDE,
    "on_disk_bytes_narrow": $TOTAL_NARROW,
    "note": "the co-resident filtered top-k decodes ONLY the 4 index columns; a ratio ~1.0 proves the analytical columns (whose on-disk size grows with width) are NOT decoded — column pruning. The wide index is much larger on disk yet the knn latency is ~unchanged."
  },
  "composed_filter_knn_plus_aggregation_ms": $COMPOSE_MS,
  "honest_ceiling": "Recall is EQUAL by construction (proven by the byte-identity pg_test m103_full_probe_byte_identical_to_exact_filtered) — NOT a claim. Cost/scale/composability win only: column-pruned filtered vector search + analytical projection in one scan. NO QPS-vs-ScaNN claim (the M73/M74 paradigm ceiling is untouched by co-residence). Out-of-RAM at billion-scale is the honest projection, not measured here."
}
JSON
