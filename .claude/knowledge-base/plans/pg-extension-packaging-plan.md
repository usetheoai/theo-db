---
slug: pg-extension-packaging
milestone_id: M15
created_at: 2026-06-28
goal: Replace TheoDB's six init-scripts with an installable CREATE EXTENSION theodb umbrella so all 12 features install on any PG17 via one command
---

# Plan: M15 — Productize TheoDB as an installable `CREATE EXTENSION theodb` umbrella extension

> **Version 1.1** (edge-case MUST-FIX absorbed: `requires += plpython3u`; Dockerfile copies the SQL-only extension instead of `make install`) — TheoDB's 12 `docs/features/` capabilities already work in the image, but the AI surface
> (`ai.*`, `nl_*`, `theodb_ml`) ships as six SQL files copied into `docker-entrypoint-initdb.d/` that run only
> on a fresh DB (`Dockerfile:64-81`). There is no `CREATE EXTENSION theodb`, no versioning, no published image.
> This plan packages that surface as a versioned, installable, SQL-only PostgreSQL umbrella extension (the
> pgvector/PGXS model, not pgrx), installs it in the image via `CREATE EXTENSION theodb` (replacing the six
> init-scripts), adds install/upgrade tests + a quickstart e2e, and publishes the image — the "scripts → product"
> jump. Anchored 100% on the SHIPPABLE_WITH_CAVEATS blueprint at
> `.claude/knowledge-base/discoveries/blueprints/pg-extension-packaging-blueprint.md`.

## Goal

> Enable TheoDB operators to install the entire AI + vector surface with a single `CREATE EXTENSION theodb CASCADE`, so that all 12 `docs/features/` capabilities are available on any PostgreSQL 17 with no init-scripts, measured by `benchmarks/tests/test_extension_install.py` passing (extension installs and every documented surface is present).

## Context

The CTO directive (2026-06-28) is that TheoDB must be a real product — *"um banco vetorial real com TODAS as
features de `docs/features/`"* — not a pile of scripts. The honest gap (verified this session): the 12 features
exist and pass `smoke.sh`, but the AI surface is delivered via `docker-entrypoint-initdb.d` (`Dockerfile:64-81`),
which only fires on fresh DB init — so it is not installable on an existing or managed PostgreSQL, has no
`CREATE EXTENSION theodb`, and no version/upgrade path. The `cycle-discover` run (`pg-extension-packaging`)
produced a blueprint that locks the packaging approach: a SQL-only PGXS umbrella extension (the pgvector model),
because TheoDB's `ai.*` are plpython3u with no compiled code — pgrx (the pgvectorscale model) would be
over-engineering (Unbreakable Rule 9 / KISS). This plan executes the blueprint's 7 recommendations.

This is packaging only: **no performance claim changes** (`.claude/rules/public-copy.md`).

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `sql/30-theodb-embed.sql` | 89 | `ba98af3` (2026-06-27) | `theodb.embed()` (plpython3u, SSRF-hardened) | `theodb.embed` signature + SSRF guard unchanged; remove internal `CREATE EXTENSION` (lines 11-12) |
| `sql/40-theodb-hybrid.sql` | 151 | `156433d` (2026-06-28) | `ai.hybrid_search_rrf` + `ai.hybrid_search(jsonb)` | RRF behavior unchanged; remove internal `CREATE EXTENSION vector` (line 11) |
| `sql/50-theodb-ai.sql` | 308 | `6972fbd` (2026-06-28) | `ai._chat` + scalar `ai.*` + `ai.agg_summarize` | function bodies + REVOKE unchanged; remove internal `CREATE EXTENSION plpython3u` (line 12) |
| `sql/60-theodb-nl.sql` | 178 | `0d3bf92` (2026-06-28) | `ai.nl_to_sql`/`ai.nl_query` (parser-grade allowlist) | anti-injection gate unchanged; remove internal `CREATE EXTENSION plpython3u` (line 18) |
| `sql/61-theodb-nl-config.sql` | 205 | `793f491` (2026-06-28) | `theodb_ai_nl` config tables + fns | config surface unchanged; loads after 60 |
| `sql/70-theodb-ml.sql` | 99 | `89c6d76` (2026-06-28) | `theodb_ml` model registry | no `api_key` column; loads after 50 |
| `theodb.control` (NEW) | 0 | — | (extension control file) | — |
| `sql/theodb--1.0.sql` (NEW, generated, gitignored) | 0 | — | (built install script — concatenation of the six bodies) | — |
| `sql/theodb--1.0--1.1.sql` (NEW, skeleton) | 0 | — | (upgrade-path convention seed) | — |
| `Makefile` (NEW) | 0 | — | (PGXS build/install + dist) | — |
| `Dockerfile` | 84 | `156433d` (2026-06-28) | image build (PG17 + pgvector + pgvectorscale + 6 init-scripts) | engine + extension binaries unchanged; replace the six `COPY … initdb.d` with extension install + `CREATE EXTENSION theodb` |
| `smoke.sh` | 165 | `156433d` (2026-06-28) | presence+privilege smoke of all surfaces | all existing assertions stay green; add `CREATE EXTENSION theodb` assertion |
| `benchmarks/tests/test_extension_install.py` (NEW) | 0 | — | (install + upgrade integration test) | — |
| `docs/quickstart.md` (NEW) | 0 | — | (e2e of the 12 features via the extension) | — |
| `README.md` | (exists) | — | product README | add install section + honest plpython3u limitation |
| `CHANGELOG.md` | (exists) | — | public contract | add `[Unreleased]` entries |
| `.gitignore` | (exists) | — | ignore rules | add `sql/theodb--1.0.sql` (generated) |

### Current callers / dependents

- **Symbol:** the six `sql/30-70` files — **Callers (production):** `Dockerfile:64-81` (six `COPY` into
  `docker-entrypoint-initdb.d/`). **Callers (tests):** `smoke.sh:16-163`; `benchmarks/tests/test_ai_sql.py`,
  `test_embed_sql.py`, `test_hybrid.py` (connect to the running container and exercise the surfaces).
  **External (other repos):** no — the SQL surface is internal to the image.
- **Symbol:** `theodb.embed`, `ai.*`, `theodb_ml.*` — **Callers:** in-DB (created by the scripts) + the pytest
  suite + `smoke.sh`. Packaging them into an extension does NOT change their signatures (invariant above).
- **External public API consumed by other repos:** none. M15 changes the *delivery mechanism*, not the SQL
  contract — every function keeps its name/signature/schema.

