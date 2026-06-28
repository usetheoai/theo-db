# Review — M3 Minimal Migration (vanilla PostgreSQL → TheoDB)

**Date:** 2026-06-27
**Verdict:** READY_TO_MERGE
**Slug:** m3-minimal-migration
**Commits reviewed:** `f313f75` (feat) → `6833d36` (review fixes)
**Plan:** `.claude/knowledge-base/plans/m3-minimal-migration-plan.md` (plan-confidence SHIPPABLE 100)
**Blueprint:** `.claude/knowledge-base/discoveries/blueprints/m3-minimal-migration-blueprint.md`

## DoD status (evidence-backed)

| DoD | Requirement | Status | Evidence |
|---|---|---|---|
| 1 | `pg_dump`/`pg_restore` documented AND tested vs vanilla Postgres | ✅ | `docs/migration/minimal-migration.md` + `migrate-smoke.sh` + CI job `migration-smoke` |
| 2 | vanilla+vector migration preserves data AND indexes (smoke) | ✅ | full-row checksum match + index-definition match + HNSW/IVFFlat usable (live) |
| 3 | minimal migration guide published | ✅ | `docs/migration/minimal-migration.md` (doc-check enforces non-divergence) |

ROADMAP `### M3` stays `[ ]` — the flip is the post-merge release step.

## Method

Two review rounds were not needed; one round of three independent specialist agents (migration
correctness/shell/CI, cross-validation, test-auditor). All three **executed the artifacts live** against
the running `m3-src`/`m3-dst` containers. Cross-validation independently reproduced the checksum and the
per-index access methods.

## Severity matrix

| # | Sev | Finding | Status |
|---|---|---|---|
| H1 | HIGH | Restore had no fail-fast — guide Option B `psql` lacked `ON_ERROR_STOP`, Option A `pg_restore` + smoke lacked `--exit-on-error`; a partial restore could exit 0 silently | **FIXED** `6833d36` — `--exit-on-error` (pg_restore, smoke + guide) + `-v ON_ERROR_STOP=1` (guide Option B) |
| H1′ | HIGH | Integrity oracle hashed only `embedding`, not `title`/`id` → "preserves data"/"bit-exact" overclaim (a title corruption passed green) | **FIXED** `6833d36` — full-row checksum `md5(string_agg(id||title||embedding…))` + non-ASCII title in seed; "bit-exact" wording softened |
| M1 | MED | IVFFlat counted but never proven usable | **FIXED** `6833d36` — symmetric `assert_index_used items_ivf` (EXPLAIN + query returns 5) |
| M2 | MED | Index check was count-only (=4), not index kind/opclass | **FIXED** `6833d36` — compares full `indexdef` set source==target |
| M3 | MED | Source image floating tag while repo pins everything by digest | **FIXED** `6833d36` — `pgvector/pgvector@sha256:be400b50…` pinned in CI |
| M4 | MED | Readiness raced with initdb temp-server (`pg_isready`) | **FIXED** `6833d36` — `wait_ready` runs `psql SELECT 1` until it really answers |
| L1 | LOW | "HNSW usable" proven at plan time only | **FIXED** `6833d36` — query executes and must return 5 rows |
| L2 | LOW | Selftest leaked source `items` table | **FIXED** `6833d36` — trap drops source table too |
| L3 | LOW | Local repro of `m3-src`/`m3-dst` undocumented | **FIXED** `6833d36` — fenced `docker run` block in the guide + smoke header |
| L4 | LOW | `DROP DATABASE` fails on stray connection | **FIXED** `6833d36` — `WITH (FORCE)` |
| I-* | INFO | guide diskann opclass mismatch (cosine vs l2) + "idempotent" wording overstated | **FIXED** `6833d36` — `vector_l2_ops`; wording corrected |

**0 BLOCKER. Both HIGH resolved and re-verified. All MEDIUM fixed. LOW/INFO addressed.**

## Known gaps (honest — out of M3 minimal scope)

- **Extension version-mismatch negative path is documentation-only.** The smoke runs aligned pgvector
  (0.8.3==0.8.3); the "source newer than target → restore fails" path is documented (guide step 0 +
  troubleshooting) but not executed (it needs a second pgvector version image — real cost). Logged as a
  follow-up, not a defect.
- **Single-table, single-vector-column scope.** The smoke proves preservation on one representative table
  (data + non-ASCII text + two ANN index types + btree). Multi-table / sequence-`nextval` / FK migrations
  are standard `pg_dump` behavior, out of "minimal" scope.
- **Plain (Option B) path is documented + version-checked but not separately executed by the smoke** (the
  smoke exercises the custom-format path, which is the recommended one). The shared verification primitives
  are doc-checked.

## Final verification (live, after fixes)

- `bash migrate-smoke.sh` → `MIGRATION SMOKE PASSED — 1000 rows, full-row checksum 338e24c0…, index defs match, HNSW+IVFFlat usable`.
- `bash migrate-smoke-selftest.sh` → `SELFTEST PASSED` (corrupt 1 row → verify fails with `data checksum mismatch`).
- `bash migrate-doc-check.sh` → `DOC-CHECK PASSED`.
- `shellcheck` (koalaman/shellcheck:stable) on all 3 scripts → exit 0, no warnings.
- `ci.yml` valid; job `migration-smoke` present, invokes doc-check + smoke + selftest against pinned source + TheoDB target.

## Reviewer-confirmed strengths

- The assert is **not theatre** — the selftest is a real negative test asserting the specific message
  `data checksum mismatch`, reusing the production asserts via `VERIFY_ONLY` (cannot drift).
- Deterministic seed (pure seeded arithmetic, no random/clock); checksum stable across runs.
- Standard tooling (Rule 9), no bespoke migration code; container-exec is justified (PG<17 client cannot
  dump a PG17 server) and the guide documents the real host-client path.

## Cycle-review hard gates

Tests green ✓ · No new secrets ✓ · On `develop` ✓ · No `Co-Authored-By` ✓ · CHANGELOG updated ✓.

## Verdict rationale

Per `rules/cycle-review.md`: READY_TO_MERGE = no BLOCKER, ≤2 HIGH with documented mitigation. Both HIGH are
**fixed and re-verified** with live evidence; all MEDIUM fixed; LOW/INFO addressed; remaining items are
honest out-of-scope known-gaps. All three M3 DoDs are complete with reproducible evidence.

**Before the M3 checkbox flips (release step):** push `develop` and confirm the first `migration-smoke` CI
run is green (mirrors the M2 pattern).
