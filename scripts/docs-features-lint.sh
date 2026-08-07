#!/usr/bin/env bash
# docs-features-lint.sh — regression checks for wiki/features/ doc↔code contract defects
# found by /review (2026-07-22). Each check reproduces one confirmed finding and fails
# while the defect is present. Run from the repo root: bash scripts/docs-features-lint.sh
set -u
FAIL=0

# --- finding 573f6c402ab07757 (BLOCKER, code) -------------------------------
# theodb.graph_expand RETURNS SETOF bigint (theodb_rs/src/graph.rs:557): in FROM its
# default output column is `graph_expand`, not `node`. An unaliased
# `SELECT node FROM theodb.graph_expand(...)` fails with `column "node" does not exist`.
if grep -qE 'SELECT[[:space:]]+node[[:space:]]+FROM[[:space:]]+theodb\.graph_expand' \
    wiki/features/13-grafo-nativo.md; then
  echo "FAIL [573f6c402ab07757] 13-grafo-nativo.md: unaliased 'SELECT node FROM theodb.graph_expand' — copy-breaking (column is 'graph_expand'; alias the SRF: AS t(node))"
  FAIL=1
else
  echo "PASS [573f6c402ab07757] graph_expand example is aliased (or renamed) — copy-safe"
fi

# --- finding d2d823232022541a (HIGH, system_design) -------------------------
# The columnar TAM is INSERT-only: UPDATE/DELETE/tuple-lock/parallel/bitmap/sample/index
# build are typed-error stubs (theodb_rs/src/am/columnar.rs:15). The doc's caveats block
# must disclose this DML contract — a doc that never mentions UPDATE/DELETE hides the
# largest operational caveat.
if grep -qE 'UPDATE.*DELETE|DELETE.*UPDATE' wiki/features/14-analitico-colunar.md; then
  echo "PASS [d2d823232022541a] 14-analitico-colunar.md discloses the INSERT-only DML contract (UPDATE/DELETE caveat present)"
else
  echo "FAIL [d2d823232022541a] 14-analitico-colunar.md: no UPDATE/DELETE typed-error caveat — INSERT-only DML surface undisclosed (columnar.rs:15)"
  FAIL=1
fi

# --- finding 52f0c1037a2d1133 (HIGH, system_design) -------------------------
# The plain seqscan path decodes ALL columns of every stripe (columnar.rs:1015-1021 —
# coldesc for 0..natts); projection pushdown exists only in the M100 CustomScan path.
# m99-columnar-tam.md records plain seqscan as parity-or-slower than heap (~16-26x).
# The doc must not claim projected-only decode for a plain SELECT, and §4 must link the
# measured m99 limit (2+ mentions: header list + §4).
if grep -q 'decodifica apenas os chunks das colunas projetadas' wiki/features/14-analitico-colunar.md; then
  echo "FAIL [52f0c1037a2d1133] 14-analitico-colunar.md: claims projected-only decode for plain seqscan — false (columnar.rs:1015-1021 decodes 0..natts)"
  FAIL=1
elif [ "$(grep -c 'm99-columnar-tam.md' wiki/features/14-analitico-colunar.md)" -lt 2 ]; then
  echo "FAIL [52f0c1037a2d1133] 14-analitico-colunar.md: seqscan section does not cite the measured m99 parity-or-slower limit"
  FAIL=1
else
  echo "PASS [52f0c1037a2d1133] 14-analitico-colunar.md: seqscan decode-all + m99 measured limit disclosed"
fi

exit $FAIL
