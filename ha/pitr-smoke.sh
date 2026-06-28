#!/usr/bin/env bash
# M4 DoD-2 — pgBackRest backup + PITR with a VALIDATED restore.
# On the running HA primary: stanza-create + check + full backup (continuous WAL archiving is on via the
# Patroni-managed archive_command). Inserts a "keep" row, records a target timestamp, then makes a
# post-target change. Restores to a THROWAWAY standalone instance from the same repo using
# --type=time --target=<ts>, and asserts: keep row present, post-target change absent.
# Env: KEEP=1 (skip teardown). Prereq: docker + theo-db-ha image + cluster reachable.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPOSE="docker compose -f $HERE/docker-compose.ha.yml"
CTL=(patronictl -c /tmp/patroni.yml)
STANZA=theodb

die() { echo "PITR SMOKE FAILED: $*" >&2; docker rm -f theodb-restore >/dev/null 2>&1; [ -z "${KEEP:-}" ] && $COMPOSE down -v >/dev/null 2>&1; exit 1; }
leader_member() { docker exec "$1" "${CTL[@]}" list -f json 2>/dev/null | python3 -c "import sys,json; print(next((m['Member'] for m in json.load(sys.stdin) if m['Role']=='Leader'),''))"; }

echo "==> ensure cluster up + find primary"
$COMPOSE up -d >/dev/null
LEADER=""
for _ in $(seq 1 30); do
  if docker exec theodb-patroni1 "${CTL[@]}" list >/dev/null 2>&1; then
    m="$(leader_member theodb-patroni1)"; [ -n "$m" ] && LEADER="theodb-$m" && break
  fi
  sleep 3
done
[ -n "$LEADER" ] || die "no primary found"
echo "    primary=$LEADER"

echo "==> pgBackRest: stanza-create + check + full backup"
docker exec "$LEADER" pgbackrest --stanza=$STANZA stanza-create 2>&1 | tail -1 || die "stanza-create failed"
docker exec "$LEADER" pgbackrest --stanza=$STANZA check 2>&1 | tail -1 || die "check failed (archiving not working)"
docker exec "$LEADER" pgbackrest --stanza=$STANZA --type=full backup 2>&1 | tail -1 || die "backup failed"

echo "==> seed keep-row, capture PITR target, then make a post-target change"
docker exec -i "$LEADER" psql -U postgres -d postgres -q -v ON_ERROR_STOP=1 <<'SQL' || die "keep-row insert failed"
CREATE EXTENSION IF NOT EXISTS vector;
DROP TABLE IF EXISTS pitr;
CREATE TABLE pitr (id int PRIMARY KEY, tag text, e vector(3));
INSERT INTO pitr VALUES (1, 'keep', '[1,1,1]');
SELECT pg_switch_wal();
SQL
TARGET="$(docker exec "$LEADER" psql -U postgres -d postgres -tAc "SELECT current_timestamp;" | tr -d '\r')"
echo "    target=$TARGET"
sleep 2
docker exec -i "$LEADER" psql -U postgres -d postgres -q -v ON_ERROR_STOP=1 <<'SQL' || die "post-target change failed"
INSERT INTO pitr VALUES (2, 'BAD_after_target', '[9,9,9]');
DROP TABLE IF EXISTS fo;
SELECT pg_switch_wal();
SQL
# give archiver a moment to push the last WAL segment
sleep 3

echo "==> restore to a throwaway standalone instance at --type=time target"
REPO_VOL="$(docker volume ls --format '{{.Name}}' | grep pgbackrest_repo | head -1)"
[ -n "$REPO_VOL" ] || die "pgbackrest repo volume not found"
docker rm -f theodb-restore >/dev/null 2>&1 || true
docker run -d --name theodb-restore -v "$REPO_VOL":/var/lib/pgbackrest --entrypoint sleep theo-db-ha 3600 >/dev/null
docker exec theodb-restore sh -c 'rm -rf /var/lib/postgresql/data/* 2>/dev/null; mkdir -p /var/lib/postgresql/data; chmod 700 /var/lib/postgresql/data'
docker exec theodb-restore pgbackrest --stanza=$STANZA --type=time "--target=$TARGET" \
  --target-action=promote --delta restore 2>&1 | tail -1 || die "pgbackrest restore failed"
docker exec theodb-restore sh -c '/usr/lib/postgresql/17/bin/pg_ctl -D /var/lib/postgresql/data -w -t 60 start' 2>&1 | tail -2 || die "restored postgres did not start"
# wait for recovery to finish (promote)
for _ in $(seq 1 20); do
  inrec="$(docker exec theodb-restore psql -U postgres -d postgres -tAc "SELECT pg_is_in_recovery();" 2>/dev/null | tr -d '[:space:]')"
  [ "$inrec" = "f" ] && break; sleep 2
done

echo "==> validate the restored state == target (keep present, post-target absent)"
KEEP_ROW="$(docker exec theodb-restore psql -U postgres -d postgres -tAc "SELECT count(*) FROM pitr WHERE id=1 AND tag='keep';" 2>/dev/null | tr -d '[:space:]')"
BAD_ROW="$(docker exec theodb-restore psql -U postgres -d postgres -tAc "SELECT count(*) FROM pitr WHERE id=2;" 2>/dev/null | tr -d '[:space:]')"
[ "$KEEP_ROW" = "1" ] || die "keep row missing after PITR (got '$KEEP_ROW')"
[ "$BAD_ROW" = "0" ] || die "post-target row present after PITR (expected 0, got '$BAD_ROW') — restored past target"

docker rm -f theodb-restore >/dev/null 2>&1
echo "PITR SMOKE PASSED — restored to $TARGET (keep row present, post-target change absent)"
[ -z "${KEEP:-}" ] && { echo "==> teardown"; $COMPOSE down -v >/dev/null 2>&1; } || true
exit 0
