#!/usr/bin/env bash
# M148 (#181) — teste de regressão do SHIM de compatibilidade pgvector.
#
# O bug: `CREATE EXTENSION IF NOT EXISTS vector` — o primeiro comando do bootstrap de toda app pgvector
# (theo-memory `db:push`, theo-rag, drizzle/alembic/prisma) — falhava com "extension vector is not
# available", impedindo QUALQUER app de subir contra o TheoDB (bloqueava o dogfood do M141). O tipo
# `public.vector` e os operadores já eram drop-in (ADR-0029 § D2); faltava o objeto de extensão nominal.
#
# Este harness reproduz o cenário REAL: uma app pgvector nova aponta para um TheoDB LIMPO e roda seu
# bootstrap padrão. Falhar aqui = o drop-in regrediu e nenhuma app theo-data sobe.
#
# Uso: pgvector_compat_check.sh    — imprime PGVECTOR_COMPAT_OK / PGVECTOR_COMPAT_FAIL.
# Exit: 0 = OK, 1 = regressão de compatibilidade, 2 = falha de infraestrutura.
set -uo pipefail
PGINST="${PGINST:-$HOME/.pgrx/18.4/pgrx-install}"
export PATH="$PGINST/bin:$PATH"
DATA=$(mktemp -d /tmp/pgvcompat.XXXXXX); PORT="${PORT:-$(( 45000 + RANDOM % 9000 ))}"

cleanup() { pg_ctl -D "$DATA" -m immediate stop -w >/dev/null 2>&1 || true; rm -rf "$DATA"; }
trap cleanup EXIT

initdb -D "$DATA" -U theo >/dev/null 2>&1 || { echo "PGVECTOR_COMPAT_FAIL initdb"; exit 2; }
{ echo "port=$PORT"; echo "shared_preload_libraries='theodb_rs'"; } >> "$DATA/postgresql.conf"
pg_ctl -D "$DATA" -l "$DATA/log" start -w >/dev/null 2>&1 || { echo "PGVECTOR_COMPAT_FAIL start"; exit 2; }

psql -X -q -p "$PORT" -U theo -d postgres -c "CREATE DATABASE app_fresh" >/dev/null 2>&1 \
  || { echo "PGVECTOR_COMPAT_FAIL createdb"; exit 2; }

# O bootstrap REAL de uma app pgvector, num DB limpo — SQL em arquivo (zero quoting shell).
cat > "$DATA/compat.sql" <<'SQL'
-- 1. o primeiro comando do bootstrap de toda app pgvector (o que falhava — #181)
CREATE EXTENSION IF NOT EXISTS vector CASCADE;
-- 2. idempotência: o bootstrap roda de novo em todo deploy
CREATE EXTENSION IF NOT EXISTS vector;
-- 3. a app cria sua tabela e consulta, como faria de verdade
CREATE TABLE items (id int, embedding vector(3));
INSERT INTO items VALUES (1,'[1,1,1]'), (2,'[5,5,5]');
CREATE INDEX items_emb_idx ON items USING theodb_hnsw (embedding);
SQL
# ON_ERROR_STOP=1 é obrigatório: sem ele o psql -f segue após um erro SQL e sai 0 — o harness
# reportaria "bootstrap ok" sobre um CREATE EXTENSION que falhou (falso verde).
psql -X -q -v ON_ERROR_STOP=1 -p "$PORT" -U theo -d app_fresh -f "$DATA/compat.sql" >"$DATA/out" 2>&1 || {
  echo "PGVECTOR_COMPAT_FAIL bootstrap"; sed -n '1,6p' "$DATA/out"; exit 1; }

q() { psql -X -q -p "$PORT" -U theo -d app_fresh -tAc "$1" 2>&1; }

# Asserção 1 — ambas as extensões presentes (o CASCADE puxou o theodb_rs sozinho).
EXTS=$(q "SELECT string_agg(extname, ',' ORDER BY extname) FROM pg_extension WHERE extname IN ('vector','theodb_rs');")
[ "$EXTS" = "theodb_rs,vector" ] || { echo "PGVECTOR_COMPAT_FAIL extensions=$EXTS"; exit 1; }

# Asserção 2 — o tipo é o own-code em public (não um pgvector reintroduzido).
TYPE_NS=$(q "SELECT n.nspname FROM pg_type t JOIN pg_namespace n ON n.oid=t.typnamespace WHERE t.typname='vector';")
[ "$TYPE_NS" = "public" ] || { echo "PGVECTOR_COMPAT_FAIL type_schema=$TYPE_NS"; exit 1; }

# Asserção 3 — a distância é CORRETA, não só "não deu erro": |[1,1,1]-[2,2,2]| = sqrt(3) = 1.7321.
DIST=$(q "SELECT round((embedding <-> '[2,2,2]'::vector)::numeric,4) FROM items WHERE id=1;")
[ "$DIST" = "1.7321" ] || { echo "PGVECTOR_COMPAT_FAIL dist=$DIST (esperado 1.7321)"; exit 1; }

# Asserção 4 — o índice ANN existe (a app indexa sua coluna).
IDX=$(q "SELECT count(*) FROM pg_indexes WHERE tablename='items' AND indexname='items_emb_idx';")
[ "$IDX" = "1" ] || { echo "PGVECTOR_COMPAT_FAIL index_count=$IDX"; exit 1; }

# Asserção 5 (HONESTIDADE) — o comment declara que a implementação é own-code, não pgvector.
COMMENT=$(q "SELECT obj_description(oid,'pg_extension') FROM pg_extension WHERE extname='vector';")
case "$COMMENT" in
  *"NOT by pgvector"*) : ;;
  *) echo "PGVECTOR_COMPAT_FAIL comment nao declara own-code: $COMMENT"; exit 1 ;;
esac

echo "PGVECTOR_COMPAT_OK — bootstrap pgvector sobe em DB limpo (CASCADE), tipo own-code public.vector, dist=$DIST, indice ANN, comment honesto"
