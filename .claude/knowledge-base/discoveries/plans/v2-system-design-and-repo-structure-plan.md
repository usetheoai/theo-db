# Discovery Plan: FAANG-level System Design + Repo Structure for TheoDB V2

> **Version 1.1** — (v1.1 absorbs the edge-case review `v2-system-design-and-repo-structure-edge-cases-2026-06-29.md`: MUST-FIX EC-1 → ADR D4 marks paradedb+citus+vectorchord as AGPL structure-observation-only, NO code copied (D1); SHOULD-TEST EC-2 + DOCUMENT EC-3 → D3 now mandates a scale-appropriate, milestone-keyed, non-binding proposal.) Investigate, with a Staff-Engineer (Google) lens + the project's PhD-rigor profile
> (`.claude/rules/discover-phd-rigor.md`), how mature OSS Postgres-based databases + extensions structure
> their **system design** (layering, boundaries) and their **repository/monorepo** (where Rust + Go + SQL +
> tests + docs + CI + packaging live), so TheoDB can adopt a FAANG-level architecture + folder organization
> for the V2 journey (ROADMAP-v2 M17→M24). In scope (all already cloned): `paradedb` (primary — real
> multi-crate Rust/pgrx product), `supabase-postgres` (Postgres distribution repo), `citus` (large C
> extension), `cloudnative-pg` (Go K8s control plane — the M23 target), `duckdb` (engine layering), with
> `pgvectorscale` / `pg_mooncake` / `hydra` as scale contrasts. Output: a blueprint that unblocks a future
> `/to-plan` for an incremental FAANG restructuring + the V2 system-design foundation.

**Slug:** `v2-system-design-and-repo-structure`
**Owner:** TheoDB maintainers
**Created:** 2026-06-29
**Time budget:** 10h (per-project breakdown in ADR D1)

## Context

ROADMAP-v2 (ADR `docs/adr/0006-own-code-postgres-based-rust-go.md`) commits TheoDB to own code in Rust
(pgrx) + Go (control plane), incrementally, M17→M24. M17 just shipped the first Rust extension (`theodb_rs`)
— but the repo today is **flat and accreting clutter**: the root holds loose scripts (`smoke.sh`,
`migrate-smoke.sh`, `migrate-smoke-selftest.sh`, `migrate-doc-check.sh`), `sql/` is a flat list of 9 files,
`theodb_rs/` sits isolated, and there is no workspace/monorepo discipline. As M18→M24 add more Rust crates
(ai, nl, vector type, ANN index, quantization) + a Go control plane (M23) + observability (M24), this WILL
become a big-ball-of-mud without an architecture + folder contract decided NOW. This is a pre-V2 design
spike: decide the system-design layering + the repo structure before the bulk of V2 lands. The investigation
is grounded ONLY in cloned references (no fabricated citations — `discover-blueprint-golden-rule.md` hard
cap); AGPL refs (`vectorchord`) are pattern-only, never copied (CLAUDE.md D1).

## Objective

Produce a blueprint that lets us decide **(a)** TheoDB's V2 system-design layering (core vs PG-integration
glue vs adapters vs interface; the pgrx/Go/SQL boundaries) and **(b)** the repo/monorepo folder structure
(workspace layout; where Rust crates, Go control plane, SQL surface, tests, benchmarks, docs, CI, packaging
live), each anchored in real reference evidence.

- [ ] All research questions answered with citations to `.claude/knowledge-base/references/`
- [ ] Cross-cutting comparison table populated for every in-scope reference project
- [ ] Recommendations section provides ≥ 1 concrete decision proposal per research question (incl. a proposed
      target tree + an incremental migration ordering — not big-bang)
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS

## In-Scope / Out-of-Scope

### In-Scope (per reference project)

