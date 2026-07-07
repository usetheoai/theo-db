#!/usr/bin/env bash
# M56 fase 2 (T4.2) crash-injection e2e: prove the IN-PLACE INSERT (slot-reuse) is WAL-DURABLE across a hard crash.
#
# The slot-reuse insert (`insert_inplace`) writes the revived element + its neighbor tuple + the backward links
# through `page::modify_item_at` → GenericXLog (the same crash-safe primitive as the M56 tombstone sweep). This
# test proves durability end-to-end for the NEW insert path:
#
#   build index → DELETE 2 nearest rows → VACUUM (tombstone them in place) → INSERT 2 new rows (aminsert REUSES the
#   tombstoned slots via the in-place insert) → capture the index scan → `docker kill -s KILL` (forces crash
#   recovery + WAL replay on restart) → restart → the new rows are STILL found by the index (their in-place links
#   survived), the deleted rows stay gone, and the row count is consistent.
#
# Uses a named volume so PGDATA survives the kill/restart. Self-contained. Usage: IMAGE=theodb:m56 scripts/m56-slot-reuse-crash-e2e.sh
set -euo pipefail

IMAGE="${IMAGE:-theodb:m56}"
CTR="theodb-m56-reuse-crash"
VOL="theodb-m56-reuse-crash-data"
PORT="${PORT:-55498}"
PW="postgres"
PSQL=(env PGPASSWORD="$PW" psql -h localhost -p "$PORT" -U postgres -tA)

cleanup() { docker rm -f "$CTR" >/dev/null 2>&1 || true; docker volume rm "$VOL" >/dev/null 2>&1 || true; }
trap cleanup EXIT
cleanup

echo "== M56 slot-reuse crash-e2e: image=$IMAGE =="
docker volume create "$VOL" >/dev/null
docker run -d --name "$CTR" -v "$VOL":/var/lib/postgresql/data -p "$PORT":5432 -e POSTGRES_PASSWORD="$PW" "$IMAGE" >/dev/null

wait_healthy() {
  for _ in $(seq 1 40); do
    [ "$(docker inspect -f '{{.State.Health.Status}}' "$CTR" 2>/dev/null)" = "healthy" ] && return 0
    sleep 2
  done
  echo "FATAL: container never healthy"; docker logs --tail 30 "$CTR"; exit 1
}
wait_healthy

# WAL-logged table + index; 200 distinct points.
"${PSQL[@]}" >/dev/null <<'SQL'
CREATE EXTENSION IF NOT EXISTS theodb CASCADE;
LOAD 'theodb_rs';
DROP TABLE IF EXISTS rz;
CREATE TABLE rz (id int PRIMARY KEY, e vector(4));
INSERT INTO rz SELECT g, format('[%s,%s,%s,%s]', g, g%7, g%5, g*0.1)::vector FROM generate_series(0,199) g;
CREATE INDEX rz_idx ON rz USING theodb_hnsw (e);
SQL

PROBE='[40,5,0,4.0]'   # near ids ~40
topk() {
  "${PSQL[@]}" -c "SET theodb_hnsw.ef_search=200; SET enable_seqscan=off; SET enable_bitmapscan=off;
    SELECT id FROM rz ORDER BY e <-> '${PROBE}'::vector LIMIT 5;" | tr '\n' ',' | sed 's/,$//'
}

BEFORE="$(topk)"
echo "top-5 before delete: $BEFORE"
V1="$(echo "$BEFORE" | cut -d, -f1)"; V2="$(echo "$BEFORE" | cut -d, -f2)"

# Delete the 2 nearest, VACUUM → tombstone them in place (1% deleted stays under the default compact_pct).
"${PSQL[@]}" -c "DELETE FROM rz WHERE id IN ($V1,$V2);" >/dev/null
"${PSQL[@]}" -c "VACUUM rz;" >/dev/null

# Insert 2 NEW rows near the same region → aminsert REUSES the 2 fresh tombstoned slots (in-place insert).
"${PSQL[@]}" -c "INSERT INTO rz VALUES (1001,'[40,5,0,4.01]'),(1002,'[41,6,1,4.11]');" >/dev/null
sync

AFTER_INS="$(topk)"
echo "top-5 after reuse-insert (pre-crash): $AFTER_INS"
# the new rows must be found by the index (they were linked by the in-place insert)
echo "$AFTER_INS" | grep -qw 1001 || { echo "FAIL: new row 1001 not found pre-crash (in-place insert didn't link it)"; exit 1; }

# --- HARD CRASH ---
echo "== SIGKILL (forcing crash recovery on restart) =="
docker kill -s KILL "$CTR" >/dev/null
sleep 2
docker start "$CTR" >/dev/null
wait_healthy
if docker logs "$CTR" 2>&1 | grep -qiE "database system was interrupted|redo starts|automatic recovery|not properly shut down"; then
  echo "recovery: crash recovery / WAL replay observed"
else
  echo "recovery: (log line not captured — proceeding to the durability assertion)"
fi

AFTER_CRASH="$(topk)"
echo "top-5 after crash+restart: $AFTER_CRASH"

FAIL=0
echo "$AFTER_CRASH" | grep -qw 1001 || { echo "FAIL: new row 1001 lost after crash — in-place insert not WAL-durable"; FAIL=1; }
echo "$AFTER_CRASH" | grep -qw 1002 || { echo "FAIL: new row 1002 lost after crash"; FAIL=1; }
if echo "$AFTER_CRASH" | grep -qw "$V1" || echo "$AFTER_CRASH" | grep -qw "$V2"; then
  echo "FAIL: a deleted id ($V1/$V2) reappeared after crash"; FAIL=1
fi
CNT="$("${PSQL[@]}" -c "SELECT count(*) FROM rz;")"
[ "$CNT" = "200" ] || { echo "FAIL: heap count $CNT != 200 (198 survivors + 2 new)"; FAIL=1; }
# the reused-insert rows are queryable by their own vector
SELF="$("${PSQL[@]}" -c "SET theodb_hnsw.ef_search=200; SET enable_seqscan=off; SELECT id FROM rz ORDER BY e <-> '[40,5,0,4.01]'::vector LIMIT 1;")"
[ "$SELF" = "1001" ] || { echo "FAIL: row 1001 not nearest to its own vector after crash (got $SELF)"; FAIL=1; }

if [ "$FAIL" = 0 ]; then
  echo "PASS: M56 in-place insert (slot-reuse) is WAL-durable across a hard crash (new rows survive + stay linked; deleted stay gone)."
else
  echo "M56 SLOT-REUSE CRASH-E2E FAILED"; exit 1
fi
