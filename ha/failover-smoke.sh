#!/usr/bin/env bash
# M4 DoD-1 — automatic failover with MEASURED RTO + a real anti-split-brain (network-partition) proof.
# Phase A (crash failover + RTO): seed a vector table, wait for the replica to be byte-for-byte caught up
#   (RPO=0, deterministic — not a sleep), KILL the primary, measure RTO (kill → former replica accepts
#   writes), assert RTO <= target and data preserved, then restart the killed node and assert it rejoins
#   as a single streaming replica.
# Phase B (split-brain avoidance): network-PARTITION the current primary from the cluster; assert the
#   isolated old primary goes READ-ONLY (cannot accept writes) WHILE the majority elects a new primary that
#   DOES accept writes — i.e. never two writable primaries. Reconnect and assert one leader.
# Env: RTO_TARGET (default 30), KEEP=1 (skip teardown). Prereq: docker + theo-db-ha image built.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPOSE="docker compose -f $HERE/docker-compose.ha.yml"
NET=theodb-ha_ha
RTO_TARGET="${RTO_TARGET:-30}"
CTL=(patronictl -c /tmp/patroni.yml)

die() { echo "FAILOVER SMOKE FAILED: $*" >&2; [ -z "${KEEP:-}" ] && $COMPOSE down -v >/dev/null 2>&1; exit 1; }
roles_json() { docker exec "$1" "${CTL[@]}" list -f json 2>/dev/null; }
leader_member() { roles_json "$1" | python3 -c "import sys,json; print(next((m['Member'] for m in json.load(sys.stdin) if m['Role']=='Leader'),''))" 2>/dev/null; }
count_leaders() { roles_json "$1" | python3 -c "import sys,json; print(sum(1 for m in json.load(sys.stdin) if m['Role']=='Leader'))" 2>/dev/null; }
streaming_replicas() { roles_json "$1" | python3 -c "import sys,json; print(sum(1 for m in json.load(sys.stdin) if m['Role']=='Replica' and m['State']=='streaming'))" 2>/dev/null; }
# write attempt; returns 0 only if the node is a writable primary
can_write() { docker exec "$1" psql -U postgres -d postgres -v ON_ERROR_STOP=1 -c "$2" >/dev/null 2>&1; }
wait_healthy() { # $1 = a running node to query from
  for _ in $(seq 1 30); do
    [ "$(count_leaders "$1")" = "1" ] && [ "$(streaming_replicas "$1")" = "1" ] && return 0; sleep 3
  done; return 1
}

echo "==> bring up HA cluster + wait for 1 leader + 1 streaming replica"
$COMPOSE up -d >/dev/null
wait_healthy theodb-patroni1 || die "cluster did not form (1 leader + 1 streaming replica)"
docker exec theodb-patroni1 "${CTL[@]}" list 2>/dev/null | head -6

LEADER="theodb-$(leader_member theodb-patroni1)"
SURVIVOR_M=$([ "$LEADER" = theodb-patroni1 ] && echo patroni2 || echo patroni1)
SURVIVOR="theodb-$SURVIVOR_M"
echo "==> phase A: leader=$LEADER survivor=$SURVIVOR"

echo "==> seed vector table on the primary"
docker exec -i "$LEADER" psql -U postgres -d postgres -q -v ON_ERROR_STOP=1 <<'SQL' || die "seed failed on primary"
CREATE EXTENSION IF NOT EXISTS vector;
DROP TABLE IF EXISTS fo;
CREATE TABLE fo (id bigint PRIMARY KEY, e vector(4) NOT NULL);
INSERT INTO fo SELECT g, ('[' || g || ',' || (g+1) || ',' || (g+2) || ',' || (g+3) || ']')::vector FROM generate_series(1,500) g;
SQL
PRE_SUM="$(docker exec "$LEADER" psql -U postgres -d postgres -tAc "SELECT count(*)||':'||md5(string_agg(e::text,',' ORDER BY id)) FROM fo;" | tr -d '[:space:]')"
echo "    pre-kill: $PRE_SUM"

