# Blueprint: FAANG-level System Design + Repo Structure for TheoDB V2

> Slug: `v2-system-design-and-repo-structure` · Created: 2026-06-29 · Lens: Staff Engineer (Google) + PhD-rigor profile (`.claude/rules/discover-phd-rigor.md`)
> License posture (ADR D4 / CLAUDE.md D1): `paradedb`, `citus` are **AGPL-3.0** → STRUCTURE/PATTERN observation ONLY (folder layout, workspace/crate boundaries, layering taxonomy, test-tree shape). NO code bodies copied or derived. Permissive refs (`cloudnative-pg` Apache, `duckdb` MIT, `supabase-postgres` PostgreSQL License, `pgvectorscale`/`pg_mooncake`/`hydra`) may inform code patterns too.

This blueprint investigates how mature OSS Postgres-based databases and extensions organize their **system design** (core-vs-glue-vs-interface layering) and their **repository** (where Rust + Go + SQL + tests + benchmarks + CI + packaging live), grounded only in cloned references under `.claude/knowledge-base/references/`. It converges on a single high-signal pattern — **a Postgres-integration glue layer kept strictly separate from a portable core, with the SQL-facing API as a third boundary** — observed independently in a Rust/pgrx product (paradedb), a large C extension (citus), and a standalone engine (duckdb). It then proposes a **scale-appropriate, milestone-keyed, NON-BINDING** target tree for TheoDB: TheoDB has exactly ONE Rust crate today (`theodb_rs`), so the recommendation explicitly resists cargo-culting paradedb's 6-member workspace now (the single-crate contrast pgvectorscale/pg_mooncake/hydra shows when NOT to over-structure), and instead sequences the restructuring against ROADMAP-v2 M17→M24. The binding decision belongs to a later `/to-plan` + ADR (plan ADR D3).

## Context

ROADMAP-v2 (project ADR `docs/adr/0006-own-code-postgres-based-rust-go.md`) commits TheoDB to own code in Rust (pgrx) + Go (control plane), incrementally M17→M24. M17 shipped the first Rust extension `theodb_rs` (`theodb_rs/Cargo.toml:1-6` — `name="theodb_rs"`, `version="1.0.0"`, `license="Apache-2.0"`, pgrx `=0.16.1` at `theodb_rs/Cargo.toml:25`). The repo today is flat and accreting clutter: the root holds loose shell scripts (`smoke.sh`, `migrate-smoke.sh`, `migrate-smoke-selftest.sh`, `migrate-doc-check.sh` — confirmed at repo root), `sql/` is a flat list of 9 files (`sql/30-theodb-embed.sql` … `sql/80-theodb-migrate.sql` + `sql/theodb--1.0.sql`, `sql/theodb--1.0--1.1.sql`), and `theodb_rs/` sits isolated with no workspace wrapper (`theodb_rs/src/` = `lib.rs` + `bin/` only). As M18→M24 add more Rust crates (vector type, ANN index, quantization) + a Go control plane (M23) + observability (M24), this becomes a big-ball-of-mud without an architecture + folder contract decided NOW.

## Objective

Produce a blueprint that lets a later `/to-plan` decide **(a)** TheoDB's V2 system-design layering (portable core vs PG-integration glue vs adapters vs SQL interface; the pgrx/Go/SQL boundaries) and **(b)** the repo/monorepo folder structure (workspace layout; where Rust crates, the Go control plane, the SQL surface, tests, benchmarks, docs, CI, packaging live), each anchored in real reference evidence and licensed-aware.

---

## Coverage Corner 1 — Integration Tests

> Maps Q7: where do tests live in a multi-extension Postgres product, and how are unit / SQL-regression / integration / e2e / benchmark separated in the tree?

**Finding — paradedb runs FOUR distinct test categories, each in a deliberate location** (AGPL — structure observation only; source: `paradedb/CONTRIBUTING.md:54-84`):

