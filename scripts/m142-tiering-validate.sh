#!/usr/bin/env bash
# M142 tiering validation — prova o tier-out do pg_duckdb contra as DUAS imagens reais (default + theodb-htap).
#
# Gates (todos honestos — falha exit≠0 se qualquer um falhar):
#   BUILD    — as duas imagens buildam.
#   DEFAULT  — pg_extension SEM pg_duckdb; shared_preload SEM ele; theodb_rs+vector+theodb_columnar verdes;
#              o guard M142 (theodb.olap_sql) RAISE SQLSTATE 0A000; htap_guard_test.sql verde (caminho guard).
#   HTAP     — pg_extension COM pg_duckdb; theodb.htap_refresh_sql/olap_sql produzem e o cliente executa e2e;
#              htap_guard_test.sql verde (caminho positivo).
#   DELTA    — size(htap) - size(default) >= 150 MB (o único componente C++/DuckDB + libcurl4).
#
# Uso (no droplet com Docker, a partir da raiz do repo):
#   bash scripts/m142-tiering-validate.sh
# Env: DEFAULT_TAG (default theodb:m142-default), HTAP_TAG (default theodb:m142-htap), SKIP_BUILD=1 p/ reusar tags.
set -euo pipefail

DEFAULT_TAG="${DEFAULT_TAG:-theodb:m142-default}"
HTAP_TAG="${HTAP_TAG:-theodb:m142-htap}"
MIN_DELTA_MB="${MIN_DELTA_MB:-150}"
BENCH_DOC="docs/benchmarks/m142-pgduckdb-tiering.md"

fail() { echo "FAIL: $1" >&2; exit 1; }
psql_d() { docker exec "$1" psql -U postgres -tAc "$2"; }

wait_healthy() {
  local ctr="$1"
  for _ in $(seq 1 60); do
    [ "$(docker inspect -f '{{.State.Health.Status}}' "$ctr" 2>/dev/null)" = healthy ] && return 0
    sleep 2
  done
  docker logs "$ctr" 2>&1 | tail -30 >&2
  fail "$ctr não ficou healthy (init quebrou?)"
}

# ── BUILD ──────────────────────────────────────────────────────────────────────────────────────
if [ "${SKIP_BUILD:-0}" != "1" ]; then
  echo "== BUILD default ($DEFAULT_TAG) =="
  docker build -t "$DEFAULT_TAG" . || fail "build da imagem default falhou"
  echo "== BUILD htap ($HTAP_TAG, FROM $DEFAULT_TAG) =="
  docker build -f packaging/Dockerfile.htap --build-arg THEODB_BASE="$DEFAULT_TAG" -t "$HTAP_TAG" . \
    || fail "build da imagem htap falhou"
fi

# ── DEFAULT smoke ──────────────────────────────────────────────────────────────────────────────
CTR_D="theodb-m142-default-smoke"
docker rm -f "$CTR_D" >/dev/null 2>&1 || true
docker run -d --name "$CTR_D" -e POSTGRES_PASSWORD=postgres "$DEFAULT_TAG" >/dev/null
trap 'docker rm -f "$CTR_D" theodb-m142-htap-smoke >/dev/null 2>&1 || true' EXIT
wait_healthy "$CTR_D"

echo "== DEFAULT smoke =="
[ "$(psql_d "$CTR_D" "SELECT count(*) FROM pg_extension WHERE extname='pg_duckdb'")" = "0" ] \
  || fail "default NÃO deveria ter pg_duckdb"
# captura-depois-assere (não mascarar falha do psql com `&& fail || true`)
SP_D="$(psql_d "$CTR_D" "SHOW shared_preload_libraries")"
if echo "$SP_D" | grep -q pg_duckdb; then fail "default NÃO deveria ter pg_duckdb no shared_preload_libraries (got: $SP_D)"; fi
# theodb_rs + tipo vector own-code
[ "$(psql_d "$CTR_D" "SELECT count(*) FROM pg_extension WHERE extname IN ('theodb','theodb_rs')")" = "2" ] \
  || fail "default: theodb/theodb_rs ausentes"
