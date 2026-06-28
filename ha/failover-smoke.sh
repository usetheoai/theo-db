#!/usr/bin/env bash
# M4 DoD-1 — automatic failover smoke with MEASURED RTO.
# Brings up the Patroni cluster (3 etcd + 2 TheoDB nodes), seeds a vector table on the primary, KILLS the
# primary container, measures wall-clock RTO (kill → a former replica accepts writes), and asserts:
#   - RTO <= target (default 30s)
#   - pre-kill data preserved on the new primary (replication had no loss)
#   - exactly ONE leader after the killed node rejoins (no split-brain)
# Env: RTO_TARGET (default 30), KEEP=1 (skip teardown). Prereq: docker + theo-db-ha image built.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPOSE="docker compose -f $HERE/docker-compose.ha.yml"
RTO_TARGET="${RTO_TARGET:-30}"
CTL=(patronictl -c /tmp/patroni.yml)

die() { echo "FAILOVER SMOKE FAILED: $*" >&2; [ -z "${KEEP:-}" ] && $COMPOSE down -v >/dev/null 2>&1; exit 1; }
# run patronictl from any RUNNING node arg
roles_json() { docker exec "$1" "${CTL[@]}" list -f json 2>/dev/null; }
leader_member() { roles_json "$1" | python3 -c "import sys,json; print(next((m['Member'] for m in json.load(sys.stdin) if m['Role']=='Leader'),''))" 2>/dev/null; }
count_leaders() { roles_json "$1" | python3 -c "import sys,json; print(sum(1 for m in json.load(sys.stdin) if m['Role']=='Leader'))" 2>/dev/null; }
streaming_replicas() { roles_json "$1" | python3 -c "import sys,json; print(sum(1 for m in json.load(sys.stdin) if m['Role']=='Replica' and m['State']=='streaming'))" 2>/dev/null; }

echo "==> bring up HA cluster"
$COMPOSE up -d >/dev/null

echo "==> wait for 1 leader + 1 streaming replica (up to 90s)"
ready=""
for _ in $(seq 1 30); do
  if docker exec theodb-patroni1 "${CTL[@]}" list >/dev/null 2>&1; then
    [ "$(count_leaders theodb-patroni1)" = "1" ] && [ "$(streaming_replicas theodb-patroni1)" = "1" ] && { ready=1; break; }
  fi
  sleep 3
done
[ -n "$ready" ] || die "cluster did not form (1 leader + 1 streaming replica)"
docker exec theodb-patroni1 "${CTL[@]}" list 2>/dev/null | head -6

LEADER_M="$(leader_member theodb-patroni1)"
LEADER="theodb-$LEADER_M"
SURVIVOR_M=$([ "$LEADER_M" = patroni1 ] && echo patroni2 || echo patroni1)
SURVIVOR="theodb-$SURVIVOR_M"
echo "==> leader=$LEADER  survivor=$SURVIVOR"

echo "==> seed vector table on the primary (TheoDB-specific HA proof)"
docker exec -i "$LEADER" psql -U postgres -d postgres -q -v ON_ERROR_STOP=1 <<'SQL' || die "seed failed on primary"
CREATE EXTENSION IF NOT EXISTS vector;
DROP TABLE IF EXISTS fo;
CREATE TABLE fo (id bigint PRIMARY KEY, e vector(4) NOT NULL);
INSERT INTO fo SELECT g, ('[' || g || ',' || (g+1) || ',' || (g+2) || ',' || (g+3) || ']')::vector
FROM generate_series(1,500) g;
SQL
PRE_SUM="$(docker exec "$LEADER" psql -U postgres -d postgres -tAc "SELECT count(*)||':'||md5(string_agg(e::text,',' ORDER BY id)) FROM fo;" | tr -d '[:space:]')"
echo "    pre-kill: $PRE_SUM"
# wait until the survivor has caught up (so failover keeps the data)
sleep 3

echo "==> KILL the primary container ($LEADER) and measure RTO"
t0=$(date +%s)
docker kill "$LEADER" >/dev/null
rto=""
for _ in $(seq 1 40); do  # up to ~80s
  # the survivor accepts a write only once it is promoted to primary
  if docker exec "$SURVIVOR" psql -U postgres -d postgres -v ON_ERROR_STOP=1 \
       -c "INSERT INTO fo VALUES (100001, '[9,9,9,9]') ON CONFLICT DO NOTHING;" >/dev/null 2>&1; then
    rto=$(( $(date +%s) - t0 )); break
  fi
  sleep 2
done
[ -n "$rto" ] || die "no node accepted writes after the primary was killed"
echo "    RTO = ${rto}s (target <= ${RTO_TARGET}s)"
[ "$rto" -le "$RTO_TARGET" ] || die "RTO ${rto}s exceeds target ${RTO_TARGET}s"

echo "==> assert survivor is now Leader and pre-kill data preserved"
[ "$(leader_member "$SURVIVOR")" = "$SURVIVOR_M" ] || die "survivor $SURVIVOR_M is not the new leader"
POST_SUM="$(docker exec "$SURVIVOR" psql -U postgres -d postgres -tAc "SELECT count(*)||':'||md5(string_agg(e::text,',' ORDER BY id)) FROM fo WHERE id<=500;" | tr -d '[:space:]')"
[ "$POST_SUM" = "$PRE_SUM" ] || die "data not preserved across failover (pre=$PRE_SUM post=$POST_SUM)"

echo "==> restart the killed node and assert it rejoins as Replica (no split-brain)"
$COMPOSE up -d "$SURVIVOR_M" >/dev/null 2>&1 || true   # noop; ensure survivor up
docker start "$LEADER" >/dev/null
rejoined=""
for _ in $(seq 1 30); do
  if [ "$(count_leaders "$SURVIVOR")" = "1" ] && [ "$(streaming_replicas "$SURVIVOR")" = "1" ]; then rejoined=1; break; fi
  sleep 3
done
[ -n "$rejoined" ] || die "killed node did not rejoin as a single streaming replica (possible split-brain)"
[ "$(count_leaders "$SURVIVOR")" = "1" ] || die "split-brain: more than one leader"

echo "FAILOVER SMOKE PASSED — RTO=${rto}s, data preserved, exactly 1 leader (no split-brain)"
[ -z "${KEEP:-}" ] && { echo "==> teardown"; $COMPOSE down -v >/dev/null 2>&1; } || true
exit 0
