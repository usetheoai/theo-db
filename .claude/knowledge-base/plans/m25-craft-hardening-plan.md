---
slug: m25-craft-hardening
milestone_id: M25
created_at: 2026-07-01
goal: Close every MEDIUM/LOW craft finding of the theodb_rs architecture audit behavior-preservingly, proven by the unchanged test+benchmark suites passing at parity and measured complexity dropping below thresholds (each extracted fn CCN < 10, lib.rs < 200 LoC), all green in Docker.
---

# Plan: M25 — Craft hardening of the theodb_rs engine (behavior-preserving)

> **Version 1.0** — Close the audit's craft debt without changing observable behavior. Six mechanical fixes:
> (T2) DRY the duplicated distance mapping, (T4) name magic numbers, (T3) decompose the CCN hotspots +
> add the missing unit tests, add chat/embed parser tests, (T1) split `lib.rs` into per-feature modules.
> Every change is a move/rename/extract/const — the existing suites are the characterization oracle.

## Goal

Close every MEDIUM/LOW craft finding of the `theodb_rs` architecture audit behavior-preservingly, proven by the
unchanged test + benchmark suites passing at parity AND measured complexity dropping below thresholds (each
extracted function CCN < 10; `lib.rs` < 200 LoC), all green in Docker (single observable metric: `make`-equivalent
Docker `cargo pgrx test` + pytest benchmarks green + `docs/benchmarks/m25-craft-hardening.md` records the
before/after lizard complexity + lib.rs LoC + recall@k parity).

## Context

Driven by the architecture audit (`.claude/knowledge-base/audits/theodb_rs-architecture-verdict-2026-07-01.md`,
composite ~84/100 "Refactor Lightly") and its blueprint (`.claude/knowledge-base/discoveries/blueprints/m25-craft-hardening-blueprint.md`,
SHIPPABLE 98.8). Behavior-preserving is the hard invariant (per the blueprint contract). theodb_rs compiles ONLY via
Docker (no local PGRX_HOME).

## Baseline Context

### Files that will be touched

| File | LoC | Audit finding | Change |
|---|---|---|---|
| `theodb_rs/src/ann/mod.rs` | 246 | `Metric::dist` is `pub(super)` | widen to `pub(crate)` |
| `theodb_rs/src/sbq.rs` | 282 | `rerank_dist` dup; `knn` 12 params + `#[allow]` | delete dup; `Params` struct |
| `theodb_rs/src/nl.rs` | 356 | `nl_to_sql` CCN 19, L2 composition untested | extract `l2_validate`+`l4_validate_relations`+tests |
| `theodb_rs/src/hybrid.rs` | 176 | `run_rrf` 84 NLOC | extract `resolve_query_vector` |
| `theodb_rs/src/http.rs` | 101 | `with_timeout(30)` magic | `HTTP_TIMEOUT_SECS` const |
| `theodb_rs/src/ann/ivf.rs` | 164 | `0..10` Lloyd magic | `LLOYD_ITERS` const |
| `theodb_rs/src/chat.rs` | 242 | pure parsers untested | add `#[cfg(test)]` unit tests |
| `theodb_rs/src/embed.rs` | 174 | `format_embedding` untested | add unit tests |
| `theodb_rs/src/lib.rs` | 721 | god-file (externs + 8 DDL blocks) | split externs+`extension_sql!` into feature modules |
| `docs/benchmarks/m25-craft-hardening.md` | 0 (NEW) | — | before/after evidence |

### Current callers / dependents

- Every `#[pg_extern]` in `lib.rs` is a 1-line delegate to a feature module (`_embed_text`→`embed::run`, …). Moving the extern next to its module preserves the SQL symbol name (pgrx binds by the `#[pg_extern]` fn name / `extension_sql`), so **no caller changes** — the SQL surface (`theodb.*`) is identical.
- `sbq.rs` imports `read_corpus`/`require`/`valid_ident` from `ann_query` and `IvfflatIndex` from `ann` — unchanged. `rerank_dist` has one caller (`sbq::knn`) — swapped to `metric.dist()`.
- `Metric::dist` callers: `hnsw.rs`, `ivf.rs`, `ann_query`; widening visibility is additive (no caller breaks).

