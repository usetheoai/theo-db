#!/usr/bin/env bash
# M146 T1.3 — prova de CRASH da durabilidade do export Parquet (`theodb.write_parquet`).
#
# POR QUE ESTE ARQUIVO EXISTE. O M146 trocou a escrita "atômica" (temp + rename) por "atômica E DURÁVEL"
# (temp → fsync do arquivo → rename → fsync do diretório-pai), seguindo o protocolo `durable_rename` do
# PostgreSQL (`src/backend/storage/file/fd.c:782`). O plano exigia a prova sob crash em quatro pontos, e o
# `/review` (F-xval-2) apontou corretamente que ela NUNCA foi executada: havia só evidência de caminho feliz
# (header/footer `PAR1`, sem temp órfão), enquanto o CHANGELOG já afirmava ao consumidor o comportamento sob
# crash. Uma alegação de durabilidade sem crash real é fé, não engenharia.
#
# O QUE ESTE HARNESS PODE E NÃO PODE PROVAR — honestidade primeiro:
#
#   PODE: que após um crash IMEDIATO do servidor (SIGQUIT via `pg_ctl -m immediate`) logo depois de o
#         `write_parquet` retornar, o arquivo publicado continua ÍNTEGRO e legível — magic `PAR1` no início
#         e no fim, e o `read_parquet` devolve as linhas gravadas. É a propriedade que o operador consome.
#
#   NÃO PODE: distinguir "o fsync salvou o arquivo" de "o page cache do SO ainda tinha os dados e o kernel
#         escreveu de qualquer forma". Provar isso exigiria derrubar a MÁQUINA (corte de energia / reset do
#         kernel), não o processo — um `pg_ctl -m immediate` mata o postgres, mas o page cache do Linux
#         sobrevive intacto. Um teste que não distingue os dois casos NÃO prova que o fsync é necessário;
#         prova apenas que o arquivo publicado é consistente e que o protocolo não corrompe nada.
#
# Essa limitação é intrínseca a qualquer harness que rode no mesmo kernel. Registrada aqui em vez de
# apresentada como prova completa (Regra 3). O argumento de que o fsync é NECESSÁRIO permanece o do upstream:
# o `durable_rename` do PostgreSQL existe exatamente porque rename sem fsync pode se perder num crash de
# máquina, e nós seguimos o mesmo protocolo, passo a passo.
#
# Exit 0 = arquivo íntegro e legível após o crash. Exit 1 = corrompido, truncado, ausente ou temp órfão.
set -uo pipefail

PGINST="${PGINST:-$HOME/.pgrx/18.4/pgrx-install}"
export PATH="$PGINST/bin:$PATH"
DATA=${DATA:-/tmp/crash_parquet_data}
PORT=${PORT:-59741}
OUTDIR=$(mktemp -d /tmp/crash_parquet.XXXXXX)
PQ="$OUTDIR/export.parquet"

cleanup() { pg_ctl -D "$DATA" -m immediate stop -w >/dev/null 2>&1 || true; }
trap cleanup EXIT
rm -rf "$DATA"

initdb -D "$DATA" -U theo >/dev/null 2>&1 || { echo "CRASH_PARQUET_FAIL initdb"; exit 1; }
{ echo "port=$PORT"; echo "shared_preload_libraries='theodb_rs'"; } >> "$DATA/postgresql.conf"
pg_ctl -D "$DATA" -l "$DATA/log" start -w >/dev/null || { echo "CRASH_PARQUET_FAIL start"; exit 1; }

q() { psql -X -q -p "$PORT" -U theo -d postgres -tAc "$1" 2>&1; }

q "CREATE EXTENSION theodb_rs CASCADE;" >/dev/null
q "CREATE TABLE pq (id int, name text);" >/dev/null
q "INSERT INTO pq SELECT g, 'row'||g FROM generate_series(1,5000) g;" >/dev/null

ROWS=$(q "SELECT public.write_parquet('pq', '$PQ');")
[ "$ROWS" = "5000" ] || { echo "CRASH_PARQUET_FAIL write_parquet devolveu '$ROWS' (esperado 5000)"; exit 1; }

# CRASH: `-m immediate` é um SIGQUIT — sem shutdown limpo, sem checkpoint final.
pg_ctl -D "$DATA" -m immediate stop -w >/dev/null 2>&1
echo "crash imediato executado logo após o write_parquet retornar"

# ------- as asserções, sobre o arquivo em disco, com o servidor MORTO -------
[ -f "$PQ" ] || { echo "CRASH_PARQUET_FAIL arquivo publicado desapareceu: $PQ"; exit 1; }
SIZE=$(stat -c %s "$PQ")
[ "$SIZE" -gt 0 ] || { echo "CRASH_PARQUET_FAIL arquivo publicado tem 0 bytes"; exit 1; }

HEAD4=$(head -c 4 "$PQ")
TAIL4=$(tail -c 4 "$PQ")
[ "$HEAD4" = "PAR1" ] || { echo "CRASH_PARQUET_FAIL magic inicial ausente (got '$HEAD4')"; exit 1; }
[ "$TAIL4" = "PAR1" ] || { echo "CRASH_PARQUET_FAIL magic FINAL ausente — arquivo truncado (got '$TAIL4')"; exit 1; }

ORPHAN=$(find "$OUTDIR" -name '*.tmp' | head -1)
[ -z "$ORPHAN" ] || { echo "CRASH_PARQUET_FAIL temp órfão sobreviveu ao crash: $ORPHAN"; exit 1; }

# ------- e o arquivo continua LEGÍVEL depois da recuperação -------
pg_ctl -D "$DATA" -l "$DATA/log2" start -w >/dev/null || { echo "CRASH_PARQUET_FAIL servidor não voltou"; exit 1; }
BACK=$(q "SELECT count(*) FROM public.read_parquet('$PQ');")
[ "$BACK" = "5000" ] || { echo "CRASH_PARQUET_FAIL read_parquet devolveu '$BACK' (esperado 5000)"; exit 1; }

echo "arquivo=${SIZE}B magic=PAR1/PAR1 temp_órfão=nenhum linhas_relidas=$BACK"
echo "CRASH_PARQUET_OK — o export publicado sobreviveu ao crash íntegro e legível"
echo "  (limite honesto: não distingue fsync de page cache — ver o cabeçalho deste arquivo)"
