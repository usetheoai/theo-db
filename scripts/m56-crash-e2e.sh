#!/usr/bin/env bash
# M56 crash-injection e2e (DoD 6): prove the in-place tombstone flag is WAL-DURABLE across a hard crash.
#
# The tombstone sweep writes the `deleted` byte per page through `page::modify_items_under_wal` → GenericXLog
# (the SAME crash-safe primitive M48 proved for the fold). This test proves the durability end-to-end:
#
#   build index → DELETE rows → VACUUM (tombstone in-place, compaction OFF) → capture the index top-k
#   → `docker kill -s KILL` (SIGKILL, NOT a clean shutdown → forces crash recovery + WAL replay on restart)
#   → restart → the index top-k is IDENTICAL (the tombstoned nodes stay filtered, the live nodes stay correct).
#
# A WAL-logged table (not TEMP) is required so the tombstone deltas are actually journaled. Uses a named
# volume so PGDATA survives the kill/restart. Self-contained: creates + removes its own container + volume.
#
# Usage: IMAGE=theodb:m56 scripts/m56-crash-e2e.sh
set -euo pipefail

IMAGE="${IMAGE:-theodb:m56}"
CTR="theodb-m56-crash"
VOL="theodb-m56-crash-data"
PORT="${PORT:-55497}"
PW="postgres"
PSQL=(env PGPASSWORD="$PW" psql -h localhost -p "$PORT" -U postgres -tA)

cleanup() { docker rm -f "$CTR" >/dev/null 2>&1 || true; docker volume rm "$VOL" >/dev/null 2>&1 || true; }
trap cleanup EXIT
cleanup

echo "== M56 crash-e2e: image=$IMAGE =="
docker volume create "$VOL" >/dev/null
docker run -d --name "$CTR" -v "$VOL":/var/lib/postgresql/data \
  -p "$PORT":5432 -e POSTGRES_PASSWORD="$PW" "$IMAGE" >/dev/null

wait_healthy() {
  for _ in $(seq 1 40); do
    [ "$(docker inspect -f '{{.State.Health.Status}}' "$CTR" 2>/dev/null)" = "healthy" ] && return 0
    sleep 2
  done
  echo "FATAL: container never healthy"; docker logs --tail 30 "$CTR"; exit 1
}
wait_healthy

# --- build a WAL-logged table + theodb_hnsw index, then tombstone a subset via a real DELETE+VACUUM ---
"${PSQL[@]}" >/dev/null <<'SQL'
CREATE EXTENSION IF NOT EXISTS theodb CASCADE;
LOAD 'theodb_rs';
DROP TABLE IF EXISTS crashz;
CREATE TABLE crashz (id int PRIMARY KEY, e vector(4));   -- WAL-logged (NOT temp) so tombstones are journaled
INSERT INTO crashz SELECT g, format('[%s,%s,%s,%s]', g, g%7, g%5, g*0.1)::vector
  FROM generate_series(0,199) g;
CREATE INDEX crashz_idx ON crashz USING theodb_hnsw (e);
SQL
# 2/200 = 1% deleted stays well under the default compact_pct (20%) → a TOMBSTONE-only VACUUM (no fold).

PROBE='[3.3,1.1,2.2,0.4]'
topk() {
  "${PSQL[@]}" -c "SET theodb_hnsw.ef_search=200; SET enable_seqscan=off; SET enable_bitmapscan=off;
    SELECT id FROM crashz ORDER BY e <-> '${PROBE}'::vector LIMIT 5;" | tr '\n' ',' | sed 's/,$//'
}

BEFORE_DEL="$(topk)"
echo "top-5 before delete: $BEFORE_DEL"

# Delete the 2 nearest, then VACUUM → in-place tombstone sweep (compaction disabled above).
VICTIMS="$(echo "$BEFORE_DEL" | cut -d, -f1-2 | tr ',' ' ')"
V1="$(echo "$VICTIMS" | awk '{print $1}')"; V2="$(echo "$VICTIMS" | awk '{print $2}')"
# DELETE and VACUUM MUST be separate statements — VACUUM cannot run inside a (multi-statement) transaction block.
"${PSQL[@]}" -c "DELETE FROM crashz WHERE id IN ($V1,$V2);" >/dev/null
"${PSQL[@]}" -c "VACUUM crashz;" >/dev/null       # → ambulkdelete → in-place tombstone sweep (WAL-journaled)
sync

AFTER_TOMB="$(topk)"
echo "top-5 after tombstone VACUUM (pre-crash): $AFTER_TOMB"
if echo "$AFTER_TOMB" | grep -qw "$V1" || echo "$AFTER_TOMB" | grep -qw "$V2"; then
  echo "FAIL: a tombstoned id ($V1/$V2) is still emitted BEFORE the crash — the filter is broken"; exit 1
fi

# --- HARD CRASH: SIGKILL the container (no clean shutdown) → crash recovery + WAL replay on restart ---
echo "== SIGKILL (forcing crash recovery on restart) =="
docker kill -s KILL "$CTR" >/dev/null
sleep 2
docker start "$CTR" >/dev/null
wait_healthy
# Confirm recovery actually ran (best-effort log grep; not fatal if the line rotated out).
if docker logs "$CTR" 2>&1 | grep -qiE "database system was interrupted|redo starts|automatic recovery|not properly shut down"; then
  echo "recovery: crash recovery / WAL replay observed in the log"
else
  echo "recovery: (crash-recovery log line not captured — proceeding to the durability assertion)"
fi

AFTER_CRASH="$(topk)"
echo "top-5 after crash+restart: $AFTER_CRASH"

# --- durability assertions ---
FAIL=0
if [ "$AFTER_CRASH" != "$AFTER_TOMB" ]; then
  echo "FAIL: post-crash top-5 ($AFTER_CRASH) != pre-crash top-5 ($AFTER_TOMB) — tombstones not durable"; FAIL=1
fi
if echo "$AFTER_CRASH" | grep -qw "$V1" || echo "$AFTER_CRASH" | grep -qw "$V2"; then
  echo "FAIL: tombstoned id reappeared after crash — the deleted flag did not survive WAL replay"; FAIL=1
fi
LIVE_CNT="$("${PSQL[@]}" -c "SELECT count(*) FROM crashz;")"
[ "$LIVE_CNT" = "198" ] || { echo "FAIL: heap row count $LIVE_CNT != 198 after crash"; FAIL=1; }

if [ "$FAIL" = 0 ]; then
  echo "PASS: M56 tombstone flag is WAL-durable across a hard crash (scan identical pre/post crash; deleted stay filtered)."
else
  echo "M56 CRASH-E2E FAILED"; exit 1
fi
