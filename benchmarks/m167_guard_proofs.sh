#!/bin/bash
# M167 § 5 — the two guard proofs, as a re-runnable script rather than a transcript.
#
# Every other section of the verdict ships a harness (`m167_paired_ab.sql`, `m167_hits_topk_ab.sql`,
# `m158_ec_harness.sql`, `columnar_type_ab.py`); a reviewer noted § 5 shipped only a log. This is that harness.
#
#   PROOF A — a database created with LOCALE_PROVIDER icu + LC_COLLATE C reports `datcollate = C` while ordering by
#             ICU. Reading `datcollate` alone would admit a text sort key and return wrong rows. Expected: DECLINE.
#             Its counterpart control is M167-C in `m158_ec_harness.sql`: C + libc, same shape, ROUTES. The pair is
#             what proves the `datlocprovider` clause is load-bearing rather than inert.
#   PROOF B — a columnar table is never ANALYSEd (no pgstat counting; `relation_vacuum` is an error stub), so
#             `pg_class.relpages` stays 0 and a relpages-only guard is inert. Expected: DECLINE at 64kB, ROUTE at 1GB.
#
# Usage:  ./m167_guard_proofs.sh          (expects a running cluster with theodb_rs installed)
# Paths are overridable; the defaults are the theo-e2e-runner layout the verdict's artifact was produced on.
P="${PGRX_BIN:-/root/.pgrx/18.4/pgrx-install/bin}"
PGPORT="${PGPORT:-28900}"
PGUSER="${PGUSER:-postgres}"
RUNAS="${RUNAS:-pgtest}"          # initdb refuses root, so the cluster runs as an unprivileged user
PSQL="$P/psql -h localhost -p $PGPORT -U $PGUSER"
echo "=== § 5 guard proofs start $(date -Is) (postmaster: $(su - "$RUNAS" -c "$PSQL -d postgres -tAc \"SELECT pg_postmaster_start_time()\""))  ==="

echo; echo "--- PROOF A: ICU database with datcollate=C must DECLINE a text sort key ---"
su - "$RUNAS" -c "$PSQL -d postgres -c \"DROP DATABASE IF EXISTS m167_icu\"" 2>&1 | tail -1
su - "$RUNAS" -c "$PSQL -d postgres -c \"CREATE DATABASE m167_icu TEMPLATE template0 LOCALE_PROVIDER icu ICU_LOCALE 'en-US' LC_COLLATE 'C' LC_CTYPE 'C'\"" 2>&1 | tail -1
su - "$RUNAS" -c "$PSQL -d m167_icu -c \"CREATE EXTENSION theodb_rs CASCADE\"" 2>&1 | tail -1
su - "$RUNAS" -c "$PSQL -d m167_icu -tAc \"SELECT 'provider='||datlocprovider::text||' datcollate='||datcollate FROM pg_database WHERE datname=current_database()\""
su - "$RUNAS" -c "$PSQL -d m167_icu -c \"
  CREATE TABLE ticu (k int, s text) USING theodb_columnar;
  INSERT INTO ticu SELECT g, 's'||g FROM generate_series(1,2000) g;
  SET theodb.enable_columnar_agg=on; SET theodb.enable_columnar_late_mat=on;
  EXPLAIN (COSTS OFF) SELECT k FROM ticu ORDER BY s LIMIT 10;\"" 2>&1 | grep -E "Sort|Custom Scan|Limit"

echo; echo "--- PROOF B: relpages=0 table -> guard DECLINES at 64kB, ROUTES at 1GB ---"
su - "$RUNAS" -c "$PSQL -d postgres -c \"
  DROP TABLE IF EXISTS m167_guard;
  CREATE TABLE m167_guard (k int, v bigint) USING theodb_columnar;
  INSERT INTO m167_guard SELECT g, g FROM generate_series(1,200000) g;\"" 2>&1 | tail -1
echo "relpages (deve ser 0 — nenhum ANALYZE/VACUUM roda em tabela colunar):"
su - "$RUNAS" -c "$PSQL -d postgres -tAc \"SELECT relpages FROM pg_class WHERE relname='m167_guard'\""
for wm in 64kB 1GB; do
  echo "  work_mem=$wm:"
  su - "$RUNAS" -c "PGOPTIONS='-c work_mem=$wm' $PSQL -d postgres -c \"
    SET theodb.enable_columnar_agg=on; SET theodb.enable_columnar_late_mat=on;
    EXPLAIN (COSTS OFF) SELECT k FROM m167_guard ORDER BY v LIMIT 10;\"" 2>&1 | grep -E "Sort|Custom Scan|Limit" | sed "s/^/    /"
done
echo; echo "=== § 5 guard proofs rc=$? end $(date -Is) ==="
