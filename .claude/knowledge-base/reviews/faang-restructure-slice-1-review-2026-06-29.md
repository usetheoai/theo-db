# Review — faang-restructure-slice-1

**Date:** 2026-06-29
**Slug:** faang-restructure-slice-1 (no milestone_id — off-roadmap structural refactor)
**Commits reviewed:** 4d25c29 (plan), c7c57c8 (T1.1 declutter), a56ceca (T2.1/T3.1 split + toolchain), ef3e3b4 (review polish)
**Verdict:** **READY_TO_MERGE**

## Scope

First slice of the `v2-system-design-and-repo-structure` blueprint restructuring — a PURE REFACTOR:
declutter root scripts → `scripts/`; split `theodb_rs/src/lib.rs` into the 3-boundary layering
(`pg.rs` glue / `embed.rs` domain / `lib.rs` api); pin the Rust toolchain. No behavior change.

## Specialist agents + verdicts

| Agent | Focus | Verdict |
|---|---|---|
| refactor-correctness + parity | byte-identical behavior of the split; layering direction; benchmark honesty | **REFACTOR_OK** |
| reference-integrity | script `git mv` history; CI/doc ref repointing; no broken live refs | **REFS_OK** |

## Findings + resolution

| Sev | Finding | Resolution |
|---|---|---|
| INFO | `embed.rs` doc said logic is "testable + portable" (aspirational) | **FIXED** (ef3e3b4) — reworded to "concentrates PG coupling at one boundary". |
| INFO | Plan "Current callers" still said generated SQL "byte-identical" (stale vs EC-1) | **FIXED** (ef3e3b4) — object-set semantic. |
| INFO | Benchmark improves on the plan's literal Goal (apples-to-apples interleaved vs the invalid M17-absolute comparison) | **ACCEPTED** — the executed method is more correct + transparently documented. |
| LOW | `docs/migration/minimal-migration.md:5` + notebook prose mentioned bare script names | **FIXED** migration prose (ef3e3b4); notebook runnable cell already `scripts/`; remaining notebook mentions are non-executable prose (LOW, non-breaking). |

No BLOCKER / HIGH / MEDIUM from either reviewer.

## Hard gates (BLOCKER-level) — all pass

- ✅ Tests green on the EXACT committed source: rebuilt `theo-db:slice1` from post-polish source → **18/18 passed** (`test_embed_sql.py` 10 + `test_embed_failure_scenarios.py` 3 + `test_bench_embed.py` 5). Zero source/image drift.
- ✅ No new secrets; no direct commit to `main`; no Co-Authored-By trailer; CHANGELOG `[Unreleased]` updated.

## Evidence (parity — the Goal metric)

- **Behavior parity:** `\df` surface identical (`theodb_rs._embed_text` C + `theodb.embed` SQL); the embedding logic body + all helpers + the `extension_sql!` wrapper + the 4 `#[pg_test]`s are **byte-identical** to the pre-split source (reviewer verified normalized diff empty); generated-SQL OBJECT SET identical (order-insensitive — EC-1).
- **Tests:** 18/18 green UNCHANGED vs the rebuilt image.
- **Quality:** `cargo clippy` 0 warnings; `/code-quality` PASS (100); no dead code (pg helpers used in embed.rs; embed::run called by lib.rs).
- **Refs:** `bash scripts/migrate-doc-check.sh` exit 0 from new location; 0 live root-relative refs remain; git history preserved (`git log --follow`).
- **Benchmark (T4.1):** `docs/benchmarks/faang-restructure-slice-1-parity.md` — interleaved pre-split (`theo-db:m17`, 26.15±1.29 ms) vs post-split (`theo-db:slice1`, 27.36±1.44 ms), Δ +1.21 ms **within 1σ → no regression**; honest I/O-bound framing; no perf claim. (Apples-to-apples same-machine/same-stub/interleaved — NOT vs the stale M17 absolute number.)

## Layering (blueprint ADR-1) — verified

`pg.rs` imports only `pgrx::prelude` (does NOT depend on `embed.rs` — correct direction); `embed.rs`
delegates all Postgres specifics to `crate::pg`; `lib.rs` is the api/composition root (thin `#[pg_extern]`
→ `crate::embed::run`). Strengthens SRP/module cohesion per `.claude/rules/architecture.md` §1–3. Workspace +
new CI correctly DEFERRED (blueprint ADR-2 — YAGNI; only 1 crate today).

## Verdict: READY_TO_MERGE

Pure refactor with behavior parity proven (byte-identical logic + 18 tests green on the committed source +
identical SQL surface + non-regression benchmark); both reviewers OK; all INFO/LOW addressed. Proceed to
`/release` (next version after v0.16.0 merges; this is off-roadmap — `cycle-release` skips the checkbox flip with WARN).
