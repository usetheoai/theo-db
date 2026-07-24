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

# Reproduz o initdb da imagem (Dockerfile /docker-entrypoint-initdb.d): a dependência vai em `template1`
# para que TODO banco criado depois a herde já satisfeita — sem isso o `CREATE EXTENSION vector` sem
# CASCADE (o que o tooling real emite) falha em qualquer banco novo. É o fix da finding HIGH do review.
psql -X -q -v ON_ERROR_STOP=1 -p "$PORT" -U theo -d template1 \
  -c "CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE;" \
  -c "CREATE EXTENSION IF NOT EXISTS vector;" >/dev/null 2>&1 \
  || { echo "PGVECTOR_COMPAT_FAIL template1_bootstrap"; exit 1; }

# TEMPLATE template0: banco VERDADEIRAMENTE limpo (não herda o template1 acima), para que o teste do
# CASCADE abaixo exercite a instalação de fato, e não um no-op sobre extensão já herdada.
psql -X -q -p "$PORT" -U theo -d postgres -c "CREATE DATABASE app_fresh TEMPLATE template0" >/dev/null 2>&1 \
  || { echo "PGVECTOR_COMPAT_FAIL createdb"; exit 2; }

# ESCOPO HONESTO (#181, não #182): este harness cobre o BOOTSTRAP + o tipo, NÃO o drop-in completo.
# A criação de índice abaixo usa `USING theodb_hnsw` — a nomenclatura do TheoDB, NÃO a que uma app
# pgvector escreve (`USING hnsw (embedding vector_cosine_ops)`, que ainda falha: #182). Não confunda o
# verde daqui com "app pgvector roda inteira": a migration real do theo-memory ainda quebra no CREATE
# INDEX. Quando #182 fechar, a asserção 4 DEVE virar `USING hnsw (embedding vector_cosine_ops)` — senão
# este teste continua verde sobre um drop-in quebrado.
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

# Asserção 6 — a versão declarada é o contrato que o tooling inspeciona (ADR-0058 § Decisão). Bumpar sem
# script de upgrade quebra toda instalação existente (a classe de defeito do M137) — travar aqui.
VER=$(q "SELECT extversion FROM pg_extension WHERE extname='vector';")
[ "$VER" = "0.5.1" ] || { echo "PGVECTOR_COMPAT_FAIL extversion=$VER (esperado 0.5.1; bumpar exige sql/vector--0.5.1--X.sql)"; exit 1; }

# Asserção 7 — o caminho que o tooling REAL emite: `CREATE EXTENSION IF NOT EXISTS vector` SEM CASCADE,
# num banco criado DEPOIS do bootstrap da imagem. É o caso que o review pegou como falso-verde: com
# `requires` e sem a dependência pré-instalada em template1, isto falha com
# `required extension "theodb_rs" is not installed` e a app não sobe.
psql -X -q -p "$PORT" -U theo -d postgres -c "CREATE DATABASE app_sem_cascade TEMPLATE template1" >/dev/null 2>&1
NOCASC=$(psql -X -q -v ON_ERROR_STOP=1 -p "$PORT" -U theo -d app_sem_cascade \
  -c "CREATE EXTENSION IF NOT EXISTS vector;" 2>&1) || {
  echo "PGVECTOR_COMPAT_FAIL sem_cascade: $NOCASC"; exit 1; }

echo "PGVECTOR_COMPAT_OK — bootstrap pgvector (com e SEM cascade), tipo own-code public.vector, dist=$DIST, extversion=$VER, comment honesto. ESCOPO: #181 (bootstrap+tipo); indices USING hnsw/vector_*_ops ainda falham (#182)"
