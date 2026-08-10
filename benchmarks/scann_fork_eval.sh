#!/usr/bin/env bash
# M14 — reproducible DiskANN-vs-ScaNN-quality fork evaluation.
# Runs the recall@k harness for DiskANN (the shipped permissive StreamingDiskANN), HNSW, and IVFFlat on the
# SAME seeded dataset and writes each one's recall x QPS so the fork/no-fork decision (wiki/decisions/0004) rests on
# measured numbers, not opinion. It does NOT build a native ScaNN AM — that is gated on this evidence
# (PRD fork-gate policy / anti-sunk-cost). The harness itself is unchanged; this is a thin orchestration.
#
# Usage: PGHOST=localhost PGPORT=5432 PGUSER=postgres PGPASSWORD=postgres PGDATABASE=postgres bash scann_fork_eval.sh
# Tunables (env): N (5000), DIM (32), NQ (100), K (10), RUNS (3), METRIC (cosine), SEED (14), OUT (benchmarks/artifacts)
set -euo pipefail

command -v psql >/dev/null 2>&1 || { echo "scann_fork_eval: psql not found on PATH (needed to ensure the diskann AM)"; exit 2; }

N="${N:-5000}"; DIM="${DIM:-32}"; NQ="${NQ:-100}"; K="${K:-10}"; RUNS="${RUNS:-3}"
METRIC="${METRIC:-cosine}"; SEED="${SEED:-14}"; OUT="${OUT:-benchmarks/artifacts}"

echo "== ScaNN fork evaluation (recall@${K}, n=${N}, dim=${DIM}, metric=${METRIC}, runs=${RUNS}, seed=${SEED}) =="
echo "== Target: ScaNN-quality bar = recall@${K} >= 0.90 at usable QPS (ann-benchmarks band). =="
# Ensure the DiskANN access method exists (pgvectorscale is shipped in the image; the AM must be CREATEd).
psql -h "${PGHOST:-localhost}" -p "${PGPORT:-5432}" -U "${PGUSER:-postgres}" -d "${PGDATABASE:-postgres}" \
  -v ON_ERROR_STOP=1 -c "CREATE EXTENSION IF NOT EXISTS vectorscale CASCADE" >/dev/null
for idx in diskann hnsw ivfflat; do
  echo "--- ${idx} ---"
  # per-index --out so each JSON artifact survives (the 3 share a dataset_label stem otherwise -> clobber).
  mkdir -p "${OUT}/${idx}"
  python3 -m theodb_bench --index "$idx" --n "$N" --dim "$DIM" --n-queries "$NQ" \
    --k "$K" --runs "$RUNS" --metric "$METRIC" --seed "$SEED" --out "${OUT}/${idx}"
done
echo "== Decision input: DiskANN is the permissive ScaNN-quality substitute. See wiki/decisions/0004-scann-fork-decision.md =="
