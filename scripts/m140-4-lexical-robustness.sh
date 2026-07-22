#!/bin/bash
# M140.4 — robustez da engine BM25 de PRODUÇÃO contra o BINÁRIO SHIPADO (padrão M99/M135, não um mock).
# Prova os três: (1) CRASH — um índice bm25_build COMMITADO sobrevive a SIGABRT + replay do WAL; (2) VACUUM —
# após rebuilds (segmentos antigos deletados) o VACUUM recupera espaço sem corromper a busca; (3) MVCC — sob
# REPEATABLE READ **e** READ COMMITTED o cache serve a versão certa do snapshot (fecha o straddle do M140.3 LOW).
# Requer `cargo pgrx install --features spike-lexical`. Roda no e2e-runner (cluster real que sobe/cai).
set +e
PGINST=/root/.pgrx/18.4/pgrx-install; PORT=28961
P=$(mktemp -d); chmod 777 "$P"; DATA=$P/d; LOG=$P/log
chmod -R o+rX "$PGINST"; chmod o+x /root
start_pg() { runuser -u pgtest -- bash -c "$PGINST/bin/pg_ctl -D $DATA -o '-p $PORT' -l $LOG start >/dev/null 2>&1"; }
runuser -u pgtest -- bash -c "
  $PGINST/bin/initdb -D $DATA -U postgres --locale=C.UTF-8 >/dev/null 2>&1
  echo \"listen_addresses='localhost'\" >> $DATA/postgresql.conf
  echo \"fsync=on\" >> $DATA/postgresql.conf
  echo \"full_page_writes=on\" >> $DATA/postgresql.conf
"
start_pg; sleep 4
PSQL="$PGINST/bin/psql -h 127.0.0.1 -p $PORT -U postgres -d postgres -tA"
$PSQL -c "CREATE EXTENSION theodb_rs" >/dev/null 2>&1

FAIL=0
chk() { if [ "$2" = "$3" ]; then echo "OK   $1 = $2"; else echo "FAIL $1 = $2 (esperado $3)"; FAIL=1; fi; }

# ================= 1. CRASH + WAL replay =================
$PSQL -c "CREATE TABLE docs (id bigint, body text)" >/dev/null 2>&1
$PSQL -c "INSERT INTO docs VALUES (1,'quick brown fox'),(2,'error timeout blk_crashzeta connection'),(3,'datanode terminating')" >/dev/null 2>&1
$PSQL -c "SELECT bm25_build(300,'docs','id','body')" >/dev/null 2>&1   # COMMITADO (autocommit)
BEFORE=$($PSQL -c "SELECT id FROM bm25_search(300,'blk_crashzeta',10) ORDER BY score DESC LIMIT 1")
echo "antes do crash: bm25_search(300,'blk_crashzeta') top id = $BEFORE   [esperado 2]"
PM=$(head -1 "$DATA/postmaster.pid")
echo "== SIGABRT no postmaster pid $PM =="
kill -ABRT "$PM" 2>/dev/null; sleep 4
if $PSQL -c "SELECT 1" >/dev/null 2>&1; then DOWN=0; echo "AVISO: servidor ainda responde (crash NÃO pegou)"; else DOWN=1; echo "servidor caiu (esperado)"; fi
start_pg; sleep 6
if grep -qE "redo|recovery|replay|starting up" "$LOG" 2>/dev/null; then REPLAYED=1; echo "recovery/replay ocorreu no restart"; else REPLAYED=0; echo "AVISO: sem sinal de recovery no log"; fi
AFTER=$($PSQL -c "SELECT id FROM bm25_search(300,'blk_crashzeta',10) ORDER BY score DESC LIMIT 1")
echo "após crash+recovery: top id = $AFTER   [esperado 2]"
# Gate honesto (review M2): CRASH_OK exige BEFORE==AFTER==2 E que o crash PEGOU (DOWN) E que houve recovery
# no restart (REPLAYED) — não só "dado presente após um restart" (que um restart limpo satisfaria).
if [ "$BEFORE" = "2" ] && [ "$AFTER" = "2" ] && [ "$DOWN" = "1" ] && [ "$REPLAYED" = "1" ]; then
  echo "CRASH_OK — índice bm25 COMMITADO durável através de SIGABRT + recovery"
else
  echo "CRASH_FAIL (BEFORE=$BEFORE AFTER=$AFTER DOWN=$DOWN REPLAYED=$REPLAYED)"; FAIL=1
fi

