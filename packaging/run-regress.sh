#!/usr/bin/env bash
# Start a throwaway TheoDB cluster and run the upstream PG17 regression suite against it (pg_regress
# directly, so we control bindir/outputdir/dlpath). Runs as the postgres user. Exits 0 only if ALL pass.
set -uo pipefail
export PGDATA=/tmp/pgdata PGHOST=/tmp PGPORT=5432
BIN=/usr/lib/postgresql/17/bin
REG=/src/src/test/regress
OUT=/tmp/regress_out

# Prove the engine-under-test IS the distribution (PGDG 17.10), matching the source tag REL_17_10.
VER="$("$BIN/postgres" --version)"
echo "engine under test: $VER"
echo "$VER" | grep -q "17\.10" || { echo "ERROR: engine is not 17.10 (got: $VER) — source tag mismatch"; exit 2; }

rm -rf "$PGDATA"; mkdir -p "$PGDATA" "$OUT"
"$BIN/initdb" -D "$PGDATA" -U postgres -E UTF8 >/tmp/initdb.log 2>&1 || { tail -20 /tmp/initdb.log; exit 2; }
"$BIN/pg_ctl" -D "$PGDATA" -w -t 60 \
  -o "-c unix_socket_directories=/tmp -p 5432 -c listen_addresses=''" start >/tmp/pgstart.log 2>&1 \
  || { tail -20 /tmp/pgstart.log; exit 2; }

set -o pipefail
# --bindir='' => use psql from PATH (the TheoDB binaries); --dlpath/--inputdir => the built test tree.
"$REG/pg_regress" \
  --inputdir="$REG" --outputdir="$OUT" --bindir='' --dlpath="$REG" \
  --host=/tmp --port=5432 --user=postgres --dbname=regression \
  --schedule="$REG/parallel_schedule" --max-concurrent-tests=20 2>&1 | tee /tmp/regress.log
rc=${PIPESTATUS[0]}

echo "======================================================"
echo "  UPSTREAM PG17.10 REGRESSION SUITE — TheoDB distro"
echo "======================================================"
grep -E "All [0-9]+ tests passed|[0-9]+ of [0-9]+ tests failed" /tmp/regress.log | tail -3
if [ "$rc" -ne 0 ]; then
  echo "--- failed test diffs (head) ---"
  [ -f "$OUT/regression.diffs" ] && head -50 "$OUT/regression.diffs"
fi
"$BIN/pg_ctl" -D "$PGDATA" stop -m immediate >/dev/null 2>&1 || true
exit "$rc"
