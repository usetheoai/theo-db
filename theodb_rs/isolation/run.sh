#!/usr/bin/env bash
# M99 D1 — run the columnar MVCC isolation permutation specs against a temp instance of the pgrx-managed
# PostgreSQL (which has the theodb_rs extension installed). CI does not run cargo pgrx test, so this is a
# standalone harness (Citus-style), run on the build droplet. Usage: bash run.sh [spec...]
set -euo pipefail
PGINST="${PGINST:-$HOME/.pgrx/17.10/pgrx-install}"
ISODIR="$PGINST/lib/postgresql/pgxs/src/test/isolation"
HERE="$(cd "$(dirname "$0")" && pwd)"
SPECS=("${@:-columnar_write_concurrency columnar_reader_vs_writer columnar_abort_vs_reader}")
export PATH="$PGINST/bin:$PATH"
rm -rf /tmp/iso_tmp
"$ISODIR/pg_isolation_regress" \
    --bindir="$PGINST/bin" \
    --inputdir="$HERE" \
    --outputdir="$HERE" \
    --temp-instance=/tmp/iso_tmp \
    --port=59713 \
    ${SPECS[@]}
