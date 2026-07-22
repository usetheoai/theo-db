---
slug: m98-pgrx19-datafusion-coexistence
milestone_id: M98
created_at: 2026-07-14
goal: Upgrade theodb_rs from pgrx 0.16.1 to 0.19.0 and link Apache DataFusion + Arrow into the single crate, proving coexistence by a green build — all 277 existing tests pass on 0.19.0 AND a CustomScan drives a DataFusion ExecutionPlan returning a Postgres tuple.
---

# M98 — pgrx 0.19 upgrade + DataFusion/Arrow coexistence spike (the pillar GATE)

## Context

The `single-planner-columnar-ai` blueprint (SHIPPABLE 98.8, GO-CONDITIONAL) found the sharp Q6 gate: pg_search proves
the DataFusion-CustomScan vectorized stack at **pgrx 0.19.0** (`paradedb/Cargo.toml:42`), but TheoDB is on **pgrx
0.16.1** (`theodb_rs/Cargo.toml:25`). Coexistence of datafusion-54 + arrow-58 + pgrx in ONE crate is UNPROVEN until a
build spike (the blueprint locked this as a build-spike gate — a version pin is not proof, Rule 5). M98 is the pillar's go/no-go
gate: an honest-negative (coexistence fails) re-scopes the pillar (stays on pg_duckdb) at zero code cost; a green
build unblocks M99 (columnar TAM) and M100 (the executor).

## Goal

Upgrade theodb_rs from pgrx 0.16.1 to 0.19.0 and link Apache DataFusion + Arrow into the single crate, proving
coexistence by a green build — all 277 existing tests pass on 0.19.0 AND a CustomScan drives a DataFusion
ExecutionPlan returning a Postgres tuple.

## Baseline Context

### Files that will be touched

| File | LoC today | Role in the upgrade |
|---|---|---|
| `theodb_rs/Cargo.toml` | ~90 | edition 2021→2024; `crate-type ["cdylib","lib"]`→`["cdylib"]`; remove `[[bin]] pgrx_embed_theodb_rs`; bump `pgrx =0.16.1`→`=0.19.0`; add `datafusion`/`arrow` deps |
| `theodb_rs/src/bin/pgrx_embed.rs` | 2 | DELETE (One-Compile removed pgrx_embed, pgrx 0.18) |
| `theodb_rs/src/dtype.rs` | ~330 | migrate the `SqlTranslatable for Vector` impl (`:139-145`) from method-based (`argument_sql`/`return_sql` → `SqlMapping::As`/`Returns::One`) to the const-based One-Compile API (`TYPE_IDENT`/`TYPE_ORIGIN`/`ARGUMENT_SQL`/`RETURN_SQL`) |
| `theodb_rs/src/am/customscan.rs` + others | ~26k total | edition-2024 fixes the compiler flags (static-mut `&raw` refs, `unsafe extern` blocks) — iterative |
| `theodb_rs/src/am/build_stream.rs` (NEW small) | ~40 | the smoke-test: a minimal CustomScan node that runs a DataFusion `ExecutionPlan` and returns a tuple |

### Current callers / dependents

| Symbol / artifact | Defined | Depended on by | Upgrade impact |
|---|---|---|---|
| `pgrx = "=0.16.1"` | `theodb_rs/Cargo.toml:25` | ALL of `theodb_rs/src/**` (126 `#[pg_test]`, 33 `#[pg_extern]`, 28 `#[pg_guard]`, 31 `extern "C-unwind"`, `pg_sys::*` ×hundreds) | the version bump ripples everywhere; the compiler drives the fixes |
| `pgrx_embed!()` | `src/bin/pgrx_embed.rs:2` | the `[[bin]]` + `crate-type "lib"` (Cargo.toml) | deleted (One-Compile) — no source caller |
| `SqlTranslatable for Vector` | `dtype.rs:139` | schema-gen for `public.vector` → every `::vector` cast/operator DDL + the ~44 `::vector` prod refs (M69/M70) | const migration must keep the SQL name `vector` byte-identical |
| `PREV_HOOK` static mut + other `static mut` | `customscan.rs` + AM files | the pathlist hook chain | edition-2024 requires `&raw` refs (already done in M94/M95 for `PREV_HOOK`) |
| `IndexAmRoutine`/`CustomScanMethods` FFI | `am/mod.rs`, `am/customscan.rs` | the registered AMs `theodb_ivfflat`/`theodb_hnsw` + the vecfilter node | structurally UNCHANGED (v18 guide) — recompile only |

