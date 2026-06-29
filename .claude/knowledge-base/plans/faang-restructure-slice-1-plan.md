---
slug: faang-restructure-slice-1
created_at: 2026-06-29
goal: Apply the blueprint's first restructuring slice (declutter root scripts + in-crate module split + toolchain pin) at behavior parity, measured by the full 18-test suite passing unchanged against the rebuilt image plus a non-regression latency benchmark.
---

# Plan: FAANG Restructure — Slice 1 (declutter + in-crate layering + toolchain pin)

> **Version 1.1** — (v1.1 absorbs the edge-case review: EC-1 → schema parity is SEMANTIC, not byte-identical — pgrx declares emission order unstable; the authoritative oracle is the 18-test suite + the deployed `\df` surface. EC-2 → T1.1 fixes `migrate-doc-check.sh`'s `$HERE/docs/...` GUIDE path to `$HERE/../docs/...` after the move.) The first, mechanical, behavior-preserving slice of the FAANG restructuring decided by the
> `v2-system-design-and-repo-structure` blueprint (SHIPPABLE 97.8). It pays the structural debt that M18→M24
> would otherwise multiply, WITHOUT any behavior change: move the loose root scripts into `scripts/`, split the
> single `theodb_rs/src/lib.rs` into the blueprint's 3-boundary layering (pg-glue / domain / api-surface), and
> pin the Rust toolchain. The Cargo workspace + new CI are explicitly DEFERRED (blueprint ADR-2 — workspace at
> the 2nd crate/M18; CI already exists and is only path-updated here). Parity is proven by the existing 18-test
> suite passing unchanged + a non-regression latency benchmark.

## Goal

> "Restructure TheoDB's root scripts + `theodb_rs` crate internals (move loose scripts to `scripts/`, split
> `lib.rs` into pg/embed/api modules, pin the toolchain) at full behavior parity, measured by the complete
> 18-test suite (`test_embed_sql.py` 10 + `test_embed_failure_scenarios.py` 3 + `test_bench_embed.py` 5)
> passing UNCHANGED against the rebuilt image (the authoritative parity oracle — NOT a byte-diff of the
> pgrx-generated SQL, whose item order is declared unstable) AND `docs/benchmarks/faang-restructure-slice-1-parity.md`
> showing no latency regression vs the M17 baseline (13.92 ms/call)."

## Context

The `v2-system-design-and-repo-structure` blueprint (`/discover-confidence` SHIPPABLE 97.8) decided TheoDB's
V2 system-design layering + FAANG repo structure. Its migration ordering puts FOUR mechanical, low-risk moves
"now" (before M18 multiplies the debt): declutter loose root scripts, pin the toolchain, and split the crate
internals into the 3-boundary layering — while explicitly DEFERRING the Cargo workspace to the 2nd crate
(blueprint ADR-2, YAGNI). This plan executes exactly that first slice. It is a **pure refactor**: no SQL
contract, no behavior, no dependency changes — so parity is the gate, proven by the existing test suite passing
unchanged + a non-regression benchmark (CTO requirement: measured data).

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `smoke.sh` | 177 | `03575f3`-era (tracked) | Root smoke test (extension surface) | Must run from `scripts/smoke.sh`; CI invokes it (ci.yml:67,262,323,436) |
| `migrate-smoke.sh` | 120 | tracked | M3 migration smoke (pg_dump→restore→assert) | Sibling scripts reference it via `$HERE/migrate-smoke.sh` (BASH_SOURCE-relative); CI ci.yml:162 |
| `migrate-smoke-selftest.sh` | 33 | tracked | TDD self-test of the migration asserts | Uses `HERE="$(dirname BASH_SOURCE)"` + `$HERE/migrate-smoke.sh` — preserved if moved together; CI ci.yml:165 |
| `migrate-doc-check.sh` | 26 | tracked | Proves `docs/migration/minimal-migration.md` commands match `migrate-smoke.sh` | Uses `$HERE/migrate-smoke.sh` (sibling) + reads the guide doc; guide-path resolution MUST hold after move; CI ci.yml:159 |
| `.github/workflows/ci.yml` | (existing) | — | CI pipeline | The 4 root-script invocations (`bash smoke.sh`×4, `bash migrate-doc-check.sh`, `bash migrate-smoke.sh`, `bash migrate-smoke-selftest.sh`) MUST be repointed to `scripts/`; `ha/*`, `packaging/*` already in subdirs (unchanged) |
| `docs/migration/minimal-migration.md` | (existing) | — | User guide; lines 100-102 show `bash migrate-smoke.sh` etc. | The documented invocation commands must match the new path AND still pass `migrate-doc-check.sh` |
| `theodb_rs/src/lib.rs` | 240 | `03575f3` (2026-06-29) | The whole Rust extension: helpers (err_input/err_external/guc/truncate), `_embed_text`, `#[pg_schema] mod theodb_rs`, `extension_sql!` wrapper, 4 `#[pg_test]` | After split: `cargo pgrx schema` MUST emit the SAME SQL (`theodb_rs._embed_text` + `theodb.embed`); same behavior; clippy 0 warnings |
| `theodb_rs/src/pg.rs` (NEW) | 0 | — | pg-glue layer: `err_input`/`err_external` (ereport SQLSTATE) + `guc()` (Spi current_setting) | — |
| `theodb_rs/src/embed.rs` (NEW) | 0 | — | domain layer: the embedding logic (minreq POST + SSRF + parse + format + typed errors) | — |
| `theodb_rs/rust-toolchain.toml` (NEW) | 0 | — | toolchain pin `channel = "1.91.0"` | Must equal Dockerfile `ARG RUST_VERSION=1.91.0` |
| `docs/benchmarks/faang-restructure-slice-1-parity.md` (NEW) | 0 | — | non-regression benchmark report | — |
| `CHANGELOG.md` | (existing) | — | Public contract | `[Unreleased]` updated (Changed — refactor) |

