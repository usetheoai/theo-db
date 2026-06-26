#!/usr/bin/env bash
set -euo pipefail

HOST="${PGHOST:-localhost}"
PORT="${PGPORT:-5432}"
USER="${PGUSER:-postgres}"
PGPASSWORD="${PGPASSWORD:-postgres}"
export PGPASSWORD

for i in $(seq 1 10); do
  pg_isready -h "$HOST" -p "$PORT" -U "$USER" -q && break
  sleep 1
done
pg_isready -h "$HOST" -p "$PORT" -U "$USER" -q

psql -h "$HOST" -p "$PORT" -U "$USER" -v ON_ERROR_STOP=1 <<'SQL'
CREATE EXTENSION IF NOT EXISTS vector;
SELECT '[1,2,3]'::vector <=> '[4,5,6]'::vector;
SQL

echo "SMOKE PASSED"
