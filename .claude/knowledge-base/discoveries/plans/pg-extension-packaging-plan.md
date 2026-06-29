# Discovery Plan: PostgreSQL extension packaging — turning TheoDB's init-scripts into an installable product

> **Version 1.1** — This discovery investigates how mature permissive PostgreSQL extensions (`pgvector`, `pgvectorscale`) and a SOTA Postgres distribution (`supabase-postgres`) package themselves as **installable, versioned, upgradeable products** — control files, install/upgrade SQL, PGXS build, and multi-platform distribution. The blueprint output unblocks **M15 (productization)**: replacing TheoDB's `docker-entrypoint-initdb.d` scripts (which only run on a fresh DB) with a real `CREATE EXTENSION theodb` umbrella extension that is installable on any PG17, versioned, and shippable beyond the container.

**Slug:** `pg-extension-packaging`
**Owner:** TheoDB maintainers
**Created:** 2026-06-28
**Time budget:** 8h (per-project breakdown in ADR D1)

## Context

TheoDB ships its AI surface (`ai.*`, `nl_*`, `theodb_ml`) as six SQL files copied into
`docker-entrypoint-initdb.d/` (`Dockerfile:64-81`). These run **only when the database is initialized
from scratch** — confirmed: there is no `theodb.control`, and `grep 'CREATE EXTENSION theodb'` matches
only the aspirational API docs (`docs/features/05-indice-scann.md`, `docs/features/12-linguagem-natural.md`),
not real code. Consequences that block the "product" claim (CTO directive 2026-06-28 — *"um produto, um
banco vetorial real com TODAS as features de `docs/features/`"*):

- No `CREATE EXTENSION theodb` → not installable on an existing PG, nor on managed Postgres.
- No `ALTER EXTENSION theodb UPDATE` → no versioned upgrade path; a running DB cannot receive new features.
- M1 (empacotamento) is still `[ ]` in `README.md`; the image is unpublished (no `ghcr.io/usetheodev`).

This discovery is the **prior-art gate** before planning M15. It honors:

- **Unbreakable Rule 9 (Don't Reinvent)** + `.claude/rules/parsimony-ladder.md` rung 2-4: use the standard
  PostgreSQL extension mechanism (control file + PGXS) — do not invent a bespoke loader.
- **`.claude/rules/architecture.md` § 1/§ 3** — extensions live in their own namespaces (the `ai`/`theodb_ml`
  schemas already do); the umbrella must respect that boundary, not flatten it.
- **`.claude/rules/discover-phd-rigor.md`** — ≥ 2 primary sources per technique; SOTA-anchored (the AlloyDB
  packaging model is closed, so the permissive SOTA anchors are `pgvector`/`pgvectorscale`/`timescaledb`).
- **CLAUDE.md TheoDB rule 3 (no engine fork; extensions OK)** and **rule 5 (no perf claims)** — packaging is
  not an engine change; this discovery makes no performance claim.

**Prior art already in this repo:** `.claude/knowledge-base/discoveries/blueprints/m1-core-packaging-blueprint.md`
established that TheoDB uses the unmodified PGDG `postgresql-17` engine and that `CREATE EXTENSION vector 0.8.3`,
`vectorscale 0.9.0`, `plpython3u 1.0` already succeed on a fresh container. This discovery extends that toward a
**first-party** `theodb` umbrella extension and its distribution.

## Objective

Produce a blueprint that lets us decide **how to package TheoDB's AI surface as a versioned, installable
`CREATE EXTENSION theodb` umbrella (SQL-only, plpython3u) with upgrade paths, plus a multi-platform
distribution strategy** — so M15 can be planned with a locked packaging approach.

Measurable success criteria for the blueprint:

- [ ] All research questions in this plan answered with citations to `.claude/knowledge-base/references/`
- [ ] Cross-cutting comparison table populated for every in-scope reference project (pgvector / pgvectorscale / supabase-postgres)
- [ ] Recommendations section provides at least one concrete decision proposal per in-scope research question
- [ ] An explicit recommendation on **SQL-only umbrella vs pgrx vs per-feature extensions** with evidence
- [ ] An explicit **upgrade-from-init-script-DB** migration strategy (the hardest technique)
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS

## In-Scope / Out-of-Scope

### In-Scope (per reference project)

| Project | In-scope subdirectories | Reason |
|---|---|---|
| `.claude/knowledge-base/references/pgvector/` | `vector.control`, `sql/vector--*.sql`, `Makefile`, `test/` | C extension that ships a versioned type + AMs via PGXS; the closest model for a **SQL+control** product (relocatable type, upgrade chain). |
| `.claude/knowledge-base/references/pgvectorscale/` | `pgvectorscale/vectorscale.control`, `pgvectorscale/sql/vectorscale--*.sql`, `Makefile`, `tests/`, `scripts/` | Umbrella-style `requires = 'vector'` + the richest **idempotent upgrade-script** corpus (20+ `--X--Y.sql`); the pgrx packaging path to contrast against SQL-only. |
| `.claude/knowledge-base/references/supabase-postgres/` | `migrations/tests/extensions/`, `migrations/db/`, `Dockerfile-17`, `Dockerfile-kubernetes`, `ansible/`, `nix/`, `*.pkr.hcl` | SOTA Postgres **distribution** — how a product versions extensions against PG majors and ships across container / RPM-deb (ansible) / nix / cloud images (packer). |

### Out-of-Scope (explicit)

| Project / Subdir | Why excluded |
|---|---|
| `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/` (Rust internals) | The DiskANN algorithm itself is out of scope — we package, we do not re-implement the AM (Rule 9). Only the pgrx **packaging artifacts** (control/sql generation) are studied. |
| `.claude/knowledge-base/references/supabase-postgres/{rfcs,docs,audit-specs}/` | Narrative/marketing/governance — not the packaging mechanism. |
| `.claude/knowledge-base/references/*/{target,build,dist,.venv}/` | Build artifacts. |
| Vector DB peers (Qdrant/Milvus/Weaviate/Chroma) | Standalone DBs, not Postgres extensions — irrelevant to extension packaging. |
| Any project NOT cloned into `.claude/knowledge-base/references/` | Cross-Project Rule: never claim a feature without reading its source. `timescaledb` is studied via its **official docs** (allowlisted `docs.timescale.com`), not a clone. |

## ADRs

### D1 — Time budget + stop conditions

**Decision:** pgvectorscale: 3h (richest upgrade corpus + umbrella `requires` + pgrx contrast), pgvector: 2h
(canonical control + PGXS Makefile), supabase-postgres: 2h (distribution/multi-platform), web (postgresql.org
`CREATE EXTENSION` / "Packaging Related Objects into an Extension" + docs.timescale.com upgrade): 1h.

**Rationale:** pgvectorscale is the deepest analog (an umbrella-ish extension with `requires='vector'` and a
mature idempotent upgrade-script chain — exactly the two hardest M15 problems), so it earns the deepest dive.
pgvector is the canonical minimal control+PGXS reference. supabase-postgres answers the distribution corner only.

**Alternatives considered:** equal split (rejected — pgvectorscale carries the hardest evidence); single-project
deep-dive (rejected — the distribution corner needs supabase, the upgrade corner needs pgvectorscale, no single
ref covers both).

**Stop condition — per question (mandatory):** When a question's Fase A returns empty matches after 3 consecutive
retries with different query variants (pattern → kind-based → alternate path → broader scope), mark the question
BLOCKED with reason "Fase A exhausted — no hotspots found" and continue. Do NOT pad with unrelated hotspots.

**Stop condition — per project (mandatory):** When a project's time budget is exhausted with N questions still
pending, mark all remaining questions for that project BLOCKED with reason "budget exhausted" and continue. If
every remaining project is in the same state (every question `done` or honestly `blocked`), emit
`<promise>BLUEPRINT_BLOCKED</promise>` (NOT `BLUEPRINT_COMPLETE`) with the honest blocked-questions report.

**Anti-pattern:** NEVER fabricate Fase B answers to close a question whose Fase A was exhausted. Honest BLOCKED
with reason is required (Unbreakable Rule 3).

**Consequences:** the halt-loop stops iterating on a project when its budget is exhausted; blocked questions are
surfaced explicitly and become next-discovery seed.

### D2 — Investigation depth

**Decision:** Read control files, Makefiles, and a representative sample of upgrade scripts **end-to-end** (they
are small and load-bearing); use Grep/Glob + targeted Read for the test directories and distribution tooling
(packer/ansible/nix are large — sample the extension-relevant entrypoints, not the whole tree).

**Rationale:** the control file + one full upgrade script + the PGXS Makefile ARE the packaging contract —
reading them partially loses the exact idiom we must copy. The distribution tooling only needs the
extension-install entrypoints, not full infra coverage.

**Consequences:** deep, line-exact citations for the control/upgrade/PGXS technique; intentionally shallower
(entrypoint-level) coverage of the distribution corner — flagged honestly in the blueprint.

### D3 — Coverage corners (all four covered; none deferred)

**Decision:** All four corners are covered (see Coverage Matrix). No ADR-deferral needed.

**Rationale:** packaging touches tests (does `CREATE EXTENSION`/`ALTER EXTENSION UPDATE` pass?), deps (what the
umbrella `requires`), tools (PGXS build + distribution), and techniques (control anatomy + upgrade-from-init).

**Consequences:** the blueprint must populate all four; the frontier-profile techniques corner carries 3 questions.

## Research Questions

- **Fase A (broad)** — produces a hotspot map (which files, how many).
- **Fase B (deep)** — reads each hotspot in detail, capturing intent + exact `path:line` citation.

| # | Question | Corner | Reference project(s) | Fase A (broad — map) | Fase B (deep — Read at each hotspot) | Expected answer shape |
|---|---|---|---|---|---|---|
| Q1 | How do pgvector and pgvectorscale **test** that `CREATE EXTENSION` and `ALTER EXTENSION ... UPDATE` succeed (install + upgrade regression)? | tests | `.claude/knowledge-base/references/pgvector/test/`, `.claude/knowledge-base/references/pgvectorscale/tests/` | Glob `test/**` / `tests/**`; Grep `CREATE EXTENSION`, `ALTER EXTENSION`, `installcheck` across both test dirs + Makefiles | Read each matching test/harness to capture how a fresh install and an upgrade-path are asserted | Table: test name → install vs upgrade → assertion, with `path:line` per row |
| Q2 | How does `supabase-postgres` **test** that its bundled extensions install on the distribution image? | tests | `.claude/knowledge-base/references/supabase-postgres/migrations/tests/extensions/` | Glob `migrations/tests/extensions/**`; Grep `CREATE EXTENSION` | Read the extension-test entrypoints to capture the per-extension install assertion pattern | Description of the test harness + `path:line` citations |
| Q3 | What must a **SQL-only umbrella** extension declare to depend on `vector` + `vectorscale` + `plpython3u`, and what does `requires` actually enforce at install time? | deps | `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/vectorscale.control`, `.claude/knowledge-base/references/pgvector/vector.control` + web (postgresql.org CREATE EXTENSION) | SKIP Fase A (text-shape) — Read both `.control` files fully; WebFetch postgresql.org "CREATE EXTENSION" §requires/§schema | Read control fields (`requires`, `schema`, `relocatable`, `superuser`, `default_version`); confirm `requires` ordering + whether plpython3u (a PL) can be a `requires` target | Field-by-field control contract for a `theodb` umbrella + the dependency-install order + citations |
| Q4 | How is a **SQL-only / control+SQL** extension built and installed via PGXS (where do `.control` + `--version.sql` land; what does `pg_config` resolve)? | tools | **PRIMARY** `.claude/knowledge-base/references/pgvector/Makefile` (pure PGXS SQL-only model) | Grep `PGXS`, `EXTENSION`, `DATA`, `DATA_built`, `pg_config` in `pgvector/Makefile` | Read the PGXS variable block (`EXTENSION = vector` / `DATA = $(wildcard sql/*--*--*.sql)` / `DATA_built` / `include $(PGXS)`) to capture the SQL-only install path (no `.so` needed for a plpython3u-only umbrella) | The minimal PGXS Makefile shape for a SQL-only extension + install-dir resolution + citations. **EC-1 fix:** `pgvectorscale/Makefile` is pgrx-hybrid (`cargo build`) → NOT the SQL-only model; it is the pgrx **contrast** in Q8 only. |
| Q5 | How does `supabase-postgres` package its **distribution across platforms** (container + RPM/deb via ansible + nix + cloud images via packer)? | tools | `.claude/knowledge-base/references/supabase-postgres/{Dockerfile-17,Dockerfile-kubernetes,ansible/,nix/,amazon-amd64-nix.pkr.hcl}` | Glob the listed entrypoints; Grep `extension`, `pgvector`, `apt`, `nix` in `ansible/` + `Dockerfile-17` | Read the extension-install steps in Dockerfile-17 + the ansible/nix entrypoints to capture the multi-platform packaging skeleton | Distribution matrix: platform → how extensions are installed → artifact, with citations |
| Q6 | What is the exact **control-file anatomy** for an umbrella extension (`default_version`, `relocatable`, `superuser`, `schema`, `requires`) and what trade-offs do `pgvector` (relocatable) vs `pgvectorscale` (non-relocatable, superuser) encode? | techniques | `.claude/knowledge-base/references/pgvector/vector.control`, `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/vectorscale.control` | SKIP Fase A (text-shape) — Read both control files fully | Compare every field across the two; map each to a `theodb` umbrella decision (relocatable? superuser? schema-bound `ai`/`theodb_ml`?) | Side-by-side control-field table → recommended `theodb.control` shape + citations |
| Q7a | What is the **idempotent upgrade-script idiom** (`CREATE OR REPLACE`, `DO $$` existence guards, `@extschema@`)? | techniques | `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/sql/vectorscale--0.8.0--0.9.0.sql` (+ 2 more `--X--Y.sql` samples) | Glob `pgvectorscale/pgvectorscale/sql/vectorscale--*.sql`; pick 3 representative; Grep `CREATE OR REPLACE`, `DO $$`, `@extschema@`, `IF NOT EXISTS` | Read the 3 scripts end-to-end to extract the idempotency idiom | The upgrade-idiom checklist + citations |
| Q7b | How can a new `theodb` extension **adopt objects already created by init-scripts** (e.g. `ALTER EXTENSION theodb ADD FUNCTION`) — a TheoDB-specific migration the clones never do (they are greenfield `CREATE EXTENSION`)? | techniques | **web** postgresql.org + `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/sql/` (to confirm absence of orphan-adoption) | Grep `ALTER EXTENSION` across `pgvectorscale/.../sql/` (expect none → confirms greenfield); WebFetch postgresql.org "Packaging Related Objects into an Extension" for `ALTER EXTENSION ADD` | A concrete **init-script → extension adoption** strategy, citing the PG doc; **EC-2 fix:** state explicitly the clones are greenfield here (no orphan-adoption evidence in them) — honesty (Rule 3), not a fabricated citation |
| Q8 | For a **plpython3u (SQL-only)** surface, is pgrx relevant, or is a control+SQL extension the right tool — what concretely differs in the packaging artifacts vs pgvectorscale's pgrx output? | techniques | `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/sql/vectorscale--0.8.0--0.9.0.sql` (pgrx-generated, `LANGUAGE c`), `.claude/knowledge-base/references/pgvector/sql/` (hand-written C+SQL) | Grep `LANGUAGE c`, `auto generated by pgrx`, `module_pathname` across both `sql/` dirs + control files | Read a pgrx-generated script header vs a hand-written one; confirm TheoDB's `ai.*` (plpython3u, no `.so`) needs neither pgrx nor `module_pathname` | Decision matrix: pgrx vs C+SQL vs pure-SQL(plpython3u) → recommendation for `theodb` + citations |

## Coverage Matrix

| Corner | Questions mapped | Status |
|---|---|---|
| Integration tests | Q1, Q2 | Covered |
| Dependencies | Q3 | Covered |
| Tools | Q4, Q5 | Covered |
| Techniques | Q6, Q7a, Q7b, Q8 | Covered |

**Coverage: 4/4 corners covered (100%)** — techniques carries 4 (Q6, Q7a, Q7b, Q8); total 9 questions, ≤ 5 per
corner, within the frontier-profile budget 6-14 (`discover-phd-rigor.md § 2`).

## Halt-loop Checkpoints

| Checkpoint | Assertion | Action if fails |
|---|---|---|
| Before answering Qx | every `.claude/knowledge-base/references/{project}/{path}` declared in Fase A exists | Mark Qx BLOCKED with reason "path not found", continue |
| Per-question Fase A budget | Fase A returned ≥ 1 hotspot OR 3 query-variant retries attempted | After 3 retries empty, mark Qx BLOCKED "Fase A exhausted"; continue |
| After answering Qx | Blueprint section under Qx has ≥ 1 citation | Re-iterate Qx (1 retry max) |
| Web-source discipline | every WebFetch host ∈ `.claude/rules/discover-web-allowlist.txt` (postgresql.org / docs.timescale.com / supabase.com / github.com) | Drop the off-allowlist source; mark the sub-claim UNBENCHMARKED/uncited |
| Mid-loop sanity | citations to `.claude/knowledge-base/references/` ≥ 1 per 200 words of prose | Add citations to under-cited paragraphs (1 retry max) |
| Per-project time budget | project budget not exhausted | When exhausted, mark remaining Qx for that project BLOCKED "budget exhausted"; advance |
| Q3 superuser implication (EC-3) | before Q3 DONE, blueprint states whether `requires '…, plpython3u'` forces `superuser = true` (plpython3u is untrusted, superuser-only); evidence pointer `vectorscale.control` sets `superuser = true` | Re-iterate Q3 to capture the superuser consequence (1 retry) |
| Q2 prefer local timescaledb (EC-4) | use `supabase-postgres/migrations/tests/extensions/{01..10}-*.sql` as the per-extension install-test pattern; WebFetch `docs.timescale.com` ONLY for an upgrade idiom the local test lacks | Default to the local refs; avoid unnecessary network round-trips |
| Before promising complete | all 4 coverage corners populated AND Q7b has a concrete init-script→extension adoption strategy (doc-cited) AND Q8 has a pgrx-vs-SQL-only recommendation | Refuse promise, continue iterating |

## Acceptance Criteria

- [ ] All research questions answered OR explicitly marked BLOCKED with reason
- [ ] All four coverage corners have populated sections in the blueprint
- [ ] Every citation in the blueprint points to a real `.claude/knowledge-base/references/{...}` path
- [ ] A concrete recommendation: SQL-only umbrella vs pgrx vs per-feature (Q8) with evidence
- [ ] A concrete init-script → extension adoption strategy (Q7)
- [ ] At least one ADR section in the blueprint synthesizes decisions taken
- [ ] Time budget respected per project
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS
- [ ] Blueprint saved at `.claude/knowledge-base/discoveries/blueprints/pg-extension-packaging-blueprint.md`

## Global Definition of Done

- [ ] All phases completed (plan → edge-cases → plan-confidence → execute → confidence → improve if needed → confidence re-score)
- [ ] Final `/discover-confidence` verdict recorded in the blueprint header
- [ ] No fabricated citations
- [ ] Coverage Matrix 100% covered
- [ ] ADRs reference at least one project rule/principle (`architecture.md` boundaries, `parsimony-ladder.md` Rule 9, `discover-phd-rigor.md`)
