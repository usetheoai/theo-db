#!/usr/bin/env bash
# M1 DoD-3 — reproducible license due-diligence: prove ZERO AGPL in the TheoDB package.
# (a) apt system packages in the image; (b) the pgvectorscale Rust crate tree statically linked into
# vectorscale.so (pinned commit). Exits non-zero if any real AGPL/Affero license is found.
# This is the deterministic implementation of the DoD-3 license gate (see docs/packaging/license-audit.md
# § "Tool note" for why this is used in lieu of the loop-check-licence multi-agent plugin).
set -euo pipefail
IMG="${IMG:-theo-db:dev}"
PGVS_REF="${PGVS_REF:-57c88b7b4fe40a2afa20b195f60047a983279c19}"
fail=0

echo "== (a) apt system packages — AGPL/Affero scan over /usr/share/doc/*/copyright =="
hits="$(docker run --rm --entrypoint sh "$IMG" -c 'grep -rliE "affero|agpl" /usr/share/doc/*/copyright 2>/dev/null || true')"
for f in $hits; do
  # ca-certificates is a known false positive: the MPL tri-license prose *enumerates* AGPL; the
  # package itself is GPL-2+/MPL-2.0. Any OTHER hit is a real finding.
  case "$f" in
    */ca-certificates/copyright) echo "  (false positive, MPL/GPL-2+) $f" ;;
    *) echo "  AGPL FINDING: $f"; fail=1 ;;
  esac
done
[ -z "$hits" ] && echo "  no matches"

echo "== (b) pgvectorscale Rust crate tree (cargo metadata @ $PGVS_REF) — AGPL scan =="
docker run --rm rust:1-slim sh -c "
  set -e
  apt-get update >/dev/null 2>&1 && apt-get install -y --no-install-recommends git ca-certificates >/dev/null 2>&1
  git clone -q --filter=blob:none https://github.com/timescale/pgvectorscale /tmp/pgvs
  cd /tmp/pgvs && git checkout -q $PGVS_REF && cd pgvectorscale
  cargo metadata --format-version 1 2>/dev/null
" | python3 -c "
import json,sys,collections
d=json.load(sys.stdin); pkgs=d['packages']
agpl=[p['name'] for p in pkgs if 'AGPL' in (p.get('license') or '').upper() or 'AFFERO' in (p.get('license') or '').upper()]
c=collections.Counter((p.get('license') or 'workspace-crate (PostgreSQL License)') for p in pkgs)
print(f'  total crates: {len(pkgs)}')
print(f'  AGPL/Affero crates: {len(agpl)} {agpl}')
for lic,n in c.most_common(): print(f'    {n:3} {lic}')
sys.exit(1 if agpl else 0)
" || fail=1

echo "======================================================"
if [ "$fail" -eq 0 ]; then echo "LICENSE SWEEP PASSED — zero AGPL in the TheoDB package"; else echo "LICENSE SWEEP FAILED — AGPL found"; fi
exit "$fail"
