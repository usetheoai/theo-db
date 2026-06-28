#!/usr/bin/env bash
# T2 consistency gate: every core pg_dump/pg_restore/verification command the migration GUIDE documents
# must also appear (literally) in migrate-smoke.sh — so the published guide never drifts from what is tested.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GUIDE="$HERE/docs/migration/minimal-migration.md"
SMOKE="$HERE/migrate-smoke.sh"

# literal (grep -F) shared commands — the load-bearing migration + verification primitives
SHARED=(
  "pg_dump -Fc"
  "pg_restore --no-owner --exit-on-error"
  "md5(string_agg(id::text || '|' || title || '|' || embedding::text, ',' ORDER BY id))"
  "USING hnsw"
  "USING ivfflat"
)

fail=0
for cmd in "${SHARED[@]}"; do
  if ! grep -qF "$cmd" "$GUIDE"; then echo "MISSING in guide:  $cmd"; fail=1; continue; fi
  if ! grep -qF "$cmd" "$SMOKE"; then echo "MISSING in smoke:  $cmd"; fail=1; continue; fi
  echo "shared ok: $cmd"
done

[ "$fail" -eq 0 ] || { echo "DOC-CHECK FAILED: guide drifted from migrate-smoke.sh"; exit 1; }
echo "DOC-CHECK PASSED — guide commands are real (match migrate-smoke.sh)"
