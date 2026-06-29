#!/usr/bin/env bash
# M3 minimal-migration smoke: migrate a vanilla PostgreSQL+pgvector database into TheoDB via the
# STANDARD pg_dump/pg_restore (Rule 9 — no bespoke tool), and assert data + indexes are preserved.
#
# Hermetic by design: runs pg_dump/pg_restore/psql INSIDE the two containers (PG17 client tools live
# there — a PG<17 host client cannot dump a PG17 server), so it behaves identically locally and in CI.
# Prereq: docker + a source container (vanilla pgvector) and a target container (TheoDB) already up.
# Bring them up locally with:
#   docker run -d --name m3-src -e POSTGRES_PASSWORD=postgres pgvector/pgvector:pg17
#   docker run -d --name m3-dst -e POSTGRES_PASSWORD=postgres theo-db:dev
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
# Full-row integrity oracle: hashes id + title + embedding (not just the vector) so a change to ANY
# column is caught. ORDER BY id makes physical row order irrelevant but logical mispairing detectable.
CHECKSUM_SQL="SELECT md5(string_agg(id::text || '|' || title || '|' || embedding::text, ',' ORDER BY id)) FROM items;"
INDEXDEFS_SQL="SELECT string_agg(indexdef, E'\n' ORDER BY indexname) FROM pg_indexes WHERE tablename='items';"

die() { echo "MIGRATION SMOKE FAILED: $*" >&2; exit 1; }
src_psql() { docker exec -i -e PGPASSWORD="$PW" "$SRC_CONTAINER" psql -U postgres -d "$1" -v ON_ERROR_STOP=1 "${@:2}"; }
dst_psql() { docker exec -i -e PGPASSWORD="$PW" "$DST_CONTAINER" psql -U postgres -d "$1" -v ON_ERROR_STOP=1 "${@:2}"; }

# Real readiness: a trivial query must succeed (pg_isready can report ready during the initdb temp-server
# window before the server actually accepts client queries).
wait_ready() {
  local c="$1"
  for _ in $(seq 1 30); do
    docker exec -e PGPASSWORD="$PW" "$c" psql -U postgres -d postgres -tAc "SELECT 1" >/dev/null 2>&1 && return 0
    sleep 2
  done
  die "$c did not become ready"
}

# Assert an ANN index is BOTH planner-chosen (EXPLAIN with the sibling index dropped) AND executable
# (the query actually returns k rows). Args: db, expected_index, sibling_to_drop, extra_set_sql.
assert_index_used() {
  local db="$1" idx="$2" drop="$3" extra="${4:-}" plan rows
  plan="$(dst_psql "$db" <<SQL
BEGIN;
DROP INDEX $drop;
SET LOCAL enable_seqscan = off;
$extra
EXPLAIN (COSTS OFF) SELECT id FROM items ORDER BY embedding <-> '${QVEC}' LIMIT 5;
ROLLBACK;
SQL
)"
  grep -q "$idx" <<<"$plan" || die "$idx not used after restore (planner: $(echo "$plan" | tr '\n' ' '))"
  rows="$(dst_psql "$db" -tAc "SELECT count(*) FROM (SELECT id FROM items ORDER BY embedding <-> '${QVEC}' LIMIT 5) t;" | tr -d '[:space:]')"
  [ "$rows" = "5" ] || die "$idx query returned $rows rows, expected 5"
}

if [ -z "${VERIFY_ONLY:-}" ]; then
  echo "==> readiness gate"
  wait_ready "$SRC_CONTAINER"
  wait_ready "$DST_CONTAINER"

  echo "==> seed source (vanilla pgvector): items table + deterministic vectors + non-ASCII titles + hnsw/ivfflat/btree"
  src_psql postgres <<'SQL'
CREATE EXTENSION IF NOT EXISTS vector;
DROP TABLE IF EXISTS items;
CREATE TABLE items (id bigserial PRIMARY KEY, title text NOT NULL, embedding vector(8) NOT NULL);
-- non-ASCII title ('ítem … café 日本語') proves text columns survive the dump/restore, not just the vector
INSERT INTO items (title, embedding)
SELECT 'ítem ' || g || ' café 日本語',
       ARRAY(SELECT round(((((g*8+d) % 97)::numeric)/97.0), 6) FROM generate_series(1,8) d)::vector
FROM generate_series(1, 1000) g;
CREATE INDEX items_hnsw  ON items USING hnsw (embedding vector_l2_ops);
CREATE INDEX items_ivf   ON items USING ivfflat (embedding vector_l2_ops) WITH (lists = 10);
CREATE INDEX items_title ON items (title);
ANALYZE items;
SQL

  echo "==> fresh target database $DST_DB"
  dst_psql postgres -c "DROP DATABASE IF EXISTS $DST_DB WITH (FORCE);"
  dst_psql postgres -c "CREATE DATABASE $DST_DB;"

  echo "==> migrate: pg_dump -Fc (source) | pg_restore --no-owner --exit-on-error (target)"
  docker exec -e PGPASSWORD="$PW" "$SRC_CONTAINER" pg_dump -Fc -U postgres -d postgres \
    | docker exec -i -e PGPASSWORD="$PW" "$DST_CONTAINER" pg_restore --no-owner --exit-on-error -U postgres -d "$DST_DB"
fi

echo "==> verify: row count preserved"
src_rows="$(src_psql postgres -tAc "SELECT count(*) FROM items;" | tr -d '[:space:]')"
dst_rows="$(dst_psql "$DST_DB" -tAc "SELECT count(*) FROM items;" | tr -d '[:space:]')"
[ "$src_rows" = "$dst_rows" ] && [ -n "$src_rows" ] || die "row count mismatch (src=$src_rows dst=$dst_rows)"

echo "==> verify: full-row data integrity (checksum source == target)"
src_sum="$(src_psql postgres -tAc "$CHECKSUM_SQL" | tr -d '[:space:]')"
dst_sum="$(dst_psql "$DST_DB" -tAc "$CHECKSUM_SQL" | tr -d '[:space:]')"
[ -n "$src_sum" ] || die "source checksum empty (no data?)"
[ "$src_sum" = "$dst_sum" ] || die "data checksum mismatch (src=$src_sum dst=$dst_sum)"

echo "==> verify: index DEFINITIONS preserved (kind + opclass, not just count)"
src_idx="$(src_psql postgres -tAc "$INDEXDEFS_SQL")"
dst_idx="$(dst_psql "$DST_DB" -tAc "$INDEXDEFS_SQL")"
[ "$src_idx" = "$dst_idx" ] || die "index definitions differ:
--- source ---
$src_idx
--- target ---
$dst_idx"

echo "==> verify: HNSW + IVFFlat indexes usable on the migrated table"
assert_index_used "$DST_DB" items_hnsw items_ivf
assert_index_used "$DST_DB" items_ivf items_hnsw "SET LOCAL ivfflat.probes = 10;"

if [ -z "${KEEP:-}" ] && [ -z "${VERIFY_ONLY:-}" ]; then
  echo "==> cleanup"
  dst_psql postgres -c "DROP DATABASE IF EXISTS $DST_DB WITH (FORCE);" >/dev/null
  src_psql postgres -c "DROP TABLE IF EXISTS items;" >/dev/null
fi

echo "MIGRATION SMOKE PASSED — $src_rows rows, full-row checksum $src_sum, index defs match, HNSW+IVFFlat usable"
