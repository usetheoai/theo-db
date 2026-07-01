---
slug: m25-craft-hardening
version: 1.0
owner: paulo
created_at: 2026-07-01
milestone_id: M25
sources:
  - .claude/knowledge-base/audits/theodb_rs-architecture-verdict-2026-07-01.md (the audit — primary design source)
  - references/pgvectorscale (Rust/pgrx SOTA — module organization)
  - references/paradedb (Rust/pgrx SOTA — feature-package layout)
---

# Blueprint: M25 — Craft hardening of the theodb_rs engine (behavior-preserving)

## Objective

Give `/to-plan` a citation-backed, SOTA-anchored design to close every MEDIUM/LOW craft finding of the
architecture audit **without changing observable behavior** — proven by the existing test/benchmark suites
still passing at parity + measured complexity dropping below thresholds.

## Context

The discovery for M25 is the just-completed 7-dimension architecture audit
(`.claude/knowledge-base/audits/theodb_rs-architecture-verdict-2026-07-01.md`), which is more rigorous than a
standard discover blueprint: it carries measured complexity (lizard/Rust CC), a module dependency graph
(0 cycles), and SOTA peer comparison with file:line evidence. This blueprint consolidates the refactor-relevant
findings + the one genuinely-new discovery: the target `lib.rs` organization, locked against the SOTA peers.

## Coverage Corner 1 — Integration Tests

**How to prove a refactor preserves behavior (the M25 evidence contract).** M25 changes structure, not
behavior, so the test strategy is *characterization*: the existing suites must stay green unchanged, plus new
unit tests for the newly-extracted units.
- The engine already has `#[pg_test]` suites (`ann/mod.rs` hnsw_*/ivfflat_* tests, `vec.rs`, `nl.rs`, `sbq.rs`)
  and pytest container benchmarks (`benchmarks/tests/test_ann_index.py`, `test_sbq_index.py`). These are the
  behavior oracle — they MUST pass unchanged after the refactor.
- SOTA pattern for the pg_test harness: pgvectorscale keeps the `pg_test` module at the crate root
  (`references/pgvectorscale/pgvectorscale/src/lib.rs:30-46`) — TheoDB already follows this.
- **New tests M25 must add** (the audit's gap): fast Rust `#[pg_test]` for the extracted `l2_validate` /
  `l4_validate_relations` (nl.rs) asserting the L2 composition rejects `"SELECT 1; DROP TABLE t"` and a
  disallowed relation WITHOUT the Python oracle; and unit tests for the pure `chat`/`embed` parsers
  (`first_number`, `parse_batch`, `strip_fence`, `format_embedding`) — parity with the vec/nl/sbq discipline.

## Coverage Corner 2 — Dependencies

**Zero new dependencies.** M25 is a pure refactor (Rule 9 / YAGNI): no crate added, no version bump. Every fix
is a code move, a visibility widening (`pub(super)` → `pub(crate)` for `Metric::dist`), a function extraction,
or a named const. `Cargo.toml` is untouched. Confirmed against the audit — no fix requires a new capability.

## Coverage Corner 3 — Tools

**Measurement + gate tooling (all already available, no new tool).**
- **Complexity delta (before/after):** `lizard -l rust theodb_rs/src` — the audit's baseline is `nl_to_sql`
  CCN 19, `knn@ann_query` CCN 15, `run_rrf` 84 NLOC, `lib.rs` 721 LoC. M25's numeric DoD is re-running lizard
  and showing each extracted function < CCN 10 and `lib.rs` < ~200 LoC.
- **Lint gate:** `cargo clippy` (in Docker `--target theodb-rs-builder`) — must be clean with NO new `#[allow]`
  (M25 explicitly REMOVES the `sbq.rs` `#[allow(too_many_arguments)]`).
- **Build/test substrate:** theodb_rs compiles only via Docker (no local PGRX_HOME). `docker build --target
  theodb-rs-builder` for compile/clippy/`cargo pgrx test`; full `theo-db:m25` image for pytest benchmarks.
- **Cycle check:** the audit's 0-cycles result must be preserved (re-verify the import graph after the split).

## Coverage Corner 4 — Techniques

**T1 — `lib.rs` split: externs + DDL move to feature modules (SOTA-proven).** The audit flagged `lib.rs` at
721 LoC (extern shims + 8 `extension_sql!` DDL blocks). The SOTA peer proves the target shape:
- pgvectorscale `lib.rs` = **47 LoC**: only `pgrx::pg_module_magic!()`, `pub mod` declarations, `_PG_init`
  (`references/pgvectorscale/pgvectorscale/src/lib.rs:15-21`), and the `pg_test` module. **No `#[pg_extern]`,
  no `extension_sql!` in lib.rs.**
- `#[pg_extern]` lives in feature modules (`references/pgvectorscale/pgvectorscale/src/access_method/mod.rs`),
  and `extension_sql!` lives next to its feature (same file). `#[pg_extern]` works in ANY module in pgrx.
- **TheoDB target:** each feature module (`embed`, `chat`, `nl`, `hybrid`, `migrate`, `vec`, `ann_query`, `sbq`)
  owns its own `_x` extern shims + its `extension_sql!` DDL; `lib.rs` becomes a thin module-map + `pg_module_magic!`
  + `_PG_init` wiring. Behavior-preserving: `#[pg_extern]`/`extension_sql!` are position-independent.

**T2 — DRY via visibility widening (zero-risk).** `sbq::rerank_dist` (sbq.rs:100-106) is a byte-for-byte copy
of `Metric::dist` (ann/mod.rs:46-52). `Metric::dist` is `pub(super)`; widen to `pub(crate)` and delete the
copy — `sbq` (crate-root sibling) can then call `metric.dist()`. Single source of truth for the metric→kernel
mapping. No behavior change (identical math).

**T3 — Function decomposition (extract-stage, keep behavior).** `nl_to_sql` (CCN 19) is linear guard-clause
stages: extract `l2_validate(sql)` (L65-88) + `l4_validate_relations(sql, allow)` (L90-121); `nl_to_sql`
becomes orchestration < CCN 10, each stage independently unit-testable (security boundary). `run_rrf` (84 NLOC):
extract `resolve_query_vector(query_text, query_vector_text)` (L82-108). `sbq::knn` (12 params): adopt the
sibling's `Params` struct (ann_query.rs:19-27 precedent) and delete the `#[allow(too_many_arguments)]`.

**T4 — Magic numbers → named consts.** `http.rs:47` `with_timeout(30)` → `HTTP_TIMEOUT_SECS`; `ivf.rs:77`
`0..10` Lloyd iterations → `LLOYD_ITERS`. Matches the existing `MAX_RETRIES`/`M_MAX`/`EF_MAX` const discipline.

## ADRs

### D1 — Discovery is the architecture audit, not a fresh ralph-loop

M25's prior art is the audit (measured metrics + SOTA file:line). Re-deriving it via a discover-execute
ralph-loop would be the exact re-work the milestone forbids. Alternative rejected: full discover-execute — it
would reproduce, at cost, evidence that already exists and is more rigorous. This blueprint formalizes the
discover artifact by consolidating the audit + the one new bit (SOTA lib.rs organization).

### D2 — Behavior-preserving is the hard invariant; characterization tests are the proof

Every M25 change is a move/rename/extract/const — never a logic change. Alternative rejected: "improve while
we're here" (change algorithm behavior during the refactor) — forbidden (mixes refactor with feature, breaks
the parity proof). The existing suites are the characterization oracle; they pass unchanged or the refactor is wrong.