echo "==> wait (deterministic, RPO=0) until the survivor has all 500 rows before the kill"
caught=""
for _ in $(seq 1 20); do
  [ "$(docker exec "$SURVIVOR" psql -U postgres -d postgres -tAc "SELECT count(*) FROM fo;" 2>/dev/null | tr -d '[:space:]')" = "500" ] && { caught=1; break; }
  sleep 1
done
[ -n "$caught" ] || die "survivor did not catch up to 500 rows (replication lag / RPO not 0)"

echo "==> KILL the primary ($LEADER) and measure RTO"
t0=$(date +%s)
docker kill "$LEADER" >/dev/null
rto=""
for _ in $(seq 1 40); do
  can_write "$SURVIVOR" "INSERT INTO fo VALUES (100001, '[9,9,9,9]') ON CONFLICT DO NOTHING;" && { rto=$(( $(date +%s) - t0 )); break; }
  sleep 2
done
[ -n "$rto" ] || die "no node accepted writes after the primary was killed"
echo "    RTO = ${rto}s (target <= ${RTO_TARGET}s)"
[ "$rto" -le "$RTO_TARGET" ] || die "RTO ${rto}s exceeds target ${RTO_TARGET}s"
[ "$(leader_member "$SURVIVOR")" = "$SURVIVOR_M" ] || die "survivor is not the new leader"
POST_SUM="$(docker exec "$SURVIVOR" psql -U postgres -d postgres -tAc "SELECT count(*)||':'||md5(string_agg(e::text,',' ORDER BY id)) FROM fo WHERE id<=500;" | tr -d '[:space:]')"
[ "$POST_SUM" = "$PRE_SUM" ] || die "data not preserved across failover (pre=$PRE_SUM post=$POST_SUM)"

echo "==> restart the killed node; assert it rejoins as a single streaming replica"
docker start "$LEADER" >/dev/null
wait_healthy "$SURVIVOR" || die "killed node did not rejoin as a single streaming replica"

echo "==> phase B: anti-split-brain via network partition"
P_LEADER="theodb-$(leader_member "$SURVIVOR")"      # current primary
P_OTHER=$([ "$P_LEADER" = theodb-patroni1 ] && echo theodb-patroni2 || echo theodb-patroni1)
echo "    partitioning current primary $P_LEADER from the cluster network"
docker network disconnect "$NET" "$P_LEADER" >/dev/null
# the majority side ($P_OTHER) must elect a new primary that accepts writes
newp=""
for _ in $(seq 1 25); do
  can_write "$P_OTHER" "INSERT INTO fo VALUES (200001, '[1,1,1,1]') ON CONFLICT DO NOTHING;" && { newp=1; break; }
  sleep 2
done
[ -n "$newp" ] || { docker network connect "$NET" "$P_LEADER" >/dev/null 2>&1; die "majority did not elect a writable primary after partition"; }
# CRITICAL invariant: at this moment the isolated old primary must NOT accept writes (no two writable primaries)
if can_write "$P_LEADER" "INSERT INTO fo VALUES (200002, '[2,2,2,2]');"; then
  docker network connect "$NET" "$P_LEADER" >/dev/null 2>&1
  die "SPLIT-BRAIN: isolated old primary $P_LEADER still accepts writes while a new primary exists"
fi
echo "    isolated old primary is read-only; new primary $P_OTHER accepts writes — no split-brain"
echo "==> reconnect the partitioned node; assert one leader again"
docker network connect "$NET" "$P_LEADER" >/dev/null
wait_healthy "$P_OTHER" || die "cluster did not heal to a single leader after reconnect"

echo "FAILOVER SMOKE PASSED — RTO=${rto}s (RPO=0), data preserved; partition test: isolated primary went read-only (no split-brain)"
[ -z "${KEEP:-}" ] && { echo "==> teardown"; $COMPOSE down -v >/dev/null 2>&1; } || true
exit 0
