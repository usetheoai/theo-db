#!/usr/bin/env bash
# M14 — reproducible DiskANN-vs-ScaNN-quality fork evaluation.
# Runs the recall@k harness for DiskANN (the shipped permissive StreamingDiskANN), HNSW, and IVFFlat on the
# SAME dataset and prints each one's recall x QPS so the fork/no-fork decision (docs/adr/0004) rests on
# measured numbers, not opinion. It does NOT build a native ScaNN AM — that is gated on this evidence
# (PRD fork-gate policy / anti-sunk-cost). The harness itself is unchanged; this is a thin orchestration.
#
# Usage: PGHOST=localhost PGPORT=5432 PGUSER=postgres PGPASSWORD=postgres PGDATABASE=postgres bash scann_fork_eval.sh
# Tunables (env): N (default 5000), DIM (32), NQ (100), K (10), RUNS (2), METRIC (cosine), OUT (docs/benchmarks)
set -euo pipefail

N="${N:-5000}"; DIM="${DIM:-32}"; NQ="${NQ:-100}"; K="${K:-10}"; RUNS="${RUNS:-2}"
METRIC="${METRIC:-cosine}"; OUT="${OUT:-docs/benchmarks}"

echo "== ScaNN fork evaluation (recall@${K}, n=${N}, dim=${DIM}, metric=${METRIC}, runs=${RUNS}) =="
echo "== Target: ScaNN-quality bar = recall@${K} >= 0.90 at usable QPS (ann-benchmarks band). =="
# Ensure the DiskANN access method exists (pgvectorscale is shipped in the image; the AM must be CREATEd).
psql -h "${PGHOST:-localhost}" -p "${PGPORT:-5432}" -U "${PGUSER:-postgres}" -d "${PGDATABASE:-postgres}" \
  -v ON_ERROR_STOP=1 -c "CREATE EXTENSION IF NOT EXISTS vectorscale CASCADE" >/dev/null
for idx in diskann hnsw ivfflat; do
  echo "--- ${idx} ---"
  python3 -m theodb_bench --index "$idx" --n "$N" --dim "$DIM" --n-queries "$NQ" \
    --k "$K" --runs "$RUNS" --metric "$METRIC" --out "$OUT"
done
echo "== Decision input: DiskANN is the permissive ScaNN-quality substitute. See docs/adr/0004-scann-fork-decision.md =="
