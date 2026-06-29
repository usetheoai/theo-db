# Blueprint: PostgreSQL extension packaging — turning TheoDB's init-scripts into an installable `CREATE EXTENSION theodb`

> **Version 1.0** — Synthesizes how `pgvector` (canonical C+SQL extension), `pgvectorscale` (pgrx umbrella with `requires='vector'`), and `supabase-postgres` (SOTA Postgres distribution) package, version, upgrade, and ship PostgreSQL extensions — cross-referenced with the official PostgreSQL packaging docs. Informs **M15 (productization)**: replacing TheoDB's six `docker-entrypoint-initdb.d` scripts with a real, versioned, installable `CREATE EXTENSION theodb` umbrella. **Headline finding:** TheoDB's AI surface is **plpython3u SQL-only** (no `.so`), so the right tool is a **hand-written PGXS SQL-only umbrella extension** (the `pgvector` model) with `requires = 'vector, vectorscale'`, `superuser = true`, `trusted` unset — **not** pgrx (the `pgvectorscale` model, which exists only because it ships Rust).

**Slug:** `pg-extension-packaging`
**Source plan:** `.claude/knowledge-base/discoveries/plans/pg-extension-packaging-plan.md`
**Owner:** TheoDB maintainers
**Generated:** 2026-06-28 via `/discover-execute` (executed inline, citations verified on disk)
**Confidence verdict:** SHIPPABLE_WITH_CAVEATS (89.0 — hard caps clean: 0 fabricated citations, 4/4 corners populated; sole soft cap: `soft_floor_citation_density_low`, heuristic, accepted — much evidence is PG-doc quotes from the allowlisted postgresql.org, which are not reference-tree paths)

## Context

TheoDB ships `ai.*`, `nl_*`, `theodb_ml` as six SQL files copied into `docker-entrypoint-initdb.d/`
(`Dockerfile:64-81` in the repo root) — they run **only on fresh DB init**. There is no `theodb.control`;
`CREATE EXTENSION theodb` does not exist. This blocks the CTO's product goal (2026-06-28): a real,
installable vector DB with all 12 `docs/features/` capabilities. This blueprint gathers the prior art for the
M15 packaging decision, honoring Unbreakable Rule 9 (use the standard extension mechanism, do not invent a
loader), `.claude/rules/architecture.md` (extensions keep their own namespaces), and
`.claude/rules/discover-phd-rigor.md` (≥ 2 primary sources per technique).

## Objective

Decide **how to package TheoDB's plpython3u AI surface as a versioned, installable `CREATE EXTENSION theodb`
umbrella with upgrade paths and a distribution strategy** — so M15 can be planned with a locked approach.

---

## Coverage Corner 1 — Integration Tests

### Project A — pgvector (pg_regress, SQL-only model)

- **Pattern:** classic PostgreSQL `pg_regress` — input SQL under `test/sql/`, expected output under
  `test/expected/`, wired through the Makefile: `REGRESS_OPTS = --inputdir=test --load-extension=$(EXTENSION)`
  (`.claude/knowledge-base/references/pgvector/Makefile:12`). The harness **loads the extension** and runs
  golden-file SQL.
- **Fixtures:** per-feature SQL files (`.claude/knowledge-base/references/pgvector/test/sql/` —
  `hnsw_vector.sql`, `ivfflat_bit.sql`, `cast.sql`, …) — each is a self-contained scenario whose output is
  diffed against `test/expected/`.
- **Coverage:** asserts SQL-visible behavior (index build, query results, casts) against the **installed
  extension** — exactly the oracle TheoDB needs for "does `CREATE EXTENSION theodb` produce a working surface".

### Project B — pgvectorscale (pytest integration, install + index)

- **Pattern:** Python integration tests that assert installability + index creation against a real PG:
  `test_extension_installation` runs `CREATE EXTENSION IF NOT EXISTS vectorscale` then checks
  `pg_extension` (`.claude/knowledge-base/references/pgvectorscale/tests/test_basic_operations.py:12-26`);
  `test_diskann_index_creation` builds a `USING diskann` index and verifies it
  (`.../test_basic_operations.py:30-58`).