### Architecture boundaries affected

- No layer change: the pg-glue / domain / SPI-adapter layering (audit dim 1) is preserved. The lib.rs split moves the pgrx composition-root shims DOWN next to their feature modules — `lib.rs` stays the composition root (module-map + `pg_module_magic!` + `_PG_init`), matching pgvectorscale (`references/pgvectorscale/pgvectorscale/src/lib.rs:15-21`).
- 0-cycles invariant (audit dim 4) MUST be preserved — re-verify the import graph after the split.

### Domain glossary

- **Characterization test** — a test that pins CURRENT behavior so a refactor that changes it fails. The existing `#[pg_test]` + pytest suites are the oracle.
- **`#[pg_extern]` shim** — a thin Rust fn exposed as a SQL function by pgrx; position-independent across modules.
- **`extension_sql!`** — a pgrx macro embedding DDL (CREATE FUNCTION/COMMENT/REVOKE) into the extension install script; also position-independent.
- **CCN** — cyclomatic complexity (lizard/McCabe); consensus threshold ≤ 10.

## Prior Art & Related Work

- Audit (primary): `.claude/knowledge-base/audits/theodb_rs-architecture-verdict-2026-07-01.md` — measured metrics + SOTA file:line.
- Blueprint: `.claude/knowledge-base/discoveries/blueprints/m25-craft-hardening-blueprint.md`.
- SOTA module layout: `references/pgvectorscale/pgvectorscale/src/lib.rs` (47 LoC), `access_method/mod.rs` (externs+DDL in feature module); `references/paradedb` (feature packages).

## ADRs

### ADR-1 — Behavior-preserving only; characterization suites are the proof

Every change is move/rename/extract/const/visibility — never a logic change. **Alternatives:** (a) "improve
algorithm while here" — rejected: mixes refactor with feature, breaks the parity proof (per the blueprint); (b) rewrite
modules — rejected: YAGNI, the audit said "Refactor Lightly" not rewrite. The unchanged `#[pg_test]` + pytest +
recall benchmark are the oracle: they pass unchanged or the refactor is wrong.

### ADR-2 — `lib.rs` split = per-feature modules own their externs + DDL (SOTA layout)

Move each `_x` shim + its `extension_sql!` next to its feature module; `lib.rs` → module-map + `pg_module_magic!`
+ `_PG_init`. **Alternatives:** (a) an extern-registration macro/registry — rejected: over-engineering, the peers
prove plain per-module `#[pg_extern]` is idiom (per the blueprint); (b) move DDL to `.sql` files — rejected: the repo's
pattern is `extension_sql!` co-located; keep consistency. Peer-proven: pgvectorscale `lib.rs`=47 LoC.

### ADR-3 — DRY the metric mapping via `pub(crate)`, not a new trait

Widen `Metric::dist` visibility + delete `sbq::rerank_dist`. **Alternatives:** a `DistanceKernel` trait — rejected:
single mapping, 3 stable variants, YAGNI (the audit explicitly praised resisting this over-abstraction).

## Dependencies

### Existing — use as-is

| Package | Version | Ecosystem | Why |
|---|---|---|---|
| pgrx | 0.16.1 | rust | the extension framework (unchanged) |

### New — to be introduced

| Package | Version | Ecosystem | Rule 9 rationale | Why this one |
|---|---|---|---|---|
| (none) | — | — | M25 is a pure refactor — no new capability, no crate | — |

### Removed

| Package | Last version | Why removed |
|---|---|---|
| (none) | | |

## Dependency Graph

```
Phase 1 (quick wins: DRY + consts) ── least risk, no behavior surface
   ↓
Phase 2 (decompositions + new tests: nl_to_sql, run_rrf, sbq Params, chat/embed parsers)
   ↓
Phase 3 (lib.rs split — largest diff, full suite as the net)
   ↓
Final Phase (integration validation: Docker cargo pgrx test + pytest benchmark parity + lizard complexity delta)
```