### Current callers / dependents

- **Root scripts** — invoked by `.github/workflows/ci.yml` (the only mechanizable caller; lines 67/159/162/165/262/323/436) + documented in `docs/migration/minimal-migration.md:100-102`. Historical mentions in `knowledge-base/**`, `CHANGELOG.md`, `ROADMAP.md` are audit-trail prose — NOT updated (immutable history; Rule 6 forbids editing released CHANGELOG entries).
- **`theodb_rs` Rust symbols** — `_embed_text` is called by the `theodb.embed` SQL wrapper (extension_sql, same file); `err_input`/`err_external`/`guc`/`truncate` are private to the crate (no external caller). `grep -rn '_embed_text' theodb_rs/` → only `lib.rs`. The split is internal; the SQL surface (`theodb._embed_text`, `theodb.embed`) is the only external contract and MUST be preserved (signatures byte-identical; the generated-SQL OBJECT SET identical order-insensitively — EC-1, pgrx item order is unstable).

### Domain glossary

- **3-boundary layering** (blueprint ADR-1) — pg-glue (Postgres/pgrx ABI: GUCs, ereport) / domain (portable business logic) / api-surface (`#[pg_extern]` + SQL wrapper). The recurring pattern across paradedb/citus/duckdb.
- **`$HERE` idiom** — `HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"`; sibling-script references via `$HERE/x.sh` survive a move iff all siblings move together.
- **Parity** — the refactor changes ZERO behavior; proven by the existing suite passing unchanged + identical generated SQL + non-regression latency.

### Architecture boundaries affected

Per `.claude/rules/architecture.md` §1–3: this slice INTRODUCES the layering boundaries inside `theodb_rs`
(pg-glue / domain / api) that were implicit in the flat `lib.rs` — strengthening SRP + module cohesion. No
dependency-direction change (domain `embed.rs` may use `pg.rs` helpers for typed errors; `lib.rs` is the
composition/api root). The repo-level move (`scripts/`) is organizational, no layering impact.

## Prior Art & Related Work

