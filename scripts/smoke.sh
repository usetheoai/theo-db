#!/usr/bin/env bash
# Smoke do produto: a imagem sobe, a extensão instala, e as capacidades que ela promete respondem.
#
# REESCRITO em 2026-08-13 (B-029), e não restaurado. O `smoke.sh` que saiu em `8605677` fazia
# `CREATE EXTENSION theodb` três vezes — o umbrella que o B-030 removeu. Medido no `theodb:b036`,
# `pg_available_extensions` lista `theodb_rs` e `vector`, e mais nada. Restaurá-lo daria um smoke
# que falha na primeira linha, contra um produto que está correto: o pior tipo de vermelho, porque
# acusa o lugar errado.
#
# O que este smoke afirma, e por que cada afirmação está aqui:
#
#   1. a extensão INSTALA           — sem isso nada mais é verificável
#   2. o tipo `vector` OPERA         — é a superfície que o pgvector expõe e que prometemos aceitar
#   3. o AM `theodb_hnsw` INDEXA     — presença da extensão não é presença da capacidade
#   4. o alias `hnsw` INDEXA         — é a sintaxe que toda app pgvector escreve (ADR-0058)
#   5. a consulta DEVOLVE LINHA      — zero resultado é indistinguível de "nada casou", que é a
#                                      classe do B-041; um smoke que aceita vazio não é oráculo
#
# É deliberadamente um smoke, não uma suíte: ele responde "o artefato está vivo e faz o que diz",
# em segundos. Correção fina é `cargo pgrx test` (rust-suite.yml); recall é benchmark.
#
# Uso: PGHOST/PGPORT/PGUSER/PGPASSWORD apontando para um servidor já de pé.
set -euo pipefail

HOST="${PGHOST:-localhost}"
PORT="${PGPORT:-5432}"
USER="${PGUSER:-postgres}"
PGPASSWORD="${PGPASSWORD:-postgres}"
export PGPASSWORD

die() { echo "SMOKE FAILED: $*" >&2; exit 1; }

# Cliente psql: do host quando existe, senão num contêiner na mesma rede. Sem isto o smoke exige
# `postgresql-client` instalado, e falharia por ausência de FERRAMENTA reportando defeito de PRODUTO.
if ! command -v psql >/dev/null 2>&1 || ! command -v pg_isready >/dev/null 2>&1; then
  IMG="${SMOKE_CLIENT_IMAGE:-postgres:18-bookworm}"
  command -v docker >/dev/null 2>&1 || die "sem psql no host e sem docker para rodar um — instale postgresql-client"
  echo "nota: sem cliente PostgreSQL no host — usando $IMG na rede do host"
  psql()       { docker run --rm -i --network host -e PGPASSWORD "$IMG" psql "$@"; }
  pg_isready() { docker run --rm    --network host -e PGPASSWORD "$IMG" pg_isready "$@"; }
fi

q() { psql -h "$HOST" -p "$PORT" -U "$USER" -t -A -q -v ON_ERROR_STOP=1 "$@"; }

# Prontidão real: `pg_isready` pode dizer "pronto" durante a janela do servidor temporário do initdb.
for _ in $(seq 1 20); do
  pg_isready -h "$HOST" -p "$PORT" -U "$USER" -q && q -c 'SELECT 1' >/dev/null 2>&1 && break
  sleep 1
done
q -c 'SELECT 1' >/dev/null 2>&1 || die "servidor não aceita consultas em $HOST:$PORT após 20s"

# ---- 1. a extensão instala ----------------------------------------------------------------
VER=$(q <<'SQL' || true
CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE;
SELECT extversion FROM pg_extension WHERE extname = 'theodb_rs';
SQL
)
[ -n "$VER" ] || die "extensão 'theodb_rs' não instalou (este endereço serve TheoDB?)"
echo "ok  theodb_rs instalada (v$VER)"

q -c "CREATE EXTENSION IF NOT EXISTS vector CASCADE" >/dev/null || die "shim 'vector' não instalou"
echo "ok  shim vector instalado"

# ---- 2. o tipo vector opera --------------------------------------------------------------
DIST=$(q -c "SELECT round(('[1,2,3]'::vector <=> '[4,5,6]'::vector)::numeric, 6)") \
  || die "o operador <=> do tipo vector não respondeu"
[ -n "$DIST" ] || die "o operador <=> devolveu vazio"
echo "ok  vector <=> vector = $DIST"

# ---- 3, 4 e 5. os AMs indexam e a busca devolve linha -------------------------------------
# Os dois nomes de AM são exercitados de propósito: `theodb_hnsw` é o nosso, e `hnsw` é o alias
# que TODA aplicação pgvector escreve. Um smoke que só testasse o nome próprio deixaria passar
# uma quebra no alias — que é justamente o caminho do consumidor real (ver B-021).
for AM in theodb_hnsw hnsw; do
  case "$AM" in
    theodb_hnsw) OPS="theodb_hnsw_l2_ops" ;;
    hnsw)        OPS="vector_l2_ops" ;;
  esac
  TBL="smoke_${AM}"
  q >/dev/null <<SQL || die "CREATE INDEX USING $AM falhou"
DROP TABLE IF EXISTS $TBL;
CREATE TABLE $TBL (id int PRIMARY KEY, e vector(8));
INSERT INTO $TBL
  SELECT g, ('[' || array_to_string(array(SELECT ((g * 7 + j * 13) % 29) * 0.3 FROM generate_series(1, 8) j), ',') || ']')::vector
  FROM generate_series(1, 200) g;
CREATE INDEX ${TBL}_idx ON $TBL USING $AM (e $OPS);
SQL

  HITS=$(q <<SQL
SET enable_seqscan = off;
SELECT count(*) FROM (SELECT id FROM $TBL ORDER BY e <-> '[3,3,3,3,3,3,3,3]'::vector LIMIT 10) s;
SQL
)
  # Zero linha aqui não é "nada casou": há 200 linhas na tabela. É a classe do B-041 — silêncio
  # indistinguível de resultado —, e é o caso que este smoke existe para barrar.
  [ "$HITS" = "10" ] || die "busca por $AM devolveu $HITS linhas de 10 esperadas (índice construído sobre 200 linhas)"
  echo "ok  $AM: CREATE INDEX + busca devolveu $HITS linhas"

  q -c "DROP TABLE $TBL" >/dev/null
done

echo "SMOKE PASSED — extensão instalada, tipo operante, os dois nomes de AM indexam e a busca devolve linha."