## Phase 1: Quick wins (DRY + named consts)

### T1.1 — DRY the metric→kernel mapping + name the magic numbers

#### Why this step
`sbq::rerank_dist` (sbq.rs:100-106) byte-copies `Metric::dist` (ann/mod.rs:46-52) — a second source of truth for
the metric mapping (audit MEDIUM, DRY consensus). Magic numbers (`http.rs:47` timeout 30, `ivf.rs:77` Lloyd 10)
lack names (audit LOW). Both are zero-behavior-change, lowest-risk — done first.

#### Files to edit
```
theodb_rs/src/ann/mod.rs — Metric::dist pub(super) → pub(crate)
theodb_rs/src/sbq.rs — delete rerank_dist; sbq::knn calls metric.dist()
theodb_rs/src/http.rs — const HTTP_TIMEOUT_SECS: u64 = 30; use it
theodb_rs/src/ann/ivf.rs — const LLOYD_ITERS: usize = 10; use it
```

#### Deep file dependency analysis
`Metric::dist` widening is additive (existing callers unaffected). `rerank_dist` has a single caller
(`sbq::knn`) — swap to `metric.dist()`. Consts are local; no cross-module effect.

#### TDD
```
RED: the EXISTING sbq #[pg_test] (sbq_hamming_correlates_with_f32_distance) is the characterization oracle — it MUST stay green after deleting rerank_dist (proves metric.dist() gives identical results).
GREEN: widen visibility, delete rerank_dist, introduce consts.
REFACTOR: none beyond the change.
VERIFY: docker build --target theodb-rs-builder → cargo pgrx test sbq:: green; cargo clippy clean.
```

#### Concurrency tests
(none — single-threaded). No new shared state; pure visibility/const changes.

#### Acceptance Criteria
- [ ] `sbq::rerank_dist` deleted; `sbq::knn` uses `metric.dist()`; existing sbq `#[pg_test]` green (identical results).
- [ ] `HTTP_TIMEOUT_SECS` + `LLOYD_ITERS` consts introduced; no literal `30`/`10` left at those sites.
- [ ] `cargo clippy` clean, no new `#[allow]`.

#### DoD
- [ ] Docker `cargo pgrx test` green; no behavior change (characterization tests unchanged).

## Phase 2: Decompositions + missing unit tests

### T2.1 — Decompose CCN hotspots + `sbq::knn` Params + chat/embed parser tests

#### Why this step
`nl_to_sql` (CCN 19) is a security boundary whose L2 composition has no fast Rust test (audit MEDIUM); `run_rrf`
(84 NLOC) inlines query-vector resolution; `sbq::knn` carries `#[allow(too_many_arguments)]`; `chat`/`embed`
pure parsers are untested (audit MEDIUM, discipline inconsistency). Decompose + test — behavior-preserving.

#### Files to edit
```
theodb_rs/src/nl.rs — extract l2_validate(sql) + l4_validate_relations(sql, allow); #[pg_test] for L2 composition
theodb_rs/src/hybrid.rs — extract resolve_query_vector(query_text, query_vector_text)
theodb_rs/src/sbq.rs — internal knn Params struct; remove #[allow(too_many_arguments)]
theodb_rs/src/chat.rs — #[cfg(test)] unit tests for first_number/parse_batch/strip_fence
theodb_rs/src/embed.rs — #[cfg(test)] unit tests for format_embedding
```

#### Deep file dependency analysis
Extractions are internal (private fns); the public `#[pg_extern]` signatures are unchanged, so no SQL surface
change. `sbq::knn` Params is internal (the `#[pg_extern]` in lib.rs keeps its flat pgrx-mapped signature).

#### TDD
```
RED: new #[pg_test] nl_l2_rejects_multistatement (assert l2_validate("SELECT 1; DROP TABLE t") → typed error) + nl_l4_rejects_disallowed_relation — FAIL before extraction (fns don't exist).
RED: new #[test] chat_first_number_parses / chat_strip_fence / embed_format_embedding — FAIL before.
GREEN: extract the stages + add Params struct; implement/expose the pure fns for test.
REFACTOR: nl_to_sql becomes orchestration; re-measure CCN < 10.
VERIFY: docker cargo pgrx test nl:: chat:: embed:: sbq:: green; existing nl/hybrid/sbq behavior tests unchanged.
```