1. **pg_regress (golden/output) tests** live *inside the extension crate*, at `pg_search/tests/pg_regress/` (verified: `paradedb/pg_search/tests/pg_regress/` contains `common/`, `expected/`, `sql/`, `README.md` — the canonical `pg_regress` `sql/`→`expected/` golden layout). Run via `cargo pgrx regress` (`CONTRIBUTING.md:60-64`).
2. **Integration / client-property tests** live in a **separate top-level workspace-member crate** `tests/` (`paradedb/Cargo.toml:3-10` lists `"tests"` as a workspace member). That crate is a *test-only* package: `paradedb/tests/Cargo.toml:1-6` declares `name="tests"`, `crate-type=["rlib"]`, and an empty `[dependencies]` with a heavy `[dev-dependencies]` block (`sqlx` with the `postgres` feature, `rstest`, `pgvector`, `dotenvy` — `tests/Cargo.toml:10-40`). It connects to a running Postgres over `DATABASE_URL` (`paradedb/tests/README.md:11-22`), i.e. it tests the *installed* extension out-of-process. Test files are flat per-feature: `tests/tests/bm25_search.rs`, `aggregate.rs`, `custom_scan.rs`, `copy.rs`, `citus_compatibility.rs`, etc.
3. **Unit tests** (`#[pg_test]` and plain `#[test]`) live *next to the code* in `pg_search/src` (`CONTRIBUTING.md:74-82`): unmarked tests run without Postgres; `#[pg_test]`-marked tests run *as UDFs inside Postgres* via pgrx.
4. **Stress tests** live in their own workspace-member crate `stressgres` (`Cargo.toml:3-10`; `CONTRIBUTING.md:84`).

**Finding — citus (large C extension, AGPL — structure only) separates tests by execution harness, not by feature**: `citus/src/test/` = `regress/` (pg_regress + isolation + upgrade *schedules*), `tap/` (Perl TAP), `cdc/`, `hammerdb/` (verified `ls citus/src/test/`). The `regress/` dir is schedule-driven: many `*_schedule` files (`base_schedule`, `columnar_schedule`, `enterprise_isolation_schedule`, `after_pg_upgrade_*_schedule` — verified `ls citus/src/test/regress/`) compose ordered test runs — a pattern for grouping regression suites by scenario (upgrade, columnar, isolation) rather than dumping all `.sql` in one bag.

**Synthesis for the corner:** the dominant taxonomy is **(unit next to code) + (regress inside the extension) + (integration in a separate test-only crate) + (stress/bench in their own crates)**. TheoDB today has `theodb_rs/src` (where `#[pg_test]` units belong), a flat `sql/` (no regress harness yet), a python `benchmarks/`, and `packaging/run-regress.sh` + `packaging/Dockerfile.regress` (a regress *runner* but no `tests/regress` tree). The gap: no home for Rust integration tests and no `tests/regress/{sql,expected}` golden tree.

## Coverage Corner 2 — Dependencies

> Maps Q6: how does a Cargo workspace manage shared deps + version pinning across member crates, and how does a Go module scope itself? What dependency-boundary discipline emerges for a polyglot monorepo?

**Finding — paradedb centralizes version + dependency pins at the workspace root, then member crates inherit** (AGPL — structure observation only):