No cross-repo callers — `theodb_rs` is the single extension crate. The 277 tests are the exhaustive dependent set that
proves the upgrade preserved behavior.

### Current state (from code read + pgrx release notes)

- `Cargo.toml:4` `edition = "2021"`, `:9` `crate-type = ["cdylib", "lib"]`, `:11-13` `[[bin]] pgrx_embed_theodb_rs`, `:25` `pgrx = "=0.16.1"`.
- `src/bin/pgrx_embed.rs:2` `::pgrx::pgrx_embed!();` — removed in the 0.18 One-Compile model.
- `dtype.rs:139-145` — the custom `public.vector` type (M69/M70) uses the OLD method-based `SqlTranslatable`; `vector` is a type THIS extension creates → `TYPE_ORIGIN::ThisExtension`.
- No `pgrx::datetime` usage (0.17 moved datetime types — not applicable).
- `#[pg_extern]`/`#[pg_guard]`/`extern "C-unwind"`/`IndexAmRoutine`/`CustomScan`/`module_pathname` are UNCHANGED across 0.17→0.19 (per the pgrx v18 migration guide) — the AM + customscan code survives structurally.
- TheoDB's Rust toolchain is `1.91.0` (`rust-toolchain.toml`) ≥ datafusion-54's MSRV 1.88 — satisfied.

### Domain glossary

- **One-Compile (pgrx 0.18)** — pgrx dropped the second `pgrx_embed` build pass; schema metadata (`SqlTranslatable`) is now compile-time consts, and the `pgrx_embed` bin + `crate-type "lib"` are removed.
- **coexistence** — datafusion + arrow + pgrx all resolving to compatible crate versions in ONE cdylib, linking + running without an arrow-version/ABI conflict — provable ONLY by `cargo build` + `cargo tree`, not by reading pins.
- **the smoke seam** — a minimal `CustomScan` that builds a trivial DataFusion `ExecutionPlan` (e.g. a one-row in-memory batch), `block_on`s its stream, and projects the Arrow row into a `TupleTableSlot` — proving the seam links end-to-end.

### Architecture boundaries affected

Build-system + `dtype.rs` (schema-gen boundary) + a new smoke node. No page format, no AM behavior change — the 277
existing tests are the byte-identical no-regression gate. The DataFusion dep is a new external boundary (contained
behind our own thin module, DIP).

## Prior Art & Related Work

- Blueprint `knowledge-base/discoveries/blueprints/single-planner-columnar-ai-blueprint.md` (Corner 2 Q6/Q7 — the version matrix + TableAmRoutine feasibility).
- pgrx release notes v0.17.0 (edition 2024, datetime move, removed `variadic!`), v0.18.0 (One-Compile + the pgrx v18.0 migration guide (`v18-0-migration`)), v0.19.0 (edition 2024 finalize) — the migration map.
- pg_search `Cargo.toml` (the proven datafusion-54/arrow-58/pgrx-0.19 version set) — [AGPL, versions only].
- TheoDB `theodb_rs/src/am/customscan.rs` (the M94/M95 CustomScan seam the smoke node extends; already uses `&raw` static-mut refs).

## ADRs

### D1 — upgrade pgrx in-place to 0.19.0 (not a parallel crate)