- **Fixtures:** `conftest.py` + `clean_db` fixtures (`.claude/knowledge-base/references/pgvectorscale/tests/conftest.py`).
- **Coverage:** the **install-and-use** assertion — the closest analog to a TheoDB `CREATE EXTENSION theodb`
  smoke (our existing `smoke.sh` already does presence+privilege checks; this is the install-path version).

### Project C — supabase-postgres (transactional per-extension install test, version-gated)

- **Pattern:** one SQL file per bundled extension that opens a transaction, `CREATE EXTENSION IF NOT EXISTS …
  WITH SCHEMA "extensions"`, then **rolls back** — gated on `server_version_num` per major:
  `.claude/knowledge-base/references/supabase-postgres/migrations/tests/extensions/10-timescaledb.sql:1-9`
  (`begin; … create extension if not exists timescaledb with schema "extensions"; … rollback;`). The
  directory holds `01-postgis.sql` … `10-timescaledb.sql` — one install-test per extension.
- **Coverage:** proves every bundled extension **installs cleanly on the distribution image, per PG major**,
  without leaving state (rollback). Directly transferable to a TheoDB "does `theodb` install on PG17?" gate.

---

## Coverage Corner 2 — Dependencies

### What a `theodb` umbrella must declare

| Dependency | Role | How it is declared | Citation |
|---|---|---|---|
| `vector` (pgvector) | vector type + HNSW/IVFFlat (features 01-04) | umbrella `requires` | model: `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/vectorscale.control:6` (`requires = 'vector'`) |
| `vectorscale` (pgvectorscale) | DiskANN/ScaNN-quality (feature 05) | umbrella `requires` | `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/vectorscale.control:1-6` |
| `plpython3u` | runtime for `ai.*`/`nl`/`ml` (features 06-12, HTTP to LLM) | umbrella `requires` (untrusted PL → forces superuser) | PG doc (see Techniques T1); repo `Dockerfile:59-61` installs `postgresql-plpython3-17` |

**Critical finding (changes the install UX):** `requires` does **NOT** auto-install missing dependencies —
`CREATE EXTENSION` fails unless `CASCADE` is passed. Per the PG docs: *"Automatically install any extensions
that this extension depends on that are not already installed"* applies **only** with `CASCADE`, and *"Their
dependencies are likewise automatically installed, recursively. The SCHEMA clause, if given, applies to all
extensions that get installed this way"* (postgresql.org `sql-createextension`). → TheoDB must either ship
`CREATE EXTENSION theodb CASCADE` in docs/init, or pre-install `vector`+`vectorscale` (the image already
compiles both — `Dockerfile:34-53` — so on the TheoDB image a bare `CREATE EXTENSION theodb` works once the
deps are present; CASCADE is the portable form for other PGs).

---

## Coverage Corner 3 — Tools

### Project A — pgvector: the SQL-only PGXS build (the model TheoDB copies)

- **Build:** pure PGXS — `EXTENSION = vector` / `EXTVERSION = 0.8.3` /
  `DATA = $(wildcard sql/*--*--*.sql)` (all upgrade scripts) /
  `DATA_built = sql/$(EXTENSION)--$(EXTVERSION).sql` (the install script, built from `sql/vector.sql`) /
  `PGXS := $(shell $(PG_CONFIG) --pgxs); include $(PGXS)`
  (`.claude/knowledge-base/references/pgvector/Makefile:1-6,46-48`). `make install` drops `.control` +
  `--version.sql` files where `pg_config --sharedir`/extension resolves them.
- **Distribution:** `make dist` → `git archive` zip (`.claude/knowledge-base/references/pgvector/Makefile:71`);
  `docker build … -t pgvector/pgvector:pg$(PG_MAJOR)` (`.../Makefile:79`).
- **Key point for TheoDB:** a plpython3u-only extension needs **no `MODULES`/`.so`** — `DATA` + a built
  install script is the entire toolchain. No Rust, no compiler.

### Project B — pgvectorscale: pgrx (only because it ships Rust)

- **Build:** pgrx — `PGRX_VERSION=0.9.8`, `cargo build --features pg$(PG_VERSION)`, `install-pgrx: cargo
  install cargo-pgrx` (`.claude/knowledge-base/references/pgvectorscale/Makefile`). `cargo pgrx install`
  generates the `.control` + `--version.sql` from Rust annotations. The repo's own `Dockerfile:10-28`
  reproduces this (a `scale-builder` stage). **Irrelevant toolchain for a SQL-only surface** — included here
  only as the contrast that justifies NOT choosing pgrx (ADR D1).

