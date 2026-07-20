# Review — M120 fail-closed filter + M121 spherical k-means (honest-negative)

**Date:** 2026-07-20
**Slug:** hybrid-fail-closed-filter (M120) + ivf-spherical-kmeans (M121)
**Branch:** develop (commits `55b908b`, `675c8f4`, `36505ca` ahead of main)
**Verdict:** READY_TO_MERGE

## Scope

Two milestones delivered on develop since v0.106.0:

- **M120** — fail-closed structured `filter` for `ai.hybrid_search` (`theodb_rs/src/hybrid.rs`).
- **M121** — IVF cosine/ip spherical k-means investigation → **honest-negative** (no code shipped; benchmark doc only).

## M120 — reviewed by council-security (domain-appropriate reviewer)

The substantive gate for a security milestone is the security lens. `council-security` reviewed the structured-filter
composition and found:

- **Injection fail-closed COMPLETE** — no structured-path byte reaches the raw `%5$s` slot without
  `quote_identifier` (col) + `quote_literal`/bare-numeric (value) + operator allowlist. Verified in-PG: parity with
  `filter_sql`, un-allowlisted op → 22023, injection value (`DROP TABLE`) quoted-as-literal → table survives.
- **[MEDIUM → FIXED]** a present-but-non-array `filter` was fail-OPEN (silently ran unfiltered) → now SQLSTATE 22023
  (`675c8f4`). Re-validated in-PG (test 5).
- **[LOW → FIXED]** empty `IN`/`&&` array → typed 22023 instead of a raw syntax error (`675c8f4`). Re-validated (test 6).
- **[INFO]** tenant isolation is correctly out of scope (a positive predicate is not a forced `workspace_id`
  boundary; isolation is a GRANT/RLS concern). `filter_sql` retained as a documented caller-privilege escape hatch.

All findings resolved. Evidence: `docs/security/m120-fail-closed-filter.md`, `scratchpad/m120_ab.sql` (6/6 pass).

### Gate checks (M120)
- Tests: in-PG A/B, 6/6 pass (parity + 4 fail-closed assertions + 2 review-fix assertions).
- No secrets committed. No `Co-Authored-By` trailer. No direct main commits.
- CHANGELOG `[Unreleased]` updated. Wiring: the structured path is reached from the `ai.hybrid_search` jsonb surface (caller present).

## M121 — honest-negative, docs-only (low review surface)

M121 shipped **zero production code** (the spherical implementation was reverted to byte-identical per the DoD's
measurement-first honest-negative gate). The deliverable is `docs/benchmarks/m121-spherical-kmeans-honest-negative.md`:
a scale-invariance **proof** (cosine) plus a measured A/B (`recall_mean == recall_spherical`, cosine + ip). EXPLAIN
confirmed the index was exercised (no seqscan false-positive). Nothing to review beyond the honesty of the framing,
which is explicit about the uniform-dataset ceiling caveat.

### Gate checks (M121)
- No code delta (`git diff` on the 4 candidate files is empty — byte-identical revert).
- CHANGELOG `[Unreleased] § Changed` documents the honest-negative + the revert decision.
- Honest framing per Unbreakable Rule 3: the cosine no-op is a proof; the ip result is measured-identical with an
  explicit ceiling caveat; no overclaim.

## Verdict

**READY_TO_MERGE** — no BLOCKER, no unresolved HIGH. M120's security findings are fixed + re-validated in-PG; M121
is a documented honest-negative with no code surface. Proceed to `/release`.
