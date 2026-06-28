# Discover Edge Case Review — pg-extension-packaging

Date: 2026-06-28
Discovery plan analyzed: .claude/knowledge-base/discoveries/plans/pg-extension-packaging-plan.md
Research questions analyzed: 8
Edge cases found: 5 (MUST FIX: 2, SHOULD TEST: 2, DOCUMENT: 1)

All cited reference paths were verified to exist (Q1 `pgvector/test` + `pgvectorscale/tests`; Q2
`supabase-postgres/migrations/tests/extensions/` with `01-postgis.sql`…`10-timescaledb.sql`; Q4 both
Makefiles; Q5 all distribution entrypoints; Q6/Q7/Q8 control + upgrade + pgrx scripts). No fabricated
citation. The edge cases below are about **method correctness**, not missing paths.

## MUST FIX

### EC-1: Q4 points at the wrong Makefile for the "SQL-only" shape
- **Affected question:** Q4 (tools)
- **Family:** Method
- **Scenario:** Q4 asks for "the minimal PGXS Makefile shape for a SQL-only extension" and lists BOTH
  `pgvector/Makefile` and `pgvectorscale/Makefile`. Verification shows `pgvectorscale/Makefile` is a
  **pgrx-hybrid** build (`PGRX_VERSION`, `cargo build --features pg…`), NOT SQL-only — it compiles a
  `.so`. Reading it as the SQL-only model would teach the wrong idiom.
- **Impact:** The blueprint could recommend a pgrx/cargo build for what is a pure-SQL (plpython3u)
  extension — over-engineering + a wrong toolchain (violates Rule 9 / KISS).
- **Suggested fix:** Q4 method → make `pgvector/Makefile` the **primary** SQL-only model
  (`EXTENSION = vector` / `DATA = $(wildcard sql/*--*--*.sql)` / `DATA_built` / `PGXS`), and use
  `pgvectorscale/Makefile` ONLY as the pgrx **contrast** (which is already Q8's job).

### EC-2: Q7's central technique (adopt init-script objects) is greenfield in every reference
- **Affected question:** Q7 (techniques)
- **Family:** Interpretation / Citation
- **Scenario:** Q7 wants the "init-script → extension adoption" strategy (`ALTER EXTENSION theodb ADD
  FUNCTION …`). The pgvectorscale upgrade scripts (`vectorscale--X--Y.sql`) all assume a **clean**
  `CREATE EXTENSION` lineage — none adopt pre-existing orphan objects. So Fase B against the clones
  will find the idempotent-upgrade idiom (`CREATE OR REPLACE`, `DO $$` guards, `@extschema@`) but
  **NOT** the orphan-adoption move. That move is a TheoDB-specific scenario (our `ai.*` already exist
  via init-script on running DBs).
- **Impact:** If Q7 demands orphan-adoption evidence from the clones, Fase A is exhausted → Q7 BLOCKED →
  the hardest M15 decision is left unanswered.
- **Suggested fix:** Split Q7 honestly: (a) idempotent-upgrade idiom → evidence from
  `pgvectorscale/.../sql/vectorscale--*.sql` (rich); (b) orphan-object adoption → evidence from the
  **official PG doc** "Packaging Related Objects into an Extension" (`postgresql.org`, allowlisted) +
  state explicitly that the clones are greenfield here. Add a halt-loop checkpoint so Q7 is DONE when
  BOTH (a) the idiom and (b) a doc-cited adoption strategy are captured.

## SHOULD TEST

### EC-3: Q3 assumes plpython3u can be a `requires` target without noting the superuser consequence
- **Affected question:** Q3 (deps)
- **Suggested halt-loop checkpoint:** Before answering Q3 DONE, assert the blueprint states whether
  `requires = '…, plpython3u'` forces `superuser = true` in the control file (plpython3u is an
  **untrusted** PL — superuser-only). Evidence pointer already in hand: `vectorscale.control` sets
  `superuser = true`. If the umbrella requires plpython3u, the control likely must too — capture that.

### EC-4: Q2 can lean on the local timescaledb test instead of the network
- **Affected question:** Q2 (tests)
- **Suggested halt-loop checkpoint:** `supabase-postgres/migrations/tests/extensions/10-timescaledb.sql`
  exists locally — prefer it (and `01..09`) as the per-extension install-test pattern; only WebFetch
  `docs.timescale.com` if an **upgrade-specific** idiom is needed that the local test does not show.
  Reduces network dependency (fewer allowlist round-trips, less BLOCKED risk).

## DOCUMENT

### EC-5: Q5 (distribution corner) is intentionally shallow
- **Accepted risk:** `ansible/`, `nix/`, `*.pkr.hcl` are large; D2 already declares entrypoint-level
  (not full-infra) coverage. Risk of scope creep / budget exhaustion is consciously accepted — the
  blueprint will cite the extension-install entrypoints only and flag the shallow coverage honestly.
  No plan change beyond what D2 + the per-project budget already enforce.

## Summary

| Question | Edges found | MUST FIX | SHOULD TEST | DOCUMENT |
|----------|-------------|----------|-------------|----------|
| Q1 | 0 | 0 | 0 | 0 |
| Q2 | 1 | 0 | 1 | 0 |
| Q3 | 1 | 0 | 1 | 0 |
| Q4 | 1 | 1 | 0 | 0 |
| Q5 | 1 | 0 | 0 | 1 |
| Q6 | 0 | 0 | 0 | 0 |
| Q7 | 1 | 1 | 0 | 0 |
| Q8 | 0 | 0 | 0 | 0 |

**Verdict:** DISCOVERY PLAN NEEDS ADJUSTMENT — 2 MUST FIX absorbed into plan v1.1 (Q4 ref correction;
Q7 split + doc-cited adoption). SHOULD TEST items added as halt-loop checkpoints.
