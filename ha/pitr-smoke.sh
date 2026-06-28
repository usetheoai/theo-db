#!/usr/bin/env bash
# M4 DoD-2 — pgBackRest backup + PITR with a VALIDATED restore (happy path) + a NEGATIVE case.
# On the running HA primary: stanza-create + check + full backup (continuous WAL archiving is on via the
# Patroni-managed archive_command). Inserts a "keep" row, records a target timestamp (strictly AFTER the
# backup stop time), makes a post-target change, waits until the relevant WAL is actually archived, then
# restores to a THROWAWAY standalone instance with --type=time and asserts: keep present, post-target absent.
# Negative: a target before any backup must FAIL cleanly (no silent wrong restore).
# Env: KEEP=1 (skip teardown). Prereq: docker + theo-db-ha image.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPOSE="docker compose -f $HERE/docker-compose.ha.yml"
CTL=(patronictl -c /tmp/patroni.yml)
STANZA=theodb
REPO_VOL=theodb-ha_pgbackrest_repo

cleanup_restore() { docker rm -f theodb-restore >/dev/null 2>&1 || true; }
die() { echo "PITR SMOKE FAILED: $*" >&2; cleanup_restore; [ -z "${KEEP:-}" ] && $COMPOSE down -v >/dev/null 2>&1; exit 1; }
trap cleanup_restore EXIT
leader_member() { docker exec "$1" "${CTL[@]}" list -f json 2>/dev/null | python3 -c "import sys,json; print(next((m['Member'] for m in json.load(sys.stdin) if m['Role']=='Leader'),''))"; }
psql_p() { docker exec "$LEADER" psql -U postgres -d postgres -tAc "$1" | tr -d '[:space:]'; }

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

# The PITR target MUST be strictly after the backup stop time (pgBackRest auto-select compares at 1s
# granularity, strictly-less-than). Sleep past the second boundary before capturing the target.
sleep 2

echo "==> seed keep-row, capture PITR target, then make a post-target change (same WAL segment)"
docker exec -i "$LEADER" psql -U postgres -d postgres -q -v ON_ERROR_STOP=1 <<'SQL' || die "keep-row insert failed"
CREATE EXTENSION IF NOT EXISTS vector;
DROP TABLE IF EXISTS pitr;
CREATE TABLE pitr (id int PRIMARY KEY, tag text, e vector(3));
INSERT INTO pitr VALUES (1, 'keep', '[1,1,1]');
SQL
# preserve the internal space in the timestamptz (strip only CR/trailing newline)
TARGET="$(docker exec "$LEADER" psql -U postgres -d postgres -tAc "SELECT current_timestamp" | tr -d '\r')"
echo "    target=$TARGET"
sleep 2
docker exec -i "$LEADER" psql -U postgres -d postgres -q -v ON_ERROR_STOP=1 <<'SQL' || die "post-target change failed"
INSERT INTO pitr VALUES (2, 'BAD_after_target', '[9,9,9]');
SQL

echo "==> force-archive the WAL segment holding keep+target+bad, and wait for it deterministically"
CUR="$(psql_p "SELECT pg_walfile_name(pg_current_wal_lsn())")"   # segment with all the above (no switch yet)
docker exec "$LEADER" psql -U postgres -d postgres -tAc "SELECT pg_switch_wal();" >/dev/null  # completes CUR
archived=""
for _ in $(seq 1 60); do
  la="$(psql_p "SELECT coalesce(last_archived_wal,'') FROM pg_stat_archiver")"
  # la >= CUR lexically (zero-padded hex WAL names sort correctly) => CUR is in the repo
  [ -n "$la" ] && [ ! "$la" \< "$CUR" ] && { archived=1; break; }
  sleep 1
done
[ -n "$archived" ] || die "WAL segment $CUR not archived in time (last_archived=$(psql_p "SELECT coalesce(last_archived_wal,'none') FROM pg_stat_archiver"))"

restore_to() {  # $1 = target timestamp ; echoes pgbackrest exit via return
  cleanup_restore
  docker run -d --name theodb-restore -v "$REPO_VOL":/var/lib/pgbackrest --entrypoint sleep theo-db-ha 3600 >/dev/null
  docker exec theodb-restore sh -c 'rm -rf /var/lib/postgresql/data/* 2>/dev/null; mkdir -p /var/lib/postgresql/data; chmod 700 /var/lib/postgresql/data'
  docker exec theodb-restore pgbackrest --stanza=$STANZA --type=time "--target=$1" --target-action=promote restore
}

echo "==> [happy path] restore to a throwaway standalone instance at --type=time target"
restore_to "$TARGET" 2>&1 | tail -1 || die "pgbackrest restore failed for a valid target"
docker exec theodb-restore sh -c '/usr/lib/postgresql/17/bin/pg_ctl -D /var/lib/postgresql/data -w -t 60 start' 2>&1 | tail -1 || die "restored postgres did not start"
promoted=""
for _ in $(seq 1 20); do
  [ "$(docker exec theodb-restore psql -U postgres -d postgres -tAc "SELECT pg_is_in_recovery();" 2>/dev/null | tr -d '[:space:]')" = "f" ] && { promoted=1; break; }
  sleep 2
done
[ -n "$promoted" ] || die "restored instance never promoted out of recovery"

echo "==> validate restored state == target (keep present, post-target absent)"
KEEP_ROW="$(docker exec theodb-restore psql -U postgres -d postgres -tAc "SELECT count(*) FROM pitr WHERE id=1 AND tag='keep';" 2>/dev/null | tr -d '[:space:]')"
BAD_ROW="$(docker exec theodb-restore psql -U postgres -d postgres -tAc "SELECT count(*) FROM pitr WHERE id=2;" 2>/dev/null | tr -d '[:space:]')"
[ "$KEEP_ROW" = "1" ] || die "keep row missing after PITR (got '$KEEP_ROW')"
[ "$BAD_ROW" = "0" ] || die "post-target row present after PITR (expected 0, got '$BAD_ROW') — restored past target"

echo "==> [negative] a target before any backup must FAIL cleanly (no silent wrong restore)"
if restore_to "2000-01-01 00:00:00+00" >/dev/null 2>&1; then
  die "restore to an impossible (pre-backup) target unexpectedly SUCCEEDED"
fi
echo "    negative case OK — pgBackRest refused the impossible target"
cleanup_restore

echo "PITR SMOKE PASSED — restored to $TARGET (keep present, post-target absent); impossible target rejected"
[ -z "${KEEP:-}" ] && { echo "==> teardown"; $COMPOSE down -v >/dev/null 2>&1; } || true
exit 0
