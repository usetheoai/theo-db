# M98 — pgrx 0.19 upgrade + DataFusion/Arrow coexistence (the pillar GATE)

**Hardware:** Intel(R) Xeon(R) Platinum 8358 CPU @ 2.60GHz (DO c-8) · **Date:** 2026-07-14 · **Verdict:** GATE PASSED (coexistence proven by build+link+tests).

## What was proven (measurement-first, no workaround)

The single-planner columnar+AI pillar's go/no-go gate: does pgrx 0.19.0 + Apache DataFusion 54 + Arrow 58 coexist
in ONE crate, and does DataFusion execute inside a Postgres backend?

| Gate | Result |
|---|---|
| pgrx 0.16.1 → 0.19.0 upgrade (edition 2021→2024 + One-Compile) | DONE — `cargo fix --edition` migrated the mechanical breaks; the `public.vector` type moved to `TypeOrigin::External` (manual extension_sql mapping) so it stays `vector` (no REINDEX / no user-SQL change) |
| Rust MSRV | pgrx 0.19 needs rustc 1.96; bumped `rust-toolchain.toml` 1.91→1.97.0 (≥1.96 ✓) |
| **277 existing tests GREEN on pgrx 0.19** | zero regression — the byte-identical behavior oracle |
| DataFusion + Arrow linked | `datafusion v54.0.0` + `arrow-array v58` |
| **Arrow single major (coexistence proof)** | `cargo tree` shows arrow-* all v58 — NO duplicate-arrow/ABI conflict with pgrx |
| DataFusion runs IN-PROCESS | `m98_datafusion_links_in_process` — count over a 4-row Arrow batch = 4 |
| **DataFusion runs INSIDE a PG backend** | `m98_datafusion_runs_in_backend` — `SELECT theodb_df_probe()` = 3 (a DataFusion aggregate over a 3-row Arrow batch, under the `HeldInterrupts` discipline) |
| Full suite | **279 passed, 0 failed** (277 + 2 M98 smoke) |

## Honest scope

- This is the GATE (rung M-0): it proves COEXISTENCE + DataFusion-runs-in-a-backend. The full planner-integrated
  `CustomScan` executor (planner hooks, qual pushdown, batch materialization) is M100 — the smoke `theodb_df_probe`
  only de-risks it.
- Safety artifact carried from day one (blueprint Q1): the synchronous `block_on` runs under a `HeldInterrupts`
  RAII guard (a `CHECK_FOR_INTERRUPTS` mid-block_on would `proc_exit` → drop the tokio runtime → crash the backend).
- No page-format change; the `public.vector` SQL name is byte-identical (no REINDEX). NOT a performance claim —
  it is a build/link/runtime feasibility gate.
- The pillar's honest ceiling (locked): DuckDB/Photon-class 15-30× on columnar-resident data — capability-match
  AlloyDB, never superiority (M73/M97).

## Scope caveats (M98 review M3/H3)

- **Core-features coexistence only.** `datafusion = { default-features = false }` — the probe uses only
  `SessionContext::read_batch().count()` (core in-memory compute). The GATE proves LINKAGE + core execution
  coexist; M100 MUST re-verify coexistence when it enables the expression/qual-pushdown features it needs.
- **Reproducibility (B1 fix):** the `Cargo.lock` resolving pgrx 0.19 + datafusion 54 + arrow 58 (the exact graph
  the 279-GREEN run used) is committed alongside this note, so the single-arrow-major claim is reproducible from
  the repo, not only from the droplet.
