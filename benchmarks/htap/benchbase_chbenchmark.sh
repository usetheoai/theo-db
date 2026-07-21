#!/usr/bin/env bash
# M130 — OUR container-entry script for BenchBase CH-benCHmark against TheoDB. It drives the Apache-2.0 BenchBase
# tool (cmu-db) from a PINNED git SHA inside a Java-23 image — NO BenchBase source is vendored/forked/linked into
# the TheoDB tree (ADR M130-1). Config comes from env (set by run_m130_htap.py). CH-benCHmark = TPC-C mix + 22
# TPC-H-style analytical queries on one schema; the dual metric is DERIVED downstream and labeled a proxy.
set -euo pipefail

: "${BENCHBASE_SHA:?pinned BenchBase SHA required}"
BB_SCALE="${BB_SCALE:-4}"; BB_TERMINALS="${BB_TERMINALS:-4}"; BB_DURATION="${BB_DURATION:-120}"
PG_HOST="${PG_HOST:-localhost}"; PG_PORT="${PG_PORT:-28900}"; PG_USER="${PG_USER:-postgres}"
PG_PASSWORD="${PG_PASSWORD:-postgres}"; PG_DB="${PG_DB:-postgres}"

echo "M130: cloning BenchBase @ ${BENCHBASE_SHA} (Apache-2.0, external — not vendored)"
apt-get update -qq && apt-get install -y -qq git >/dev/null 2>&1 || true
git clone --quiet https://github.com/cmu-db/benchbase /bb
cd /bb
git checkout --quiet "${BENCHBASE_SHA}"

echo "M130: building BenchBase (Java 23, mvnw, postgres profile) — this is the documented Java-23 build liability"
./mvnw -q clean package -P postgres -DskipTests
# The postgres profile packages a distributable tarball (target/benchbase-postgres.tgz); extract it to get the
# runnable dir + benchbase.jar (the built dir does not exist until the tgz is unpacked).
cd target
test -f benchbase-postgres.tgz || { echo "M130: FATAL benchbase-postgres.tgz not produced by the build"; ls -la; exit 1; }
tar xzf benchbase-postgres.tgz
cd benchbase-postgres

# Materialize the config with the runtime PG coordinates + scale/terminals/duration substituted in.
sed -e "s#@PG_HOST@#${PG_HOST}#g" -e "s#@PG_PORT@#${PG_PORT}#g" -e "s#@PG_USER@#${PG_USER}#g" \
    -e "s#@PG_PASSWORD@#${PG_PASSWORD}#g" -e "s#@PG_DB@#${PG_DB}#g" -e "s#@SCALE@#${BB_SCALE}#g" \
    -e "s#@TERMINALS@#${BB_TERMINALS}#g" -e "s#@DURATION@#${BB_DURATION}#g" \
    /chbenchmark_config.xml > /tmp/config.xml

echo "M130: running CH-benCHmark (tpcc + chbenchmark, one mixed phase) against TheoDB ${PG_HOST}:${PG_PORT}"
# tee the full BenchBase output to /out so failures are diagnosable even though the container is --rm. Do not let a
# BenchBase non-zero exit abort before we capture the log (temporarily relax -e around the run).
set +e
java -jar benchbase.jar -b tpcc,chbenchmark -c /tmp/config.xml \
     --create=true --load=true --execute=true -d /out/results 2>&1 | tee /out/benchbase.log
BB_RC=${PIPESTATUS[0]}
set -e
echo "M130: BenchBase exit code = ${BB_RC}"

# BenchBase writes a single combined <bench>_<ts>.summary.json + per-transaction-type results.<Name>.csv into
# /out/results; the driver (run_m130_htap.py) reads them directly — no copy/rename here.
echo "M130: done — outputs in /out/results (exit ${BB_RC})"
exit "${BB_RC}"
