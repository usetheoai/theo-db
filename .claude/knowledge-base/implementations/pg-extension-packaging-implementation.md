# Implementation Summary — M15 pg-extension-packaging

**Slug:** pg-extension-packaging · **Milestone:** M15 · **Date:** 2026-06-28
**Plan:** `.claude/knowledge-base/plans/pg-extension-packaging-plan.md` (v1.1, plan-confidence SHIPPABLE_WITH_CAVEATS 80.8)
**Completion promise:** IMPLEMENTATION_COMPLETE
**Commits:** `6c1dddb` (Phase 1) · `164a1c8` (Phases 2-4)

## What shipped

TheoDB now installs as a real, versioned, upgradeable PostgreSQL extension: **`CREATE EXTENSION theodb
CASCADE`** provisions the whole AI + vector surface (the 12 `docs/features/` capabilities) on any PostgreSQL
17 — replacing the six `docker-entrypoint-initdb.d` scripts that only ran on a fresh DB. The "pile of scripts
→ product" jump.

## Tasks (plan) → result

| Task | Result | Evidence |
|---|---|---|
| T1.1 `theodb.control` + remove internal `CREATE EXTENSION` | done | `theodb.control` (requires='vector, vectorscale, plpython3u', superuser=true, relocatable=false, no module_pathname); `sql/30,40,50,60` deps→requires |
| T1.2 PGXS Makefile (SQL-only, no MODULES) | done | `Makefile` (concat the 6 bodies → `theodb--1.0.sql`; `include $(PGXS)` conditional so concat works without the dev package — EC-2) |
| T1.3 extension-safety + install | done | scan: 0 `CREATE EXTENSION`, 0 top-level tx; `CREATE EXTENSION theodb CASCADE` → 25 functions, 40 ext members |
| T2.1 Dockerfile installs ext + init creates it | done | `Dockerfile`: build+`install` into `/usr/share/postgresql/17/extension/`; `00-create-theodb.sql`; 6 init COPYs removed |
| T3.1 upgrade skeleton `theodb--1.0--1.1.sql` | done | `ALTER EXTENSION theodb UPDATE TO '1.1'` → extversion 1.1 (test green) |
| T3.2 smoke.sh asserts extension | done | `smoke.sh` theodb block; `SMOKE PASSED` against theo-db:m15 |
| T4.1 quickstart e2e (12 features) | done | `docs/quickstart.md` (15 sql blocks); vector+hybrid e2e validated (fixed a 'tbl'→'table' bug) |
| T4.2 make dist + publish instructions | done | `make dist` → `dist/theodb-1.0.zip` (1.7MB); README ghcr pull instructions |
| T4.3 README plpython3u limitation | done | README "Instalação" section + honest managed-PG limitation note |

## Wiring triad (per the new surface)

1. **Caller:** the image init (`docker-entrypoint-initdb.d/00-create-theodb.sql`) runs `CREATE EXTENSION
   theodb CASCADE` — the production path that exercises the extension end-to-end on every fresh container.
2. **Integration test:** `benchmarks/tests/test_extension_install.py` (6 tests) against a real container —
   install/surface/upgrade/idempotency/no-cascade-error; `smoke.sh` extended with the extension assertion.
3. **Runtime observability:** `pg_extension` row (`extversion`) is the in-DB signal that the surface is
   installed + at which version; the init logs `installing required extension …` + `CREATE EXTENSION`.

## Integration validation (against theo-db:m15, rebuilt)

- `docker build -t theo-db:m15 .` — OK (cache-reused the heavy pgvector/pgvectorscale stages).
- Init creates `theodb 1.0` via CASCADE (vector+vectorscale+plpython3u pulled) — confirmed in container logs.
- `bash smoke.sh` → `SMOKE PASSED` (all surfaces present via the extension + the new theodb assertion).
- `python3 -m pytest benchmarks/tests/test_extension_install.py` → 6 passed (0.56s).
- `ruff check` → All checks passed.
- `make dist` → `dist/theodb-1.0.zip`.
- quickstart vector + hybrid e2e → green (bug found + fixed: feature-06 JSON key `tbl`→`table`).

## Honest notes / caveats

- **plpython3u/superuser limitation** (ADR D2): managed PostgreSQL without plpython3u gets features 01-05
  only; documented in README + quickstart (no hidden overclaim).
- **No performance claim** — packaging does not change performance (CLAUDE.md TheoDB rule 5).
- **Greenfield only** (ADR D3): no orphan-adoption migration (pre-1.0, no installed base).
- **Image publish** to `ghcr.io/usetheodev/theo-db` is wired (README + make dist) but the actual push happens
  in `/release` with the human-approved tag (Unbreakable Rule 4) — not done in implement.
- plan-confidence soft floor `concurrency_tests_missing` is a false positive (single-threaded packaging;
  signal came from "transactional"/"concurrent" in citations) — does not affect the score.

## Next

`/code-quality` → `/review` → `/release` (publishes the image + flips ROADMAP M15).