**Decision:** bump `theodb_rs` directly to pgrx 0.19.0 + edition 2024, fixing the migration in-place.

**Rejected alternatives:** (a) *a separate DataFusion crate at 0.19 while theodb_rs stays 0.16* — REJECTED: the whole
point is ONE crate / ONE planner (the pillar's premise); two crates reintroduce the two-engine split M100 must avoid.
(b) *stay on 0.16 and vendor DataFusion's arrow* — REJECTED: arrow-version conflicts are exactly the coexistence risk;
the honest test is the real upgrade.

### D2 — const-based SqlTranslatable for the `vector` type via `pgrx_resolved_type!`

**Decision:** migrate `dtype.rs`'s `SqlTranslatable for Vector` to the One-Compile const API, using
`pgrx_resolved_type!` for `TYPE_IDENT` and `TypeOrigin::ThisExtension` (the extension creates `public.vector`).

**Rejected alternative:** *`impl_sql_translatable!(Vector, "vector")`* — REJECTED: that macro is for EXTERNAL types
mapping to existing SQL (uuid/internal); `public.vector` is extension-owned, so `TYPE_ORIGIN::ThisExtension` +
the const form is correct (else schema-gen emits a wrong/duplicate type — the v18 guide's "type ident did not resolve").

### D3 — honest-negative is a valid terminal (measurement-first gate)

**Decision:** if the upgrade OR the datafusion/arrow coexistence cannot be made green (irreconcilable arrow-version
conflict, pgrx-0.19 incompatibility), M98 emits an honest-negative: document the blocker in the benchmark note, the
pillar re-scopes (stays on pg_duckdb), and M99-M103 pause. No workaround, no forced/partial pass.

**Rejected alternative:** *ship a partial upgrade to "keep moving"* — REJECTED (Rule 3, the goal's SEM WORKAROUNDS):
the gate is binary (builds+tests green, or honest-negative), never a partial claim.

## Dependency Graph

```
Phase 1 (pgrx 0.16→0.19 upgrade: Cargo.toml + One-Compile + edition-2024 fixes; 277 tests green) ──> Phase 2 (add datafusion+arrow; prove coexistence build + cargo tree) ──> Phase 3 (the smoke seam: CustomScan→DataFusion→tuple) ──> Phase 4 (review + release)
```

## Phase 1 — the pgrx 0.16.1 → 0.19.0 upgrade

### Task T1.1 — Cargo.toml + One-Compile removal + const SqlTranslatable + edition-2024 fixes

#### Why this step

**Action:** (a) `Cargo.toml`: `edition="2024"`, `crate-type=["cdylib"]`, remove the `[[bin]] pgrx_embed_theodb_rs`,
`pgrx="=0.19.0"` + `pgrx-tests="=0.19.0"`; (b) delete `src/bin/pgrx_embed.rs`; (c) migrate `dtype.rs`'s
`SqlTranslatable for Vector` to the const API (ADR D2); (d) fix every edition-2024 + pgrx-0.19 compile error the
build surfaces (static-mut `&raw`, `unsafe extern` blocks, any moved API), iteratively, until `cargo pgrx test pg17`
is green.

**Reasoning:** the One-Compile model (0.18) + edition 2024 (0.17/0.19) are the load-bearing breaks; `#[pg_extern]`/
`#[pg_guard]`/AM code is structurally unchanged (v18 guide), so the churn is bounded to the build config + schema-gen +
mechanical edition fixes. The 277 existing tests are the correctness oracle — the upgrade is byte-identical behavior.

#### Files to edit

- `theodb_rs/Cargo.toml`, `theodb_rs/src/bin/pgrx_embed.rs` (delete), `theodb_rs/src/dtype.rs`, + whatever files the compiler flags (iterative, edition-2024).

#### Deep file dependency analysis

`dtype.rs` is the schema-gen boundary for `public.vector` — every `::vector` cast/operator DDL depends on the type
resolving; the const migration must keep the SQL name `vector` identical (no REINDEX, no user-SQL change). The
edition-2024 fixes are mechanical and compiler-driven; `customscan.rs` already uses `&raw` refs (M94), reducing churn.

#### TDD

```
The 277 existing tests ARE the regression suite (no new unit test for the upgrade itself — the behavior is unchanged).
GATE: `cargo pgrx test pg17` on pgrx 0.19.0 → 277 passed, 0 failed (byte-identical behavior).
An edition-2024/One-Compile break that changes the vector type's SQL name would fail the existing dtype round-trip tests
(m20/m69 vector I/O tests) — those are the oracle that the migration preserved the type.
```

#### Concurrency tests

(none — a build/config migration; no new concurrency surface.)

#### Acceptance criteria

- `cargo pgrx test pg17` exits 0 with **277 passed, 0 failed** on pgrx 0.19.0 (droplet).
- `grep -c 'edition = "2024"' Cargo.toml` == 1; `src/bin/pgrx_embed.rs` does not exist; `grep -c 'crate-type = \["cdylib"\]' Cargo.toml` == 1.
- The `public.vector` type's SQL name is unchanged (the m69 vector round-trip tests pass — no REINDEX/user-SQL break).

#### DoD

- Green 277-test suite on pgrx 0.19.0; zero regression; the extension installs (`CREATE EXTENSION theodb_rs`).

## Phase 2 — DataFusion + Arrow coexistence

### Task T2.1 — add datafusion + arrow deps; prove they link + resolve cleanly

#### Why this step

**Action:** add `datafusion` (upstream `apache/datafusion`, pinned to the version that resolves with pgrx-0.19's
arrow — start at pg_search's proven `54`/arrow `58.1`, adjust if `cargo tree` shows a conflict) + `arrow` to
`Cargo.toml`; a trivial `use datafusion::prelude::*;` in a new `am/datafusion_probe.rs` forces the link; confirm
`cargo build --release` succeeds and `cargo tree` shows a SINGLE arrow major (no duplicate-arrow split with pgrx).

**Reasoning:** this is the coexistence proof the whole gate exists for (blueprint Q6). Duplicate-arrow-version
resolution or an ABI conflict only surfaces at build/link — not readable from pins (Rule 5, EC-2). Upstream datafusion,
not pg_search's `datafusion-distributed` fork (Rule 9, supply-chain).

#### Files to edit

- `theodb_rs/Cargo.toml` (deps), `theodb_rs/src/am/datafusion_probe.rs` (NEW — the link-forcing probe + a pure unit test).

#### Deep file dependency analysis

pgrx 0.19 pulls its own arrow? No — pgrx does not depend on arrow; the risk is datafusion 54 needing arrow 58 while
some transitive dep needs a different major. `cargo tree -i arrow` is the diagnostic. Contained behind
`datafusion_probe.rs` (DIP — the rest of the codebase does not import datafusion yet).

#### TDD

```
test_m98_datafusion_links (unit — a plain #[test], no pg): build a trivial DataFusion in-memory table + collect a
RecordBatch, assert one row. Proves the crate links + the async runtime + arrow work in-process (no pg needed).
GATE: `cargo tree -i arrow` shows a single arrow major; `cargo build --release` succeeds.
```

#### Concurrency tests

(none — the probe is a single-threaded in-process DataFusion collect.)

#### Failure scenarios

- **Duplicate arrow major (datafusion vs a transitive dep):** `cargo tree -i arrow` reveals it → pin/patch to unify; if irreconcilable → honest-negative (ADR D3), document the conflicting versions, pause the pillar.
- **datafusion 54 incompatible with pgrx 0.19's edition/toolchain:** the build fails loud → try the datafusion version pg_search proved, else honest-negative.

#### Acceptance criteria

- `test_m98_datafusion_links` passes (one row from a DataFusion collect).
- `cargo tree -i arrow` output shows a single arrow major version (captured in the benchmark note).

#### DoD

- The crate builds+links with datafusion+arrow; the link-probe unit test green; `cargo tree` clean.

## Phase 3 — the smoke seam

### Task T3.1 — a CustomScan node that runs a DataFusion ExecutionPlan and returns a PG tuple

#### Why this step

**Action:** add a minimal, GUC-gated (`theodb.enable_df_probe`, default off) CustomScan provider that, on exec, builds
a trivial DataFusion `ExecutionPlan` (a one-row `MemoryExec`/values plan), `block_on`s its `SendableRecordBatchStream`
with the `HeldInterrupts` discipline (blueprint Q1 safety artifact), and projects the Arrow row into the node's
`TupleTableSlot`. A pg_test runs `SELECT` through it and asserts the row.

**Reasoning:** this proves the SEAM links end-to-end inside a real PG backend — the single-planner CustomScan↔Arrow↔
DataFusion path that M100 builds on. Minimal (one row, no pushdown) — the gate is "it links + runs a tuple", not the
full executor. The `HeldInterrupts` discipline is included from day one (never-panic-across-C).

#### Files to edit

- `theodb_rs/src/am/datafusion_probe.rs` (the smoke CustomScan) + `theodb_rs/src/am/guc.rs` (the probe GUC) + `mod.rs` (wire).

#### Deep file dependency analysis

Reuses the M94/M95 CustomScan registration pattern (`customscan.rs`) — the smoke node is a second, trivial provider.
The `block_on` + `HeldInterrupts` is new (the blueprint's Q1 safety artifact); the Arrow→slot projection reuses the
copy-out discipline.

#### TDD

```
test_m98_customscan_datafusion_returns_row (pg_test): with theodb.enable_df_probe=on, a SELECT routed through the smoke
CustomScan returns the one row DataFusion produced (assert the value). Proves the seam: PG plan → CustomScan exec →
DataFusion ExecutionPlan → Arrow batch → TupleTableSlot → PG tuple, in one plan, one backend.
```

#### Concurrency tests

(none — single backend, single-thread-pinned; no DataFusion multi-partition — the blueprint's `unsafe impl Send` guard.)

#### Failure scenarios

- **`proc_exit` across the tokio runtime (interrupt mid-block_on):** the `HeldInterrupts` RAII (HOLD/RESUME_INTERRUPTS around block_on) prevents the backend crash — the blueprint's top safety artifact. Asserted by the test running under a normal query (interrupt-safe path).

#### Acceptance criteria

- `test_m98_customscan_datafusion_returns_row` passes — the DataFusion-produced row surfaces as a PG tuple through the CustomScan.

#### DoD

- The smoke seam green; the full suite (277 + 1 link probe + 1 smoke) green on pgrx 0.19.0 + datafusion/arrow.

## Phase 4: Integration Validation

- Full `cargo pgrx test pg17` GREEN (279 tests) on the droplet with pgrx 0.19.0 + datafusion + arrow linked.
- `cargo tree -i arrow` single-major (coexistence proven); `docs/benchmarks/m98-coexistence.md` records the version
  matrix + the build/link evidence + the smoke-seam result.
- Review: council-rust-pgrx (the pgrx upgrade + the FFI seam + the interrupt discipline). Findings fixed before `/release`.

## Failure scenarios

The build/link touches the external DataFusion+Arrow boundary + the pgrx toolchain.

| Failure mode | How reproduced | Expected behavior |
|---|---|---|
| Irreconcilable arrow-version conflict (datafusion vs transitive) | `cargo tree -i arrow` shows ≥2 majors, no unifying pin | honest-negative (ADR D3): document the versions, pause the pillar — NO workaround |
| `SqlTranslatable` migration breaks the `vector` type's SQL name | the m69 vector round-trip tests fail | fix the const `TYPE_IDENT`/`TYPE_ORIGIN` until the type name is byte-identical (no REINDEX) |
| interrupt/`proc_exit` across the tokio runtime | a `CHECK_FOR_INTERRUPTS` mid-block_on | `HeldInterrupts` RAII prevents the crash (Q1 discipline) |
| edition-2024 break the compiler cannot auto-fix | `cargo build` error | fix in-place (static-mut `&raw`, `unsafe extern`), iterative |

## Coverage Matrix

| # | Gap / Requirement | Task(s) | Resolution |
|---|---|---|---|
| 1 | pgrx 0.16.1→0.19.0 (edition 2024 + One-Compile) | T1.1 | Cargo.toml + delete pgrx_embed + const SqlTranslatable + edition fixes |
| 2 | Custom `vector` type survives One-Compile (no user-SQL/REINDEX break) | T1.1 | const `TYPE_ORIGIN::ThisExtension` (ADR D2); m69 tests are the oracle |
| 3 | 277 existing tests green on 0.19 (zero regression) | T1.1 | the byte-identical behavior gate |
| 4 | datafusion + arrow link + resolve in one crate (coexistence) | T2.1 | deps + link probe + `cargo tree` single arrow major |
| 5 | The single-planner seam links end-to-end (CustomScan→DataFusion→tuple) | T3.1 | the GUC-gated smoke node + pg_test |
| 6 | Interrupt/panic-across-C safety from day one | T3.1 | `HeldInterrupts` around `block_on` (Q1 artifact) |
| 7 | Honest-negative path if coexistence fails | T2.1 | ADR D3 — documented blocker, pillar pauses, no workaround |
| 8 | sign-off council-rust-pgrx | T3.1 | the council review is dispatched at integration validation (T3.1 DoD chain); findings fixed before `/release` |

**Coverage: 8/8 gaps covered (100%)**

## Drawbacks & Risks

| # | Risk | Severity | Mitigation | Owner |
|---|---|---|---|---|
| 1 | 3-major pgrx upgrade churns 26k LoC | HIGH | most churn is bounded (edition-2024 mechanical + One-Compile config); `#[pg_extern]`/AM code unchanged (v18 guide); 277 tests catch any behavior drift | impl |
| 2 | Arrow-version conflict makes coexistence impossible | HIGH | `cargo tree` diagnostic + pin/patch; honest-negative terminal if irreconcilable (ADR D3) — no workaround | impl |
| 3 | The `vector` type One-Compile migration breaks user SQL | MEDIUM | const `TYPE_ORIGIN::ThisExtension`; the m69 round-trip tests are the byte-identical oracle | impl |
| 4 | The FFI smoke seam (async runtime in C callback) crashes the backend | MEDIUM | `HeldInterrupts` discipline from the blueprint (Q1); single-thread-pinned, no multi-partition | impl |

## Unresolved Questions

- The exact datafusion version that coexists with pgrx-0.19's toolchain is resolved at T2.1 build time (start at
  pg_search's proven 54/arrow-58, adjust via `cargo tree`) — a build question, not a design one.

## Global DoD

- Full suite ≥ 279 tests, 0 failed (droplet, pgrx 0.19.0 + datafusion + arrow); the 277 pre-existing tests byte-identical.
- `cargo tree -i arrow` single major; `docs/benchmarks/m98-coexistence.md` records the version matrix + build evidence + smoke result.
- No page-format change; the `public.vector` type SQL name unchanged (no REINDEX).
- CHANGELOG `[Unreleased]` updated.

## Final Phase: Integration Validation

- The 279-test suite green on pgrx 0.19.0 with datafusion+arrow linked (the coexistence PROOF — this IS the gate).
- The smoke seam returns a DataFusion row as a PG tuple through a CustomScan in one plan.
- Review by council-rust-pgrx; honest-negative documented + pillar paused if coexistence cannot be made green.
