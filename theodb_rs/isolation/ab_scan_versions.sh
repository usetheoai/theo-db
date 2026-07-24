#!/usr/bin/env bash
# M147 — A/B byte-idêntico do scan IVF através das 6 versões de formato (v3..v8).
#
# POR QUE ESTE ARQUIVO EXISTE. O M147 refatora o dispatch de versão de `am/scan.rs` (if-ladder → enum),
# os gathers (Vec → Result+?) e o kernel Stage-1 (compartilhado) — com COMPORTAMENTO BYTE-IDÊNTICO. A única
# prova possível dessa propriedade é construir um índice em CADA versão, rodar as MESMAS queries no binário
# baseline (pré-refactor) e no binário novo, e assertar que o top-k retornado (ctid + distância arredondada) é
# idêntico. Metodologia A/B do M145. O v3 está incluído (EC-1 do edge-case-plan): o v3 grava discriminante 3u32
# mas o if-ladder o trata como fallback, então o refactor de dispatch o afeta.
#
# MODOS:
#   capture <arquivo>  — roda as queries no binário INSTALADO e grava os resultados em <arquivo> (o baseline).
#   compare <arquivo>  — roda as mesmas queries no binário INSTALADO e diffa contra <arquivo>. Exit 1 se diferir.
#
# O baseline (EC-4) é capturado do binário pré-refactor e committado; o compare roda contra o binário novo.
# Padrão "corpus versionado" do lance (test_data/ + assert de proveniência) — reproduzível, sem dois binários vivos.
#
# Exit 0 = capture OK, OU compare sem diferença. Exit 1 = compare acusou divergência (comportamento mudou).
set -uo pipefail

MODE="${1:?uso: ab_scan_versions.sh <capture|compare> <arquivo>}"
FILE="${2:?falta o arquivo de baseline}"
PGINST="${PGINST:-$HOME/.pgrx/18.4/pgrx-install}"
export PATH="$PGINST/bin:$PATH"
DATA=$(mktemp -d /tmp/ab_scan.XXXXXX)
PORT="${PORT:-$(( 25000 + RANDOM % 20000 ))}"
DB=postgres

cleanup() { pg_ctl -D "$DATA" -m immediate stop -w >/dev/null 2>&1 || true; rm -rf "$DATA"; }
trap cleanup EXIT

initdb -D "$DATA" -U theo >/dev/null 2>&1 || { echo "AB_FAIL initdb"; exit 2; }
{ echo "port=$PORT"; echo "shared_preload_libraries='theodb_rs'"; echo "autovacuum=off"; } >> "$DATA/postgresql.conf"
pg_ctl -D "$DATA" -l "$DATA/log" start -w >/dev/null || { echo "AB_FAIL start"; cat "$DATA/log"; exit 2; }

q() { psql -X -q -p "$PORT" -U theo -d "$DB" -tAc "$1" 2>&1; }

q "CREATE EXTENSION theodb_rs CASCADE;" >/dev/null

# Dataset DETERMINÍSTICO: 2000 vetores dim-8 gerados por um hash puro de (g, d) — SEM random(). O `setseed` +
# `random()` num agregado tem ORDEM DE AVALIAÇÃO não-determinística (o mesmo binário produz corpus diferente
# entre runs — medido), o que inviabiliza qualquer A/B byte-idêntico. Um hash de Knuth por (g,d) é reprodutível
# e não-colinear (variedade suficiente para 16 listas IVF). + coluna de labels smallint[] para o v7.
q "CREATE TABLE t (id int, e vector(8), labels smallint[]);" >/dev/null
q "INSERT INTO t
   SELECT g,
          ('[' || string_agg((((g*2654435761 + d*2246822519 + 1013904223) % 10007) % 1000)::numeric::text, ',' ORDER BY d) || ']')::vector(8),
          ARRAY[(g % 5)::smallint, ((g+1) % 7)::smallint]
   FROM generate_series(1,2000) g, LATERAL generate_series(1,8) d
   GROUP BY g;" >/dev/null