psql_d "$CTR_D" "SELECT '[1,2,3]'::vector <-> '[1,2,4]'::vector" >/dev/null \
  || fail "default: tipo vector own-code não funciona"
# theodb_columnar own-code (TableAM)
docker exec "$CTR_D" psql -U postgres -c \
  "CREATE TABLE m142c(id int, category text) USING theodb_columnar; INSERT INTO m142c VALUES (1,'a'),(2,'b')" >/dev/null \
  || fail "default: theodb_columnar TableAM não funciona"
[ "$(psql_d "$CTR_D" "SELECT count(*) FROM m142c")" = "2" ] || fail "default: columnar readback falhou"
# guard M142 — olap_sql RAISE 0A000 sem pg_duckdb
docker exec "$CTR_D" psql -U postgres -c "CREATE TABLE m142g(category text, amount numeric)" >/dev/null
guard_state="$(docker exec "$CTR_D" psql -U postgres -tAc \
  "DO \$\$ BEGIN PERFORM theodb.olap_sql('m142g'::regclass); RAISE EXCEPTION 'NO_GUARD'; EXCEPTION WHEN sqlstate '0A000' THEN RAISE NOTICE 'GUARD_OK'; END \$\$" 2>&1 || true)"
echo "$guard_state" | grep -q "GUARD_OK" || fail "default: guard M142 não disparou 0A000 (got: $guard_state)"
# htap_guard_test.sql (caminho guard). Gate no EXIT CODE do psql (ON_ERROR_STOP=1 → assert falho = exit≠0),
# robusto a supressão de NOTICE; imprime o output em caso de falha.
docker cp sql/tests/htap_guard_test.sql "$CTR_D":/tmp/g.sql
if ! docker exec "$CTR_D" psql -U postgres -v ON_ERROR_STOP=1 -f /tmp/g.sql > /tmp/m142_g_default.out 2>&1; then
  echo "--- guard test output (default) ---"; cat /tmp/m142_g_default.out; fail "default: htap_guard_test.sql falhou (caminho guard)"
fi
echo "DEFAULT_OK"

# ── HTAP smoke ─────────────────────────────────────────────────────────────────────────────────
CTR_H="theodb-m142-htap-smoke"
docker rm -f "$CTR_H" >/dev/null 2>&1 || true
docker run -d --name "$CTR_H" -e POSTGRES_PASSWORD=postgres "$HTAP_TAG" >/dev/null
wait_healthy "$CTR_H"

echo "== HTAP smoke =="
[ "$(psql_d "$CTR_H" "SELECT count(*) FROM pg_extension WHERE extname='pg_duckdb'")" = "1" ] \
  || fail "htap: pg_duckdb ausente"
psql_d "$CTR_H" "SHOW shared_preload_libraries" | grep -q pg_duckdb || fail "htap: pg_duckdb não no preload"
# segurança (defense-in-depth): community extensions do DuckDB DEVEM estar off (código não-auditado). Gate que o
# m61-smoke tinha e ficou órfão no tier-out (M142 security review).
psql_d "$CTR_H" "SHOW duckdb.allow_community_extensions" | grep -qi off \
  || fail "htap: duckdb.allow_community_extensions não é off (segurança)"
# M62 e2e: refresh (COPY parquet) → register → olap (duckdb.query) executado pelo cliente
# amount = double precision (não numeric sem precisão): o COPY→Parquet do pg_duckdb rejeita NUMERIC sem precisão
# ("DuckDB requires the precision of a NUMERIC to be set") — a mesma escolha do harness M61/M62. É limitação
# pré-existente da superfície M62, não do tier-out.
docker exec "$CTR_H" psql -U postgres -c \
  "CREATE TABLE m142h(category text, amount double precision); INSERT INTO m142h VALUES ('a',10),('a',20),('b',5)" >/dev/null