#### Concurrency tests
(none — single-threaded).

#### Acceptance Criteria
- [ ] `nl_to_sql`, `run_rrf` each CCN < 10 (lizard re-measure); `l2_validate`/`l4_validate_relations` unit-tested WITHOUT the Python oracle.
- [ ] `sbq::knn` uses a `Params` struct; `#[allow(too_many_arguments)]` removed; clippy clean.
- [ ] `chat`/`embed` pure parsers have `#[cfg(test)]` unit tests (parity with vec/nl/sbq).
- [ ] All EXISTING behavior tests + pytest benchmarks unchanged and green (no behavior drift).

#### DoD
- [ ] Docker `cargo pgrx test` green; lizard shows the hotspots resolved.

## Phase 3: lib.rs split

### T3.1 — Move externs + extension_sql! into feature modules; lib.rs → module-map

#### Why this step
`lib.rs` at 721 LoC is a two-responsibility append-magnet (audit MEDIUM); SOTA peers keep `lib.rs` tiny (47/192).
Largest diff — done last with the full green suite as the safety net.

#### Files to edit
```
theodb_rs/src/lib.rs — remove per-feature #[pg_extern] shims + extension_sql! blocks; keep pg_module_magic!, mod map, _PG_init, pg_test
theodb_rs/src/{embed,chat,nl,hybrid,migrate,vec,ann_query,sbq}.rs — each gains its own _x shims + its extension_sql! DDL
```

#### Deep file dependency analysis
`#[pg_extern]` and `extension_sql!` are position-independent in pgrx — moving them preserves the generated SQL
(same function names, same DDL, same `requires` ordering). The `extension_sql!` `requires`/`name` attributes are
carried verbatim so the install-script ordering is unchanged.

#### TDD
```
RED: the FULL existing suite (all #[pg_test] + pytest against the built extension) is the characterization net — after the move, the generated schema + every SQL function MUST be byte-identical in behavior.
GREEN: move the shims + DDL module by module; rebuild.
REFACTOR: lib.rs reduced to module-map + magic + init (< 200 LoC).
VERIFY: docker full image build (theo-db:m25) → pytest benchmarks/tests/ green; diff the generated schema is behavior-equivalent; re-verify 0 cycles.
```

#### Concurrency tests
(none — single-threaded).

#### Acceptance Criteria
- [ ] `lib.rs` < 200 LoC (module-map + `pg_module_magic!` + `_PG_init` + `pg_test` only); no `#[pg_extern]`/`extension_sql!` left in it.
- [ ] Every `theodb.*` / `ai.*` SQL function exists with identical signature + REVOKE (schema behavior unchanged) — proven by the unchanged pytest suite.
- [ ] 0 cycles preserved (re-verified import graph).

#### DoD
- [ ] Full Docker image builds; pytest benchmark suite green; lib.rs LoC recorded.

## Failure scenarios (external I/O)

M25 is a refactor — it introduces NO new external I/O and changes NO failure behavior. The existing external-I/O
seams (`http.rs` → embedding/chat providers; `Spi` → Postgres) keep their exact error handling; the only touch is
naming the HTTP timeout constant (same value, same behavior). The existing negative-case tests + typed-error paths
(err_input/err_external/err_unsupported → SQLSTATE 22023/38000/0A000) are preserved and remain the proof. **(none new — no external I/O behavior changed; existing failure paths preserved and covered by the unchanged suites.)**

## Coverage Matrix

