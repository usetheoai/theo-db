#!/usr/bin/env bash
# docs-features-lint.sh — regression checks for docs/features/ doc↔code contract defects
# found by /review (2026-07-22). Each check reproduces one confirmed finding and fails
# while the defect is present. Run from the repo root: bash scripts/docs-features-lint.sh
set -u
FAIL=0

# --- finding 573f6c402ab07757 (BLOCKER, code) -------------------------------
# theodb.graph_expand RETURNS SETOF bigint (theodb_rs/src/graph.rs:557): in FROM its
# default output column is `graph_expand`, not `node`. An unaliased
# `SELECT node FROM theodb.graph_expand(...)` fails with `column "node" does not exist`.
if grep -qE 'SELECT[[:space:]]+node[[:space:]]+FROM[[:space:]]+theodb\.graph_expand' \
    docs/features/13-grafo-nativo.md; then
  echo "FAIL [573f6c402ab07757] 13-grafo-nativo.md: unaliased 'SELECT node FROM theodb.graph_expand' — copy-breaking (column is 'graph_expand'; alias the SRF: AS t(node))"
  FAIL=1
else
  echo "PASS [573f6c402ab07757] graph_expand example is aliased (or renamed) — copy-safe"
fi

exit $FAIL
