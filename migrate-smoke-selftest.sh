#!/usr/bin/env bash
# TDD self-test for migrate-smoke.sh — proves the integrity asserts are NOT theatre.
# Runs a real migration (KEEP=1 retains source table + target db), corrupts ONE row on the target,
# then re-runs the verification (VERIFY_ONLY=1) and asserts it FAILS with "data checksum mismatch".
# Uses the SAME container-exec addressing as migrate-smoke.sh (SRC_CONTAINER/DST_CONTAINER).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

SRC_CONTAINER="${SRC_CONTAINER:-m3-src}"
DST_CONTAINER="${DST_CONTAINER:-m3-dst}"
PW="${PW:-postgres}"
DST_DB="${DST_DB:-migrate_smoke}"
dst_psql() { docker exec -i -e PGPASSWORD="$PW" "$DST_CONTAINER" psql -U postgres -d "$1" -v ON_ERROR_STOP=1 "${@:2}"; }
cleanup() {
  dst_psql postgres -c "DROP DATABASE IF EXISTS $DST_DB WITH (FORCE);" >/dev/null 2>&1 || true
  docker exec -i -e PGPASSWORD="$PW" "$SRC_CONTAINER" psql -U postgres -d postgres -c "DROP TABLE IF EXISTS items;" >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "[selftest] 1/3 — run a real migration, keep source table + target db"
KEEP=1 bash "$HERE/migrate-smoke.sh" >/dev/null

echo "[selftest] 2/3 — corrupt one row on the target (changes the data checksum)"
dst_psql "$DST_DB" -c "UPDATE items SET embedding = '[9,9,9,9,9,9,9,9]'::vector WHERE id = 1;" >/dev/null

echo "[selftest] 3/3 — verify-only MUST fail with a checksum mismatch"
set +e
out="$(VERIFY_ONLY=1 bash "$HERE/migrate-smoke.sh" 2>&1)"; rc=$?
set -e

[ "$rc" -ne 0 ] || { echo "SELFTEST FAILED: verification passed on corrupted data (assert is theatre)"; exit 1; }
grep -q "data checksum mismatch" <<<"$out" || { echo "SELFTEST FAILED: wrong reason (expected 'data checksum mismatch'):"; echo "$out" | tail -3; exit 1; }
echo "SELFTEST PASSED — checksum assert correctly rejects corrupted data (exit $rc)"
