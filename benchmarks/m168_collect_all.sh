#!/bin/bash
# M168 — roda TODA a evidência do milestone num único binário, com cabeçalho de proveniência em cada log.
#
# WHY. Uma revisão encontrou o verdict nomeando um `so_md5` que nenhum artefato carregava, com a tabela de memória
# tirada de um binário e a de throughput de outro (mais antigo, anterior ao fail-open), e três dos cinco logs sem
# cabeçalho algum — enquanto o documento mandava "ver o cabeçalho de cada artefato". Não havia binário único sobre
# o qual todos os números publicados tivessem sido tomados.
#
# Este script existe para que isso não dependa de eu lembrar: um `so_md5`, um `postmaster`, um cabeçalho por
# arquivo, tudo numa passada. Ele imprime o md5 no fim para o verdict copiar.
set -uo pipefail

P="${PGRX_BIN:-/root/.pgrx/18.4/pgrx-install/bin}"
PGPORT="${PGPORT:-28900}"
PGUSER="${PGUSER:-postgres}"
PGDATABASE="${PGDATABASE:-postgres}"
RUNAS="${RUNAS:-pgtest}"
BENCH="${BENCH:-$(cd "$(dirname "$0")" && pwd)}"
OUT="${OUT:-/root/m168-artifacts}"

as_pg() { su - "$RUNAS" -c "$*"; }

SO_PATH=$(find "$(dirname "$P")" -name theodb_rs.so 2>/dev/null | head -1)
SO_MD5=$(md5sum "$SO_PATH" 2>/dev/null | cut -d' ' -f1)
if [ -z "$SO_MD5" ]; then
  echo "FATAL: não consegui somar o .so — artefato sem proveniência não é evidência"
  exit 2
fi
PM=$(as_pg "$P/psql -h localhost -p $PGPORT -U $PGUSER -d $PGDATABASE -tAc \"SELECT pg_postmaster_start_time()\"" 2>/dev/null)
mkdir -p "$OUT"

# nome | sql | psql extra | repetições
run() {
  local name=$1 sql=$2 extra=$3 reps=${4:-1}
  local f="$OUT/$name.log"
  {
    for i in $(seq 1 "$reps"); do
      echo "=== $name (execução $i/$reps) start $(date -Is) ==="
      echo "so_md5=$SO_MD5"
      echo "so_path=$SO_PATH"
      echo "postmaster=$PM"
      as_pg "$P/psql -h localhost -p $PGPORT -U $PGUSER -d $PGDATABASE $extra -f $sql" 2>&1
      echo "=== $name (execução $i/$reps) rc=$? end $(date -Is) ==="
    done
  } > "$f" 2>&1
  printf '  %-28s -> %s\n' "$name" "$f"
}

echo "so_md5=$SO_MD5"
echo "postmaster=$PM"
echo
run peak-both-arms            "$BENCH/m168_peak.sql"        "" 1
run paired-ab-stream          "$BENCH/m168_stream_ab.sql"   "" 2
run pending-rows              "$BENCH/m168_pending_rows.sql" "" 1
run large-k                   "$BENCH/m168_large_k.sql"     "" 1
run window-probe              "$BENCH/m168_window_probe.sql" "" 1
run cancel-oracle             "$BENCH/m168_cancel_oracle.sql" "" 1
# O self-test do gate de cancelamento como ARTEFATO, não como afirmação. Uma revisão apontou que o verdict listava
# "self-test aborta" numa tabela de coisas medidas, mas o coletor nunca passava a flag — a linha era verificada por
# leitura de código, não por execução. Espera-se que ESTE log contenha um ERROR; é o ponto.
run cancel-oracle-selftest    "$BENCH/m168_cancel_oracle.sql" "-v gate_selftest=1" 1
run routing-shapes            "$BENCH/m168_routing_shapes.sql" "" 1

# A REGRESSÃO DO M167, como artefato. Uma revisão apontou que o verdict afirmava `rc=0` para os dois oráculos do
# M167 numa tabela em que TODAS as outras linhas têm artefato — e este coletor nunca os rodava. Rodar aqui é o que
# torna a linha verificável em vez de afirmada. Ele é um shell script (não um .sql), então não passa pelo `run`.
{
  echo "=== m167-regression start $(date -Is) ==="
  echo "so_md5=$SO_MD5"; echo "so_path=$SO_PATH"; echo "postmaster=$PM"
  ( cd "$(dirname "$BENCH")" && PGRX_BIN="$P" PGPORT="$PGPORT" bash "$BENCH/m167_run_oracles.sh" 2>&1 )
  echo "=== m167-regression rc=$? end $(date -Is) ==="
} > "$OUT/m167-regression.log" 2>&1
printf '  %-28s -> %s\n' "m167-regression" "$OUT/m167-regression.log"
run inversion-narrow          "$BENCH/m168_inversion_narrow.sql" "" 1
run inversion-wide-small-k    "$BENCH/m168_inversion.sql"    "" 1
echo
echo "Todos os logs acima carregam o MESMO so_md5=$SO_MD5 — é este que o verdict deve citar."