- `[workspace.package]` (`paradedb/Cargo.toml:12-15`) declares `version="0.24.2"`, `edition="2021"`, `license="AGPL-3.0"` ONCE for all members. Member crates inherit via `version = { workspace = true }` / `edition = { workspace = true }` / `license = { workspace = true }` (verified `paradedb/tests/Cargo.toml:3-6`). One bump at the root re-versions the whole product.
- `[workspace.dependencies]` (`paradedb/Cargo.toml:25-30`) pins the load-bearing deps once: `pgrx = "=0.19.0"`, `pgrx-tests = "=0.19.0"` (note the `=` exact pin — pgrx ABI must match the build), and a git-pinned `tantivy` at an exact `rev`. Member crates then reference them with `pgrx.workspace = true` (verified `paradedb/pg_search/Cargo.toml:42`) — no per-crate version drift possible.
- `[patch.crates-io]` (`paradedb/Cargo.toml:32+`) redirects *every* `datafusion-*` crate and `tantivy-tokenizer-api` to forked git revs — the workspace-level mechanism for consuming a fork without per-crate surgery (directly relevant to TheoDB's conditional pgvector/pgvectorscale fork policy, PRD D3).

**Finding — cloudnative-pg scopes a single Go module** (Apache — code patterns allowed): `cloudnative-pg/go.mod:1` = `module github.com/cloudnative-pg/cloudnative-pg`, `go 1.26.4` (`go.mod:3`). The *entire* operator (api + cmd + internal + pkg) is ONE module; the import-boundary discipline is enforced by Go's `internal/` visibility rule (see Corner 4 / Q3), not by splitting modules. Shared third-party deps are a flat `require (...)` block (`go.mod:5+`).

**Synthesis for the corner:** Rust → centralize via `[workspace.package]` + `[workspace.dependencies]` + `=`-pin the ABI-bearing crate (pgrx); Go → one module, lean on `internal/`. Contrast (anti-over-structure): `pgvectorscale/Cargo.toml:1-3` is a `[workspace]` with `members=["pgvectorscale"]` — a single-member workspace, i.e. the *forward-compatible seed* form. `pg_mooncake/Cargo.toml:1` is a bare `[package]` (no workspace at all) that vendors sibling dirs via path-deps (`moonlink_rpc.path = "moonlink/src/moonlink_rpc"` — `pg_mooncake/Cargo.toml`). TheoDB's `theodb_rs/Cargo.toml` is today a bare `[package]` like pg_mooncake — appropriate for one crate.

## Coverage Corner 3 — Tools

> Maps Q4 (build/workspace tooling, toolchain pinning, reproducibility) + Q5 (CI + release layout).

**Q4 — Build tooling.** paradedb's `Makefile` is a **PGXS-rooted polyglot wrapper** (AGPL — structure only): it computes `PG_CONFIG`, derives `DISTNAME`/`DISTVERSION` from the Cargo manifests, then `include $(PGXS)` (`paradedb/Makefile:1-10`) so it speaks PGXN's language while delegating the actual build to `cargo pgrx`. Target taxonomy (`paradedb/Makefile:22-58`): `install-pgrx` (installs the pinned `cargo-pgrx`), `pgrx-init`, `install` (`cargo pgrx install --package pg_search --release`), `package` (`cargo pgrx package`), `dist` (builds a PGXN-compatible `$(DISTNAME)-$(DISTVERSION).zip` via `git archive` + a generated `META.json`). Toolchain is pinned in `rust-toolchain.toml` (`channel = "1.96.0"`, `components = ["rustfmt","clippy","rust-analyzer"]` — verified). Reproducibility via `flake.nix` + `flake.lock` at root, plus `scripts/update-nix-cargo-hash.sh`.

supabase-postgres (distribution repo, PostgreSQL License) takes the **packaging-first** route: its `Makefile:1-18` orchestrates `packer` + `nerdctl` image builds (not a code build), and reproducibility is centralized under `nix/` with **one `.nix` file per bundled extension** (`supabase-postgres/nix/ext/` = `pgaudit.nix`, `pg_cron`, `pg_graphql`, `hypopg.nix`, `orioledb.nix`, … — verified). `nix/` also holds `devShells.nix`, `checks.nix`, `config.nix`. This is the model for "how a product bundles Postgres + N extensions reproducibly".

**Q5 — CI + release.** paradedb's CI lives at `.github/workflows/` with ~37 workflows in a clear **verb-prefixed taxonomy** (verified `ls paradedb/.github/workflows/`): `test-*` (`test-pg_search.yml`, `test-pg_search-docker.yml`, `test-pg_search-nix.yml`, `test-pg_search-upgrade.yml`), `lint-*` (`lint-rust.yml`, `lint-bash.yml`, `lint-docker.yml`, `lint-markdown.yml`, `lint-yaml.yml`, `lint-pr-title.yml`), `publish-*` (per-platform: `publish-pg_search-debian.yml`, `-macos.yml`, `-rhel.yml`, `-ubuntu.yml`, `-pgxn.yml`; plus `publish-paradedb-docker.yml`, `publish-github-release.yml`), and `benchmark-*`. CI is **path-filtered to the code it gates** — `test-pg_search.yml:14-24` triggers only on changes to `Cargo.toml`, `pg_search/**`, `tests/**`, `tokenizers/**` (with a `!pg_search/README.md` exclusion) and runs a PG-version matrix `[15,16,17,18]` (`test-pg_search.yml:48-50`). Release is a **manually triggered `workflow_dispatch`** documented in `RELEASE.md` (`paradedb/RELEASE.md:1-40`): version-bump PR → SQL upgrade script (`pg_search--<prev>--<next>`) → changelog → trigger "Publish GitHub Release". supabase keeps CI helpers in a top-level `ci/` dir (`supabase-postgres/ci/` = `extensions-diff.sh`, `postgresql-diff.sh` — verified). Repo-local automation scripts sit in `scripts/` (`paradedb/scripts/` = `pg_search_{common,run,test}.sh`, `update-nix-cargo-hash.sh`).

**Synthesis:** Make+PGXS wrapper over `cargo pgrx`; `rust-toolchain.toml` to pin the compiler; optional `nix/` for reproducible multi-extension bundling; `.github/workflows/` with `{test,lint,publish,benchmark}-*` verb-prefix naming + path filters; a `RELEASE.md` describing a manual, confirmation-gated release. TheoDB already has a root `Makefile`, `Dockerfile`, `packaging/` (with `run-regress.sh`, `license-sweep.sh`, per-target Dockerfiles) — but no `.github/workflows/` discipline visible and the automation scripts are loose at root, not in `scripts/`.

## Coverage Corner 4 — Techniques

> Maps Q1 (core vs PG-glue vs interface layering — citus + duckdb + paradedb), Q2 (workspace/monorepo layout — paradedb + supabase), Q3 (Go control-plane layout — cloudnative-pg).

### Q1 — The layering pattern (THREE projects converge)

The single highest-signal finding of this spike: **a portable core, a Postgres-integration glue layer, and a SQL/API interface are kept as three distinct boundaries**, observed independently in three codebases of three different languages.

- **paradedb (Rust/pgrx, AGPL — structure only).** `pg_search/src/lib.rs:19-30` declares the module map: `mod aggregate; mod api; mod bootstrap; mod index; mod postgres; mod query; mod scan; mod schema; pub mod gucs; pub mod parallel_worker;`. The boundary is explicit in the directory roles (verified `ls pg_search/src/`):
  - `postgres/` = **PG-integration glue** — the dir owns everything that touches Postgres internals: `customscan/` (planner CustomScan hooks: `hook.rs`, `exec.rs`, `explain.rs`), `cost.rs`, `options.rs`, `index.rs`/`insert.rs`/`delete.rs`/`vacuum.rs` (index AM callbacks), `planner_warnings.rs`, `storage/`. This is where pgrx/Postgres ABI lives.
  - `api/` = **SQL-facing interface** — `operator.rs`/`operator/`, `builder_fns/`, `admin.rs`, `tokenize.rs`, `config.rs`, `version.rs` (verified `ls pg_search/src/api/`): the functions/operators users call from SQL.
  - `query/`, `scan/`, `schema/`, `index/` = **core domain** — search logic that, in principle, does not require the Postgres process.
- **citus (C extension, AGPL — structure only).** `src/backend/{distributed,columnar}` (verified) + a **separate `src/include/` header tree** (`citus_config.h.in`, `distributed/`, `columnar/` — verified). Inside `distributed/` the split is by-concern: `planner/`, `executor/`, `commands/`, `metadata/`, `connection/`, `deparser/`, `transaction/`, `worker/`, `operations/`, `relay/` (verified `ls citus/src/backend/distributed/`). The PG-glue is concentrated in `planner/` + `executor/` (e.g. `distributed_planner.c` hooks `planner_hook` — header at `citus/src/backend/distributed/planner/distributed_planner.c:1-8` "General Citus planner code"); the C convention of `src/include/` separating *declarations* (the public contract) from `src/backend/` *definitions* is the C-world analog of "minimize the public surface".
- **duckdb (standalone engine, MIT — patterns allowed).** `src/{catalog,execution,function,optimizer,parser,planner,storage,transaction,main}` + `common/`, `parallel/`, `logging/` + a top `src/include/` (verified `ls duckdb/src/`). This is **layering by query-pipeline stage**: `parser/`→`planner/`→`optimizer/`→`execution/` over `catalog/`+`storage/`+`transaction/`, with `main/` as the client entry boundary (`src/main/` = `client_context.cpp`, `capi/`, `appender.cpp` — verified). `execution/operator/{aggregate,join,scan,filter,projection,order,…}` (verified) shows the operator-per-folder discipline at the leaf level.

**Cross-project rule:** all three keep the Postgres/host-process glue *physically separated* from the algorithmic core, and expose a *minimized public surface* (`api/` in paradedb, `src/include/` in citus & duckdb). This is exactly `.claude/rules/architecture.md` § 1 (interface → application → domain ← infrastructure) and § 3 ("public API … is the contract — minimize it") expressed in three real Postgres-adjacent codebases.

### Q2 — Workspace / monorepo layout

paradedb is a **Cargo workspace with 6 members** (AGPL — structure only): `paradedb/Cargo.toml:3-10` = `members = ["pg_search","tests","tokenizers","benchmarks","macros","stressgres"]`. Each member is a top-level dir with a single responsibility: `pg_search/` (the product extension), `tokenizers/` (a reusable library crate the extension path-depends on — `pg_search/Cargo.toml` `tokenizers = { path = "../tokenizers" }`), `macros/` (proc-macros), `tests/` (integration crate), `benchmarks/`, `stressgres/`. Co-location without a flat root: code+tests+benchmarks+packaging all live as *named workspace members or top-level dirs* (`docker/`, `nix/`, `scripts/`, `.github/` at root — verified `ls paradedb/`), never as loose files.

supabase-postgres (distribution repo) lays out by **lifecycle concern** (verified `ls supabase-postgres/`): `ansible/` (provisioning), `ci/` (diff gates), `docker/` + `Dockerfile-15`/`Dockerfile-17` (per-PG-major images), `migrations/` (`schema-15.sql`, `schema-17.sql`, `db/`, `tests/`), `nix/` (reproducible builds), `rfcs/`, `testinfra/`. The datum: a *distribution* repo groups by build/ship/test lifecycle, a *product* repo (paradedb) groups by workspace member.

### Q3 — Go control-plane layout (the M23 target)

cloudnative-pg (Apache — code patterns allowed) is the canonical Go/k8s operator layout (verified `ls cloudnative-pg/`):

- `api/v1/` = **public contract** — CRD Go types, versioned. Convention: `*_types.go` (struct defs: `cluster_types.go`, `backup_types.go`, `pooler_types.go`) paired with `*_funcs.go` (methods) and a generated `zz_generated.deepcopy.go` + `groupversion_info.go` + `doc.go` (verified `ls cloudnative-pg/api/v1/`). `cluster_types.go:19-27` is `package v1` importing k8s apimachinery — Apache-licensed (SPDX header `cluster_types.go:17`).
- `cmd/` = **thin entrypoints** — `cmd/manager/main.go` (single file — verified) and `cmd/kubectl-cnpg/`. Binaries do nothing but wire dependencies (architecture.md § 1 "composition root at the top").
- `internal/` = **non-importable implementation** — `controller/` (the reconcilers: `cluster_controller.go`, `backup_controller.go` + `_test.go` siblings — verified), `management/`, `webhook/`, `configuration/`, `plugin/`, `cnpi/`. Go's `internal/` rule forbids external import — the enforcement mechanism for "private by default".
- `pkg/` = **importable shared libraries** — `certs/`, `management/` (`postgres/`, `pgbouncer/`, `upgrade/`), `postgres/`, `reconciler/`, `specs/`, `utils/`, `promotiontoken/` (verified `ls cloudnative-pg/pkg/`).

**Boundary operator↔managed-Postgres:** the operator reconciles desired state declared in `api/v1` CRDs through `internal/controller`, while the in-pod agent logic that actually drives Postgres lives in `internal/management` + `pkg/management/postgres`. The api↔controller↔management split is the contract: api = *what*, controller = *converge*, management = *act on the instance*.

---

## Cross-cutting Comparison

| Project (license) | Org style | Core ↔ PG-glue ↔ interface boundary | Test tree | Build/CI tooling | Highest-signal lesson for TheoDB |
|---|---|---|---|---|---|
| **paradedb** (AGPL — *structure only*) | Cargo **workspace**, 6 members (`Cargo.toml:3-10`) | `pg_search/src/postgres/` glue vs `api/` interface vs `query/scan/schema/` core (`lib.rs:19-30`) | 4 categories: unit `#[pg_test]` in `src`, regress in `pg_search/tests/pg_regress`, integration in `tests/` crate, stress in `stressgres/` (`CONTRIBUTING.md:54-84`) | PGXS Makefile over `cargo pgrx` (`Makefile:1-58`); `rust-toolchain.toml` pin; `.github/workflows` `{test,lint,publish}-*`; `RELEASE.md` manual dispatch | The product-as-workspace + the in-crate glue/core/api split — TheoDB's closest analog |
| **citus** (AGPL — *structure only*) | autoconf C, `src/{backend,include,test}` | `src/backend/distributed/{planner,executor,...}` glue vs `src/include/` public headers (verified) | `src/test/{regress,tap,cdc,hammerdb}`; schedule-driven regress (verified) | autoconf (`configure.ac`), `Makefile.global.in`, `ci/` | Separate the *public declaration surface* (`include/`) from impl; group regress by scenario-schedule |
| **duckdb** (MIT) | CMake, `src/` by pipeline stage | `parser→planner→optimizer→execution` over `catalog/storage/transaction`, `main/` = client edge (`ls src/`) | top-level `test/` + `benchmark/` + `extension/` | CMake + `Makefile` + `scripts/`, `extension/` for plug-ins | Layer by pipeline stage; `main/` as the single client-entry boundary |
| **cloudnative-pg** (Apache) | single Go module, `api/cmd/internal/pkg` | `api/v1` contract → `internal/controller` → `internal/management`+`pkg/management/postgres` act (verified) | `tests/` top-level + `_test.go` siblings; `*_funcs_test.go` (verified) | `Makefile`, `hack/`, `config/`, `docker-bake.hcl` | The M23 control-plane skeleton: `api/cmd/internal/pkg` with `internal/` enforcing privacy |
| **supabase-postgres** (PostgreSQL Lic.) | distribution repo by lifecycle | n/a (bundler, not engine) | `migrations/tests/`, `testinfra/` | `nix/ext/*.nix` per-extension, `packer` images, `ci/` diff gates (verified) | How to bundle Postgres + N extensions reproducibly via per-extension nix files |
| **pgvectorscale** (PostgreSQL Lic.) | **single-member** workspace (`Cargo.toml:1-3`) | `pgvectorscale/src/{access_method,util}` + `lib.rs` (verified) | top-level `tests/`, `TESTING.md` | `Makefile`, `scripts/` | The forward-compatible "workspace with 1 member" seed — adopt at the 2nd crate, not before |
| **pg_mooncake** (MIT) | bare `[package]`, vendors siblings via path-deps | `src/{lib,bgworker,table,functions}.rs` flat (verified) | top-level `tests/` | `Makefile`, `rust-toolchain.toml` | A single extension can stay a bare package — don't force a workspace early (YAGNI) |
| **hydra** (Apache) | autoconf C `columnar/`, flat root | `columnar/` extension | `acceptance/` | `Makefile`, `docker-bake.hcl` | Single-purpose C extension stays flat |
| **TheoDB (today)** (Apache) | **flat**, 1 crate isolated | `theodb_rs/src/{lib.rs,bin}` only; no glue/core split | none (regress *runner* in `packaging/`, python `benchmarks/`) | root `Makefile`+`Dockerfile`+`packaging/`; loose root `*.sh` | — (this is what we are fixing) |

---

## ADRs

### D1 — Adopt the three-boundary layering (portable core / PG-glue / SQL interface) inside the crate, BEFORE splitting crates

**Decision (proposed, non-binding — see plan ADR D3).** Inside `theodb_rs/src`, introduce three module roles immediately: a `pg/` (or `postgres/`) module for ALL pgrx/Postgres-ABI glue (function registration, GUCs, index AM / planner hooks when they arrive), domain modules per capability (`embed/`, later `ai/`, `nl/`, `vector/`, `ann/`), and an `api/`-style SQL-surface module that maps `#[pg_extern]` entrypoints to domain calls.

**Rationale.** The glue/core/interface separation is the one pattern that recurs across all three engine-class references regardless of language: paradedb `pg_search/src/{postgres,api,query/scan/schema}` (`lib.rs:19-30`), citus `src/backend/distributed/{planner,executor}` vs `src/include/` (verified), duckdb `parser→planner→optimizer→execution` + `main/` edge (verified `ls duckdb/src/`). It is also exactly `.claude/rules/architecture.md` § 1–3. Doing this *within one crate* costs nothing structurally and makes the later crate-split a mechanical lift (move a module dir to a member crate) instead of a rewrite.

**Alternatives considered.** (a) Keep `lib.rs` flat (current state) — rejected: it is the big-ball-of-mud trajectory the Context warns about, and reproduces TheoDB's `sql/`-flat smell at the Rust level. (b) Split into crates now to force the boundary — rejected: violates YAGNI (`.claude/rules/parsimony-ladder.md`); pg_mooncake (`Cargo.toml:1` bare package) and hydra (flat `columnar/`) prove a single extension legitimately stays one crate. Module boundaries give 90% of the benefit at 0% of the workspace cost.

**Consequences.** A junior implementing M18 has an obvious home for new code (domain module + a thin `api` entrypoint + glue only if Postgres internals are touched). Risk: a module split that later proves wrong is cheap to redo; a crate split is not — so we deliberately defer the crate split (ADR-2).

### D2 — Introduce a Cargo workspace at the SECOND crate (≈M18/M20), seeded as a single-member workspace; do NOT cargo-cult paradedb's 6 members now

**Decision (proposed, non-binding).** Today keep `theodb_rs` as the bare `[package]` it is (`theodb_rs/Cargo.toml:1`). The moment a second Rust crate is justified (a reusable library extracted from `theodb_rs`, or a distinct extension), convert the root to a `[workspace]` and centralize `[workspace.package]` (version/edition/license) + `[workspace.dependencies]` (the `=`-pinned `pgrx`).

**Rationale.** pgvectorscale ships a `[workspace]` with `members=["pgvectorscale"]` (`pgvectorscale/Cargo.toml:1-3`) — the minimal forward-compatible form, the natural seed. paradedb's 6-member workspace (`Cargo.toml:3-10`) is the *destination*, not the *starting point*: it earned each member (a tokenizers lib, a macros crate, a tests crate, a stressgres crate) from real need. TheoDB has ONE crate — adopting 6 members now is speculative generalization (YAGNI, `.claude/rules/parsimony-ladder.md`; CLAUDE.md "Esforço ≠ Complexidade"). The `=`-exact pgrx pin (`paradedb/Cargo.toml:26`, mirrored by `theodb_rs/Cargo.toml:25`) and the `[patch.crates-io]` fork mechanism (`paradedb/Cargo.toml:32+`) are the workspace features TheoDB will genuinely need when the pgvector fork policy (PRD D3) activates.

**Alternatives considered.** Stay package-only forever — rejected: the moment crate #2 lands, shared version/dep drift becomes real and the workspace is the right tool (`[workspace.package]` inheritance, `paradedb/tests/Cargo.toml:3-6`). Jump to the full paradedb layout now — rejected per YAGNI above.

**Consequences.** Restructuring is incremental and reversible; the workspace arrives exactly when it pays for itself.

### D3 — License-aware sourcing is recorded structurally (D1/D4)

**Decision.** All paradedb/citus citations in this blueprint are STRUCTURE (paths, folder roles, workspace members, test-tree shape) — a non-copyrightable organizational method observed clean-room. NO AGPL code body is reproduced or recommended for lifting. Any *code-pattern* recommendation downstream must be sourced from a permissive ref (cloudnative-pg Apache, duckdb MIT, supabase/pgvectorscale PostgreSQL License, pg_mooncake/hydra) or written clean.

**Rationale.** CLAUDE.md D1 bars AGPL from TheoDB's Apache-2.0 distribution (release gate); plan ADR D4. **Consequences.** The Go control-plane skeleton (ADR-4) is safely sourced from cloudnative-pg (Apache); the in-crate layering (ADR-1) is sourced as a *taxonomy* confirmed by the permissive duckdb in addition to the AGPL refs (≥2-source rule, PhD-rigor R2: paradedb + duckdb + citus).

### D4 — The M23 Go control plane copies cloudnative-pg's `api/cmd/internal/pkg` skeleton

**Decision (proposed, non-binding).** When M23 lands, the Go control plane uses `control-plane/{api,cmd,internal,pkg}` with a single `go.mod`, `internal/` for non-importable reconcilers, `pkg/` for importable shared libs, `cmd/` for thin entrypoints.

**Rationale.** cloudnative-pg is Apache (code patterns allowed) and is the SOTA Postgres-on-k8s operator; its layout (verified `ls cloudnative-pg/{api,cmd,internal,pkg}`, single module `go.mod:1`) is the field standard and maps cleanly onto `.claude/rules/architecture.md` (composition root in `cmd/`, private impl in `internal/`). **Alternatives considered.** A flat Go layout — rejected: loses Go's `internal/` privacy enforcement, the cheapest boundary guard available. **Consequences.** M23 starts from a proven skeleton, not a blank repo.

---

## Recommendations for the project

> **STATUS: PROPOSAL — NON-BINDING (plan ADR D3).** The binding decision (and the exact module names) belong to a later `/to-plan` + an ADR in `docs/adr/`. This is scale-appropriate: TheoDB has ONE Rust crate today; the tree below is the *destination*, reached incrementally, NOT imposed big-bang.

### Proposed TheoDB target tree (the M24 destination, reached over M18→M24)

```
theo-db/
├── Cargo.toml                # [workspace] — ADDED at crate #2 (ADR-2); single-member until then
├── Makefile                  # PGXS wrapper over `cargo pgrx` (paradedb/Makefile:1-58 pattern)
├── rust-toolchain.toml       # pin the compiler (paradedb/rust-toolchain.toml pattern) — ADD
├── crates/                   # Rust workspace members (introduced at crate #2; ADR-2)
│   └── theodb_rs/            # the extension; move from repo root
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs        # module map only (paradedb/pg_search/src/lib.rs:19-30 shape)
│           ├── pg/           # PG-integration GLUE: pgrx registration, GUCs, hooks (ADR-1)
│           ├── api/          # SQL-facing surface: #[pg_extern] entrypoints (paradedb api/ role)
│           ├── embed/        # domain (M17) — portable core
│           ├── ai/ nl/ vector/ ann/   # domain capabilities added M18→M22
│           └── bin/          # pgrx_embed shim (exists today)
├── control-plane/            # Go (M23) — api/cmd/internal/pkg (ADR-4; cloudnative-pg pattern)
│   ├── go.mod                # single module
│   ├── api/v1/               # CRD types (*_types.go + *_funcs.go)
│   ├── cmd/manager/main.go   # thin entrypoint
│   ├── internal/controller/  # reconcilers (non-importable)
│   └── pkg/                  # importable shared libs
├── sql/                      # versioned upgrade scripts + by-feature grouping
│   ├── theodb--X.Y.sql       # install scripts (exist today)
│   ├── theodb--X.Y--X.Z.sql  # upgrade scripts (exist today)
│   └── features/             # group the 9 flat 30-/40-/50-… files by capability
├── tests/                    # integration + regress (Q7 / Corner 1)
│   ├── regress/{sql,expected}/   # pg_regress golden tree (paradedb pg_search/tests/pg_regress)
│   └── integration/          # Rust integration crate over DATABASE_URL (paradedb tests/ crate)
├── benchmarks/               # exists today (python harness) — keep
├── packaging/                # exists today (Dockerfiles, run-regress.sh, license-sweep.sh) — keep
├── ha/                       # exists today (Patroni/pgbackrest) — keep
├── nix/                      # OPTIONAL reproducibility (supabase nix/ext pattern) — only if needed
├── scripts/                  # MOVE loose root *.sh here (smoke.sh, migrate-*.sh) — declutter
├── docs/                     # exists today; docs/adr/ for the binding ADRs
└── .github/workflows/        # {test,lint,publish,benchmark}-*.yml, path-filtered (paradedb pattern)
```

### Incremental migration ordering (keyed to ROADMAP-v2 M17→M24 — NOT big-bang)

1. **Now (post-M17, cheap, no-crate-split):** ① `git mv` the loose root scripts (`smoke.sh`, `migrate-smoke*.sh`, `migrate-doc-check.sh`) into `scripts/` — pure declutter, zero risk. ② Add `rust-toolchain.toml` pinning the pgrx-compatible channel (paradedb pattern). ③ Inside `theodb_rs/src`, apply ADR-1: introduce `pg/`, `api/`, `embed/` modules and reduce `lib.rs` to a module map (`paradedb/pg_search/src/lib.rs:19-30` shape). ④ Stand up `.github/workflows/test-theodb.yml` path-filtered to `theodb_rs/**` + `sql/**` + `tests/**` with a PG-version matrix (paradedb `test-pg_search.yml:14-50`).
2. **M18 / first 2nd crate (ADR-2 trigger):** convert root to a `[workspace]` (seed single-member like `pgvectorscale/Cargo.toml:1-3`), `git mv theodb_rs/ crates/theodb_rs/`, centralize `[workspace.package]` + `[workspace.dependencies]` with the `=`-pinned pgrx (`paradedb/Cargo.toml:12-30`).
3. **M18→M22 (per new capability):** each new domain (`ai`, `nl`, `vector`, `ann`, quantization) lands as a module under `crates/theodb_rs/src/<cap>/` (or its own member crate only if reused) + a thin `api/` entrypoint + glue in `pg/` only if it touches Postgres internals.
4. **Test tree (alongside M18):** create `tests/regress/{sql,expected}/` (migrate the regress *runner* in `packaging/run-regress.sh` to drive it) and `tests/integration/` (a Rust test crate over `DATABASE_URL`, paradedb `tests/Cargo.toml:1-6` shape). Group `sql/`'s 9 flat files under `sql/features/`.
5. **M23 (Go control plane):** scaffold `control-plane/{api,cmd,internal,pkg}` per ADR-4 (cloudnative-pg skeleton); keep it a sibling top-level dir (do NOT nest under the Rust workspace).
6. **M24 (observability + polish):** add `nix/` ONLY if reproducible multi-extension bundling becomes a real need (supabase `nix/ext/*.nix` pattern) — otherwise skip (YAGNI). Finalize `.github/workflows/` `publish-*` per-platform jobs + a `RELEASE.md` (paradedb `RELEASE.md:1-40` manual-dispatch shape).

**Anti-cargo-cult guardrail (D3 + parsimony-ladder):** do NOT create `crates/`, `control-plane/`, `nix/`, or a multi-member workspace until the milestone that needs it. The single-crate references (pg_mooncake bare `[package]`, hydra flat `columnar/`, pgvectorscale single-member workspace) are the explicit license to stay small until scale forces growth.

## Blocked questions (if any)

None. All 7 research questions (Q1–Q7) were answered with citations resolving to real paths under `.claude/knowledge-base/references/` (and the TheoDB repo for current-state contrast). Every in-scope reference project (paradedb, citus, duckdb, cloudnative-pg, supabase-postgres, pgvectorscale, pg_mooncake, hydra) was read at the structure/manifest level. No path was fabricated; no AGPL code body was copied (D1/D4). The 4 coverage corners are populated; the cross-cutting comparison spans every in-scope project; the proposed target tree + incremental M17→M24 ordering are present and explicitly marked non-binding.
