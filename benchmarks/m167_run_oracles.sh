#!/bin/bash
# M167 — runs every correctness oracle and every gate self-test, stamping each log with the binary that produced it.
#
# WHY THIS IS IN THE REPO. A reviewer noted that the `postmaster=` / `so_md5=` / `rc=` banner appeared in seven
# committed logs but in no committed script: the provenance convention shipped as a transcript. That is the same
# "harness, not transcript" gap that produced `m167_guard_proofs.sh` one pass earlier. This is the wrapper.
#
# WHY A CHECKSUM AND NOT A TIMESTAMP. In this repository the build necessarily precedes the commit that contains
# it, so "this run started before commit X" never implies "this binary lacks X". A wall-clock cannot pin a run to a
# binary here; `so_md5` can. `so_mtime < postmaster` additionally proves the stamped file is the loaded image —
# the .so was not rewritten under a running postmaster.
#
# Produces, under $OUT (default /root):
#   m167-hits-topk-final.log   1M top-k oracle          expect rc=0
#   m167-ec-final.log          fixture oracle           expect rc=0
#   m167-h0-control.log        H0 gate control          expect rc=3  (routing precondition must abort)
#   m167-gate-control.log      1M FINAL GATE control    expect rc=3  (seeded mismatch must abort)
#   m167-ec-control.log        EC FINAL GATE control    expect rc=3  (seeded mismatch must abort)
#
# Usage:  ./m167_run_oracles.sh          then check the tail of each log
set -uo pipefail

P="${PGRX_BIN:-/root/.pgrx/18.4/pgrx-install/bin}"
PGPORT="${PGPORT:-28900}"
PGUSER="${PGUSER:-postgres}"
PGDATABASE="${PGDATABASE:-postgres}"
RUNAS="${RUNAS:-pgtest}"
BENCH="${BENCH:-$(cd "$(dirname "$0")" && pwd)}"
OUT="${OUT:-/root}"

as_pg() { su - "$RUNAS" -c "$*"; }

SO_PATH=$(find "$(dirname "$P")" -name theodb_rs.so 2>/dev/null | head -1)
SO_MD5=$(md5sum "$SO_PATH" 2>/dev/null | cut -d' ' -f1)
if [ -z "$SO_PATH" ] || [ -z "$SO_MD5" ]; then
  echo "FATAL: could not stamp the installed .so (path='$SO_PATH') — an artifact without provenance is not evidence"
  exit 2
fi

# name | sql | PGOPTIONS | extra psql args | expected rc
run() {
  local name=$1 sql=$2 opts=$3 extra=$4 want=$5
  {
    echo "=== $name start $(date -Is) ==="
    echo "postmaster=$(as_pg "$P/psql -h localhost -p $PGPORT -U $PGUSER -d $PGDATABASE -tAc \"SELECT pg_postmaster_start_time()\"" 2>/dev/null)"
    echo "so_path=$SO_PATH"
    echo "so_mtime=$(stat -c '%y' "$SO_PATH")"
    echo "so_md5=$SO_MD5"
    echo "expected_rc=$want"
    as_pg "PGOPTIONS=\"$opts\" $P/psql -h localhost -p $PGPORT -U $PGUSER -d $PGDATABASE $extra -f $sql" 2>&1
    local rc=$?
    echo "=== $name rc=$rc end $(date -Is) ==="
    if [ "$rc" != "$want" ]; then
      echo "!!! UNEXPECTED: rc=$rc but this run was expected to exit $want"
    fi
  } > "$OUT/$6" 2>&1
  local got; got=$(grep -oE 'rc=[0-9]+' "$OUT/$6" | tail -1 | cut -d= -f2)
  printf '  %-46s rc=%-3s expected=%-3s %s\n' "$name" "$got" "$want" \
         "$([ "$got" = "$want" ] && echo OK || echo MISMATCH)"
  [ "$got" = "$want" ]
}

echo "so_md5=$SO_MD5  so_mtime=$(stat -c '%y' "$SO_PATH")"
fail=0
run "M167 1M top-k oracle"        "$BENCH/m167_hits_topk_ab.sql" "-c work_mem=64MB" ""                   0 m167-hits-topk-final.log || fail=1
run "M167 fixture oracle"         "$BENCH/m158_ec_harness.sql"   ""                 ""                   0 m167-ec-final.log        || fail=1
run "H0 gate POSITIVE CONTROL"    "$BENCH/m167_hits_topk_ab.sql" "-c work_mem=64kB" ""                   3 m167-h0-control.log      || fail=1
run "1M FINAL GATE POS. CONTROL"  "$BENCH/m167_hits_topk_ab.sql" "-c work_mem=64MB" "-v gate_selftest=1" 3 m167-gate-control.log    || fail=1
run "EC FINAL GATE POS. CONTROL"  "$BENCH/m158_ec_harness.sql"   ""                 "-v gate_selftest=1" 3 m167-ec-control.log      || fail=1

# A control that does not fail is not a control; a run whose rc differs from the declared expectation is a defect
# in the evidence, so this wrapper exits non-zero rather than leaving a human to notice.
exit $fail
