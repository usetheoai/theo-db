#!/usr/bin/env bash
# M3 minimal-migration smoke: migrate a vanilla PostgreSQL+pgvector database into TheoDB via the
# STANDARD pg_dump/pg_restore (Rule 9 — no bespoke tool), and assert data + indexes are preserved.
#
# Hermetic by design: runs pg_dump/pg_restore/psql INSIDE the two containers (PG17 client tools live
# there — a PG<17 host client cannot dump a PG17 server), so it behaves identically locally and in CI.
# Prereq: docker + a source container (vanilla pgvector) and a target container (TheoDB) already up.
#
# Env (defaults): SRC_CONTAINER=m3-src  DST_CONTAINER=m3-dst  PW=postgres  DST_DB=migrate_smoke
# Modes: KEEP=1 keeps the seeded source table + target db (no cleanup); VERIFY_ONLY=1 re-runs only the
#        integrity asserts against the already-migrated state (used by migrate-smoke-selftest.sh).
set -euo pipefail

SRC_CONTAINER="${SRC_CONTAINER:-m3-src}"
DST_CONTAINER="${DST_CONTAINER:-m3-dst}"
PW="${PW:-postgres}"
DST_DB="${DST_DB:-migrate_smoke}"
QVEC="[0.1,0.2,0.3,0.4,0.5,0.6,0.7,0.8]"
CHECKSUM_SQL="SELECT md5(string_agg(embedding::text, ',' ORDER BY id)) FROM items;"

die() { echo "MIGRATION SMOKE FAILED: $*" >&2; exit 1; }
src_psql() { docker exec -i -e PGPASSWORD="$PW" "$SRC_CONTAINER" psql -U postgres -d "$1" -v ON_ERROR_STOP=1 "${@:2}"; }
dst_psql() { docker exec -i -e PGPASSWORD="$PW" "$DST_CONTAINER" psql -U postgres -d "$1" -v ON_ERROR_STOP=1 "${@:2}"; }

if [ -z "${VERIFY_ONLY:-}" ]; then
  echo "==> readiness gate"
  docker exec "$SRC_CONTAINER" pg_isready -U postgres -q || die "source DB not ready"
  docker exec "$DST_CONTAINER" pg_isready -U postgres -q || die "target DB not ready"

  echo "==> seed source (vanilla pgvector): items table + deterministic vectors + hnsw/ivfflat/btree"
  src_psql postgres <<'SQL'
CREATE EXTENSION IF NOT EXISTS vector;
DROP TABLE IF EXISTS items;
CREATE TABLE items (id bigserial PRIMARY KEY, title text NOT NULL, embedding vector(8) NOT NULL);
INSERT INTO items (title, embedding)
SELECT 'item ' || g,
       ARRAY(SELECT round(((((g*8+d) % 97)::numeric)/97.0), 6) FROM generate_series(1,8) d)::vector
FROM generate_series(1, 1000) g;
CREATE INDEX items_hnsw  ON items USING hnsw (embedding vector_l2_ops);
CREATE INDEX items_ivf   ON items USING ivfflat (embedding vector_l2_ops) WITH (lists = 10);
CREATE INDEX items_title ON items (title);
ANALYZE items;
SQL

  echo "==> fresh target database $DST_DB"
  dst_psql postgres -c "DROP DATABASE IF EXISTS $DST_DB;"
  dst_psql postgres -c "CREATE DATABASE $DST_DB;"

  echo "==> migrate: pg_dump -Fc (source) | pg_restore --no-owner (target)"
  docker exec -e PGPASSWORD="$PW" "$SRC_CONTAINER" pg_dump -Fc -U postgres -d postgres \
    | docker exec -i -e PGPASSWORD="$PW" "$DST_CONTAINER" pg_restore --no-owner -U postgres -d "$DST_DB"
fi

echo "==> verify: data integrity (checksum source == target)"
src_sum="$(src_psql postgres -tAc "$CHECKSUM_SQL" | tr -d '[:space:]')"
dst_sum="$(dst_psql "$DST_DB" -tAc "$CHECKSUM_SQL" | tr -d '[:space:]')"
[ -n "$src_sum" ] || die "source checksum empty (no data?)"
[ "$src_sum" = "$dst_sum" ] || die "data checksum mismatch (src=$src_sum dst=$dst_sum)"

echo "==> verify: all 4 indexes preserved"
idx="$(dst_psql "$DST_DB" -tAc "SELECT count(*) FROM pg_indexes WHERE tablename='items';" | tr -d '[:space:]')"
[ "$idx" = "4" ] || die "index count mismatch: got $idx, expected 4"

echo "==> verify: HNSW index usable on the migrated table"
plan="$(dst_psql "$DST_DB" <<SQL
BEGIN;
DROP INDEX items_ivf;
SET LOCAL enable_seqscan = off;
EXPLAIN (COSTS OFF) SELECT id FROM items ORDER BY embedding <-> '${QVEC}' LIMIT 5;
ROLLBACK;
SQL
)"
grep -q "items_hnsw" <<<"$plan" || die "HNSW index not used after restore (planner: $(echo "$plan" | tr '\n' ' '))"

if [ -z "${KEEP:-}" ] && [ -z "${VERIFY_ONLY:-}" ]; then
  echo "==> cleanup"
  dst_psql postgres -c "DROP DATABASE IF EXISTS $DST_DB;" >/dev/null
  src_psql postgres -c "DROP TABLE IF EXISTS items;" >/dev/null
fi

echo "MIGRATION SMOKE PASSED — data checksum $src_sum, 4 indexes, HNSW usable"