### D3 — `lib.rs` split follows the SOTA module-per-feature layout, not a new abstraction

Move externs+DDL to feature modules (pgvectorscale pattern), NOT introduce a macro/registry/framework to
"manage" externs. Alternative rejected: an extern-registration abstraction — YAGNI/over-engineering; the peers
prove plain per-module `#[pg_extern]` is the idiom.

## Recommendations

1. **Order the work least-risk-first:** T2 (rerank_dist DRY, 3 lines) → T4 (consts) → T3 (decompositions + their
   new tests) → chat/embed parser tests → T1 (lib.rs split, largest diff, do last with the full suite as the net).
2. **Gate every step in Docker:** `cargo clippy` clean + `cargo pgrx test` green after each task; full pytest
   benchmark parity + lizard complexity re-measure at the end (the measurement-first evidence).
3. **Evidence doc:** `docs/benchmarks/m25-craft-hardening.md` records before/after complexity (lizard) + lib.rs
   LoC + suite-green + benchmark parity (recall@k unchanged) — the "data + validation in benchmark" the goal demands.

## Cross-cutting comparison

| Fix | Audit finding | SOTA anchor | Risk |
|---|---|---|---|
| lib.rs split | 721 LoC append-magnet | pgvectorscale lib.rs=47, externs in feature modules | LOW (position-independent) |
| rerank_dist DRY | duplicated Metric::dist | single-source (Rust idiom) | NONE (identical math) |
| nl_to_sql decompose | CCN 19, no L2 unit test | guard-clause extraction | LOW (security boundary — test first) |
| sbq Params | 12 params + allow | ann_query Params precedent | NONE |
| magic consts | http 30, Lloyd 10 | existing const discipline | NONE |
| chat/embed tests | pure parsers untested | vec/nl/sbq test discipline | NONE (additive) |

## Cross-references

- Audit (primary source): `.claude/knowledge-base/audits/theodb_rs-architecture-verdict-2026-07-01.md`
- Roadmap milestone: `ROADMAP.md` § M25
- Cycle: `.claude/rules/cycle-plan.md` → `cycle-implement.md` · Conventions: `architecture.md`, `testing.md`, `parsimony-ladder.md`