### Domain glossary

- **umbrella extension** — a PostgreSQL extension that bundles a surface and declares `requires` on other
  extensions (here `vector`, `vectorscale`), so `CREATE EXTENSION theodb CASCADE` pulls the whole stack.
- **PGXS** — PostgreSQL's extension build system (`include $(PGXS)`); for a SQL-only extension it installs the
  `.control` + `--version.sql` into `$(pg_config --sharedir)/extension/`.
- **init-script** — a file in `docker-entrypoint-initdb.d/` that the postgres image runs once on first DB init.
- **`requires`** — control-file field listing extensions that must be installed first (does NOT auto-install
  without `CASCADE`).
- **greenfield install** — installing the extension on a DB that never ran the old init-scripts (the only
  supported path per blueprint ADR D3; TheoDB is pre-1.0, no installed base).

### Architecture boundaries affected

Per `.claude/rules/architecture.md § 1/§ 3`: the AI surface keeps its own namespaces (`theodb`, `ai`,
`theodb_ml`) — the extension creates these schemas in-script (does NOT use the `schema` control param, which
forces a single schema). The packaging change is at the **distribution/composition-root layer** (the image +
install mechanism), not the domain layer — no function logic moves. The extension depends inward on
pgvector/pgvectorscale via `requires` (a declared dependency, the DIP-correct direction for an adapter over
those extensions).

## Prior Art & Related Work

- **Internal blueprint:** `.claude/knowledge-base/discoveries/blueprints/pg-extension-packaging-blueprint.md`
  — the source of truth (4 ADRs + 7 recommendations); this plan implements its Recommendations 1-7.
- **Internal blueprint:** `.claude/knowledge-base/discoveries/blueprints/m1-core-packaging-blueprint.md` — M1
  established the engine-unmodified packaging baseline (`CREATE EXTENSION vector/vectorscale/plpython3u` already
  succeed).
- **Reference project:** `.claude/knowledge-base/references/pgvector/Makefile:1-6` — the SQL-only PGXS model
  (`EXTENSION`/`DATA`/`DATA_built`); `.claude/knowledge-base/references/pgvector/vector.control` — control-file
  shape.
- **Reference project:** `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/vectorscale.control:1-6`
  — umbrella `requires = 'vector'` + `superuser = true`; `.../sql/vectorscale--0.8.0--0.9.0.sql` — idempotent
  upgrade idiom (`CREATE OR REPLACE` + `DO $$` guards + `@extschema@`).
- **Reference project:** `.claude/knowledge-base/references/supabase-postgres/migrations/tests/extensions/10-timescaledb.sql:1-9`
  — transactional per-extension install test (`begin; create extension …; rollback;`).