# ================= 2. VACUUM (após rebuilds, sem corromper) =================
$PSQL -c "CREATE TABLE big (id bigint, body text)" >/dev/null 2>&1
$PSQL -c "INSERT INTO big SELECT g, 'documento numero '||g||' vixenzz raposa preguicosa '||(g%50) FROM generate_series(1,3000) g" >/dev/null 2>&1
# 4 rebuilds acumulam tuplas mortas em theodb.lexical_files (cada build DELETA as linhas antigas)
for i in 1 2 3 4; do $PSQL -c "SELECT bm25_build(301,'big','id','body')" >/dev/null 2>&1; done
DEAD_BEFORE=$($PSQL -c "SELECT n_dead_tup FROM pg_stat_user_tables WHERE relname='lexical_files'")
$PSQL -c "VACUUM theodb.lexical_files" >/dev/null 2>&1
$PSQL -c "SELECT pg_sleep(0.5)" >/dev/null 2>&1
DEAD_AFTER=$($PSQL -c "SELECT coalesce(n_dead_tup,0) FROM pg_stat_user_tables WHERE relname='lexical_files'")
# a busca segue correta após o VACUUM (sem corrupção do índice vivo)
VAC_HIT=$($PSQL -c "SELECT count(*) FROM bm25_search(301,'vixenzz',10)")
echo "VACUUM: n_dead_tup antes=$DEAD_BEFORE depois=$DEAD_AFTER ; busca pós-VACUUM hits=$VAC_HIT"
# Gate honesto (review L1): exige que HAVIA tuplas mortas (rebuilds deixaram lixo) E que o VACUUM
# RECUPEROU de fato (DEAD_AFTER < DEAD_BEFORE, não só <=) E a busca segue funcional (sem corrupção).
if [ "${VAC_HIT:-0}" -gt "0" ] && [ "${DEAD_BEFORE:-0}" -gt "0" ] && [ "${DEAD_AFTER:-99}" -lt "${DEAD_BEFORE:-0}" ]; then
  echo "VACUUM_OK"
else
  echo "VACUUM_FAIL (dead $DEAD_BEFORE->$DEAD_AFTER, hits=$VAC_HIT)"; FAIL=1
fi

# ================= 3. MVCC — REPEATABLE READ e READ COMMITTED =================
mvcc_test() { # $1 = isolation level; imprime "A_sees_new=<n>"
  local ISO="$1"; local FIFO="$P/mv_$RANDOM.sql"
  $PSQL -c "DROP TABLE IF EXISTS mv" >/dev/null 2>&1
  $PSQL -c "CREATE TABLE mv (id bigint, body text)" >/dev/null 2>&1
  $PSQL -c "INSERT INTO mv VALUES (1,'alpha')" >/dev/null 2>&1
  $PSQL -c "SELECT bm25_build(302,'mv','id','body')" >/dev/null 2>&1   # geração 1 (só alpha), commitada
  mkfifo "$FIFO"
  ( $PSQL -f "$FIFO" > "$P/a_$ISO.out" 2>&1 ) &
  exec 9>"$FIFO"
  echo "BEGIN ISOLATION LEVEL $ISO;" >&9
  echo "SELECT 'A_base='||count(*) FROM bm25_search(302,'alpha',10);" >&9   # estabelece snapshot @ gen1
  sleep 2
  $PSQL -c "INSERT INTO mv VALUES (2,'betazz'); SELECT bm25_build(302,'mv','id','body')" >/dev/null 2>&1  # gen2, commit (B)
  sleep 1
  echo "SELECT 'A_sees_new='||count(*) FROM bm25_search(302,'betazz',10);" >&9
  echo "COMMIT;" >&9
  sleep 1
  exec 9>&-; wait 2>/dev/null
  grep -oE "A_sees_new=[0-9]+" "$P/a_$ISO.out" | head -1 | cut -d= -f2
}

RR=$(mvcc_test "REPEATABLE READ")
chk "MVCC RR: A (snapshot antigo) NÃO vê o build de B" "${RR:-x}" "0"
# READ COMMITTED: cada statement de A pega um snapshot novo -> APÓS B commitar, o próximo bm25_search de A
# lê a geração 2 (novo snapshot) -> o cache reconstrói -> A VÊ betazz. É o comportamento CORRETO de RC (ver
# mudanças committadas), e prova que o cache invalida corretamente quando o snapshot avança (não serve stale).
RC=$(mvcc_test "READ COMMITTED")
chk "MVCC RC: A vê o build de B no próximo statement (cache invalida no snapshot novo)" "${RC:-x}" "1"

runuser -u pgtest -- $PGINST/bin/pg_ctl -D $DATA stop -m immediate >/dev/null 2>&1
rm -rf "$P"
if [ "$FAIL" = "0" ]; then echo "M140_4_ROBUSTNESS_OK"; else echo "M140_4_ROBUSTNESS_FAIL"; fi
exit $FAIL
