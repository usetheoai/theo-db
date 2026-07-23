#!/usr/bin/env bash
# M146 T3.1 — corruption-injection proof for the persisted HNSW index blob.
#
# WHY THIS EXISTS. `HnswIndex::from_bytes` deserializes a blob read from an index page
# (`am/index.rs` ← `am/scan.rs`). Before M146 it validated node counts and the entry point but NOT the
# neighbour indices, while `search_layer` does `visited[nb]` / `self.vectors[nb]` with no bounds check.
# A semantically-corrupt blob therefore reached `search` and panicked ACROSS THE C FFI BOUNDARY, tearing
# down the backend — the file's own comment claimed to guard exactly this. Neither a unit test nor
# `#[pg_test(error=)]` can prove the fix: the former never touches a real page, the latter cannot observe
# whether the backend survived. Only corrupting a real relation file and querying it can.
#
# METHOD (ported from PostgreSQL's own amcheck TAP suite, `contrib/amcheck/t/001_verify_heapam.pl:178-207`):
#   stop the cluster (shared buffers would otherwise overwrite the edit) → write garbage into the relation
#   file at a byte offset → restart → query → observe. `autovacuum=off` at cluster level and
#   `autovacuum_enabled=false` on the table keep anything from rewriting the injected state.
#
# We do NOT target a "magic" offset for the neighbour array — that would be brittle against any layout
# change. We sweep a RANGE of offsets and assert the PROPERTY that matters to an operator:
#
#     no corruption of the index may kill the backend; every one must surface as a clean SQL error
#     (or, if the corrupted bytes happen to be inert, a correct answer).
#
# Run on a machine with a pgrx PG install (the droplet). Regenerate the extension first:
#   cargo pgrx install --release --features pg18 --pg-config $PGINST/bin/pg_config
#
# Exit 0 = property holds (0 backend deaths). Exit 1 = at least one corruption killed the backend.
set -uo pipefail

PGINST="${PGINST:-$HOME/.pgrx/18.4/pgrx-install}"
export PATH="$PGINST/bin:$PATH"
DATA=/tmp/corrupt_idx_tmp
PORT=${PORT:-59731}
DB=postgres
# WHERE to corrupt, and why. A naive sweep (page_start + small delta) lands in the page HEADER / line-pointer
# array, so PostgreSQL's own page verification rejects it with "invalid page in block N" before our Rust code
# ever deserializes the blob — a real guarantee, but it proves nothing about `from_bytes`. To reach the blob
# PAYLOAD we must write near the END of a page, where PostgreSQL stores item data (it grows backwards from the
# page end, while the header + line pointers grow forward from the start).
# And to reach the NEIGHBOUR array specifically, we target the LATER pages: `to_bytes` lays the blob out as
# vectors → ids → levels → neighbours → entry (`ann/hnsw.rs`), so the neighbour indices live in the final third.
# 8-byte writes keep the damage inside the payload instead of wrecking the page structure.
PAGE=${PAGE:-8192}
CORRUPT_LEN=${CORRUPT_LEN:-8}
OFFSETS_DEFAULT=""
for p in 6 7 8 9 10 11 12 13 14; do
    for d in 6000 6800 7400 7900; do
        OFFSETS_DEFAULT="$OFFSETS_DEFAULT $((p * PAGE + d))"
    done
done
OFFSETS=${OFFSETS:-$OFFSETS_DEFAULT}

cleanup() { pg_ctl -D "$DATA" -m immediate stop -w >/dev/null 2>&1 || true; rm -rf "$DATA"; }
trap cleanup EXIT
rm -rf "$DATA"

# CHECKSUMS: PostgreSQL 18 enables data checksums by default, and `PageIsVerified` then rejects ANY byte-level
# corruption of an index page ("invalid page in block N") BEFORE the extension deserializes the blob. That is a
# real and welcome guarantee — but it means disk corruption alone cannot exercise `from_bytes`. Set
# CHECKSUMS=off to reproduce the supported configuration in which torn/silently-corrupt pages DO reach us.
CHECKSUMS=${CHECKSUMS:-on}
INITDB_FLAGS=""
[ "$CHECKSUMS" = "off" ] && INITDB_FLAGS="--no-data-checksums"
initdb -D "$DATA" -U theo $INITDB_FLAGS >/dev/null 2>&1 || { echo "CORRUPT_HARNESS_FAIL initdb"; exit 1; }
echo "data_checksums=$CHECKSUMS"
{ echo "port=$PORT"; echo "autovacuum=off"; echo "fsync=off"; } >> "$DATA/postgresql.conf"
pg_ctl -D "$DATA" -l "$DATA/log" start -w >/dev/null || { echo "CORRUPT_HARNESS_FAIL start"; exit 1; }