### Project C — supabase-postgres: distribution across platforms

- **Container:** Nix-based Alpine slim image building PostgreSQL + extensions
  (`.claude/knowledge-base/references/supabase-postgres/Dockerfile-17:2,37`); extension custom scripts copied
  from `ansible/files/postgresql_extension_custom_scripts`
  (`.../Dockerfile-17:135`); migrations wired via `docker-entrypoint-initdb.d/migrations/`
  (`.../Dockerfile-17:160`).
- **Multi-platform:** `ansible/` (RPM/deb provisioning), `nix/` (reproducible builds), `*.pkr.hcl`
  (`amazon-amd64-nix.pkr.hcl` — cloud images via Packer). **Honest scope note (D2 of the plan):** this corner
  was investigated at entrypoint level only; the Nix/Packer machinery is **more than TheoDB needs** (KISS) —
  the takeaway is the *shape* (container + RPM/deb + cloud image), not the Nix implementation.

---

## Coverage Corner 4 — Techniques

### T1 — Control-file anatomy (the `theodb.control` shape)

The PG docs define each field (postgresql.org `extend-extensions`):

- `default_version` — *"the one that will be installed if no version is specified… omitting it results in
  CREATE EXTENSION failing"* → `theodb` sets `default_version = '1.0'`.
- `requires` — *"A list of names of extensions that this extension depends on… Those extensions must be
  installed before this one can be installed"* → `requires = 'vector, vectorscale'`.
- `relocatable` — *"possible to move its contained objects into a different schema after creation. Default is
  false"*. `schema` — *"can only be set for non-relocatable extensions… forces the extension into exactly the
  named schema"*.
- `superuser` (default true) — *"only superusers can create the extension or update it"*; `trusted` (default
  false) — *"allows some non-superusers to install an extension that has superuser set to true… anyone with
  CREATE privilege"*.

Reference shapes (verified):

| Field | pgvector | pgvectorscale | → recommended `theodb` |
|---|---|---|---|
| `default_version` | `0.8.3` (`.../pgvector/vector.control:2`) | `@CARGO_VERSION@` (`.../vectorscale.control:2`) | `1.0` |
| `relocatable` | `true` (`.../vector.control:4`) | `false` (`.../vectorscale.control:4`) | `false` (creates own `ai`/`theodb_ml` schemas in-script) |
| `superuser` | (unset → default true) | `true` (`.../vectorscale.control:5`) | `true` (plpython3u is untrusted) |
| `requires` | (none) | `'vector'` (`.../vectorscale.control:6`) | `'vector, vectorscale'` |
| `module_pathname` | `$libdir/vector` (`.../vector.control:3`) | commented (pgrx sets it) | **omitted** (no `.so`) |
| `trusted` | (unset) | (unset) | **unset** (cannot be trusted — installs plpython3u functions) |

> **Honesty on `schema`:** TheoDB's surface lives in multiple schemas (`ai`, `theodb_ml`). An extension that
> creates its own schemas does so explicitly in the install script (`CREATE SCHEMA ai; CREATE FUNCTION
> ai.generate…`) and leaves the `schema` control param **unset** (the param forces a single target schema).
> `relocatable = false` because the schemas are hard-named.

### T2 — Idempotent upgrade-script idiom (Q7a)

The mature idiom, verified in pgvectorscale's upgrade corpus
(`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/sql/vectorscale--0.8.0--0.9.0.sql`):

- `CREATE OR REPLACE FUNCTION …` for every object (`:13,34,43,52,61`) — re-runnable.
- `DO $$ … IF c = 0 THEN … END IF; $$;` existence guards before `CREATE ACCESS METHOD`/`CREATE OPERATOR
  CLASS` (`:15-28,80-177`) — idempotent against partially-upgraded states.