- **Internal blueprint:** `.claude/knowledge-base/discoveries/blueprints/v2-system-design-and-repo-structure-blueprint.md` (SHIPPABLE 97.8) — ADR-1 (3-boundary layering BEFORE crate split), ADR-2 (workspace deferred to 2nd crate — YAGNI), the migration ordering ("now = declutter + toolchain pin + in-crate split"), and the structure-only AGPL discipline (the blueprint's license-aware sourcing ADR).
- **Reference projects (structure-only for AGPL):** paradedb `pg_search/src/{postgres,api,query}` + `rust-toolchain.toml` (layering + toolchain-pin pattern — AGPL, structure observed, no code copied); duckdb `src/{parser,planner,execution}` (permissive layering confirmation); pgvectorscale/pg_mooncake/hydra (single-crate contrast → workspace deferral).
- **M17 baseline:** `docs/benchmarks/m17-embed-rust-vs-plpython.md` (Rust embed 13.92 ms/call — the non-regression baseline); `theodb_rs/src/lib.rs` (the code being split).

## Objective

- [ ] The 4 loose root scripts live in `scripts/` (via `git mv`, history preserved); every mechanizable reference (`.github/workflows/ci.yml`, `docs/migration/minimal-migration.md`) points at the new path.
- [ ] `theodb_rs/src/lib.rs` is split into `pg.rs` (glue) + `embed.rs` (domain) + `lib.rs` (api/module-map), behavior-preserving.
- [ ] `theodb_rs/rust-toolchain.toml` pins `channel = "1.91.0"`.
- [ ] The full 18-test suite passes UNCHANGED against the rebuilt image; `cargo pgrx schema` emits identical SQL; clippy 0 warnings.
- [ ] `docs/benchmarks/faang-restructure-slice-1-parity.md` shows no latency regression vs 13.92 ms/call (mean±std, ≥3 runs).

## ADRs

### D1 — Apply the 3-boundary layering in-crate NOW; defer the Cargo workspace (blueprint ADR-1 + ADR-2)
**Decision:** split `lib.rs` into `pg.rs` (glue) / `embed.rs` (domain) / `lib.rs` (api + module map); do NOT introduce a Cargo workspace in this slice.
**Rationale:** the blueprint's ADR-1 says the glue/domain/api split is the recurring engine-class pattern (paradedb/citus/duckdb) and costs nothing in-crate while making the future crate-split mechanical; ADR-2 says the workspace is earned at the 2nd crate (M18) — adding it now is speculative generalization (YAGNI — `.claude/rules/parsimony-ladder.md`; CLAUDE.md "Esforço ≠ Complexidade"). Honors `.claude/rules/architecture.md` §3 (module cohesion).
**Alternatives considered:** (a) keep `lib.rs` flat — rejected: the big-ball-of-mud trajectory the blueprint warns against; (b) split into crates now — rejected: YAGNI (1 crate today; pg_mooncake/hydra prove single-crate is legitimate); (c) full paradedb 6-member workspace — rejected: cargo-culting the END state.
**Consequences:** clean module homes for M18+ code; the workspace conversion is a future slice (clearly scoped out here).

### D2 — Pure refactor: parity proven by the EXISTING suite + a non-regression benchmark (no new behavior tests)
**Decision:** add NO new behavior tests; the gate is the existing 18-test suite passing UNCHANGED + identical generated SQL + a non-regression latency benchmark.
**Rationale:** this slice changes zero behavior (a refactor); per `.claude/rules/testing.md`, tests protect behavior — the behavior is already covered by the M17 suite, so the correct proof of a refactor is "the unchanged suite still passes" + "the generated SQL is identical" + "no perf regression" (measurement-first, ADR 0002). Writing new behavior tests for unchanged behavior would be theatre.
**Alternatives considered:** (a) add new unit tests per module — rejected: the behavior is unchanged + already covered; module-internal tests would assert implementation structure (anti-pattern in `testing.md` §6); (b) skip the benchmark — rejected: the CTO requires measured data + a refactor can accidentally regress (e.g., an extra allocation), so the non-regression measurement is the honest proof.
**Consequences:** the gate is "unchanged suite green + identical SQL + no regression"; if any test changes or SQL differs, the refactor broke parity and failed.

### D3 — `git mv` (preserve history); update only mechanizable references; leave audit-trail prose untouched
**Decision:** move scripts with `git mv`; update `.github/workflows/ci.yml` + `docs/migration/minimal-migration.md`; do NOT edit `knowledge-base/**`, `CHANGELOG.md`, `ROADMAP.md` historical mentions.
**Rationale:** `git mv` preserves blame/history (FAANG hygiene); CI + the user guide are LIVE references that must work; audit-trail docs are immutable records of what was true then (Rule 6 forbids editing released CHANGELOG entries; `audit-trail-rotation.md` keeps history).
**Alternatives considered:** (a) `rm`+`add` — rejected: loses history; (b) update every historical mention — rejected: rewrites audit trail, violates Rule 6 + `audit-trail-rotation.md`.
**Consequences:** live refs work post-move; history intact; old docs honestly reflect the path at their time.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| `migrate-doc-check.sh` reads the guide via a path relative to `$HERE`; after moving to `scripts/`, `$HERE` changes from repo-root to `scripts/`, possibly breaking the guide-path resolution | High | Read the exact guide-path line in T1.1; if it's `$HERE`-relative, repoint to `$HERE/../docs/migration/minimal-migration.md`; PROVE by running `bash scripts/migrate-doc-check.sh` green | maintainers |
| The module split could accidentally change generated SQL (e.g., `#[pg_schema]` placement) | High | T2.x asserts `cargo pgrx schema` output is byte-identical to the pre-split (diff the generated `theodb_rs--1.0.0.sql`); 18-test suite green | maintainers |
| A refactor could introduce a latency regression (extra allocation/indirection) | Medium | T4 benchmark re-measures vs 13.92 ms/call baseline; non-regression is a gate | maintainers |
| `docs/migration/minimal-migration.md` invocation lines feed `migrate-doc-check.sh`'s guide check; updating them could desync the doc-check | Medium | After updating the doc, run `bash scripts/migrate-doc-check.sh` to prove the guide still matches the smoke | maintainers |

## Unresolved Questions

- Q1 — RESOLVED (edge-case EC-2): `migrate-doc-check.sh:6` uses `GUIDE="$HERE/docs/migration/minimal-migration.md"` ($HERE-relative; $HERE=repo-root today). After the move, fix to `$HERE/../docs/...`. (SMOKE="$HERE/migrate-smoke.sh" is a sibling — unaffected.)
- Q2 — (none — every decision is resolved at plan time.)

## Dependencies

(none — pure structural refactor + file moves. NO new dependency is added: the module split reuses the
existing `pgrx =0.16.1` / `minreq` / `serde_json` already declared in `theodb_rs/Cargo.toml` + `Cargo.lock`;
`rust-toolchain.toml` is a toolchain pin, not a dependency. `/deps-audit` has no new declared dep to scan.)

## Dependency Graph

```
Phase 1 (declutter scripts) ──┐
Phase 3 (toolchain pin) ───────┤ (independent; can interleave)
Phase 2 (module split) ────────┴──▶ Phase 4 (Integration Validation: rebuild + 18 tests + benchmark)
```
Phases 1, 2, 3 are independent (different files); Phase 4 validates the whole. Sequential gate: Phase 4 last.

---

## Phase 1: Declutter root scripts → `scripts/`

**Objective:** move the 4 loose root scripts into `scripts/` with history preserved and every live reference repointed.

### T1.1 — `git mv` the 4 scripts + update ci.yml + the migration guide

#### Objective
Move `smoke.sh`, `migrate-smoke.sh`, `migrate-smoke-selftest.sh`, `migrate-doc-check.sh` to `scripts/`; update `.github/workflows/ci.yml` (7 invocations) + `docs/migration/minimal-migration.md` (invocation lines); confirm sibling + guide-path resolution holds.

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — `git mv` the 4 scripts to `scripts/`, repoint the CI invocations + the user-guide commands, and verify the BASH_SOURCE-relative sibling refs + the doc-check guide path still resolve.
2. **Why it is necessary now** — the blueprint's migration ordering puts "declutter scripts" first (lowest risk, immediate clarity); the root is the most visible FAANG smell. Cites blueprint Recommendations (target tree `scripts/`) + Baseline (ci.yml is the only mechanizable caller).

#### Evidence
- Loose scripts confirmed tracked (177/120/33/26 LoC). CI invocations: `ci.yml:67,159,162,165,262,323,436`. Sibling refs are `$HERE/migrate-smoke.sh` (BASH_SOURCE-relative). Guide cmds: `docs/migration/minimal-migration.md:100-102`.

#### Files to edit
```
smoke.sh → scripts/smoke.sh (git mv)
migrate-smoke.sh → scripts/migrate-smoke.sh (git mv)
migrate-smoke-selftest.sh → scripts/migrate-smoke-selftest.sh (git mv)
migrate-doc-check.sh → scripts/migrate-doc-check.sh (git mv); EC-2: repoint line 6 GUIDE="$HERE/docs/migration/minimal-migration.md" → GUIDE="$HERE/../docs/migration/minimal-migration.md" (SMOKE="$HERE/migrate-smoke.sh" stays — sibling moved too)
.github/workflows/ci.yml — repoint: bash smoke.sh → bash scripts/smoke.sh (×4); migrate-* → scripts/migrate-* (×3)
docs/migration/minimal-migration.md — update the `bash migrate-*.sh` invocation lines to scripts/
```

#### Deep file dependency analysis
- The 4 scripts (Baseline rows) move together → the `$HERE`-relative sibling refs (`migrate-doc-check.sh`→`migrate-smoke.sh`, `migrate-smoke-selftest.sh`→`migrate-smoke.sh`) keep resolving. `ci.yml` is the only mechanizable caller; `ha/*`+`packaging/*` already in subdirs (untouched). `migrate-doc-check.sh`'s GUIDE path (Q1) is the one risk — read + fix in this task.

#### Deep Dives
- **Invariant:** each script runs identically from `scripts/`. `migrate-doc-check.sh` must still find both `migrate-smoke.sh` (sibling — OK) AND the guide doc (Q1 — fix path if needed).
- **Edge case:** if the guide path was repo-root-relative (e.g., `docs/migration/...` resolved from CWD), the CI invocation `bash scripts/migrate-doc-check.sh` runs from repo root so CWD-relative still works; if it was `$HERE/docs/...` it breaks → repoint to `$HERE/../docs/...`.

#### Tasks
1. `git mv` the 4 scripts to `scripts/`.
2. EC-2: fix `migrate-doc-check.sh` line 6 → `GUIDE="$HERE/../docs/migration/minimal-migration.md"` (Q1 resolved: it WAS `$HERE/docs/...`, breaks from `scripts/`); `SMOKE` sibling stays.
3. Update `ci.yml` (7 invocations → `scripts/`).
4. Update `docs/migration/minimal-migration.md` invocation lines.

#### TDD
```
RED:    bash scripts/smoke.sh (against a running container) — must behave as before
RED:    bash scripts/migrate-doc-check.sh — must PASS (guide matches smoke from new location)
GREEN:  git mv + ref updates so both run green from scripts/
REFACTOR: None
VERIFY: bash scripts/migrate-doc-check.sh && echo OK  ; yamllint/parse ci.yml (or grep the new paths)
```

#### Concurrency tests (only when applicable)

(none — single-threaded) — shell scripts + file moves, no shared mutable state.

#### Acceptance Criteria
- [ ] 4 scripts in `scripts/`; `git log --follow scripts/smoke.sh` shows preserved history.
- [ ] `.github/workflows/ci.yml` has zero remaining `bash smoke.sh`/`bash migrate-*.sh` (root) references; all point to `scripts/`.
- [ ] `bash scripts/migrate-doc-check.sh` exits 0 (guide still matches the smoke).
- [ ] `docs/migration/minimal-migration.md` invocation lines reference `scripts/`.
- [ ] Pass: no broken root-relative reference remains (`grep -rnE 'bash (smoke|migrate-)' .github docs` → only `scripts/` paths).

#### DoD
- [ ] All tasks done; `migrate-doc-check.sh` green from `scripts/`; ci.yml valid; history preserved.

---

## Phase 2: In-crate module split (3-boundary layering)

**Objective:** split `theodb_rs/src/lib.rs` into `pg.rs` + `embed.rs` + `lib.rs` (api/map), behavior-preserving.

### T2.1 — Extract `pg.rs` (glue) + `embed.rs` (domain); reduce `lib.rs` to api + module map

#### Objective
Move `err_input`/`err_external`/`guc`/`truncate` into `pg.rs`; move the embedding logic into `embed.rs`; keep `lib.rs` as `pg_module_magic!` + `#[pg_schema] mod theodb_rs` (thin `#[pg_extern] _embed_text` calling `embed::run`) + `extension_sql!` wrapper + the 4 `#[pg_test]`. No behavior change.

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — physically relocates the glue + domain code into modules and rewires `lib.rs` to delegate, applying the blueprint's 3-boundary layering inside the single crate.
2. **Why it is necessary now** — blueprint ADR-1: this is the recurring engine-class pattern, cheap in-crate, and it makes the M18 crate-split mechanical; doing it before M18 prevents the flat-`lib.rs` ball-of-mud. Cites blueprint ADR-1 + `architecture.md` §3.

#### Evidence
- `theodb_rs/src/lib.rs:240` LoC today holds all four concerns (helpers, domain `_embed_text`, api `#[pg_schema]`/wrapper, tests). The split mirrors paradedb `pg_search/src/{postgres,api,...}` (structure-only, AGPL — license-aware per the blueprint) + duckdb `src/{...}` (permissive confirmation).

#### Files to edit
```
theodb_rs/src/pg.rs (NEW) — pub(crate) fn err_input(&str)->!, err_external(&str)->!, guc(&str)->Option<String>, truncate(&str,usize)->String (ereport + Spi glue)
theodb_rs/src/embed.rs (NEW) — pub(crate) fn run(content: Option<&str>, model: Option<&str>) -> String (the full embedding logic, using pg::{err_input,err_external,guc})
theodb_rs/src/lib.rs — keep pg_module_magic!; `mod pg; mod embed;`; #[pg_schema] mod theodb_rs { #[pg_extern] fn _embed_text(...) -> String { crate::embed::run(content, model) } }; extension_sql! wrapper UNCHANGED; #[pg_test]s UNCHANGED (call theodb_rs._embed_text)
```

#### Deep file dependency analysis
- `lib.rs` (Baseline row) shrinks to the composition/api root + tests. `pg.rs` + `embed.rs` are NEW, `pub(crate)`. The only external contract is the generated SQL (`theodb_rs._embed_text` + `theodb.embed`) — MUST be byte-identical (the `#[pg_extern]` + `#[pg_schema]` + `extension_sql!` stay in `lib.rs`, so the generated SQL is unchanged). `_embed_text` body becomes a one-line delegate to `embed::run`.

#### Deep Dives
- **Invariant (EC-1):** `cargo pgrx schema` output is SEMANTICALLY identical pre/post split — the SAME set of objects (`theodb_rs._embed_text` + `theodb.embed` + the schema + REVOKEs), compared ORDER-INSENSITIVELY. pgrx's generated file header declares "The ordering of items is not stable" — so a byte-`diff` is NOT a valid oracle; compare the sorted set of `CREATE`/`REVOKE` statements instead. The `#[pg_extern]`/`#[pg_schema]`/`extension_sql!` macros stay in `lib.rs` so the GENERATED objects are unaffected; emission order may differ harmlessly.
- **Edge case:** the `#[pg_test]`s reference `theodb_rs._embed_text` (SQL name) — unchanged. The helper visibility must be `pub(crate)` so `embed.rs` + `lib.rs` can call them. `ereport!`/`PgSqlErrorCode`/`Spi`/`minreq`/`serde_json` imports move to the module that uses them.

#### Pseudo-code / Signatures
```rust
// pg.rs
pub(crate) fn err_input(msg: &str) -> ! { /* ErrorReport 22023 .report(ERROR); unreachable!() */ }
pub(crate) fn err_external(msg: &str) -> ! { /* 38000 */ }
pub(crate) fn guc(name: &str) -> Option<String> { /* Spi current_setting */ }
pub(crate) fn truncate(s: &str, n: usize) -> String { s.chars().take(n).collect() }
// embed.rs
pub(crate) fn run(content: Option<&str>, model: Option<&str>) -> String { /* exact M17 logic, using crate::pg::* */ }
// lib.rs
mod pg; mod embed;
#[pg_schema] mod theodb_rs {
    #[pg_extern] fn _embed_text(content: Option<&str>, model: Option<&str>) -> String { crate::embed::run(content, model) }
}
extension_sql!(/* theodb.embed wrapper — UNCHANGED */);
```

#### Tasks
1. Create `pg.rs` with the 4 helpers (`pub(crate)`), moving their imports.
2. Create `embed.rs` with `run()` = the M17 `_embed_text` body, calling `crate::pg::*`.
3. Rewrite `lib.rs`: `mod pg; mod embed;`, thin `_embed_text` delegate, keep wrapper + tests.
4. `cargo build` + `cargo pgrx schema` (in builder) + `cargo clippy` — confirm compile + identical SQL + 0 warnings.

#### TDD
```
RED:    (the build + the 4 #[pg_test] + the 18-test suite are the AUTHORITATIVE behavior guard — must stay green)
RED:    cargo pgrx schema — the SORTED SET of CREATE/REVOKE statements MUST equal pre-split (order-insensitive; NOT a byte-diff — EC-1)
GREEN:  perform the split so build + schema-set-equal + clippy are clean
REFACTOR: None beyond the split itself
VERIFY: docker build --target theodb-rs-builder (compiles) ; diff <(grep -oE 'CREATE (FUNCTION|SCHEMA)|REVOKE' old|sort) <(...new|sort) ; cargo clippy 0 warnings
```

#### Concurrency tests (only when applicable)

(none — single-threaded) — pure module relocation; embed is a synchronous per-call function, unchanged.

#### Acceptance Criteria
- [ ] `theodb_rs/src/{pg.rs,embed.rs}` exist; `lib.rs` is a thin api/module-map (delegates to `embed::run`).
- [ ] `cargo build --release --features pg17` compiles (in builder).
- [ ] `cargo pgrx schema` emits the SAME SET of objects pre/post split (`theodb_rs._embed_text` + `theodb.embed` + schema + REVOKEs), compared order-insensitively (NOT byte-identical — EC-1); the 18-test suite + `\df` are the authoritative behavior oracle.
- [ ] `cargo clippy --release --features pg17` — 0 warnings.
- [ ] Pass: size — each of `pg.rs`/`embed.rs`/`lib.rs` ≤ 500 lines.

#### DoD
- [ ] Build + schema-diff + clippy clean; the 4 `#[pg_test]`s unchanged.

---

## Phase 3: Pin the Rust toolchain

**Objective:** add `theodb_rs/rust-toolchain.toml` pinning the channel to match the Dockerfile.

### T3.1 — Add `rust-toolchain.toml`

#### Objective
Pin `channel = "1.91.0"` (= Dockerfile `ARG RUST_VERSION=1.91.0`) for reproducible local + builder builds.

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — adds a `rust-toolchain.toml` so `cargo` selects the exact pinned Rust everywhere.
2. **Why it is necessary now** — the blueprint's "now" slice includes the toolchain pin (paradedb ships `rust-toolchain.toml`); it removes "works on my rustc" drift before more crates land. Cites blueprint Recommendations + Dockerfile `RUST_VERSION`.

#### Evidence
- `Dockerfile` pins `ARG RUST_VERSION=1.91.0` (the builder uses it); paradedb has a top-level `rust-toolchain.toml` (structure-only ref, AGPL — license-aware per the blueprint).

#### Files to edit
```
theodb_rs/rust-toolchain.toml (NEW) — [toolchain] channel = "1.91.0"
```

#### Deep file dependency analysis
- NEW file; affects only toolchain selection in `theodb_rs/`. The Dockerfile installs 1.91.0 explicitly, so the builder is unaffected (consistent); local `cargo` in `theodb_rs/` now selects 1.91.0.

#### Deep Dives
- **Invariant:** the channel string MUST equal the Dockerfile `RUST_VERSION` (1.91.0) — a mismatch would split local vs builder.
- **Edge case:** if rustup lacks 1.91.0 locally it auto-installs; the builder already has it (no change).

#### Tasks
1. Write `theodb_rs/rust-toolchain.toml` with `[toolchain] channel = "1.91.0"`.

#### TDD
```
RED:    (n/a — config file; the guard is that the image still builds with the pinned toolchain)
GREEN:  add the file
REFACTOR: None
VERIFY: docker build --target theodb-rs-builder still succeeds ; cat confirms channel = "1.91.0"
```

#### Concurrency tests (only when applicable)

(none — single-threaded) — a static config file.

#### Acceptance Criteria
- [ ] `theodb_rs/rust-toolchain.toml` exists with `channel = "1.91.0"` (= Dockerfile RUST_VERSION).
- [ ] The image still builds with the pin present.

#### DoD
- [ ] File present; build green.

---

## Coverage Matrix

| # | Gap / Requirement (blueprint slice-1) | Task(s) | Resolution |
|---|---|---|---|
| 1 | Declutter loose root scripts → `scripts/` (git mv + ref updates) | T1.1 | 4 scripts moved; ci.yml + guide repointed; doc-check green |
| 2 | In-crate 3-boundary layering (pg/domain/api) — blueprint ADR-1 | T2.1 | lib.rs split into pg.rs + embed.rs + api/map; identical SQL |
| 3 | Toolchain pin (reproducibility) | T3.1 | rust-toolchain.toml = 1.91.0 |
| 4 | PARITY proof (the Goal metric) — 18 tests unchanged + semantically-identical SQL | T2.1, Phase 4 | suite green vs rebuilt image (authoritative); schema object-set equal order-insensitively (EC-1) |
| 5 | BENCHMARK latency parity (CTO data requirement) | T4.1 | latency re-measured vs 13.92 ms/call baseline → report |

**Coverage: 5/5 requirements covered (100%)**

> Out of scope (NOT requirements — deferred per ADR D1 / blueprint ADR-2): the Cargo workspace (earned at the 2nd crate, M18) and new CI (CI already exists; this slice only repoints its script paths in T1.1).

## Global Definition of Done

- [ ] All phases complete.
- [ ] Full suite green UNCHANGED: `test_embed_sql.py` (10) + `test_embed_failure_scenarios.py` (3) + `test_bench_embed.py` (5) = 18 passed vs the rebuilt image.
- [ ] `cargo pgrx schema` object-set identical pre/post split (order-insensitive — EC-1, NOT byte-diff); `cargo clippy` 0 warnings; `cargo build` green.
- [ ] `bash scripts/migrate-doc-check.sh` green from new location; ci.yml repointed.
- [ ] `rust-toolchain.toml` = 1.91.0; image builds.
- [ ] CHANGELOG `[Unreleased]` updated (Changed — refactor, no behavior change).
- [ ] Backward compatibility: SQL surface (`theodb.embed`, `theodb_rs._embed_text`) byte-identical — public API unchanged.
- [ ] Benchmark report committed showing no latency regression vs 13.92 ms/call.
- [ ] File-size budget respected (≤ 500 lines per file).

## Failure scenarios (external I/O)

(none — no external I/O touched) — this slice is a pure structural refactor + file moves; the embedding HTTP
behavior is UNCHANGED and already covered by `test_embed_failure_scenarios.py` (3 tests) + `test_embed_sql.py`,
which run unchanged in Phase 4 (the parity gate). No new external dependency or call path is introduced.

## Final Phase: Integration Validation (MANDATORY)

> Runs AFTER Phases 1–3. The slice is NOT done until parity + non-regression are proven.

**Objective:** prove the refactor changed ZERO behavior + no latency regression.

### T4.1 — Latency parity benchmark (CTO data requirement)

#### Objective
Re-measure `theodb.embed` (Rust, post-split) latency against the SAME stub used in M17, ≥3 runs, mean±std, and compare to the M17 baseline (13.92 ms/call) to confirm EQUIVALENT latency. Write `docs/benchmarks/faang-restructure-slice-1-parity.md`. No perf claim (refactor — the expectation is equivalent latency). Reuses `benchmarks/bench_embed.py` (UNCHANGED — already unit-tested by `test_bench_embed.py`).

#### TDD
```
RED:    (n/a — this is a measurement task; the harness benchmarks/bench_embed.py is already covered by test_bench_embed.py, green unchanged)
GREEN:  run bench_embed against theodb.embed on theo-db:slice1; capture mean±std over ≥3 runs
VERIFY: docs/benchmarks/faang-restructure-slice-1-parity.md exists with mean±std + the equivalent-latency conclusion vs 13.92 ms/call
```

#### Concurrency tests (only when applicable)

(none — single-threaded) — the benchmark issues calls serially by design (measuring per-call latency, not concurrency).

#### Acceptance Criteria
- [ ] `docs/benchmarks/faang-restructure-slice-1-parity.md` exists with mean±std (≥3 runs) showing latency equivalent to the M17 baseline (13.92 ms/call); no perf claim.

### Execution
```
docker build -t theo-db:slice1 .                                  # builds with split modules + toolchain pin
docker run -d --add-host=host.docker.internal:host-gateway ... theo-db:slice1   # init creates theodb + theodb_rs
# Parity (the Goal metric):
python3 -m pytest benchmarks/tests/test_embed_sql.py benchmarks/tests/test_embed_failure_scenarios.py benchmarks/tests/test_bench_embed.py -v   # 18 passed, UNCHANGED
docker exec ... psql -c "\df theodb.embed" ; "\df theodb_rs._embed_text"            # identical surface
cargo pgrx schema — object-set equal pre/post (order-insensitive; NOT byte-diff — EC-1)
cargo clippy --release --features pg17                            # 0 warnings
bash scripts/migrate-doc-check.sh                                 # green from new location
# Benchmark (CTO data requirement):
# re-run bench_embed against theodb.embed (Rust) + write docs/benchmarks/faang-restructure-slice-1-parity.md (mean±std, ≥3 runs) vs 13.92ms baseline
```

### Acceptance Criteria
- [ ] 18/18 tests green vs `theo-db:slice1` (the SAME suite, UNCHANGED).
- [ ] Generated SQL object-set identical to pre-split (order-insensitive — EC-1); the 18-test suite + `\df` are the authoritative parity oracle.
- [ ] clippy 0 warnings; image builds with the toolchain pin.
- [ ] `scripts/migrate-doc-check.sh` green; ci.yml references updated.
- [ ] `docs/benchmarks/faang-restructure-slice-1-parity.md` shows no latency regression vs 13.92 ms/call (mean±std, ≥3 runs); no perf claim (it's a refactor).

### If Validation Fails
1. Separate refactor-caused failures from pre-existing.
2. Fix all refactor-caused failures before declaring complete (a refactor that changes behavior is a bug).
3. Re-run the chain.
