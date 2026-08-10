#!/usr/bin/env bash
# M1 DoD-3 — reproducible license due-diligence: prove ZERO AGPL in the TheoDB package.
# (a) apt system packages in the image; (b) the pgvectorscale Rust crate tree statically linked into
# vectorscale.so (pinned commit). Exits non-zero if any real AGPL/Affero license is found.
# This is the deterministic implementation of the DoD-3 license gate (see wiki/references/license-audit.md
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

echo "== (c) BM25 candidates (M7-S2) — license verdict from each canonical repo (informational: not in the shipped image) =="
# Reproducible identification (ADR 0003): pg_textsearch must be permissive (PostgreSQL License);
# VectorChord-bm25 must be AGPL/Elastic (barred). A license that cannot be fetched is UNVERIFIED (never
# assumed permissive). These candidates are NOT in the distribution image, so they do not affect the
# distribution AGPL gate above — this block records the M7-S2 identification as re-runnable evidence.
# NOTE: §(c) exit is advisory (informational identification); D1 for the SHIPPED image is gated by (a)+(b).
bm25_license() {
  # $1 = label, $2 = raw LICENSE url, $3 = expected: 'permissive' | 'barred'
  local label="$1" url="$2" expect="$3" body
  body="$(curl -fsSL --max-time 20 "$url" 2>/dev/null || true)"
  if [ -z "$body" ]; then
    echo "  UNVERIFIED $label (could not fetch $url) — not assumed permissive"
    return 0
  fi
  if printf '%s' "$body" | grep -qiE 'affero|agpl|elastic license'; then
    if [ "$expect" = "barred" ]; then
      echo "  $label: AGPL/Elastic (barred by D1) — confirmed"
    else
      echo "  AGPL FINDING (unexpected): $label is AGPL/Elastic"; fail=1
    fi
  elif printf '%s' "$body" | grep -qiE 'postgresql license|permission to use, copy, modify'; then
    echo "  $label: PostgreSQL License (permissive) — confirmed"
  elif printf '%s' "$body" | grep -qiE 'apache license|MIT License|BSD|permission is hereby granted'; then
    # 'permission is hereby granted' = the MIT/ISC body (some repos ship the MIT text without the "MIT License" title)
    echo "  $label: permissive (Apache/MIT/BSD) — confirmed"
  else
    echo "  UNVERIFIED $label (license text not recognized) — not assumed permissive"
  fi
}
bm25_license "pg_textsearch" "https://raw.githubusercontent.com/timescale/pg_textsearch/v1.3.1/LICENSE" "permissive"
bm25_license "VectorChord-bm25" "https://raw.githubusercontent.com/tensorchord/VectorChord-bm25/0.3.0/LICENSE" "barred"

echo "== (e) Columnar candidates (M6) — license verdict from each canonical repo (informational: not in the shipped image) =="
# pg_mooncake + pg_duckdb (the columnar/HTAP pieces) must be permissive (MIT). Same reproducible, fail-closed
# discipline as § (c). These are NOT in the distribution image (measurement-first gate) — informational record.
bm25_license "pg_mooncake" "https://raw.githubusercontent.com/Mooncake-Labs/pg_mooncake/v0.1.2/LICENSE" "permissive"
bm25_license "pg_duckdb" "https://raw.githubusercontent.com/duckdb/pg_duckdb/v1.0.0/LICENSE" "permissive"
bm25_license "duckdb" "https://raw.githubusercontent.com/duckdb/duckdb/v1.1.3/LICENSE" "permissive"

echo "======================================================"
if [ "$fail" -eq 0 ]; then echo "LICENSE SWEEP PASSED — zero AGPL in the TheoDB package"; else echo "LICENSE SWEEP FAILED — AGPL found"; fi
exit "$fail"