| Project | In-scope subdirectories | Reason |
|---|---|---|
| `.claude/knowledge-base/references/paradedb/` | `Cargo.toml`, `pg_search/src/`, `tests/`, `macros/`, `benchmarks/`, `Makefile`, `rust-toolchain.toml`, `scripts/`, `docker/`, `.github/` | **Primary STRUCTURE reference** — a real multi-crate Rust/pgrx PRODUCT; closest analog to TheoDB V2 (workspace, crate split, test crate, build tooling). **⚠️ AGPL-3.0 → structure/pattern observation ONLY, no code copied (D4 / D1).** |
| `.claude/knowledge-base/references/supabase-postgres/` | top-level layout, `ansible/`, `ci/`, `docker/`, `migrations/`, `nix/`, `Makefile`, `flake.nix`, `Dockerfile-17` | Postgres DISTRIBUTION repo — how a product bundles Postgres + extensions + build/CI/packaging |
| `.claude/knowledge-base/references/citus/` | `src/backend/{distributed,columnar}/`, `src/include/`, `src/test/{regress,tap}/`, `Makefile` | Large, mature C extension — system-design layering of a DB-scale extension + test tree. **⚠️ AGPL-3.0 → structure/pattern observation ONLY (D4 / D1).** |
| `.claude/knowledge-base/references/cloudnative-pg/` | `api/v1/`, `cmd/`, `internal/`, `pkg/`, `config/`, `hack/`, `Makefile`, `go.mod` | Go K8s operator — the M23 control-plane target structure |
| `.claude/knowledge-base/references/duckdb/` | `src/{catalog,execution,optimizer,parser,planner,storage,transaction,main}/` | Engine layering reference — how a real DB engine separates concerns (informs domain/layer naming) |
| `.claude/knowledge-base/references/pgvectorscale/`, `.claude/knowledge-base/references/pg_mooncake/`, `.claude/knowledge-base/references/hydra/` | top-level + `src/` | Scale contrast — smaller single-extension layouts (when NOT to over-structure) |

### Out-of-Scope (explicit)

