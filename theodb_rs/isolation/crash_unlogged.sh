#!/usr/bin/env bash
# #46 — end-to-end proof that an UNLOGGED table's theodb index survives crash-recovery / standby promotion.
# The fix (theodb_rs/src/am/build.rs::wal_log_init_fork) WAL-logs the INIT fork via log_newpage_range, because
# GenericXLog is a no-op for unlogged relations. Without the WAL records, a standby (which only ever has what
# reached the WAL) never receives the INIT fork → on promotion the unlogged relation is reset from a zeroed INIT
# fork → aminsert fails with "truncated meta page" until REINDEX. The STANDBY-PROMOTION path is the true test
# (a single-node immediate-stop keeps the OS page cache, so it does NOT reliably expose the bug).
#
# Primary → streaming standby (pg_basebackup) → build the unlogged index on the primary → let it replicate →
# promote the standby → on the PROMOTED node, an INSERT into the unlogged table + a scan must WORK.
# Run on the build droplet. Regenerate the extension first.
set -uo pipefail
PGINST="${PGINST:-$HOME/.pgrx/17.10/pgrx-install}"
export PATH="$PGINST/bin:$PATH"
PRIM=/tmp/crash_unlogged_prim
STBY=/tmp/crash_unlogged_stby
PPORT=59722
SPORT=59723

cleanup() {
  pg_ctl -D "$STBY" -m immediate stop -w >/dev/null 2>&1 || true
  pg_ctl -D "$PRIM" -m immediate stop -w >/dev/null 2>&1 || true
  rm -rf "$PRIM" "$STBY"
}
trap cleanup EXIT
rm -rf "$PRIM" "$STBY"

# --- primary ---
initdb -D "$PRIM" -U theo >/dev/null 2>&1
{ echo "port=$PPORT"; echo "wal_level=replica"; echo "max_wal_senders=4"; echo "hot_standby=on";
  echo "fsync=on"; echo "full_page_writes=on"; } >> "$PRIM/postgresql.conf"
echo "host replication theo 127.0.0.1/32 trust" >> "$PRIM/pg_hba.conf"
pg_ctl -D "$PRIM" -l "$PRIM/log" start -w >/dev/null
p() { psql -X -q -p "$PPORT" -U theo -d postgres -tAc "$1" 2>&1; }
p "CREATE EXTENSION theodb_rs;" >/dev/null

# --- streaming standby via base backup ---
pg_basebackup -h 127.0.0.1 -p "$PPORT" -U theo -D "$STBY" -R -X stream >/dev/null 2>&1
{ echo "port=$SPORT"; echo "hot_standby=on"; } >> "$STBY/postgresql.conf"
pg_ctl -D "$STBY" -l "$STBY/log" start -w >/dev/null
s() { psql -X -q -p "$SPORT" -U theo -d postgres -tAc "$1" 2>&1; }

# --- build the UNLOGGED index on the primary (this exercises ambuildempty → wal_log_init_fork) ---
p "CREATE UNLOGGED TABLE u (id int, v vector(8));" >/dev/null
p "INSERT INTO u SELECT g, ('[' || array_to_string(ARRAY(SELECT ((g*7+d*13)%97)::float4 FROM generate_series(1,8) d), ',') || ']')::vector FROM generate_series(1,500) g;" >/dev/null
p "CREATE INDEX uidx ON u USING theodb_ivfflat (v theodb_ivfflat_l2_ops);" >/dev/null
p "CHECKPOINT;" >/dev/null
# let the standby replay up to here
sleep 3

# --- promote the standby (the moment of truth: the standby only has WAL) ---
pg_ctl -D "$STBY" promote -w >/dev/null 2>&1
for i in $(seq 1 30); do [ "$(s "SELECT pg_is_in_recovery()")" = "f" ] && break; sleep 1; done

# --- on the PROMOTED node: the unlogged table is reset from its INIT fork; INSERT + scan must WORK ---
# (unlogged data does not replicate — the table is empty post-reset, but the INDEX must be valid & insertable)
INS=$(s "INSERT INTO u VALUES (1, '[1,2,3,4,5,6,7,8]'::vector);")
SCAN=$(s "SET enable_seqscan=off; SET theodb_ivfflat.probes=16; SELECT count(*) FROM (SELECT id FROM u ORDER BY v <-> '[1,2,3,4,5,6,7,8]'::vector LIMIT 5) x;")

echo "PROMOTE_INSERT=[$INS] SCAN_COUNT=[$SCAN]"
if echo "$INS$SCAN" | grep -qiE 'truncated|reindex|error'; then
  echo "CRASH_UNLOGGED_FAIL — the promoted unlogged index is broken (INIT fork not WAL-logged): $INS $SCAN"
  exit 1
elif [ "$SCAN" = "1" ]; then
  echo "CRASH_UNLOGGED_OK — the unlogged index survived standby promotion; INSERT + scan work on the promoted node"
  exit 0
else
  echo "CRASH_UNLOGGED_FAIL — unexpected state: INS=[$INS] SCAN=[$SCAN]"
  exit 1
fi
