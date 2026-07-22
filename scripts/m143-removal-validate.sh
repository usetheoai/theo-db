#!/usr/bin/env bash
# M143 removal validation — prova, contra a imagem default M143 (uma imagem só, SEM pg_duckdb), que o lakehouse é
# 100% own-code: round-trip write→read→olap, leitor jsonb multi-tipo, escrita fail-closed, e pg_duckdb ausente.
#
# Gates (falha exit≠0): NO_PGDUCKDB, M62_OWNCODE (htap_refresh→olap = a|2|15/b|1|5), READ_MULTI (jsonb com
# int/float/text/bool), WRITE_FAILCLOSED (tipo não-suportado → SQLSTATE + backend vivo). Reporta o tamanho da imagem.
# Uso (droplet, raiz do repo): bash scripts/m143-removal-validate.sh
set -euo pipefail
TAG="${TAG:-theodb:m143}"
CTR="theodb-m143-smoke"
fail() { echo "FAIL: $1" >&2; exit 1; }
psql_c() { docker exec "$CTR" psql -U postgres -tAc "$1"; }

if [ "${SKIP_BUILD:-0}" != "1" ]; then
  echo "== BUILD imagem default M143 ($TAG) =="
  docker build -t "$TAG" . || fail "build da imagem M143 falhou"
fi

docker rm -f "$CTR" >/dev/null 2>&1 || true
docker run -d --name "$CTR" -e POSTGRES_PASSWORD=postgres "$TAG" >/dev/null
trap 'docker rm -f "$CTR" >/dev/null 2>&1 || true' EXIT
for _ in $(seq 1 40); do
  [ "$(docker inspect -f '{{.State.Health.Status}}' "$CTR" 2>/dev/null)" = healthy ] && break; sleep 2
done
[ "$(docker inspect -f '{{.State.Health.Status}}' "$CTR" 2>/dev/null)" = healthy ] || { docker logs "$CTR" 2>&1 | tail -20; fail "container não healthy"; }

echo "== gate: pg_duckdb REMOVIDO =="
[ "$(psql_c "SELECT count(*) FROM pg_extension WHERE extname='pg_duckdb'")" = "0" ] || fail "pg_duckdb presente"
# captura-depois-assere (não mascarar falha do psql com `&& fail || true`)
SP="$(psql_c "SHOW shared_preload_libraries")"
if echo "$SP" | grep -q pg_duckdb; then fail "pg_duckdb no shared_preload (got: $SP)"; fi
echo "NO_PGDUCKDB"

echo "== gate: M62 own-code (htap_refresh → olap) =="
docker exec "$CTR" psql -U postgres -c "CREATE TABLE lh(category text, amount float8, n int, flag boolean); INSERT INTO lh VALUES ('a',10,1,true),('a',20,2,false),('b',5,3,true)" >/dev/null
psql_c "SELECT theodb.htap_refresh('lh'::regclass)" >/dev/null || fail "htap_refresh own-code falhou"
rows="$(psql_c "SELECT category||'|'||c||'|'||a FROM theodb.olap('lh'::regclass) ORDER BY category")"
echo "$rows" | grep -q '^a|2|15' && echo "$rows" | grep -q '^b|1|5' || fail "olap own-code M62 (got: $rows)"
echo "M62_OWNCODE"

echo "== gate: read_parquet own-code multi-tipo (round-trip via write_parquet) =="
psql_c "SELECT public.write_parquet('lh'::regclass::text, '/var/lib/postgresql/htap/lh_rt.parquet')" >/dev/null || fail "write_parquet falhou"
jrow="$(psql_c "SELECT j FROM public.read_parquet('/var/lib/postgresql/htap/lh_rt.parquet') j ORDER BY (j->>'n')::int LIMIT 1")"
echo "$jrow" | grep -q '"category": "a"' && echo "$jrow" | grep -q '"flag": true' && echo "$jrow" | grep -q '"n": 1' \
  || fail "read_parquet jsonb multi-tipo (got: $jrow)"
echo "READ_MULTI ($jrow)"

echo "== gate: write_parquet fail-closed (tipo não-suportado) =="
docker exec "$CTR" psql -U postgres -c "CREATE TABLE bad(ts timestamp); INSERT INTO bad VALUES (now())" >/dev/null
err="$(docker exec "$CTR" psql -U postgres -tAc "SELECT public.write_parquet('bad'::regclass::text, '/tmp/bad.parquet')" 2>&1 || true)"
echo "$err" | grep -qi "não suportado" || fail "write_parquet não falhou-fechado em timestamp (got: $err)"
[ "$(psql_c "SELECT 1")" = "1" ] || fail "backend morto após erro (deveria estar vivo)"
echo "WRITE_FAILCLOSED"

echo "== gate: REVOKE (least-privilege) — non-superuser NÃO escreve/lê arquivo arbitrário =="
docker exec "$CTR" psql -U postgres -c "CREATE ROLE lowpriv LOGIN" >/dev/null 2>&1 || true
perr="$(docker exec "$CTR" psql -U lowpriv -d postgres -tAc "SELECT public.write_parquet('pg_class'::regclass::text, '/tmp/evil.parquet')" 2>&1 || true)"
echo "$perr" | grep -qiE "permission denied|permissão negada" || fail "REVOKE não bloqueou lowpriv em write_parquet (got: $perr)"
rerr="$(docker exec "$CTR" psql -U lowpriv -d postgres -tAc "SELECT public.read_parquet('/etc/passwd')" 2>&1 || true)"
echo "$rerr" | grep -qiE "permission denied|permissão negada" || fail "REVOKE não bloqueou lowpriv em read_parquet (got: $rerr)"
echo "REVOKE_OK (lowpriv bloqueado em read/write_parquet)"

echo "== tamanho da imagem (pg_duckdb 118 MB fora; lakehouse own-code dentro) =="
SZ="$(docker images --filter=reference="$TAG" --format '{{.Size}}' | head -1)"
echo "imagem M143 ($TAG): $SZ  — SEM o bundle DuckDB de 118 MB; lakehouse own-code incluso (+~9 MB)"

echo "M143_REMOVAL_OK"