| Project / Subdir | Why excluded |
|---|---|
| `.claude/knowledge-base/references/{paradedb,citus,vectorchord}/` source **code bodies** | **All three are AGPL-3.0** (verified: `paradedb/LICENSE` + `Cargo.toml` `license="AGPL-3.0"`, `citus/LICENSE`, `vectorchord/LICENSE`). Per CLAUDE.md D1, AGPL is barred from TheoDB's Apache distribution. **STRUCTURE/PATTERN observation ONLY** — folder layout, crate/workspace boundaries, layering taxonomy (a non-copyrightable method/idea, observed clean-room); **NEVER copy or derive their code bodies.** See ADR D4. |
| `.claude/knowledge-base/references/*/docs/` (prose/marketing) | Not source of truth for structure (except where a docs/ FOLDER's existence/placement is the datum) |
| `.claude/knowledge-base/references/*/{target,build,dist,node_modules,.venv}/` | Build artifacts |
| `.claude/knowledge-base/references/{patroni,pgbackrest,pinecone-python-client}/` deep dives | Operability/SDK refs — out of scope for THIS spike (system-design + repo structure); seed for a follow-up |
| Any project NOT cloned into `.claude/knowledge-base/references/` | Cross-Project Rule: never claim a feature without reading its source |

## ADRs

### D1 — Time budget + stop conditions
**Decision:** paradedb 3.5h (primary), supabase-postgres 1.5h, citus 1.5h, cloudnative-pg 1.5h, duckdb 1h,
pgvectorscale/pg_mooncake/hydra 1h combined.
**Rationale:** paradedb is the closest analog (multi-crate Rust/pgrx product) so it earns the deepest dive;
duckdb + the small extensions are informational contrast.
**Alternatives considered:** equal split (rejected — paradedb deserves more), single-project deep dive
(rejected — the blueprint needs cross-cutting comparison across structure styles).
**Stop condition — per question (mandatory):** when a question's investigation returns empty after 3
query-variant retries (pattern → path-glob → broader scope), mark it BLOCKED with reason "exhausted" and
continue. Do NOT pad with unrelated findings.
**Stop condition — per project (mandatory):** when a project's budget is exhausted with questions pending,
mark them BLOCKED "budget exhausted" and continue. If every remaining question is `done` or honestly
`blocked`, emit `<promise>BLUEPRINT_BLOCKED</promise>` (never `BLUEPRINT_COMPLETE` from a blocked state).
**Anti-pattern:** NEVER fabricate an answer to close a question whose investigation was exhausted (Rule 3).
**Consequences:** the halt-loop stops per budget; blocked questions surface in the blueprint as next-discovery seed.

### D2 — Investigation depth
**Decision:** For structure questions, read manifests (`Cargo.toml`/`go.mod`/`Makefile`) end-to-end + `ls`/
`tree -L 2` the in-scope dirs; for layering questions, Read representative module entrypoints (`lib.rs`,
`src/main/`, `internal/controller/`) — NOT every file. Capture the ORGANIZING PRINCIPLE, not a file census.
**Rationale:** the deliverable is a structure/layering blueprint, not an API reference; depth is at the
boundary/folder level, not per-symbol.
**Consequences:** fast, but the blueprint asserts structure patterns (with file:line evidence), not exhaustive code behavior.

### D3 — Synthesis target is a PROPOSAL, not a mandate; scale-appropriate + incremental (anti-YAGNI)
**Decision:** the blueprint proposes a target tree + migration ordering as a RECOMMENDATION; the binding
decision happens in a later `/to-plan` + an ADR. The proposal MUST be **scale-appropriate and milestone-keyed**
— TheoDB has ONE Rust crate today (`theodb_rs`); it MUST NOT cargo-cult paradedb's 6-member workspace now.
The Recommendations state WHEN to introduce a workspace / split crates (e.g., at the 2nd crate, M18/M20),
citing the single-crate contrast (pgvectorscale/pg_mooncake/hydra).
**Rationale:** restructuring touches everything; it must be incremental + risk-aware (measurement/risk over
big-bang) AND match current scale (YAGNI — `.claude/rules/parsimony-ladder.md`). Discovery informs; it does not lock.
**Consequences:** the blueprint's "Recommendations" are inputs to a future plan, explicitly non-binding; the
proposed tree carries an incremental ordering tied to M17→M24, not an immediately-imposed FAANG tree.

### D4 — License-aware investigation (D1): AGPL refs are structure-observation-only
**Decision:** `paradedb`, `citus`, `vectorchord` are **AGPL-3.0** (verified from their LICENSE files) — used
for **structure/pattern observation ONLY** (folder layout, workspace/crate boundaries, layering taxonomy, test
tree shape). **No source code is copied or derived** from them into TheoDB's Apache-2.0 distribution. The
permissive refs (`cloudnative-pg` Apache, `duckdb` MIT, `supabase-postgres` PostgreSQL License, `pgvectorscale`
PostgreSQL License, `pg_mooncake` MIT, `hydra` Apache) may inform code patterns too.
**Rationale:** CLAUDE.md D1 bars AGPL from the distribution (release gate). Organizational structure (how a repo
lays out folders/crates) is a non-copyrightable method/idea observable clean-room; code expression is not — the
boundary must be explicit so `/discover-execute` records STRUCTURE, never copies AGPL code bodies.
**Alternatives considered:** drop the AGPL refs entirely (rejected — paradedb is the best structural analog and
its LAYOUT is the highest-signal evidence; observing layout ≠ copying code); treat all refs identically (rejected
— ignores the D1 gate).
**Consequences:** the blueprint's paradedb/citus citations describe STRUCTURE (paths, folder roles, workspace
members), not code logic to lift; any code-pattern recommendation is sourced from a permissive ref or written clean.

## Research Questions

