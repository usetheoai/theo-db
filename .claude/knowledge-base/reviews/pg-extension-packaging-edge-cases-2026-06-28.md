# Edge Case Review — M15 pg-extension-packaging (implementation plan)

> Note: distinct from the discovery-plan edge-case review of the same date (that one reviewed the *discovery*
> plan; this reviews the *implementation* plan `knowledge-base/plans/pg-extension-packaging-plan.md`).

Date: 2026-06-28
Tasks analyzed: 7 (T1.1, T1.2, T1.3, T2.1, T3.1, T3.2, T4.1, T4.2, T4.3)
Cases found: 5 (EDGE: 2, NEGATIVE: 3 | MUST FIX: 2, SHOULD TEST: 2, DOCUMENT: 1)

## MUST FIX

### EC-1: `requires` omits `plpython3u` → `CREATE EXTENSION theodb` fails creating plpython3u functions
- **Affected task:** T1.1
- **Kind:** NEGATIVE (the install fails if plpython3u is absent)
- **Family:** Boundary / Dependency
- **Scenario:** the bodies define `LANGUAGE plpython3u` functions (`sql/50-theodb-ai.sql` ×7, `sql/60-theodb-nl.sql` ×2, `sql/30-theodb-embed.sql` ×2 — verified). The plan's control declares `requires = 'vector, vectorscale'` — **plpython3u is missing**. On a DB without plpython3u, `CREATE EXTENSION theodb` (even with CASCADE) reaches the first `LANGUAGE plpython3u` function and errors mid-install (`language "plpython3u" does not exist`), leaving a partially-created extension.
- **Impact:** the extension does not install on any PG where plpython3u was not pre-installed — defeats M15 on non-TheoDB PGs and makes CASCADE not self-sufficient.
- **Suggested fix:** `requires = 'vector, vectorscale, plpython3u'` in `theodb.control` (CASCADE then installs plpython3u too, or fails fast with a clear `required extension "plpython3u" is not installed`). The blueprint Corner 2 already listed plpython3u as a dependency — align the control table to it.

### EC-2: `make install` cannot run in the runtime stage (dev package removed) → image build fails
- **Affected task:** T2.1 (and T1.2's install assumption)
- **Kind:** NEGATIVE (build-time failure)
- **Family:** Resource / Tooling
- **Scenario:** `Dockerfile:46` runs `apt-get remove -y build-essential postgresql-server-dev-$PG_MAJOR` after compiling pgvector. PGXS (`$(pg_config --pgxs)` → the global `Makefile` shipped by `postgresql-server-dev-NN`) is therefore **absent** in the runtime stage. `make install` (which does `include $(PGXS)`) fails. For a SQL-only extension there is no `.so` to build — install is just placing `theodb.control` + `theodb--*.sql` into `$(pg_config --sharedir)/extension/`.
- **Impact:** the image build breaks at the install step.
- **Suggested fix:** in the Dockerfile, install the SQL-only extension by **copying** `theodb.control` + the built `theodb--1.0.sql` + `theodb--1.0--1.1.sql` directly into `"$(pg_config --sharedir)/extension/"` (one `RUN cp`), NOT via `make install`. Keep the PGXS `Makefile` as the **local-dev** install path (where the dev package exists). `pg_config` itself stays in the base image; only `--pgxs` is gone.

## SHOULD TEST

### EC-3: `CREATE EXTENSION theodb` without CASCADE on a bare PG (deps absent)
- **Affected task:** T1.3
- **Kind:** NEGATIVE
- **Suggested test:** `test_create_without_cascade_errors_clearly()` — on a DB without `vector`, run `CREATE EXTENSION theodb` (no CASCADE) and assert it raises the typed error mentioning the missing required extension (not a generic failure). Asserts the documented remedy (CASCADE) is the real behavior. (Already in `## Failure scenarios`; promote to an explicit test.)

### EC-4: re-running the init / `CREATE EXTENSION theodb` twice (idempotency at the boundary)
- **Affected task:** T2.1
- **Kind:** EDGE
- **Suggested test:** `test_create_extension_is_idempotent()` — `CREATE EXTENSION IF NOT EXISTS theodb CASCADE` twice; assert the second is a no-op (one row in `pg_extension`, no error). The init uses `IF NOT EXISTS` (00-create-theodb.sql); this proves a container restart / re-attach does not double-create.

## DOCUMENT

### EC-5: `theodb_ai_nl` config-table rows do not survive `pg_dump` without `pg_extension_config_dump`
- **Kind:** EDGE
- **Accepted risk:** packaging the config tables (`nl_config`/`nl_templates`/`nl_value_index`) as extension members means their **user rows** are not dumped by default (extension data is recreated by the script, not dumped). This is already tracked as Unresolved Q3; not blocking install. A follow-up may call `pg_extension_config_dump('ai.nl_config','')` to opt the user rows into dumps. Documented, deferred.

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|------|------|----------|----------|-------------|----------|
| T1.1 | 0 | 1 | 1 | 0 | 0 |
| T1.2 | 0 | 0 | 0 | 0 | 0 |
| T1.3 | 0 | 1 | 0 | 1 | 1 |
| T2.1 | 1 | 1 | 1 | 1 | 0 |
| T3.x/T4.x | 0 | 0 | 0 | 0 | 0 |

**Coverage check:** the two install boundaries (dependency resolution; image install mechanism) now have both an
EDGE (idempotency) and a NEGATIVE (missing dep / missing PGXS) case considered.

**Verdict:** PLAN NEEDS ADJUSTMENT — 2 MUST FIX absorbed into plan v1.1 (requires += plpython3u; Dockerfile
copies the SQL-only extension instead of `make install`). 2 SHOULD TEST added to TDD.
