#!/bin/bash
# M140.3 — smoke SQL da engine BM25 de produção contra um PG real (o `cargo pgrx test` não linka neste
# ambiente — o problema conhecido do M139; a validação da camada pgrx é via extensão instalada + SQL, como o
# CI cassert-sql-safety e o m139-lexical-crash-smoke.sh). Requer `cargo pgrx install --features spike-lexical`.
# Valida: bm25_build (indexa+gera), bm25_search (retorna id certo), índice vazio → 0, rebuild → nova geração,
# e o crux MVCC: uma sessão com snapshot antigo NÃO vê o build de outra sessão (cache MVCC-correto).
set +e
PGINST=/root/.pgrx/18.4/pgrx-install; PORT=28951
P=$(mktemp -d); chmod 777 "$P"; DATA=$P/d; LOG=$P/log
chmod -R o+rX "$PGINST"; chmod o+x /root
runuser -u pgtest -- bash -c "
  $PGINST/bin/initdb -D $DATA -U postgres --locale=C.UTF-8 >/dev/null 2>&1
  echo \"listen_addresses='localhost'\" >> $DATA/postgresql.conf
  $PGINST/bin/pg_ctl -D $DATA -o '-p $PORT' -l $LOG start >/dev/null 2>&1
"
sleep 4
PSQL="$PGINST/bin/psql -h 127.0.0.1 -p $PORT -U postgres -d postgres -tA"
$PSQL -c "CREATE EXTENSION theodb_rs" >/dev/null 2>&1

FAIL=0
chk() { # chk "label" "got" "expected"
  if [ "$2" = "$3" ]; then echo "OK   $1 = $2"; else echo "FAIL $1 = $2 (esperado $3)"; FAIL=1; fi
}

# ---- Funcional ----
$PSQL -c "CREATE TABLE docs (id bigint, body text)" >/dev/null 2>&1
$PSQL -c "INSERT INTO docs VALUES (1,'the quick brown fox'),(2,'error timeout blk_zebra9 connection reset'),(3,'info dfs datanode packetresponder terminating')" >/dev/null 2>&1

N=$($PSQL -c "SELECT bm25_build(100,'docs','id','body')")
chk "bm25_build indexa 3 docs" "$N" "3"

G=$($PSQL -c "SELECT generation FROM theodb.lexical_index_meta WHERE index_id=100")
chk "geração após 1º build" "$G" "1"

TOP=$($PSQL -c "SELECT id FROM bm25_search(100,'blk_zebra9',10) ORDER BY score DESC LIMIT 1")
chk "bm25_search retorna o id do termo raro" "$TOP" "2"

EMPTY=$($PSQL -c "SELECT count(*) FROM bm25_search(999,'anything',10)")
chk "índice sem build → 0 linhas (não erro)" "$EMPTY" "0"

$PSQL -c "SELECT bm25_build(100,'docs','id','body')" >/dev/null 2>&1
G2=$($PSQL -c "SELECT generation FROM theodb.lexical_index_meta WHERE index_id=100")
chk "rebuild → geração 2" "$G2" "2"

# search repetida (cache-hit não deve quebrar; a lógica de cache-hit é provada no unit test do IndexCache)
R1=$($PSQL -c "SELECT count(*) FROM bm25_search(100,'timeout',10)")
R2=$($PSQL -c "SELECT count(*) FROM bm25_search(100,'timeout',10)")
chk "busca repetida estável (cache)" "$R1" "$R2"

# ---- MVCC crux: sessão A (snapshot antigo) NÃO vê o build da sessão B ----
$PSQL -c "CREATE TABLE mv (id bigint, body text)" >/dev/null 2>&1
$PSQL -c "INSERT INTO mv VALUES (1,'alpha')" >/dev/null 2>&1
$PSQL -c "SELECT bm25_build(200,'mv','id','body')" >/dev/null 2>&1   # geração 1 (só 'alpha'), commitada

FIFO=$P/a.sql; mkfifo "$FIFO"
( $PSQL -f "$FIFO" > "$P/a.out" 2>&1 ) &
exec 8>"$FIFO"
echo "BEGIN ISOLATION LEVEL REPEATABLE READ;" >&8
echo "SELECT 'A_baseline_alpha='||count(*) FROM bm25_search(200,'alpha',10);" >&8   # estabelece snapshot @ gen1
sleep 2
# Sessão B (backend separado): adiciona 'betamax' e reconstrói → geração 2, COMMITADA
$PSQL -c "INSERT INTO mv VALUES (2,'betamax'); SELECT bm25_build(200,'mv','id','body')" >/dev/null 2>&1
sleep 1
echo "SELECT 'A_sees_betamax='||count(*) FROM bm25_search(200,'betamax',10);" >&8   # DEVE ser 0 (snapshot de A é gen1)
echo "COMMIT;" >&8
echo "SELECT 'A_after_commit='||count(*) FROM bm25_search(200,'betamax',10);" >&8   # novo snapshot → vê gen2 = 1
sleep 1
exec 8>&-
wait 2>/dev/null

A_BASE=$(grep -oE "A_baseline_alpha=[0-9]+" "$P/a.out" | head -1 | cut -d= -f2)
A_SEES=$(grep -oE "A_sees_betamax=[0-9]+" "$P/a.out" | head -1 | cut -d= -f2)
A_POST=$(grep -oE "A_after_commit=[0-9]+" "$P/a.out" | head -1 | cut -d= -f2)
chk "MVCC A vê alpha no seu snapshot" "$A_BASE" "1"
chk "MVCC A NÃO vê o build de B (snapshot antigo → cache gen1)" "$A_SEES" "0"
chk "MVCC A após commit (novo snapshot) vê gen2" "$A_POST" "1"

runuser -u pgtest -- $PGINST/bin/pg_ctl -D $DATA stop -m immediate >/dev/null 2>&1
rm -rf "$P"

if [ "$FAIL" = "0" ]; then echo "M140_3_SMOKE_OK — bm25_build/bm25_search + geração + MVCC-correto"; else echo "M140_3_SMOKE_FAIL"; fi
exit $FAIL
