#!/usr/bin/env bash
# M61 smoke (DoD Fase 2): prove pg_duckdb is embedded, loads on a clean init, coexists with the vector stack,
# and that an analytic query PLANS UNDER THE DUCKDB EXECUTOR — the Goal's oracle
# `test_pg_duckdb_analytic_query_plans_under_duckdb`.
#
# Asserts, on a fresh `theodb:m61` container (greenfield init runs 00-create-theodb.sql):
#   1. shared_preload_libraries includes pg_duckdb (boot-loaded, fail-closed).
#   2. the 4 key extensions coexist: vector, vectorscale, theodb, pg_duckdb.
#   3. duckdb.query('SELECT 42') == 42 (DuckDB engine reachable).
#   4. duckdb.allow_community_extensions = off (security — no unaudited DuckDB extensions).
#   5. an analytic aggregate PLANS UNDER DUCKDB (EXPLAIN shows a DuckDB scan) under force_execution — the oracle.
#   6. the theodb_hnsw vector index still works alongside (real coexistence).
#
# Self-contained: creates + removes its own container. Exit 0 = PASS.
# Usage: IMAGE=theodb:m61 scripts/m61-pgduckdb-smoke.sh
set -euo pipefail

IMAGE="${IMAGE:-theodb:m61}"
CTR="theodb-m61-smoke"
PW="postgres"
DEXEC=(docker exec "$CTR" psql -U postgres -tAc)

cleanup() { docker rm -f "$CTR" >/dev/null 2>&1 || true; }
trap cleanup EXIT

fail() { echo "FAIL: $1" >&2; exit 1; }

echo "== M61 pg_duckdb smoke (image=$IMAGE) =="
docker rm -f "$CTR" >/dev/null 2>&1 || true
docker run -d --name "$CTR" -e POSTGRES_PASSWORD="$PW" "$IMAGE" >/dev/null

# wait for healthy (the init + preload must succeed, else the container never becomes healthy — fail-closed)
for _ in $(seq 1 40); do
  [ "$(docker inspect -f '{{.State.Health.Status}}' "$CTR" 2>/dev/null)" = healthy ] && break
  sleep 2
done
[ "$(docker inspect -f '{{.State.Health.Status}}' "$CTR" 2>/dev/null)" = healthy ] \
  || fail "container not healthy (pg_duckdb preload likely broke boot)"

# 1. preload
"${DEXEC[@]}" "SHOW shared_preload_libraries" | grep -q pg_duckdb || fail "pg_duckdb not in shared_preload_libraries"

# 2. coexistence — the 4 key extensions all present
exts="$("${DEXEC[@]}" "SELECT string_agg(extname, ',' ORDER BY extname) FROM pg_extension")"
for e in vector vectorscale theodb pg_duckdb; do
  echo "$exts" | grep -q "$e" || fail "extension $e missing (got: $exts)"
done

# 3. DuckDB engine reachable
ans="$("${DEXEC[@]}" "SELECT * FROM duckdb.query(\$\$SELECT 42 AS answer\$\$)")"
[ "$ans" = "42" ] || fail "duckdb.query('SELECT 42') returned '$ans', expected 42"

# 4. security — community extensions off
"${DEXEC[@]}" "SHOW duckdb.allow_community_extensions" | grep -qi off \
  || fail "duckdb.allow_community_extensions is not off (security)"

# 5. THE ORACLE — an analytic aggregate plans under DuckDB (test_pg_duckdb_analytic_query_plans_under_duckdb)
docker exec "$CTR" psql -U postgres -c "CREATE TABLE m61smoke(id int, amount double precision); INSERT INTO m61smoke SELECT g, g*1.5 FROM generate_series(1,1000) g" >/dev/null
plan="$(docker exec "$CTR" psql -U postgres -c "SET duckdb.force_execution=true" \
        -tAc "EXPLAIN SELECT count(*), sum(amount) FROM m61smoke")"
echo "$plan" | grep -qi duckdb || fail "analytic query did NOT plan under DuckDB (EXPLAIN: $plan)"

# 6. vector index still works (real coexistence)
docker exec "$CTR" psql -U postgres -c "CREATE TABLE m61vec(id int, e vector(3)); INSERT INTO m61vec VALUES (1,'[1,2,3]'),(2,'[4,5,6]'); CREATE INDEX ON m61vec USING theodb_hnsw (e theodb_hnsw_cosine_ops)" >/dev/null
top="$("${DEXEC[@]}" "SELECT id FROM m61vec ORDER BY e <=> '[1,2,3]' LIMIT 1")"
[ "$top" = "1" ] || fail "vector top-k wrong (got '$top', expected 1) — coexistence broken"

echo "PASS: test_pg_duckdb_analytic_query_plans_under_duckdb (+ preload, coexistence, engine, security, vector)"