| # | Goal claim / audit finding | Task(s) | Resolution |
|---|---|---|---|
| 1 | DRY `rerank_dist` (MEDIUM) | T1.1 | delete dup, `pub(crate)` `Metric::dist` |
| 2 | magic numbers named (LOW) | T1.1 | `HTTP_TIMEOUT_SECS`, `LLOYD_ITERS` |
| 3 | `nl_to_sql` CCN 19 + L2 untested (MEDIUM) | T2.1 | extract `l2_validate`/`l4_validate_relations` + Rust tests |
| 4 | `run_rrf` 84 NLOC (MEDIUM) | T2.1 | extract `resolve_query_vector` |
| 5 | `sbq::knn` 12 params + allow (LOW) | T2.1 | `Params` struct, remove `#[allow]` |
| 6 | chat/embed parsers untested (MEDIUM) | T2.1 | `#[cfg(test)]` unit tests |
| 7 | `lib.rs` 721 LoC god-file (MEDIUM) | T3.1 | split externs+DDL into feature modules |
| 8 | behavior-preserved + benchmark data | T4.1 | Docker suite green + lizard delta + recall parity |

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| `nl_to_sql` is a security boundary — a bad extract could weaken injection defense | HIGH | TDD: write the L2/L4 rejection tests FIRST; existing nl behavior tests + Python oracle stay green | paulo |
| lib.rs split could change generated SQL ordering / drop a REVOKE | MEDIUM | move `extension_sql!` verbatim with its `requires`/`name`; full pytest suite is the net; diff schema behavior | paulo |
| Docker-only build makes iteration slow; a missed clippy `#[allow]` slips | MEDIUM | gate `cargo clippy` in Docker after every task; DoD forbids new `#[allow]` | paulo |
| "refactor" accidentally changes behavior (recall drift) | MEDIUM | recall@k benchmark parity is a hard DoD; characterization suites unchanged | paulo |

## Unresolved Questions

- Should `ann_query.rs`'s generic SPI helpers (`require`/`valid_ident`/`read_corpus`) move to a `pg`/`spi_util` home (audit MEDIUM cohesion)? Resolved at plan time: **deferred** — it's a cohesion nicety, not a craft-debt gate; folding it into M25 risks scope creep. Tracked as a follow-up note, out of M25 scope.
- (none others — every M25 fix is scoped by the audit.)

## Global Definition of Done

- [ ] Docker `cargo pgrx test` (all `#[pg_test]`) green; pytest `benchmarks/tests/` green (behavior parity).
- [ ] `cargo clippy` clean with NO new `#[allow]`; the `sbq` `#[allow(too_many_arguments)]` removed.
- [ ] lizard re-measure: `nl_to_sql`/`run_rrf`/extracted fns CCN < 10; `lib.rs` < 200 LoC — recorded.
- [ ] recall@k benchmark unchanged (parity) — the measurement-first evidence.
- [ ] 0 cycles preserved.
- [ ] `docs/benchmarks/m25-craft-hardening.md` records before/after (complexity + LoC + parity); CHANGELOG updated.
- [ ] Every changed file within the 500-LoC budget (`rules/architecture.md`).

## Final Phase: Integration Validation

### T4.1 — Full Docker validation + complexity/parity benchmark

Build the full `theo-db:m25` image; run `cargo pgrx test` + `benchmarks/tests/` pytest; re-run `lizard -l rust`
for the before/after complexity delta; confirm recall@k parity vs the pre-M25 baseline; re-verify 0 cycles + clippy
clean. Record everything in `docs/benchmarks/m25-craft-hardening.md`. The milestone is NOT complete until the full
chain is green with recorded numbers.

#### Concurrency tests
(none — single-threaded).

#### Acceptance Criteria
- [ ] Docker suite + pytest green; lizard shows every targeted hotspot resolved; recall@k parity proven; lib.rs < 200 LoC.

#### DoD
- [ ] Benchmark doc records the numbers; CHANGELOG `[Unreleased]` updated.

## Cross-references

- Blueprint: `.claude/knowledge-base/discoveries/blueprints/m25-craft-hardening-blueprint.md`
- Audit: `.claude/knowledge-base/audits/theodb_rs-architecture-verdict-2026-07-01.md`
- Cycle: `.claude/rules/cycle-plan.md` → `cycle-implement.md` · Conventions: `architecture.md`, `testing.md`, `error-handling.md`, `parsimony-ladder.md`
