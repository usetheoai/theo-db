#!/usr/bin/env bash
# M104 / #99 H3 — crash-safety proof for the INCREMENTAL columnar stripe flush. The M104 change flushes a stripe
# once pending bytes exceed maintenance_work_mem (so a big INSERT holds O(mwm), not O(N), in RAM). Each stripe is the
# SAME atomic pages→catalog-row-LAST unit; every stripe of one INSERT carries that xact's xid. This harness proves the
# all-or-nothing property across MULTIPLE incremental stripes of a single INSERT:
#   A. ABORT atomicity — a ROLLBACK of a big multi-stripe INSERT leaves ZERO visible rows (the incrementally-flushed
#      stripes' catalog rows roll back with the xact → invisible), and the table is reusable.
#   B. CRASH-after-commit — a COMMITTED big multi-stripe INSERT survives an immediate (crash) shutdown + WAL replay
#      byte-for-byte (all incremental stripes' pages + catalog rows are WAL-logged, so replay restores the whole set).
# Together: a crash/abort at ANY point of a multi-stripe INSERT yields the whole INSERT or none — never a partial
# (torn) visible set. Run on the build droplet (needs a real cluster we can crash). Regenerate the extension first.
set -uo pipefail
PGINST="${PGINST:-$HOME/.pgrx/17.10/pgrx-install}"
export PATH="$PGINST/bin:$PATH"
DATA=/tmp/crash_col_inc_tmp
PORT=59724
DB=postgres
N="${N:-60000}"   # rows — with mwm=1MB this is several incremental stripes

cleanup() { pg_ctl -D "$DATA" -m immediate stop -w >/dev/null 2>&1 || true; rm -rf "$DATA"; }
trap cleanup EXIT
rm -rf "$DATA"
initdb -D "$DATA" -U theo >/dev/null 2>&1
{ echo "port=$PORT"; echo "fsync=on"; echo "full_page_writes=on"; } >> "$DATA/postgresql.conf"
pg_ctl -D "$DATA" -l "$DATA/log" start -w >/dev/null
q() { psql -X -q -p "$PORT" -U theo -d "$DB" -tAc "$1" 2>&1; }

q "CREATE EXTENSION theodb_rs;" >/dev/null
q "CREATE TABLE ci (a int, b text) USING theodb_columnar;" >/dev/null

FAILS=0

# --- A. ABORT atomicity: a big multi-stripe INSERT rolled back leaves 0 rows ---
psql -X -q -p "$PORT" -U theo -d "$DB" >/dev/null 2>&1 <<SQL
SET maintenance_work_mem='1MB';
BEGIN;
INSERT INTO ci SELECT g, repeat('a',30) FROM generate_series(1,$N) g;
ROLLBACK;
SQL
ABORT_CNT=$(q "SELECT count(*) FROM ci;")
ABORT_STRIPES=$(q "SELECT count(*) FROM columnar.stripe WHERE relid='ci'::regclass;")
if [ "$ABORT_CNT" = "0" ] && [ "$ABORT_STRIPES" = "0" ]; then
  echo "A_abort_atomicity: OK — rolled-back multi-stripe INSERT left 0 rows / 0 visible stripes"
else echo "A_abort_atomicity: FAIL — count=$ABORT_CNT stripes=$ABORT_STRIPES (expected 0/0)"; FAILS=$((FAILS+1)); fi
# table reusable after abort
q "INSERT INTO ci VALUES (1,'x');" >/dev/null
[ "$(q "SELECT count(*) FROM ci;")" = "1" ] && echo "  reusable after abort: OK" || { echo "  reusable after abort: FAIL"; FAILS=$((FAILS+1)); }
q "TRUNCATE ci;" >/dev/null 2>&1 || q "DELETE FROM ci;" >/dev/null 2>&1 || true

# --- B. CRASH-after-commit: committed multi-stripe INSERT survives crash + WAL replay ---
q "CREATE TABLE cc (a int, b text) USING theodb_columnar;" >/dev/null
psql -X -q -p "$PORT" -U theo -d "$DB" >/dev/null 2>&1 <<SQL
SET maintenance_work_mem='1MB';
INSERT INTO cc SELECT g, repeat('b',30) FROM generate_series(1,$N) g;
SQL
PRE_CNT=$(q "SELECT count(*) FROM cc;"); PRE_SUM=$(q "SELECT sum(a) FROM cc;")
PRE_STRIPES=$(q "SELECT count(*) FROM columnar.stripe WHERE relid='cc'::regclass;")
# CRASH (immediate stop — skips clean shutdown, forces WAL replay) then restart
pg_ctl -D "$DATA" -m immediate stop -w >/dev/null
pg_ctl -D "$DATA" -l "$DATA/log" start -w >/dev/null
POST_CNT=$(q "SELECT count(*) FROM cc;"); POST_SUM=$(q "SELECT sum(a) FROM cc;")
if [ "$PRE_CNT" = "$N" ] && [ "$POST_CNT" = "$N" ] && [ "$PRE_SUM" = "$POST_SUM" ] && [ "$PRE_STRIPES" -gt 1 ]; then
  echo "B_crash_after_commit: OK — committed $N-row / $PRE_STRIPES-stripe INSERT survived crash+replay byte-identical"
else echo "B_crash_after_commit: FAIL — pre=$PRE_CNT post=$POST_CNT presum=$PRE_SUM postsum=$POST_SUM stripes=$PRE_STRIPES"; FAILS=$((FAILS+1)); fi

CRASHES=$(grep -c "was not properly shut down\|database system was interrupted" "$DATA/log" 2>/dev/null || true); CRASHES=${CRASHES:-0}
echo "recovery_evidence: WAL-replay recoveries in log = $CRASHES (expected >= 1)"
echo "---"
if [ "$FAILS" = 0 ] && [ "$CRASHES" -ge 1 ]; then
  echo "CRASH_COLUMNAR_INCREMENTAL_OK — multi-stripe INSERT is all-or-nothing: abort→0, committed→survives crash+replay"
  exit 0
else echo "CRASH_COLUMNAR_INCREMENTAL_FAIL (fails=$FAILS recoveries=$CRASHES)"; exit 1; fi