- **External docs:** PostgreSQL "CREATE EXTENSION" (https://www.postgresql.org/docs/current/sql-createextension.html)
  — `CASCADE`/`requires`/`trusted` semantics; "Packaging Related Objects into an Extension"
  (https://www.postgresql.org/docs/current/extend-extensions.html) — control fields + version-script chaining.

## Dependencies

**(none — M15 adds no new package dependency.)** Per Unbreakable Rule 9 / `.claude/rules/parsimony-ladder.md`,
M15 reuses only what is already present:

| Component | Version | Already present? | Rule-9 note |
|---|---|---|---|
| PGXS / `pg_config` | PostgreSQL 17 (`postgres:17-bookworm`) | yes (base image) | standard extension build tool — not reinvented |
| `plpython3u` | PG17 (`postgresql-plpython3-17`) | yes (`Dockerfile:60`) | runtime for `ai.*`; declared in `requires` |
| `pgvector` | 0.8.x | yes (`Dockerfile:34-49`) | declared in `requires` |
| `pgvectorscale` | 0.9.0 | yes (`Dockerfile:51-53`) | declared in `requires` |
| `pytest` / `ruff` | (existing dev deps) | yes (`benchmarks/`) | test/lint — already used |

No npm / pip / cargo / go dependency is added. No CVE surface changes. The only new files are first-party
(`theodb.control`, `sql/theodb--*.sql`, `Makefile`, a test, docs).

## Objective

- [ ] SG1 — `theodb.control` exists with the blueprint-locked fields (D1, D2).
- [ ] SG2 — `sql/theodb--1.0.sql` is built (PGXS) from the six bodies, with internal `CREATE EXTENSION` removed (D1, D3).
- [ ] SG3 — `Makefile` builds + installs the extension via PGXS, no `MODULES`/`.so` (D1).
- [ ] SG4 — `Dockerfile` installs the extension and runs `CREATE EXTENSION theodb` at init, replacing the six init-script copies (D3).
- [ ] SG5 — install + upgrade-skeleton tests pass + `smoke.sh` asserts `CREATE EXTENSION theodb` (D5).
- [ ] SG6 — `docs/quickstart.md` exercises all 12 features via the extension; image publishable to GHCR; `make dist` produces a zip (D4).
- [ ] SG7 — README documents the plpython3u/superuser limitation honestly (D2).

## ADRs

### D1 — SQL-only PGXS umbrella, not pgrx, not per-feature

**Decision:** package the six bodies as ONE `theodb` extension built with the pgvector PGXS model (hand-written
SQL, `EXTENSION`/`DATA`/`DATA_built`), no pgrx, no `MODULES`/`.so`.

**Rationale:** the surface is plpython3u + plpgsql (no compiled code) — pgrx exists in pgvectorscale only to
compile Rust; adopting it for pure SQL violates Unbreakable Rule 9 + KISS (`.claude/rules/parsimony-ladder.md`
rung 2-4).

**Alternatives considered:** (a) pgrx — rejected: no Rust to compile. (b) per-feature extensions
(`theodb_ai`/`theodb_nl`/`theodb_ml` separate) — rejected for v1.0: one `CREATE EXTENSION theodb` is the
cohesive product surface; split is a future option. (c) keep init-scripts — rejected: the whole problem.

**Consequences:** toolchain-light build (no Rust for the umbrella); we hand-maintain `--X--Y.sql` upgrades.

### D2 — `superuser = true`, `trusted` unset, `relocatable = false`, install via `CASCADE`

**Decision:** `theodb.control` = `default_version='1.0'`, `requires='vector, vectorscale, plpython3u'`, `superuser=true`,
`relocatable=false`, no `schema` param, no `module_pathname`, `trusted` unset; document `CREATE EXTENSION
theodb CASCADE`.

**Rationale:** plpython3u is untrusted (outbound HTTP) → cannot be `trusted`; matches
`vectorscale.control` `superuser = true`. `requires` does not auto-install without `CASCADE` (PG docs); the
surface lives in multiple hard-named schemas → `relocatable=false`, schemas created in-script.

**Alternatives considered:** `trusted=true` — rejected (privilege-escalation surface via plpython3u);
`relocatable=true` — rejected (multi-schema surface is hard-named).

**Consequences:** install requires superuser (acceptable for DBAs); managed PGs without plpython3u get features
01-05 only (documented in D-driven README task).

### D3 — Greenfield only: no init-script→extension orphan-adoption migration

**Decision:** the image runs `CREATE EXTENSION theodb` at init instead of copying the six SQL files; no
`ALTER EXTENSION ADD` adoption path is built.

**Rationale:** zero refs adopt orphan objects (blueprint grep = 0); TheoDB is pre-1.0 with no installed base —
building a migration for nonexistent legacy DBs is the sunk-cost trap (CLAUDE.md anti-sunk-cost).

**Alternatives considered:** elaborate `ALTER EXTENSION ADD` adoption — rejected (tedious, unnecessary
pre-1.0); keep init-scripts as fallback — rejected (two code paths).

**Consequences:** single clean install path; the rare legacy dev DB is migrated by drop+recreate (documented,
low priority).

### D4 — Distribution: PGXS install in the image + publish GHCR + `make dist`; RPM/deb deferred to M5

**Decision:** M15 ships the extension inside the existing image + publishes to `ghcr.io/usetheodev/theo-db` +
a `make dist` source zip (pgvector model). RPM/deb/bare-metal stays M5.

**Rationale:** the image exists; the missing product steps are the extension, a pullable target, and a source
zip. supabase's Nix/Packer is more than needed now (KISS).

**Alternatives considered:** adopt supabase Nix now — rejected (M5 scope); skip publishing — rejected (an
unpullable image is not a product).

**Consequences:** M15 delivers an installable, pullable product; M5 adds non-container distribution.

### D5 — Install/upgrade test via pytest (existing harness) + transactional install + smoke extension

**Decision:** add `benchmarks/tests/test_extension_install.py` (pytest, the pgvectorscale model:
`CREATE EXTENSION theodb` then assert surfaces) + a transactional install assertion (supabase model) + extend
`smoke.sh`.

**Rationale:** the repo already runs pytest in `benchmarks/tests/` against the container — reuse it (Rule 9),
do not introduce a pg_regress tree. The transactional `begin; …; rollback;` proves clean install per
`.claude/rules/testing.md` (integration boundary against a real DB).

**Alternatives considered:** pg_regress golden files — rejected (new tree + toolchain; pytest already wired).

**Consequences:** install/upgrade is gated by the existing CI path.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| `requires` does not auto-install without `CASCADE` → bare `CREATE EXTENSION theodb` fails on a PG without pgvector/vectorscale | Medium | Image pre-builds both (`Dockerfile:34-53`); document `CREATE EXTENSION theodb CASCADE` for other PGs (README task) | maintainers |
| plpython3u is untrusted → extension is superuser-only; managed PGs without plpython3u lose features 06-12 | Medium | Document honestly in README (D2); features 01-05 still work via pgvector/vectorscale | maintainers |
| Concatenating the six bodies into `theodb--1.0.sql` could break ordering (50 before 70, 60 before 61) or leave a top-level transaction-control statement (forbidden in extension scripts) | High | Makefile concatenates in fixed numeric order; T1.3 asserts no top-level `BEGIN;`/`COMMIT;` (the existing `BEGIN`/`END` are plpgsql blocks, verified) | maintainers |
| Removing internal `CREATE EXTENSION` from the bodies could regress a fresh-init DB if the deps aren't present | Medium | `requires` + image pre-build guarantee deps; install test asserts the full surface post-`CREATE EXTENSION` | maintainers |
| `theodb--1.0.sql` exceeds the 500-LoC file budget (~1030 LoC concatenated) | Low | It is a **generated** artifact (gitignored), not hand-edited source; the six source bodies each stay < 500 LoC | maintainers |

## Unresolved Questions

- Q1 — Does `CREATE EXTENSION theodb` inside `docker-entrypoint-initdb.d` run as superuser (it must, for
  plpython3u)? Expected yes (the postgres image init runs as the superuser role) — verified in T2.1.
- Q2 — Are all `BEGIN`/`END` occurrences in the six bodies plpgsql blocks (allowed) and not top-level
  transaction control (forbidden in extension scripts)? Spot-check says yes; T1.3 asserts it mechanically.
- Q3 — Should `theodb_ai_nl` config tables (`nl_config`/`nl_templates`/`nl_value_index`) be extension
  **config tables** (`pg_extension_config_dump`) so user rows survive `pg_dump`? Deferred to a follow-up
  (documented in T1.3 Deep Dives) — not blocking install.

## Dependency Graph

```
Phase 1 (extension scaffold) ──▶ Phase 2 (image integration) ──▶ Phase 3 (tests)
        │                                                              │
        │                                                              ▼
        └──────────────────────────────────────────────▶ Phase 4 (distribution & docs)
                                                                       │
                                                                       ▼
                                                          Final Phase (integration validation)
```

Phase 1 is the blocker for everything. Phase 3 needs Phase 2 (the image must install the extension). Phase 4
needs Phase 1 (the extension) + Phase 2 (the image) but its docs/README parts can draft in parallel with Phase 3.

---

## Phase 1: Extension scaffold

**Objective:** produce an installable `CREATE EXTENSION theodb` (control + built install SQL + PGXS Makefile).

### T1.1 — Create `theodb.control` + remove internal `CREATE EXTENSION` from the bodies

#### Objective
Author the control file and convert the bodies' internal `CREATE EXTENSION` calls into the umbrella's `requires`.

#### Why this step (action + reasoning)
1. **What:** write `theodb.control` (blueprint-locked fields) and delete the `CREATE EXTENSION IF NOT EXISTS
   vector/plpython3u` lines from `sql/30,40,50,60` (they become `requires`).
2. **Why now:** an extension script may not `CREATE EXTENSION` internally — dependencies are declared in the
   control's `requires` (D1, D2; PG docs "Packaging Related Objects"). This must happen before the install
   script is assembled (T1.3), or the build would embed forbidden statements.

#### Evidence
- Internal deps to remove: `sql/30-theodb-embed.sql:11-12`, `sql/40-theodb-hybrid.sql:11`,
  `sql/50-theodb-ai.sql:12`, `sql/60-theodb-nl.sql:18` (verified `grep 'CREATE EXTENSION' sql/*.sql`).
- Control shape: `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/vectorscale.control:1-6`
  (`requires`, `superuser=true`, `relocatable=false`).

#### Files to edit
```
theodb.control (NEW) — control file: default_version='1.0', comment, requires='vector, vectorscale', superuser=true, relocatable=false
sql/30-theodb-embed.sql — remove lines 11-12 (CREATE EXTENSION vector/plpython3u)
sql/40-theodb-hybrid.sql — remove line 11 (CREATE EXTENSION vector)
sql/50-theodb-ai.sql — remove line 12 (CREATE EXTENSION plpython3u)
sql/60-theodb-nl.sql — remove line 18 (CREATE EXTENSION plpython3u)
benchmarks/tests/test_extension_install.py (NEW) — RED: control parse + requires assertion
```

#### Deep file dependency analysis
- The four body files (Baseline rows `sql/30,40,50,60`) currently self-install deps; after this task the deps
  come from `requires`. Downstream: `Dockerfile` (T2.1) will no longer rely on init-order for deps; the install
  test (T3.1) asserts `CREATE EXTENSION theodb CASCADE` brings vector+vectorscale.
- `sql/61,70` have no internal `CREATE EXTENSION` (verified) — untouched here.

#### Deep Dives
- **`theodb.control` exact fields:** `comment = 'TheoDB AI + vector surface (embed, hybrid search, generative
  ai.*, NL→SQL, model registry)'`, `default_version = '1.0'`, `requires = 'vector, vectorscale, plpython3u'`,
  `superuser = true`, `relocatable = false`. NO `module_pathname` (no `.so`), `trusted` unset (untrusted
  plpython3u), no `schema` (schemas created in-script).
- **Invariant:** removing `CREATE EXTENSION` must not change any function body — only the dependency-bootstrap
  lines are deleted (cite Baseline `Invariants to preserve`).

#### Tasks
1. Write `theodb.control` with the fields above.
2. Delete the internal `CREATE EXTENSION` lines from `sql/30,40,50,60`.
3. Write the RED test asserting the control parses and declares `requires` vector+vectorscale.

#### TDD
```
RED:     test_control_declares_requires() — parse theodb.control; assert requires contains 'vector', 'vectorscale' AND 'plpython3u', superuser is true, no module_pathname (FAILS: file absent)
GREEN:   create theodb.control + remove internal CREATE EXTENSION lines
REFACTOR: None expected
VERIFY:  python3 -m pytest benchmarks/tests/test_extension_install.py::test_control_declares_requires -q
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `test -f theodb.control` exits 0 and `grep -q "requires = 'vector, vectorscale, plpython3u'" theodb.control` exits 0.
- [ ] `grep -c 'CREATE EXTENSION' sql/30-theodb-embed.sql sql/40-theodb-hybrid.sql sql/50-theodb-ai.sql sql/60-theodb-nl.sql` returns 0 for each.
- [ ] `python3 -m pytest benchmarks/tests/test_extension_install.py::test_control_declares_requires -q` exits 0.
- [ ] Pass: size — `theodb.control` ≤ 500 lines (it is ~6 lines).

#### DoD
- [ ] All tasks completed; `python3 -m pytest benchmarks/tests/test_extension_install.py -q` green for the control test.
- [ ] CHANGELOG `[Unreleased]` updated.

### T1.2 — PGXS `Makefile` (SQL-only, no MODULES)

#### Objective
Add a root `Makefile` that builds `sql/theodb--1.0.sql` from the bodies and installs the extension via PGXS.

#### Why this step (action + reasoning)
1. **What:** create a `Makefile` with `EXTENSION = theodb`, `EXTVERSION = 1.0`,
   `DATA = $(wildcard sql/theodb--*--*.sql)`, `DATA_built = sql/theodb--$(EXTVERSION).sql`, a build rule that
   concatenates the bodies, and `include $(PGXS)`.
2. **Why now:** `make install` is the standard tool to place `.control` + `--version.sql` where
   `pg_config --sharedir` resolves them (D1; pgvector model). The image (T2.1) calls `make install`.

#### Evidence
- Model: `.claude/knowledge-base/references/pgvector/Makefile:1-6,46-48` (`EXTENSION`/`DATA`/`DATA_built` +
  `sql/$(EXTENSION)--$(EXTVERSION).sql: sql/$(EXTENSION).sql` build rule). No `MODULES` (no `.so`).

#### Files to edit
```
Makefile (NEW) — PGXS: EXTENSION/EXTVERSION/DATA/DATA_built + concat rule + dist target
.gitignore — add sql/theodb--1.0.sql (generated)
benchmarks/tests/test_extension_install.py — RED: assert make build produces a non-empty theodb--1.0.sql
```

#### Deep file dependency analysis
- New `Makefile` at root (none today — verified `ls Makefile` empty). Downstream: `Dockerfile` (T2.1) runs
  `make install`; `make dist` (T4.2) produces the zip.

#### Deep Dives
- **Build rule (concat in fixed order):**
  `sql/theodb--1.0.sql: sql/30-theodb-embed.sql sql/40-theodb-hybrid.sql sql/50-theodb-ai.sql sql/60-theodb-nl.sql sql/61-theodb-nl-config.sql sql/70-theodb-ml.sql` → `cat $^ > $@`. Numeric order preserves the
  load-order invariant (50 before 70, 60 before 61) documented in `Dockerfile:75-81`.
- **No `MODULES`** — SQL-only; PGXS still installs `.control` + `DATA_built`.

#### Pseudo-code / Signatures
```makefile
EXTENSION = theodb
EXTVERSION = 1.0
DATA = $(wildcard sql/theodb--*--*.sql)          # upgrade scripts
DATA_built = sql/theodb--$(EXTVERSION).sql        # built install script
PG_CONFIG ?= pg_config
PGXS := $(shell $(PG_CONFIG) --pgxs)
include $(PGXS)
sql/theodb--$(EXTVERSION).sql: sql/30-theodb-embed.sql sql/40-theodb-hybrid.sql sql/50-theodb-ai.sql sql/60-theodb-nl.sql sql/61-theodb-nl-config.sql sql/70-theodb-ml.sql
	cat $^ > $@
```

#### Tasks
1. Write the `Makefile`.
2. Add `sql/theodb--1.0.sql` to `.gitignore`.
3. RED test: `make` produces a non-empty `sql/theodb--1.0.sql`.

#### TDD
```
RED:     test_make_builds_install_script() — run `make sql/theodb--1.0.sql`; assert file exists and non-empty (FAILS: no Makefile)
GREEN:   write Makefile + concat rule
REFACTOR: None expected
VERIFY:  python3 -m pytest benchmarks/tests/test_extension_install.py::test_make_builds_install_script -q
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `make -n sql/theodb--1.0.sql` exits 0 (rule resolves).
- [ ] `make sql/theodb--1.0.sql && test -s sql/theodb--1.0.sql` exits 0 (built, non-empty).
- [ ] `git check-ignore sql/theodb--1.0.sql` exits 0 (generated artifact ignored).
- [ ] Pass: size — `Makefile` ≤ 500 lines.

#### DoD
- [ ] `python3 -m pytest benchmarks/tests/test_extension_install.py -q` green for the build test.
- [ ] CHANGELOG `[Unreleased]` updated.

### T1.3 — Validate the assembled install script is extension-safe

#### Objective
Assert the built `theodb--1.0.sql` contains no extension-forbidden statements and installs cleanly.

#### Why this step (action + reasoning)
1. **What:** add a test that scans the built script for top-level transaction control + residual
   `CREATE EXTENSION`, then runs `CREATE EXTENSION theodb CASCADE` on the container and asserts the full surface.
2. **Why now:** extension scripts forbid `BEGIN;`/`COMMIT;` and internal `CREATE EXTENSION` (PG docs); the
   bodies have plpgsql `BEGIN`/`END` blocks (allowed) — this mechanically distinguishes them before the image
   depends on the script (Q2, the High-severity risk).

#### Evidence
- plpgsql `BEGIN` blocks (allowed): `sql/70-theodb-ml.sql:30,49,74`, `sql/40-theodb-hybrid.sql:44,124` (function
  bodies / DO blocks — verified). Forbidden top-level `BEGIN;` must be absent.
- Surface to assert: the 25 functions/aggregate/tables enumerated from `grep` (theodb.embed; ai.*; theodb_ml.*).

#### Files to edit
```
benchmarks/tests/test_extension_install.py — RED: extension-safety scan + CREATE EXTENSION theodb CASCADE + surface assertions + transactional install (supabase model)
```

#### Deep file dependency analysis
- Consumes the built `sql/theodb--1.0.sql` (T1.2). Asserts the same surfaces `smoke.sh:48-163` checks, but via
  `CREATE EXTENSION` instead of init-scripts.

#### Deep Dives
- **Extension-safety scan:** assert the built script has no line matching `^\s*(BEGIN|COMMIT|START
  TRANSACTION|ROLLBACK)\s*;` (transaction control) and no `CREATE EXTENSION` (deps are `requires`). plpgsql
  `BEGIN`/`END` inside `$$ … $$` bodies are NOT matched (they have no trailing `;` on the `BEGIN` keyword).
- **Transactional install (supabase model):** `begin; CREATE EXTENSION theodb CASCADE; <assert surfaces>;
  rollback;` — proves clean install with no residue.

#### Pseudo-code / Signatures
```python
def test_extension_installs_full_surface(db_conn):
    with db_conn.cursor() as cur:
        cur.execute("CREATE EXTENSION IF NOT EXISTS theodb CASCADE")
        cur.execute("SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace "
                    "WHERE n.nspname IN ('ai','theodb','theodb_ml')")
        assert cur.fetchone()[0] >= 24   # all functions present
# Example: fresh DB -> CREATE EXTENSION theodb CASCADE -> 24+ functions in ai/theodb/theodb_ml
```

#### Tasks
1. RED: extension-safety scan test (no top-level tx control, no `CREATE EXTENSION`).
2. RED: `CREATE EXTENSION theodb CASCADE` installs + surface count ≥ 24.
3. RED: transactional install (begin/create/rollback) leaves no residue.

#### TDD
```
RED:     test_built_script_is_extension_safe() — scan built sql for forbidden top-level tx control + CREATE EXTENSION
RED:     test_extension_installs_full_surface() — CREATE EXTENSION theodb CASCADE; assert ≥24 functions in ai/theodb/theodb_ml
RED:     test_transactional_install_no_residue() — begin; create; rollback; assert extension gone after rollback
RED:     test_create_extension_is_idempotent() — CREATE EXTENSION IF NOT EXISTS theodb CASCADE twice; assert one row in pg_extension, no error (EC-4, EDGE)
RED:     test_create_without_cascade_errors_clearly() — on a DB without vector, CREATE EXTENSION theodb (no CASCADE) raises a typed error naming the missing required extension (EC-3, NEGATIVE)
GREEN:   (the install script from T1.1+T1.2 already satisfies these once deps are present in the image)
REFACTOR: None expected
VERIFY:  python3 -m pytest benchmarks/tests/test_extension_install.py -q
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `python3 -m pytest benchmarks/tests/test_extension_install.py -q` exits 0 (all install tests green).
- [ ] The built script scan finds 0 top-level `BEGIN;`/`COMMIT;` and 0 `CREATE EXTENSION` (assertion in test).
- [ ] `CREATE EXTENSION theodb CASCADE` yields ≥ 24 functions across `ai`/`theodb`/`theodb_ml` (assertion).

#### DoD
- [ ] All install tests green against the M15 container.
- [ ] CHANGELOG `[Unreleased]` updated.

---

## Phase 2: Image integration

**Objective:** the image installs the extension and runs `CREATE EXTENSION theodb` at init (no init-scripts).

### T2.1 — Dockerfile installs the extension + `CREATE EXTENSION theodb` at init

#### Objective
Replace the six `COPY … docker-entrypoint-initdb.d` lines with extension install + a single init that runs
`CREATE EXTENSION theodb`.

#### Why this step (action + reasoning)
1. **What:** in the runtime stage, build `theodb--1.0.sql` (concat) then **copy** `theodb.control` +
   `theodb--1.0.sql` + `theodb--1.0--1.1.sql` directly into `"$(pg_config --sharedir)/extension/"` (SQL-only —
   no `make install`/PGXS); replace the six init-script copies (`Dockerfile:64-81`) with one init script
   `00-create-theodb.sql` running `CREATE EXTENSION theodb CASCADE`.
2. **Why now:** this is the delivery switch — D3 (greenfield): the image stops sourcing raw SQL and starts
   installing a real extension. Everything downstream (tests, quickstart) depends on it.

#### Evidence
- Current init mechanism to replace: `Dockerfile:64-81` (six `COPY sql/*.sql /docker-entrypoint-initdb.d/`).
- **EC-2 fix:** `Dockerfile:46` removes `postgresql-server-dev-$PG_MAJOR`, so `$(pg_config --pgxs)` (the global
  PGXS Makefile) is ABSENT in the runtime stage → `make install` would fail. For a SQL-only extension there is
  no `.so`; install = copying `.control` + `--*.sql` into `$(pg_config --sharedir)/extension/`. `pg_config`
  itself stays in the base image. The PGXS `Makefile` (T1.2) is the **local-dev** install path.

#### Files to edit
```
Dockerfile — runtime stage: COPY theodb.control sql/ Makefile; RUN (make sql/theodb--1.0.sql || cat the 6 bodies) then `cp theodb.control sql/theodb--1.0.sql sql/theodb--1.0--1.1.sql "$(pg_config --sharedir)/extension/"`; replace 6 initdb.d COPYs with one 00-create-theodb.sql (CREATE EXTENSION theodb CASCADE)
docker-entrypoint-initdb.d content (NEW, in-image) — 00-create-theodb.sql
```

#### Deep file dependency analysis
- `Dockerfile` (Baseline row) — the six `COPY` lines are removed; the built `theodb.control` +
  `theodb--1.0.sql` (+ upgrade) are `cp`'d into `$(pg_config --sharedir)/extension/`
  (= `/usr/share/postgresql/17/extension/`). The init now creates the extension once.
- Q1 resolved here: the init runs as the postgres superuser, satisfying plpython3u/superuser (D2).

#### Deep Dives
- **Build SQL at image build:** concat the six bodies into `theodb--1.0.sql` (the generated script is
  gitignored, so it must be built in the image — `make sql/theodb--1.0.sql` works because the concat rule needs
  no PGXS; only the `include $(PGXS)` line does, which is irrelevant to the concat target).
- **Install = copy (SQL-only):** `cp theodb.control sql/theodb--1.0.sql sql/theodb--1.0--1.1.sql
  "$(pg_config --sharedir)/extension/"` — no compiler, no dev package, no PGXS at runtime (EC-2).
- **Install order:** deps (`vector`, `vectorscale`, `plpython3u`) — vector/vectorscale built
  (`Dockerfile:34-53`), plpython3u installed (`Dockerfile:60`); `CREATE EXTENSION theodb CASCADE` finds them →
  installs theodb. CASCADE is belt-and-suspenders.

#### Tasks
1. Add `COPY theodb.control sql/ Makefile` + a `RUN` that builds `theodb--1.0.sql` and `cp`s the control + sql into `$(pg_config --sharedir)/extension/` (no `make install`).
2. Remove the six `COPY sql/*.sql /docker-entrypoint-initdb.d/` lines.
3. Add `docker-entrypoint-initdb.d/00-create-theodb.sql` = `CREATE EXTENSION theodb CASCADE;`.

#### TDD
```
RED:     test_image_creates_extension() — build image; fresh container; assert SELECT extname FROM pg_extension WHERE extname='theodb' returns a row (FAILS pre-change: no extension, surfaces came from raw scripts)
GREEN:   edit Dockerfile per tasks
REFACTOR: None expected
VERIFY:  docker build -t theo-db:m15 . && <run container> && psql -c "SELECT extversion FROM pg_extension WHERE extname='theodb'"
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `docker build -t theo-db:m15 .` exits 0.
- [ ] On a fresh `theo-db:m15` container, `psql -tAc "SELECT extversion FROM pg_extension WHERE extname='theodb'"` returns `1.0`.
- [ ] `grep -c 'docker-entrypoint-initdb.d/30-theodb' Dockerfile` returns 0 (old copies removed).
- [ ] Pass: size — `Dockerfile` ≤ 500 lines.

#### DoD
- [ ] Image builds; extension present on fresh container; `smoke.sh` green against it.
- [ ] CHANGELOG `[Unreleased]` updated.

---

## Phase 3: Tests

**Objective:** install + upgrade-skeleton are gated; `smoke.sh` asserts the extension.

### T3.1 — Upgrade-path skeleton (`theodb--1.0--1.1.sql`) + convention test

#### Objective
Seed the `theodb--X--Y.sql` upgrade convention so v1.0→v1.1 is a real `ALTER EXTENSION theodb UPDATE`.

#### Why this step (action + reasoning)
1. **What:** add a no-op-but-valid `sql/theodb--1.0--1.1.sql` (idempotent idiom: a comment + a `CREATE OR
   REPLACE` re-affirmation guard) and a test that `ALTER EXTENSION theodb UPDATE TO '1.1'` succeeds.
2. **Why now:** establishing the convention at v1.0 makes future feature additions a versioned upgrade, not a
   re-init (blueprint Recommendation 5; D1). Doing it later means the first real upgrade has no precedent.

#### Evidence
- Idiom model: `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/sql/vectorscale--0.8.0--0.9.0.sql:13,15-28`
  (`CREATE OR REPLACE` + `DO $$` guards). Naming + chaining: PG docs "Packaging Related Objects".

#### Files to edit
```
sql/theodb--1.0--1.1.sql (NEW) — upgrade skeleton (idempotent; bumps default_version capability)
theodb.control — (no change; default_version stays 1.0 until a real 1.1 ships)
benchmarks/tests/test_extension_install.py — RED: ALTER EXTENSION theodb UPDATE TO '1.1' succeeds
```

#### Deep file dependency analysis
- New `sql/theodb--1.0--1.1.sql` is picked up by `DATA = $(wildcard sql/theodb--*--*.sql)` (T1.2). Downstream:
  none yet (it is the convention seed).

#### Deep Dives
- **Skeleton content:** a header comment + an idempotent `DO $$ BEGIN /* no schema change in 1.1 seed */ END
  $$;` — proves the chain mechanism without inventing a feature (YAGNI). Real upgrades replace it.

#### Tasks
1. Write `sql/theodb--1.0--1.1.sql` (idempotent skeleton).
2. RED: `ALTER EXTENSION theodb UPDATE TO '1.1'` succeeds.

#### TDD
```
RED:     test_upgrade_path_1_0_to_1_1() — CREATE EXTENSION theodb VERSION '1.0' CASCADE; ALTER EXTENSION theodb UPDATE TO '1.1'; assert extversion='1.1' (FAILS: no upgrade script)
GREEN:   add sql/theodb--1.0--1.1.sql
REFACTOR: None expected
VERIFY:  python3 -m pytest benchmarks/tests/test_extension_install.py::test_upgrade_path_1_0_to_1_1 -q
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `python3 -m pytest benchmarks/tests/test_extension_install.py::test_upgrade_path_1_0_to_1_1 -q` exits 0.
- [ ] `test -f sql/theodb--1.0--1.1.sql` exits 0.
- [ ] The upgrade script contains no top-level transaction control (same scan as T1.3).

#### DoD
- [ ] Upgrade test green; CHANGELOG `[Unreleased]` updated.

### T3.2 — Extend `smoke.sh` to assert `CREATE EXTENSION theodb`

#### Objective
Add an extension-install assertion to `smoke.sh` so the product smoke proves the install path, not just presence.

#### Why this step (action + reasoning)
1. **What:** prepend a check that `CREATE EXTENSION IF NOT EXISTS theodb CASCADE` succeeds and
   `pg_extension` lists `theodb` at `1.0`, keeping all existing assertions.
2. **Why now:** `smoke.sh` is the product smoke (`.claude/rules/testing.md` — the e2e oracle); after M15 the
   surfaces come from the extension, so the smoke must assert the extension exists.

#### Evidence
- Current smoke asserts surfaces directly (`smoke.sh:48-163`); it must now also assert the extension wrapper.

#### Files to edit
```
smoke.sh — add CREATE EXTENSION theodb assertion before the surface checks
```

#### Deep file dependency analysis
- `smoke.sh` (Baseline row) — existing assertions unchanged (invariant); one new block added.

#### Deep Dives
- **Placement:** the new block runs first; the existing `CREATE EXTENSION IF NOT EXISTS vector` (smoke:17)
  stays (harmless; CASCADE already pulled it).

#### Tasks
1. Add the `theodb` extension assertion block to `smoke.sh`.

#### TDD
```
RED:     (shell smoke) run smoke.sh against a pre-M15 container -> theodb block FAILS (no extension)
GREEN:   run smoke.sh against theo-db:m15 -> all blocks pass, prints SMOKE PASSED
REFACTOR: None expected
VERIFY:  bash smoke.sh   (against theo-db:m15 container)
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `bash smoke.sh` against `theo-db:m15` exits 0 and prints `SMOKE PASSED`.
- [ ] `grep -q "extname='theodb'" smoke.sh` exits 0 (assertion present).
- [ ] All pre-existing smoke assertions remain (no deletion) — `grep -c 'OK' smoke.sh` ≥ 7.

#### DoD
- [ ] `smoke.sh` green against the M15 image; CHANGELOG `[Unreleased]` updated.

---

## Phase 4: Distribution & docs

**Objective:** quickstart e2e of the 12 features + publishable image + honest limitation docs.

### T4.1 — `docs/quickstart.md` exercising all 12 features via the extension

#### Objective
Author a getting-started that installs the extension and exercises every `docs/features/` capability end to end.

#### Why this step (action + reasoning)
1. **What:** write `docs/quickstart.md`: `docker pull`/run → `CREATE EXTENSION theodb CASCADE` → a single SQL
   walkthrough touching all 12 features (vector similarity/HNSW/IVFFlat/DiskANN, hybrid, ai.* generate/
   sentiment/summarize/rank, generate_batch, NL→SQL, theodb_ml).
2. **Why now:** a product needs an onboarding path; this is the user-facing proof that "all features work
   together" (CTO directive). It doubles as the e2e script for the Final Phase.

#### Evidence
- The 12 features map to `docs/features/01..12-*.md`; the surfaces are the functions enumerated in Baseline.

#### Files to edit
```
docs/quickstart.md (NEW) — install + 12-feature e2e walkthrough
```

#### Deep file dependency analysis
- New doc; references the extension (T1) + image (T2). No code dependency.

#### Deep Dives
- **No performance numbers** in the quickstart (`.claude/rules/public-copy.md`) — it demonstrates capability,
  not speed. plpython3u-dependent features (06-12) are flagged as requiring the bundled image.

#### Tasks
1. Write `docs/quickstart.md` with a runnable 12-feature walkthrough.

#### TDD
```
RED:     test_quickstart_sql_runs() — extract the SQL fenced blocks from docs/quickstart.md and execute them against theo-db:m15; assert no error (FAILS: doc absent)
GREEN:   write docs/quickstart.md with runnable SQL
REFACTOR: None expected
VERIFY:  python3 -m pytest benchmarks/tests/test_extension_install.py::test_quickstart_sql_runs -q
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `test -f docs/quickstart.md` exits 0.
- [ ] `python3 -m pytest benchmarks/tests/test_extension_install.py::test_quickstart_sql_runs -q` exits 0 (every fenced SQL block runs without error against the image).
- [ ] `grep -c '```sql' docs/quickstart.md` ≥ 12 (one runnable block per feature area).

#### DoD
- [ ] Quickstart SQL runs green; CHANGELOG `[Unreleased]` updated.

### T4.2 — `make dist` + image publish target

#### Objective
Add `make dist` (source zip, pgvector model) and document/prepare the GHCR publish.

#### Why this step (action + reasoning)
1. **What:** add a `dist` target to the Makefile producing `dist/theodb-1.0.zip`; document the
   `docker build -t ghcr.io/usetheodev/theo-db:<version>` + push in the release flow.
2. **Why now:** an unpullable image is not a product (D4); the source zip is the extension-distribution
   artifact for non-container installs (seed for M5).

#### Evidence
- Model: `.claude/knowledge-base/references/pgvector/Makefile:71` (`git archive` zip), `:79` (docker build tag).

#### Files to edit
```
Makefile — add `dist` target (git archive theodb-1.0.zip)
README.md — add the publish/pull instructions (ghcr.io/usetheodev/theo-db)
```

#### Deep file dependency analysis
- Extends the Makefile (T1.2). README gains a distribution section. Actual `gh`/registry push happens in
  `/release` (not in this task — packaging only).

#### Deep Dives
- **Honesty:** the publish command is documented; the actual push to GHCR is performed during `/release` with
  the human-approved tag (Unbreakable Rule 4) — this task wires the target, not the credential.

#### Tasks
1. Add `dist` target to the Makefile.
2. Add publish/pull instructions to README.

#### TDD
```
RED:     test_make_dist_produces_zip() — run `make dist`; assert dist/theodb-1.0.zip exists (FAILS: no target)
GREEN:   add dist target
REFACTOR: None expected
VERIFY:  python3 -m pytest benchmarks/tests/test_extension_install.py::test_make_dist_produces_zip -q
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `make dist && test -f dist/theodb-1.0.zip` exits 0.
- [ ] `grep -q 'ghcr.io/usetheodev/theo-db' README.md` exits 0 (pull instructions present).

#### DoD
- [ ] `make dist` produces the zip; CHANGELOG `[Unreleased]` updated.

### T4.3 — README install section + honest plpython3u limitation

#### Objective
Document the install path and the plpython3u/superuser limitation honestly.

#### Why this step (action + reasoning)
1. **What:** add a README "Install" section (`CREATE EXTENSION theodb CASCADE`, superuser required) and a clear
   note that managed PGs without plpython3u get features 01-05 only.
2. **Why now:** honesty (Unbreakable Rule 3 / `.claude/rules/public-copy.md`) — the limitation is real (D2) and
   hiding it would be an overclaim.

#### Evidence
- Limitation source: D2 (plpython3u untrusted → superuser; managed PGs may forbid it).

#### Files to edit
```
README.md — Install section + plpython3u limitation note
```

#### Deep file dependency analysis
- README (Baseline row) gains an install section. No code dependency.

#### Deep Dives
- **No banned framings** (`.claude/rules/public-copy.md`): outcome-shaped install copy; the limitation is
  stated plainly, not as a competitor jab.

#### Tasks
1. Add the README install + limitation section.

#### TDD
```
RED:     test_readme_documents_limitation() — grep README for the plpython3u limitation sentence + CREATE EXTENSION theodb (FAILS: absent)
GREEN:   add the README section
REFACTOR: None expected
VERIFY:  python3 -m pytest benchmarks/tests/test_extension_install.py::test_readme_documents_limitation -q
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `grep -q 'CREATE EXTENSION theodb' README.md` exits 0.
- [ ] `grep -iq 'plpython3u' README.md` exits 0 (limitation documented).
- [ ] `bash hooks/public-copy-lint.sh README.md` (if present) reports no new banned framing (advisory).

#### DoD
- [ ] README updated; CHANGELOG `[Unreleased]` updated.

---

## Coverage Matrix

| # | Gap / Requirement (ROADMAP M15 DoD) | Task(s) | Resolution |
|---|---|---|---|
| 1 | `theodb.control` with locked fields | T1.1 | control file authored + requires test |
| 2 | `theodb--1.0.sql` built from bodies + PGXS Makefile | T1.2, T1.3 | concat build rule + extension-safety scan + install test |
| 3 | Upgrade convention `theodb--X--Y.sql` | T3.1 | 1.0→1.1 skeleton + ALTER EXTENSION UPDATE test |
| 4 | Dockerfile installs ext + `CREATE EXTENSION theodb` at init | T2.1 | runtime `make install` + 00-create-theodb.sql; six copies removed |
| 5 | install + upgrade + transactional tests + smoke extension | T1.3, T3.1, T3.2 | pytest install/upgrade/transactional + smoke.sh assertion |
| 6 | quickstart e2e + publish image + make dist | T4.1, T4.2 | runnable 12-feature quickstart + dist zip + GHCR instructions |
| 7 | README plpython3u limitation documented | T4.3 | README install + limitation section |

**Coverage: 7/7 gaps covered (100%)**

## Global Definition of Done

- [ ] All phases completed
- [ ] All tests passing — `python3 -m pytest benchmarks/tests/test_extension_install.py -q` green
- [ ] `smoke.sh` green against `theo-db:m15` (prints `SMOKE PASSED`)
- [ ] Zero lint warnings on changed files — `ruff check benchmarks/tests/test_extension_install.py`
- [ ] File-size budget respected (each source file ≤ 500 lines; `theodb--1.0.sql` is generated/gitignored)
- [ ] CHANGELOG.md updated under `[Unreleased]`
- [ ] Backward compatibility preserved — every `ai.*`/`theodb.*`/`theodb_ml.*` function keeps its signature/schema (install test asserts the surface)
- [ ] Plan-specific: bare `CREATE EXTENSION theodb` works on the TheoDB image; `CASCADE` works on any PG17 with pgvector+vectorscale buildable
- [ ] **Plan archived** — after `/review` READY_TO_MERGE AND the PR merged, move this plan to `knowledge-base/plans/completed/`

## Failure scenarios (when I/O external)

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| `postgres` (DB driver, psycopg2) | DB unavailable when the install test connects | point the test at a closed port / stopped container | test fails fast with a clear connection error; no partial assertion (the conftest fixture gates on `pg_isready`) |
| `CREATE EXTENSION theodb` (no CASCADE, deps absent) | required `vector`/`vectorscale` not installed | run `CREATE EXTENSION theodb` (without CASCADE) on a bare PG | PostgreSQL raises `required extension "vector" is not installed`; documented remedy is `CASCADE` (T4.3 README) |
| `make install` (filesystem) | `pg_config` not on PATH at image build | build stage without `postgresql-server-dev-17` | `make` fails loudly at build (not at runtime); Dockerfile ensures the dev package is present for the install step |

## Final Phase: Integration Validation (MANDATORY)

**Objective:** prove the extension installs and all 12 features work in a real container — not just unit asserts.

### Execution

```
docker build -t theo-db:m15 .                                   # image with the extension
# run a fresh container on a free port, wait for healthcheck
python3 -m pytest benchmarks/tests/test_extension_install.py -q  # install + upgrade + transactional + quickstart
bash smoke.sh                                                    # product smoke (incl. CREATE EXTENSION theodb)
ruff check benchmarks/tests/test_extension_install.py           # lint
make dist                                                       # dist zip
```

### Acceptance Criteria

- [ ] `docker build -t theo-db:m15 .` exits 0
- [ ] `python3 -m pytest benchmarks/tests/test_extension_install.py -q` exits 0 (install/upgrade/transactional/quickstart all green)
- [ ] `bash smoke.sh` prints `SMOKE PASSED` against the M15 image
- [ ] `psql -tAc "SELECT extversion FROM pg_extension WHERE extname='theodb'"` returns `1.0` on a fresh container
- [ ] Quickstart SQL (all 12 fenced blocks) runs without error against the image
- [ ] Zero lint warnings; `make dist` produces `dist/theodb-1.0.zip`
- [ ] Failure scenarios exercised: bare `CREATE EXTENSION theodb` (no CASCADE) on a bare PG raises the documented dependency error

### If Validation Fails

1. Identify plan-caused vs pre-existing failures.
2. Fix all plan-caused failures before declaring complete.
3. Re-run the chain.
4. Log pre-existing issues in the PR description; they do not block M15.