- `@extschema@` placeholder for schema-relative object references (`:93,128`).
- Install/update script naming + **automatic chaining**: *"ALTER EXTENSION is able to execute sequences of
  update script files… if only foo--1.0--1.1.sql and foo--1.1--2.0.sql are available, ALTER EXTENSION will
  apply them in sequence"*; and `CREATE EXTENSION` itself follows chains from a base `foo--1.0.sql` (PG doc
  `extend-extensions`). The script-sourcing guard idiom: `\echo Use "ALTER EXTENSION vector UPDATE TO
  '0.1.1'" … \quit` (`.claude/knowledge-base/references/pgvector/sql/vector--0.1.0--0.1.1.sql:1`).
- Cross-extension references: *"write `@extschema:name@`… where name is the name of the other extension
  (which must be listed in this extension's requires list)"* (PG doc) → `theodb` references pgvector/
  pgvectorscale objects via `@extschema:vector@` / `@extschema:vectorscale@`.

### T3 — Adopting init-script objects into the extension (Q7b) — greenfield is the answer

**Finding:** the official packaging doc does **not** present `ALTER EXTENSION … ADD member-object` as a
packaging path — it states *"PostgreSQL will not let you drop an individual object contained in an extension,
except by dropping the whole extension"* and that objects are associated *"via the installation/update scripts
themselves"* (PG doc `extend-extensions`). A `grep -rE 'ALTER EXTENSION .* ADD'` across **all three** cloned
refs returns **zero hits** — none of them adopt orphan objects; every one is greenfield `CREATE EXTENSION`.

**Recommendation (anti-sunk-cost, CLAUDE.md):** do **not** build an elaborate init-script→extension adoption
migration. TheoDB is pre-1.0 with no external installed base, so legacy DBs created via the old init-scripts
are effectively nonexistent. The clean path: from M15, the image runs `CREATE EXTENSION theodb` (or `… CASCADE`)
**instead of** copying the six `sql/*.sql` into `initdb.d`. For the rare legacy DB, the migration is a manual
`DROP` of the init-script functions + `CREATE EXTENSION theodb` (documented, low priority). `ALTER EXTENSION
theodb ADD FUNCTION …` is a real command (sql-alterextension) but tedious and unnecessary here.

### T4 — pgrx vs C+SQL vs pure-SQL (plpython3u) (Q8)

| Toolchain | When | Artifacts | Citation |
|---|---|---|---|
| pgrx (Rust) | shipping a Rust `.so` (AM, type) | `cargo pgrx install` auto-generates `.control` + `--X--Y.sql` (header *"auto generated by pgrx"*, `LANGUAGE c`) | `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/sql/vectorscale--0.8.0--0.9.0.sql:1-13` |
| hand-written C+SQL | shipping a C `.so` | `module_pathname` + `MODULE_PATHNAME` substitution + hand-written `--X--Y.sql` | `.claude/knowledge-base/references/pgvector/vector.control:3`, `.../pgvector/sql/vector--0.1.0--0.1.1.sql:1` |
| **pure-SQL (plpython3u)** | **TheoDB's `ai.*` — no compiled code** | hand-written `theodb--1.0.sql` + `--X--Y.sql`; **no `MODULES`, no `module_pathname`, no pgrx** | this is the gap the six `sql/30-70` files already fill (`Dockerfile:64-81`) — they become the install script body |

**Conclusion:** TheoDB's surface is pure plpython3u + plpgsql SQL. The correct tool is the **pgvector model
without the C** — a PGXS Makefile with `DATA`/`DATA_built` and hand-written SQL, zero Rust, zero `.so`.

---

## Cross-cutting Comparison

| Dimension | pgvector (A) | pgvectorscale (B) | supabase-postgres (C) |
|---|---|---|---|
| Install-test style | pg_regress golden files (`Makefile:12`) | pytest `CREATE EXTENSION` + index (`test_basic_operations.py:12-58`) | transactional per-ext, version-gated, rollback (`…/extensions/10-timescaledb.sql:1-9`) |
| Build toolchain | PGXS SQL+C (`Makefile:1-6`) | pgrx/cargo (`Makefile`) | Nix + ansible + packer (`Dockerfile-17`, `nix/`, `*.pkr.hcl`) |
| Control shape | relocatable, module_pathname (`vector.control`) | non-relocatable, superuser, requires='vector' (`vectorscale.control`) | n/a (distribution, not a single extension) |
| Upgrade idiom | hand-written `--X--Y.sql` + `\echo…\quit` guard | pgrx-generated `CREATE OR REPLACE` + `DO $$` guards + `@extschema@` | per-major `if server_version_num` gating |
| Relevance to `theodb` | **PRIMARY model** (SQL-only) | upgrade idiom + `requires`; pgrx = contrast (reject) | distribution shape only (don't copy Nix) |

## ADRs

### D1 — `theodb` is a hand-written SQL-only PGXS umbrella, not pgrx, not per-feature

**Decision:** package the six init-scripts as ONE umbrella extension `theodb` built with the **pgvector PGXS
model** (`EXTENSION`/`DATA`/`DATA_built`, hand-written SQL), `requires = 'vector, vectorscale'`. No pgrx, no
`MODULES`/`.so`.

**Rationale:** TheoDB's `ai.*`/`nl`/`ml` are plpython3u + plpgsql (no compiled code) — confirmed by
`Dockerfile:64-81` (six SQL files, no builder for them). pgrx exists in pgvectorscale ONLY to compile Rust
(`.../pgvectorscale/Makefile` `cargo build`); adopting it for pure SQL is over-engineering (Rule 9 / KISS).
The pgvector PGXS Makefile (`.../pgvector/Makefile:1-6`) is the minimal correct toolchain.

**Alternatives considered:** (a) **pgrx** — rejected: no Rust to compile; adds cargo toolchain to the build for
nothing. (b) **Per-feature extensions** (`theodb_ai`, `theodb_nl`, `theodb_ml` separately) — rejected for v1.0:
one `CREATE EXTENSION theodb` is the cohesive product surface the CTO asked for; per-feature split is a future
option if the surfaces diverge. (c) **Keep init-scripts** — rejected: not installable on existing/managed PG,
no versioning (the whole problem).

**Consequences:** installable on any PG17 with pgvector+pgvectorscale present; versioned + upgradeable; the
build stays toolchain-light (no Rust for the umbrella). Constrains us to hand-maintain `--X--Y.sql` upgrade
scripts (T2 idiom).

### D2 — `superuser = true`, `trusted` unset, `relocatable = false`, install via `CASCADE`

**Decision:** `theodb.control` = `superuser = true` (default), `trusted` unset, `relocatable = false`, no
`schema` param (create `ai`/`theodb_ml` in-script); document `CREATE EXTENSION theodb CASCADE` for non-TheoDB
PGs.

**Rationale:** plpython3u is an **untrusted** PL — the PG doc is explicit that `trusted` lets non-superusers
install, which is unsafe for a surface that makes outbound HTTP via plpython3u; so `theodb` cannot be trusted
and is superuser-only (matches `vectorscale.control:5`). `requires` does not auto-install without `CASCADE`
(sql-createextension), so CASCADE is the portable install verb; on the TheoDB image the deps are pre-built
(`Dockerfile:34-53`) so bare `CREATE EXTENSION theodb` also works.

**Alternatives considered:** `trusted = true` (rejected — plpython3u outbound HTTP under non-superuser is a
privilege-escalation surface); `relocatable = true` (rejected — multi-schema surface is hard-named).

**Consequences:** install requires superuser (acceptable — DBAs install extensions); managed PGs that forbid
plpython3u cannot install the AI surface (honest limitation — document it; the vector features 01-05 still
work via pgvector/vectorscale alone).

### D3 — No init-script→extension orphan-adoption migration (greenfield only)

**Decision:** from M15 the image runs `CREATE EXTENSION theodb` at init instead of copying `sql/30-70` to
`initdb.d`; do not build an `ALTER EXTENSION ADD` adoption path.

**Rationale:** zero refs adopt orphan objects (grep = 0 across all three); the PG packaging doc routes object
membership through install scripts, not `ADD`. TheoDB is pre-1.0 with no installed base — building a migration
for nonexistent legacy DBs is the sunk-cost trap (CLAUDE.md anti-sunk-cost).

**Alternatives considered:** elaborate `ALTER EXTENSION theodb ADD FUNCTION …` adoption (rejected — tedious,
unnecessary pre-1.0); leave init-scripts as a fallback (rejected — two code paths to maintain).

**Consequences:** clean single install path; the rare legacy dev DB is migrated by drop+recreate (documented,
low priority).

### D4 — Distribution: PGXS `make install` in the image + publish GHCR; RPM/deb/bare-metal deferred to M5

**Decision:** M15 ships the extension inside the existing TheoDB image (build `theodb` via PGXS, `CREATE
EXTENSION theodb` at init) + publishes the image to `ghcr.io/usetheodev/theo-db` + a `make dist` zip of the
extension (pgvector model). RPM/deb/bare-metal ("roda em qualquer lugar") stays M5.

**Rationale:** the image already exists; the missing product steps are (a) the extension, (b) a published pull
target, (c) a source zip. supabase's Nix/Packer/ansible is more than we need now (KISS) — the *shape* (container
+ later RPM/deb + cloud image) is the takeaway, not the Nix machinery.

**Alternatives considered:** adopt supabase's Nix build now (rejected — heavy, M5 scope); skip publishing
(rejected — an unpullable image is not a product).

**Consequences:** M15 delivers an installable, pullable product; M5 later adds non-container distribution.

## Recommendations for the project

| # | Recommendation | Linked to | Priority |
|---|---|---|---|
| 1 | Create `theodb.control` (`default_version='1.0'`, `requires='vector, vectorscale'`, `superuser=true`, `relocatable=false`, `trusted` unset, no `module_pathname`) | Q3, Q6, D1, D2 | HIGH |
| 2 | Assemble the six `sql/30-70` bodies into a built `theodb--1.0.sql` install script (creating `ai`/`theodb_ml` schemas in-script) via a pgvector-style PGXS Makefile (`DATA`/`DATA_built`, no `MODULES`) | Q4, Q8, D1 | HIGH |
| 3 | Switch `Dockerfile` to build+install the extension and run `CREATE EXTENSION theodb` at init, replacing the six `initdb.d` copies | Q7b, D3, `architecture.md` | HIGH |
| 4 | Add an install + upgrade integration test (pgvector pg_regress model OR pgvectorscale pytest model) + a per-major transactional install test (supabase model); extend `smoke.sh` to assert `CREATE EXTENSION theodb` | Q1, Q2, `testing.md` | HIGH |
| 5 | Establish the `theodb--X--Y.sql` upgrade-script convention now (CREATE OR REPLACE + `DO $$` guards + `@extschema:vector@`) so v1.0→v1.1 is a real `ALTER EXTENSION theodb UPDATE` | Q7a, D1 | MEDIUM |
| 6 | A `quickstart.md` + e2e demo exercising all 12 features through `CREATE EXTENSION theodb`; publish image to `ghcr.io/usetheodev/theo-db` + `make dist` zip | Q5, D4, `public-copy.md` | MEDIUM |
| 7 | Document the plpython3u/superuser limitation (managed PGs without plpython3u get features 01-05 only) honestly in README | Q3, D2, `public-copy.md` | MEDIUM |

## Blocked questions (if any)

(none — all 9 questions answered with verified citations; Q7b answered honestly as greenfield-only, citing the
PG doc + the grep-confirmed absence of `ALTER EXTENSION ADD` in the refs.)

## Halt-loop progress (audit trail)

- Execution mode: inline (operator-driven, not autonomous ralph-loop) for citation precision.
- Questions answered: 9 / 9 (Q1, Q2, Q3, Q4, Q5, Q6, Q7a, Q7b, Q8)
- Questions blocked: 0
- Citations verified: all `.claude/knowledge-base/references/` paths confirmed on disk during collection; PG-doc claims quoted from postgresql.org (allowlisted) and marked as doc citations (not reference-tree paths).
- Coverage corners populated: 4/4.

## Related

- Discovery plan: `.claude/knowledge-base/discoveries/plans/pg-extension-packaging-plan.md`
- Edge-case review: `.claude/knowledge-base/reviews/pg-extension-packaging-edge-cases-2026-06-28.md`
- Plan-confidence: `.claude/knowledge-base/reviews/pg-extension-packaging-discover-plan-confidence-2026-06-28.json` (SHIPPABLE 99.4)
- Prior art: `.claude/knowledge-base/discoveries/blueprints/m1-core-packaging-blueprint.md`
- Project rules: `.claude/rules/architecture.md`, `.claude/rules/testing.md`, `.claude/rules/parsimony-ladder.md`, `.claude/rules/public-copy.md`