# As 5 queries fixas (vetores de consulta determinísticos) + o k.
K=10
declare -a QUERIES=(
  "[10,20,30,40,50,60,70,80]"
  "[5,5,5,5,5,5,5,5]"
  "[99,1,99,1,99,1,99,1]"
  "[50,50,50,50,50,50,50,50]"
  "[1,2,3,4,5,6,7,8]"
)
# label de consulta para o v7 (overlap com o corpus).
QLABELS="{0,1}"

# Cada versão: (nome, colunas do índice, WITH-clause).
run_version() {  # $1=nome  $2=idxcols  $3=withclause
  local name="$1" cols="$2" with="$3"
  q "DROP INDEX IF EXISTS t_idx;" >/dev/null
  local out
  out=$(q "CREATE INDEX t_idx ON t USING theodb_ivfflat ($cols) $with;")
  if ! q "SELECT to_regclass('t_idx') IS NOT NULL;" | grep -q "^t$"; then
    echo "AB_FAIL create index $name: $out"; exit 2
  fi
  local qi=0
  for qv in "${QUERIES[@]}"; do
    qi=$((qi+1))
    # top-k: ctid + distância arredondada a 4 casas (a ordem é a saída do scan). enable_seqscan=off força o índice.
    local res
    res=$(q "SET enable_seqscan=off; SET enable_indexscan=on;
             SELECT string_agg(id || ':' || round((e <-> '$qv'::vector)::numeric, 4)::text, ',' ORDER BY (e <-> '$qv'::vector), id)
             FROM (SELECT id, e FROM t ORDER BY e <-> '$qv'::vector LIMIT $K) s;")
    echo "$name|q$qi|$res"
  done
}

emit_all() {
  run_version "v3"  "e"          "WITH (lists=16)"
  run_version "v4"  "e"          "WITH (lists=16, pq_subspaces=4, aq_threshold=1500)"
  run_version "v5"  "e"          "WITH (lists=16, pq_subspaces=4, aq_threshold=1500, separate_storage=1)"
  run_version "v6"  "e"          "WITH (lists=16, pq_subspaces=4, aq_threshold=1500, separate_storage=1, refine=1)"
  run_version "v7"  "e, labels"  "WITH (lists=16, pq_subspaces=4, aq_threshold=1500, separate_storage=1)"
  run_version "v8"  "e"          "WITH (lists=16, pq_subspaces=4, aq_threshold=1500, separate_storage=1, refine=2)"
}

RESULTS=$(emit_all)
# sanity: cada versão produziu 5 linhas não-vazias
NLINES=$(echo "$RESULTS" | grep -c "|q")
if [ "$NLINES" -ne 30 ]; then
  echo "AB_FAIL esperava 30 linhas (6 versões × 5 queries), obteve $NLINES:"; echo "$RESULTS" | head; exit 2
fi

if [ "$MODE" = "capture" ]; then
  echo "$RESULTS" > "$FILE"
  echo "AB_CAPTURE_OK — baseline de 6 versões × 5 queries gravado em $FILE"
  exit 0
elif [ "$MODE" = "compare" ]; then
  if [ ! -f "$FILE" ]; then echo "AB_FAIL baseline $FILE não existe (rode capture primeiro)"; exit 2; fi
  # ignora o cabeçalho de proveniência (linhas '#') do baseline; diffa só as linhas de dados.
  DIFF=$(diff <(grep -v '^#' "$FILE") <(echo "$RESULTS") || true)
  if [ -z "$DIFF" ]; then
    echo "AB_COMPARE_OK — os 6 caminhos (v3..v8) byte-idênticos ao baseline"
    exit 0
  else
    echo "AB_COMPARE_FAIL — o comportamento MUDOU em ao menos um caminho:"
    echo "$DIFF" | head -20
    exit 1
  fi
else
  echo "AB_FAIL modo desconhecido: $MODE (use capture|compare)"; exit 2
fi