| # | Question | Corner | Reference project(s) | Fase A (broad map) | Fase B (deep Read) | Expected answer shape |
|---|---|---|---|---|---|---|
| Q1 | How do a large C extension (citus `src/backend/{distributed,columnar}`), an engine (duckdb `src/{catalog,execution,optimizer,parser,planner,storage}`), and a Rust/pgrx product (paradedb `pg_search/src/{index,query,scan,schema,postgres,api}`) separate **core logic** from **PG-integration glue** from **interface**? What layering/boundary pattern is common? | techniques | `citus/src/backend/`, `duckdb/src/`, `paradedb/pg_search/src/` | `ls`/`tree -L 2` each `src/`; identify the dir taxonomy | Read the entrypoints (`paradedb/pg_search/src/lib.rs`, `paradedb/pg_search/src/postgres/`, a citus `distributed/` planner file, a duckdb `execution/` header) to see how core is insulated from PG glue | A layering model (core / PG-glue / interface) with the dir-name evidence per project + a proposed mapping to TheoDB (pgrx glue vs domain vs SQL surface), `path` per row |
| Q2 | What **monorepo/workspace layout** does paradedb use (`Cargo.toml` workspace `members = [pg_search, tests, tokenizers, benchmarks, macros, stressgres]`) and how does supabase-postgres lay out a distribution repo (`ansible/`, `ci/`, `docker/`, `migrations/`, `nix/`)? How do they co-locate code + tests + benchmarks + packaging without a flat root? | techniques | `paradedb/Cargo.toml`, `paradedb/` top-level, `supabase-postgres/` top-level | `cat paradedb/Cargo.toml` (workspace block); `ls` both repo roots | Read paradedb workspace members + the role of each top-level dir; map supabase-postgres top-level dirs to responsibilities | A workspace/monorepo layout pattern + a proposed TheoDB top-level tree (crates/, control-plane/, sql/, tests/, benchmarks/, packaging/, docs/, ci/), citations per claim |
| Q3 | How does cloudnative-pg structure a **Go control plane** (`api/v1/` CRDs, `cmd/`, `internal/{controller,management,...}`, `pkg/{certs,management,...}`)? What's the boundary between the operator and the managed Postgres? | techniques | `cloudnative-pg/api/`, `cloudnative-pg/cmd/`, `cloudnative-pg/internal/`, `cloudnative-pg/pkg/` | `tree -L 2` `api/ cmd/ internal/ pkg/` | Read `api/v1/` (a CRD type) + `internal/controller/` entrypoint + a `pkg/` shared lib to see the api↔controller↔management split | Go control-plane layout (api/cmd/internal/pkg roles) + a proposed TheoDB `control-plane/` (Go) structure for M23, citations per row |
| Q4 | What **build/workspace tooling** do paradedb (`Makefile` targets `install-pgrx`/`pgrx-init`/`package`/`dist` + `rust-toolchain.toml` + `flake.nix`) and supabase-postgres (`Makefile` + `nix/` + `ci/`) use to manage a polyglot build + pinned toolchain + reproducibility? | tools | `paradedb/Makefile`, `paradedb/rust-toolchain.toml`, `paradedb/flake.nix`, `supabase-postgres/Makefile`, `supabase-postgres/nix/` | Glob + read the `Makefile`s + toolchain/nix files | Read each fully | A build-tooling pattern (Makefile target taxonomy, toolchain pinning, optional nix) + a proposed TheoDB Makefile/workspace tooling layout, citations |
| Q5 | How does paradedb organize **CI + release** (`.github/workflows/`, `RELEASE.md`, `scripts/`) and supabase-postgres (`ci/`)? Where does CI config live relative to the code it gates? | tools | `paradedb/.github/`, `paradedb/RELEASE.md`, `paradedb/scripts/`, `supabase-postgres/ci/` | `ls` `.github/workflows/` + `ci/`; list workflow files | Read 1–2 representative workflow files + RELEASE.md to capture the CI/release shape | CI/release layout pattern + a proposed TheoDB `.github/workflows` + release-doc placement, citations |
| Q6 | How does paradedb's **Cargo workspace** manage shared dependencies + version pinning across member crates (`[workspace.package]`, workspace-level deps) and how does cloudnative-pg's `go.mod` scope its module? What dependency-boundary discipline emerges for a polyglot monorepo? | deps | `paradedb/Cargo.toml`, `cloudnative-pg/go.mod` | `cat` both, focus on `[workspace.package]` / `[workspace.dependencies]` + the go module path | Read the workspace dep declarations + how member crates reference them | Dependency-boundary pattern (workspace.package, shared pins) + a proposed TheoDB workspace dep policy for M17→M24, citations |
| Q7 | Where do **tests** live in a multi-extension Postgres product? paradedb has a SEPARATE `tests` workspace-member crate + `tests/` dir; citus has `src/test/{regress,tap,cdc}`. How are unit (`#[pg_test]`) vs SQL-regression (`pg_regress`) vs e2e/integration vs benchmarks separated in the tree? | tests | `paradedb/tests/`, `paradedb/Cargo.toml` (members), `citus/src/test/` | `ls` `paradedb/tests/` + `citus/src/test/`; identify test categories | Read `paradedb/tests/README.md` (if present) + the test crate `Cargo.toml`; inspect citus `src/test/regress/` layout | A test-tree taxonomy (unit vs regress vs e2e vs bench, where each lives) + a proposed TheoDB test layout reconciling its current `benchmarks/tests/` + Rust `#[pg_test]`, citations |

