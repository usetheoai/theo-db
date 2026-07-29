#!/bin/bash
# M167 § 5 — the two guard proofs, as a re-runnable, SELF-ASSERTING harness.
#
# Every other section of the verdict ships a harness (`m167_paired_ab.sql`, `m167_hits_topk_ab.sql`,
# `m158_ec_harness.sql`, `columnar_type_ab.py`); a reviewer noted § 5 shipped only a transcript. This is that
# harness. A later reviewer then noted the FIRST version of this file printed `rc=$?` immediately after an `echo`
# — always 0 — and asserted nothing: both proofs were `EXPLAIN | grep`, so a failed CREATE DATABASE would leave an
# empty grep and close the log green. That is the third occurrence of "printed and hoped" INSIDE the artifact
# written to end it. Every check below now RAISEs inside psql under ON_ERROR_STOP, and the exit code is psql's.
#
#   PROOF A — a database created with LOCALE_PROVIDER icu + LC_COLLATE C reports `datcollate = C` while ordering by
#             ICU. Reading `datcollate` alone would admit a text sort key and return wrong rows. MUST decline.
#             Counterpart control: M167-C in `m158_ec_harness.sql` is C + libc, same shape, and MUST route. That
#             pair is what proves the `datlocprovider` clause is load-bearing rather than inert.
#   PROOF B — a columnar table is never ANALYSEd (no pgstat counting; `relation_vacuum` is an error stub), so
#             `pg_class.relpages` stays 0 and a relpages-only guard is inert. MUST decline at 64kB, route at 1GB.
#
# Usage:  ./m167_guard_proofs.sh                          expects a running cluster with theodb_rs installed
#         PROOF_SELFTEST=1 ./m167_guard_proofs.sh         positive control: Proof B's 64kB expectation is
#                                                         inverted, so the harness MUST exit non-zero. A gate that
#                                                         has never been seen to fail is not known to be a gate.
set -uo pipefail

P="${PGRX_BIN:-/root/.pgrx/18.4/pgrx-install/bin}"
PGPORT="${PGPORT:-28900}"
PGUSER="${PGUSER:-postgres}"
RUNAS="${RUNAS:-pgtest}"          # initdb refuses root, so the cluster runs as an unprivileged user
PSQL="$P/psql -h localhost -p $PGPORT -U $PGUSER"
SELFTEST="${PROOF_SELFTEST:-0}"

as_pg() { su - "$RUNAS" -c "$*"; }

# Binary identity, stamped from the .so itself. A wall-clock cannot pin a run to a commit in this repo, because the
# build necessarily precedes the commit that contains it; the checksum can.
SO_PATH=$(find "$(dirname "$P")" -name theodb_rs.so 2>/dev/null | head -1)
echo "=== § 5 guard proofs start $(date -Is) ==="
echo "postmaster=$(as_pg "$PSQL -d postgres -tAc \"SELECT pg_postmaster_start_time()\"" 2>/dev/null)"
echo "so_path=$SO_PATH"
echo "so_mtime=$(stat -c '%y' "$SO_PATH" 2>/dev/null)"
echo "so_md5=$(md5sum "$SO_PATH" 2>/dev/null | cut -d' ' -f1)"
echo "selftest=$SELFTEST"

# ---------------------------------------------------------------- PROOF A
echo
echo "--- PROOF A: ICU database with datcollate=C MUST DECLINE a text sort key ---"
as_pg "$PSQL -d postgres -q -c \"DROP DATABASE IF EXISTS m167_icu\"" >/dev/null 2>&1
as_pg "$PSQL -d postgres -v ON_ERROR_STOP=1 -q -c \"CREATE DATABASE m167_icu TEMPLATE template0 LOCALE_PROVIDER icu ICU_LOCALE 'en-US' LC_COLLATE 'C' LC_CTYPE 'C'\"" || exit 1
as_pg "$PSQL -d m167_icu -v ON_ERROR_STOP=1 -q -c \"CREATE EXTENSION theodb_rs CASCADE\"" || exit 1

