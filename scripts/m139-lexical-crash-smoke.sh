#!/bin/bash
# M139 gate 3 — crash-real com replay do WAL. Um índice COMMITADO deve sobreviver a SIGABRT + restart, porque o
# flush escreve num heap WAL-logged (PG recupera por construção). É o mesmo método das suítes isolation/crash*.sh.
set +e
PGINST=/root/.pgrx/18.4/pgrx-install; PORT=28943
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
SCHEMA=$($PSQL -c "SELECT n.nspname FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE p.proname='lexical_spike_flush_only' LIMIT 1")
F="${SCHEMA}.lexical_spike"

# 1. Flush + COMMIT (autocommit) — dados duráveis (WAL fsync no commit).
$PSQL -c "SELECT ${F}_flush_only(777,'lazy')" >/dev/null 2>&1
BEFORE=$($PSQL -c "SELECT ${F}_search(777,'lazy')")
echo "antes do crash: search(777,'lazy') = $BEFORE   [esperado 1]"

# 2. SIGABRT no postmaster — crash real (não shutdown limpo).
PM=$(head -1 "$DATA/postmaster.pid")
echo "== SIGABRT no postmaster pid $PM =="
kill -ABRT "$PM" 2>/dev/null
sleep 4
# confirmar que caiu
$PSQL -c "SELECT 1" >/dev/null 2>&1 && echo "AVISO: servidor ainda responde (crash não pegou?)" || echo "servidor caiu (esperado)"

# 3. Restart → replay do WAL.
runuser -u pgtest -- bash -c "$PGINST/bin/pg_ctl -D $DATA -o '-p $PORT' -l $LOG start >/dev/null 2>&1"
sleep 6
grep -qE "redo|recovery" "$LOG" 2>/dev/null && echo "WAL replay ocorreu no restart"

# 4. Verificar que o índice COMMITADO sobreviveu.
AFTER=$($PSQL -c "SELECT ${F}_search(777,'lazy')")
echo "após crash+replay: search(777,'lazy') = $AFTER   [esperado 1]"

if [ "$BEFORE" = "1" ] && [ "$AFTER" = "1" ]; then
  echo "GATE3_CRASH_OK — índice commitado sobreviveu a SIGABRT + replay do WAL"
else
  echo "GATE3_CRASH_FAIL — BEFORE=$BEFORE AFTER=$AFTER"
fi
runuser -u pgtest -- $PGINST/bin/pg_ctl -D $DATA stop -m immediate >/dev/null 2>&1
rm -rf "$P"