## Coverage Matrix

| Corner | Questions mapped | Status |
|---|---|---|
| tests (integration) | Q7 | Covered |
| deps | Q6 | Covered |
| tools | Q4, Q5 | Covered |
| techniques | Q1, Q2, Q3 | Covered (≥2 per PhD-rigor profile) |

**Coverage: 4/4 corners covered (100%)**

Question count: 7 (within the PhD-rigor frontier budget 6–14; techniques corner has 3, ≤ 5 max).

## Halt-loop Checkpoints

| Checkpoint | Assertion | Action if fails |
|---|---|---|
| Before answering Qx | every `.claude/knowledge-base/references/{project}/{path}` declared for Qx exists | mark Qx BLOCKED "path not found", continue |
| Per-question budget | investigation returned ≥ 1 finding OR 3 query-variant retries attempted | after 3 retries empty, mark Qx BLOCKED "exhausted"; continue |
| After answering Qx | the blueprint section for Qx has ≥ 1 `references/` citation | re-iterate Qx (1 retry max) |
| Mid-loop sanity | citations to `references/` ≥ 1 per 200 words of prose | add citations to under-cited paragraphs (1 retry max) |
| Per-project budget | project time budget not exhausted | mark remaining Qx for that project BLOCKED "budget exhausted"; advance |
| Before promising complete | all 4 corners have populated sections AND a proposed target tree + migration ordering exist | refuse promise, continue iterating |

## Acceptance Criteria

- [ ] All research questions answered OR explicitly marked BLOCKED with reason
- [ ] Every citation resolves to a real path under `.claude/knowledge-base/references/`
- [ ] Cross-cutting comparison table across all in-scope projects populated
- [ ] Blueprint includes a PROPOSED TheoDB target tree + an INCREMENTAL migration ordering (D3 — proposal, not mandate)
- [ ] No AGPL (`vectorchord`) code copied/derived — structure observation only (D1)
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS

## Global Definition of Done

- [ ] `/discover-edge-cases` run + MUST-FIX absorbed
- [ ] `/discover-plan-confidence` ≥ SHIPPABLE_WITH_CAVEATS (no fabricated citation; corners non-empty)
- [ ] `/discover-execute` emits `BLUEPRINT_COMPLETE` (or honest `BLUEPRINT_BLOCKED`)
- [ ] `/discover-confidence` ≥ SHIPPABLE_WITH_CAVEATS per `.claude/rules/discover-blueprint-golden-rule.md`
- [ ] Honors `.claude/rules/architecture.md` (layering/DIP vocabulary), `.claude/rules/discover-phd-rigor.md` (SOTA anchoring, ≥2 sources per technique), CLAUDE.md D1 (no AGPL derivation)