q() { psql -X -q -p "$PORT" -U theo -d "$DB" -tAc "$1" 2>&1; }

q "CREATE EXTENSION theodb_rs CASCADE;" >/dev/null
q "CREATE TABLE v (id int, e vector(8)) WITH (autovacuum_enabled=false);" >/dev/null
q "INSERT INTO v SELECT g, ('[' || g || ',' || (g+1) || ',' || (g+2) || ',' || (g+3) || ',' || (g+4) || ',' || (g+5) || ',' || (g+6) || ',' || (g+7) || ']')::vector(8) FROM generate_series(1,400) g;" >/dev/null
q "CREATE INDEX vi ON v USING theodb_hnsw (e);" >/dev/null

PRE=$(q "SET enable_seqscan=off; SELECT count(*) FROM (SELECT id FROM v ORDER BY e <-> '[1,2,3,4,5,6,7,8]'::vector LIMIT 5) t;")
if [ "$PRE" != "5" ]; then echo "CORRUPT_HARNESS_FAIL baseline query returned '$PRE' (expected 5)"; exit 1; fi

REL=$(q "SELECT pg_relation_filepath('vi');")
FILE="$DATA/$REL"
[ -f "$FILE" ] || { echo "CORRUPT_HARNESS_FAIL index file not found: $FILE"; exit 1; }
SIZE=$(stat -c %s "$FILE")
echo "index file=$REL size=${SIZE}B baseline_topk=$PRE"

DEATHS=0; CLEAN=0; INERT=0; SKIPPED=0; OURS=0; PGGATE=0
for OFF in $OFFSETS; do
    if [ "$OFF" -ge "$SIZE" ]; then SKIPPED=$((SKIPPED+1)); continue; fi
    # Restore a pristine copy each round so the offsets are independent.
    pg_ctl -D "$DATA" -m fast stop -w >/dev/null 2>&1
    [ -f "$FILE.orig" ] || cp "$FILE" "$FILE.orig"
    cp "$FILE.orig" "$FILE"
    dd if=/dev/urandom of="$FILE" bs=1 seek="$OFF" count="$CORRUPT_LEN" conv=notrunc status=none 2>/dev/null
    pg_ctl -D "$DATA" -l "$DATA/log" start -w >/dev/null 2>&1 || { echo "  off=$OFF: cluster refused to restart"; DEATHS=$((DEATHS+1)); continue; }

    OUT=$(psql -X -q -p "$PORT" -U theo -d "$DB" -tAc "SET enable_seqscan=off; SELECT id FROM v ORDER BY e <-> '[1,2,3,4,5,6,7,8]'::vector LIMIT 5;" 2>&1)
    ALIVE=$(psql -X -q -p "$PORT" -U theo -d "$DB" -tAc "SELECT 1;" 2>&1)

    if echo "$OUT" | grep -qiE "server closed the connection|terminating connection|server process .* was terminated|connection to server was lost" \
       || [ "$ALIVE" != "1" ]; then
        DEATHS=$((DEATHS+1))
        echo "  off=$OFF: BACKEND DIED — $(echo "$OUT" | head -1)"
    elif echo "$OUT" | grep -qi "theodb"; then
        CLEAN=$((CLEAN+1)); OURS=$((OURS+1))
        echo "  off=$OFF: OUR typed error — $(echo "$OUT" | head -1 | cut -c1-90)"
    elif echo "$OUT" | grep -qi "^ERROR"; then
        CLEAN=$((CLEAN+1)); PGGATE=$((PGGATE+1))
        echo "  off=$OFF: PG page gate — $(echo "$OUT" | head -1 | cut -c1-80)"
    else
        INERT=$((INERT+1))
        echo "  off=$OFF: inert (query still answered)"
    fi
done

echo "RESULT deaths=$DEATHS clean_errors=$CLEAN (ours=$OURS pg_page_gate=$PGGATE) inert=$INERT skipped=$SKIPPED"
if [ "$DEATHS" -eq 0 ]; then
    echo "CORRUPT_INDEX_OK — no corruption killed the backend; every detected case surfaced as a clean SQL error"
    exit 0
else
    echo "CORRUPT_INDEX_FAIL — $DEATHS corruption(s) tore down the backend (panic across the FFI boundary)"
    exit 1
fi