COPY_SQL="$(psql_d "$CTR_H" "SELECT theodb.htap_refresh_sql('m142h'::regclass)")"
echo "$COPY_SQL" | grep -q "FORMAT parquet" || fail "htap: htap_refresh_sql não retornou COPY parquet"
docker exec "$CTR_H" psql -U postgres -c "$COPY_SQL" >/dev/null || fail "htap: COPY parquet (cliente) falhou"
PARQUET_PATH="$(psql_d "$CTR_H" "SELECT theodb._htap_path('m142h'::regclass)")"
psql_d "$CTR_H" "SELECT theodb.htap_register('m142h'::regclass, '$PARQUET_PATH')" >/dev/null
OLAP_SQL="$(psql_d "$CTR_H" "SELECT theodb.olap_sql('m142h'::regclass)")"
echo "$OLAP_SQL" | grep -q "duckdb.query" || fail "htap: olap_sql não retornou duckdb.query"
rows="$(docker exec "$CTR_H" psql -U postgres -tAc "$OLAP_SQL" 2>&1 || true)"
echo "$rows" | grep -q "^a|2|15" || fail "htap: olap e2e linha 'a' inesperada (got: $rows)"
echo "$rows" | grep -q "^b|1|5"  || fail "htap: olap e2e linha 'b' inesperada (got: $rows)"
# htap_guard_test.sql (caminho positivo). Gate no EXIT CODE (robusto); imprime output em caso de falha.
docker cp sql/tests/htap_guard_test.sql "$CTR_H":/tmp/g.sql
if ! docker exec "$CTR_H" psql -U postgres -v ON_ERROR_STOP=1 -f /tmp/g.sql > /tmp/m142_g_htap.out 2>&1; then
  echo "--- guard test output (htap) ---"; cat /tmp/m142_g_htap.out; fail "htap: htap_guard_test.sql falhou (caminho positivo)"
fi
echo "HTAP_OK"

# ── DELTA de tamanho ───────────────────────────────────────────────────────────────────────────
# size_mb: usa `docker images` (ground truth "712MB"/"1.2GB") — o `docker image inspect .Size` reporta valor
# divergente neste Docker. Converte GB/MB/kB para MB.
size_mb() {
  docker images --filter=reference="$1" --format '{{.Size}}' | head -1 | awk '
    /GB/{printf "%d", $1*1024; next}
    /MB/{printf "%d", $1; next}
    /kB|KB/{printf "%d", $1/1024; next}
    {printf "%d", $1/1024/1024}'
}
D_MB="$(size_mb "$DEFAULT_TAG")"; H_MB="$(size_mb "$HTAP_TAG")"; DELTA=$((H_MB - D_MB))
echo "== DELTA: default=${D_MB}MB htap=${H_MB}MB delta=${DELTA}MB (min=${MIN_DELTA_MB}) =="
[ "$DELTA" -ge "$MIN_DELTA_MB" ] || fail "delta ${DELTA}MB < ${MIN_DELTA_MB}MB (o tier-out não enxugou o esperado)"

# tamanho do pg_duckdb.so (medido no harness — proveniência reprodutível, M142 benchmark review M1)
SO_BYTES="$(docker run --rm --entrypoint bash "$HTAP_TAG" -lc 'stat -c%s /usr/lib/postgresql/*/lib/pg_duckdb.so' 2>/dev/null | head -1 || echo 0)"
SO_MB=$(( SO_BYTES / 1024 / 1024 ))

# ── registra a evidência medida ─────────────────────────────────────────────────────────────────
mkdir -p "$(dirname "$BENCH_DOC")"
{
  echo "<!-- M142_MEASURED_BLOCK (gerado por scripts/m142-tiering-validate.sh) -->"
  echo "- default (\`$DEFAULT_TAG\`): **${D_MB} MB**"
  echo "- theodb-htap (\`$HTAP_TAG\`): **${H_MB} MB**"
  echo "- **delta: ${DELTA} MB** (≥ ${MIN_DELTA_MB} MB — o bundle DuckDB estático + libcurl4)"
  echo "- \`pg_duckdb.so\` (bundle DuckDB estático): **${SO_MB} MB** (${SO_BYTES} bytes, \`stat\` no harness)"
} > /tmp/m142_measured.txt
echo "medidas em /tmp/m142_measured.txt (colar em $BENCH_DOC)"

echo "M142_TIERING_OK"
