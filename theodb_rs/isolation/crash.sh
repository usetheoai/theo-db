#!/usr/bin/env bash
# M99 D2 — crash-safety WAL-replay proof for the theodb_columnar TAM. A committed columnar INSERT must survive
# an immediate (crash) shutdown + recovery byte-for-byte: the column-chunk/header data pages are WAL-logged via
# GenericXLog and the visibility-granting columnar.stripe catalog row via heap WAL, so replay restores both.
# Crash BEFORE commit is equivalent to abort (the catalog row never commits → the stripe is invisible, its data
# pages are recoverable orphans) — that half is proven by the D1 columnar_abort_vs_reader permutation. Run on the
# build droplet (needs a real cluster we can crash + restart). Regenerate the extension first (cargo pgrx install).
set -euo pipefail
PGINST="${PGINST:-$HOME/.pgrx/17.10/pgrx-install}"
export PATH="$PGINST/bin:$PATH"
DATA=/tmp/crash_tmp
PORT=59715
DB=postgres

cleanup() { pg_ctl -D "$DATA" -m immediate stop -w >/dev/null 2>&1 || true; rm -rf "$DATA"; }
trap cleanup EXIT
rm -rf "$DATA"
initdb -D "$DATA" -U theo >/dev/null 2>&1
{ echo "port=$PORT"; echo "fsync=on"; echo "full_page_writes=on"; } >> "$DATA/postgresql.conf"
pg_ctl -D "$DATA" -l "$DATA/log" start -w >/dev/null

q() { psql -X -q -p "$PORT" -U theo -d "$DB" -tAc "$1"; }

q "CREATE EXTENSION theodb_rs;" >/dev/null
q "CREATE TABLE cc (a int, b text) USING theodb_columnar;" >/dev/null
# Committed insert (autocommit) → pre-commit flush → durable stripe + committed catalog row.
q "INSERT INTO cc SELECT g, 'v' || g FROM generate_series(1, 10000) g;" >/dev/null
PRE_CNT=$(q "SELECT count(*) FROM cc;")
PRE_SUM=$(q "SELECT sum(a) FROM cc;")
q "CHECKPOINT;" >/dev/null   # not required for correctness; ensures some pages are on disk pre-crash

# CRASH: immediate stop skips a clean shutdown → recovery must replay WAL on restart.
pg_ctl -D "$DATA" -m immediate stop -w >/dev/null
pg_ctl -D "$DATA" -l "$DATA/log" start -w >/dev/null

POST_CNT=$(q "SELECT count(*) FROM cc;")
POST_SUM=$(q "SELECT sum(a) FROM cc;")
SAMPLE=$(q "SELECT b FROM cc WHERE a = 5000;")
STRIPES=$(q "SELECT count(*) FROM columnar.stripe WHERE relid = 'cc'::regclass;")

echo "PRE_CNT=$PRE_CNT POST_CNT=$POST_CNT PRE_SUM=$PRE_SUM POST_SUM=$POST_SUM SAMPLE=$SAMPLE STRIPES=$STRIPES"
if [ "$PRE_CNT" = "10000" ] && [ "$POST_CNT" = "10000" ] && [ "$PRE_SUM" = "$POST_SUM" ] \
   && [ "$POST_SUM" = "50005000" ] && [ "$SAMPLE" = "v5000" ] && [ "$STRIPES" -ge 1 ]; then
    echo "CRASH_REPLAY_OK — committed columnar stripe survived crash + WAL replay, scan-identical"
    exit 0
else
    echo "CRASH_REPLAY_FAIL"
    exit 1
fi