as_pg "$PSQL -d m167_icu -v ON_ERROR_STOP=1 -c \"
  SELECT 'datcollate=' || datcollate || ' datlocprovider=' || datlocprovider::text AS locale_facts
    FROM pg_database WHERE datname = current_database();
  CREATE TABLE ticu (k int, s text) USING theodb_columnar;
  INSERT INTO ticu SELECT g, 's'||g FROM generate_series(1,2000) g;
  SET theodb.enable_columnar_agg = on;
  SET theodb.enable_columnar_late_mat = on;
  DO \\\$a\\\$
  DECLARE plan text; coll text; prov text;
  BEGIN
    SELECT datcollate, datlocprovider::text INTO coll, prov
      FROM pg_database WHERE datname = current_database();
    IF coll <> 'C' OR prov <> 'i' THEN
      RAISE EXCEPTION 'PROOF A PRECONDITION FAILED: need datcollate=C and provider=i, got % / % — the fixture does not reproduce the hole', coll, prov;
    END IF;
    EXECUTE 'EXPLAIN (COSTS OFF, FORMAT JSON) SELECT k FROM ticu ORDER BY s LIMIT 10' INTO plan;
    IF position('theodb_columnar_agg' IN plan) > 0 THEN
      RAISE EXCEPTION 'PROOF A FAILED: an ICU database ADMITTED a text sort key — wrong rows are reachable. Plan: %', plan;
    END IF;
    IF position('Sort' IN plan) = 0 THEN
      RAISE EXCEPTION 'PROOF A INCONCLUSIVE: no Sort node, so the query did not fall back to the native plan. Plan: %', plan;
    END IF;
    RAISE NOTICE 'PROOF A ok: datcollate=C but provider=i, and the text sort key DECLINED to the native Sort';
  END
  \\\$a\\\$;\"" || exit 1

# ---------------------------------------------------------------- PROOF B
echo
echo "--- PROOF B: relpages=0 table MUST decline at 64kB and MUST route at 1GB ---"
as_pg "$PSQL -d postgres -v ON_ERROR_STOP=1 -q -c \"
  DROP TABLE IF EXISTS m167_guard;
  CREATE TABLE m167_guard (k int, v bigint) USING theodb_columnar;
  INSERT INTO m167_guard SELECT g, g FROM generate_series(1,200000) g;\"" || exit 1

as_pg "$PSQL -d postgres -v ON_ERROR_STOP=1 -c \"
  SELECT relpages AS relpages_must_be_zero FROM pg_class WHERE relname = 'm167_guard';
  DO \\\$b\\\$
  DECLARE plan_small text; plan_big text; rp int; selftest bool := ($SELFTEST = 1);
  BEGIN
    SELECT relpages INTO rp FROM pg_class WHERE relname = 'm167_guard';
    IF rp <> 0 THEN
      RAISE EXCEPTION 'PROOF B PRECONDITION FAILED: relpages=% — something ANALYSEd this table, so it no longer tests the inert-guard case', rp;
    END IF;
    SET LOCAL theodb.enable_columnar_agg = on;
    SET LOCAL theodb.enable_columnar_late_mat = on;

    SET LOCAL work_mem = '64kB';
    EXECUTE 'EXPLAIN (COSTS OFF, FORMAT JSON) SELECT k FROM m167_guard ORDER BY v LIMIT 10' INTO plan_small;
    SET LOCAL work_mem = '1GB';
    EXECUTE 'EXPLAIN (COSTS OFF, FORMAT JSON) SELECT k FROM m167_guard ORDER BY v LIMIT 10' INTO plan_big;

    IF selftest THEN
      RAISE NOTICE 'PROOF B SELF-TEST ARMED: the 64kB expectation is inverted below, so this run MUST fail';
      IF position('theodb_columnar_agg' IN plan_small) = 0 THEN
        RAISE EXCEPTION 'PROOF B SELF-TEST fired as designed: at 64kB the plan DECLINED, which the inverted expectation rejects. The real assertion is the opposite and passes.';
      END IF;
    END IF;

    IF position('theodb_columnar_agg' IN plan_small) > 0 THEN
      RAISE EXCEPTION 'PROOF B FAILED: the guard did not fire at work_mem=64kB on a relpages=0 table — it is inert. Plan: %', plan_small;
    END IF;
    IF position('theodb_columnar_agg' IN plan_big) = 0 THEN
      RAISE EXCEPTION 'PROOF B FAILED: did not route at work_mem=1GB, so the decline at 64kB says nothing about the budget. Plan: %', plan_big;
    END IF;
    RAISE NOTICE 'PROOF B ok: relpages=0, DECLINED at 64kB and ROUTED at 1GB — the fallback to the real relation size is what makes the guard bite';
  END
  \\\$b\\\$;\"" || exit 1

echo
echo "=== § 5 guard proofs rc=0 end $(date -Is) ==="
