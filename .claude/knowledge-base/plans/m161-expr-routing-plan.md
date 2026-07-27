---
slug: m161-expr-routing
milestone_id: M161
created_at: 2026-07-27
goal: raise ClickBench columnar pushdown coverage by routing the SAFE expression classes (integer IN-list WHERE + const/integer-arithmetic/epoch-invariant-extract GROUP BY keys) to DataFusion, measured by non-pushdown queries flipping to pushdown with byte-identical A/B
---

# M161 — bounded expression-routing coverage (the safe subset)

## Goal

Flip a measured subset of the 11 M159 non-pushdown ClickBench queries to the vectorized columnar CustomScan by routing
the SAFE expression classes — integer `IN`-list WHERE predicates and `const` / integer-arithmetic / epoch-invariant
`extract` GROUP BY keys — to DataFusion, each cleared through its correctness gauntlet, with every routed query A/B
byte-identical to the row executor. Honest target: **~+3–5 queries** (NOT +11 — blockers compound, per M152).

## Context

Post-M159 deep-dive (`knowledge-base/discoveries/blueprints/columnar-improvement-deepdive-blueprint.md`, council-research-adr)
established that the 11 non-pushdown queries decline in TheoDB's routing, not DataFusion's capability, and ranked the
slices by (coverage × safety × cost). This milestone lands the top two SAFE slices; text `MIN/MAX` (M158-collation trap),
`regexp` group keys (honest-negative, RE2≠POSIX), and `HAVING` (structural) are explicitly deferred/out-of-scope.

## Baseline Context

| File | Role | Extend how |
|---|---|---|
| `theodb_rs/src/am/columnar_agg.rs` `extract_zone_predicate` (:173) / `extract_all_predicates` (:403) | WHERE → pushable predicates (only `T_OpExpr` `Var op Const` today) | add `ScalarArrayOpExpr` integer `IN`-list → a new predicate carried to the DataFusion filter |
| `columnar_agg.rs` `classify_target_node` (:520) / `GroupExprSpec`/`GroupFunc` (:456-470) / `encode_group_exprs` (:1150) | GROUP BY keys (Var + M157 date_trunc only) | accept `const`, integer `col±k`, `extract(epoch-invariant unit)`; extend `GroupFunc` enum + the 3rd `custom_private` channel |
| `theodb_rs/src/am/df_executor.rs` `build_filter_expr` (:277) / `run_columnar_grouped_aggs` (:446) | filter + grouped agg exec | emit `col.in_list(consts)`; reconstruct the new group exprs |

Ground-truth decline reasons (admit_trace, M159): q40 `unpushable_where_qual` (IN-list); q34/q35/q18 `target_grouping_expression_or_other`/`group_expr_func_unsupported`. Multi-key GROUP BY already routes (q31/32/33 pushdown). Git sha: v0.151.0.

## Prior Art & Related Work

- deep-dive blueprint (council-research-adr ranking + correctness gauntlet).
- `references/pg_clickhouse/src/deparse.c` — `foreign_expr_walker` allowlist model (const, opexpr, scalararrayop shippability + decline IN(NULL)).
- M151 (cross-type integer coercion + range check), M153 (deterministic-collation guard), M157 (date_trunc epoch-invariant unit whitelist — reused for `extract`), M158 (C/POSIX byte-order for text).

## ADRs

### ADR-1 — bounded allowlist, NOT a full `foreign_expr_walker`
**Decision:** extend the existing M157 `GroupExprSpec`/`GroupFunc` channel + the predicate extractor with a small explicit
allowlist ({IN-list-int, const, int±k, extract-epoch-invariant}); do NOT build a general expression deparser/serializer.
**Rationale / alternatives:** *full walker* (pg_clickhouse-style) — REJECTED for M161 (YAGNI + the `custom_private`
3-channel serializer only carries Integer|String leaves; a general tree needs `nodeToString`/`stringToNode`, a larger
change). Bounded allowlist reuses proven infra (M157) and is per-class gauntlet-able.

### ADR-2 — decline on possible integer overflow (int arithmetic group/agg)
**Decision:** for `col±k` group keys, PG evaluates in the column's int type and RAISES `22003` on overflow; Arrow wraps.
For the ClickBench constants (ClientIP−≤3) overflow never triggers, but to stay byte-identical route ONLY when overflow
is provably impossible (constant small relative to type range) OR use checked arithmetic mapped to PG's type; else decline.
**Alternatives:** *always route* — REJECTED (silent wrong result on overflow, a Rule 8 violation).

## Dependency Graph

Slice 1 (IN-list, independent) ∥ Slice 2 (expression GROUP BY). Both → validation. Slice 1 is the cheapest/safest → land first.

## Phase 1 — Slice 1: integer IN-list WHERE (q40)

### T1.1 — route `col IN (int-const,…)` to the DataFusion filter
#### Why this step
q40 declines on `TraficSourceID IN (-1,6)` (`unpushable_where_qual`) — the extractor only inspects `T_OpExpr`, never
`ScalarArrayOpExpr`. Integer IN = OR of exact `=` (the safest class, mirrors shipped `=`/`<>` zone predicates).
#### TDD
- RED: `t_col` with an int column; `... WHERE c IN (1,3,5)` columnar vs heap → byte-identical (symmetric EXCEPT = 0).
- RED negative: `... WHERE c IN (NULL, 1)` → declines to native plan (3-valued logic; not routed).
- GREEN: extract `ScalarArrayOpExpr` (op `=`, `useOr=true`, all-`Const` int array, no NULL) → an `InListPredicate{col, consts}`; `build_filter_expr` emits `col(name).in_list(consts, negated=false)`.
#### Files to edit
`columnar_agg.rs` (extract + encode InListPredicate over a 4th channel or the int channel), `df_executor.rs` (`build_filter_expr` → `in_list`).
#### Failure scenarios
IN with a non-Const / NULL / non-integer element → decline (fail-closed to native plan).
#### Acceptance criteria
- [ ] q40-shape (`WHERE ... IN (int,…)`) routes to the CustomScan; A/B byte-identical; NULL-in-list declines.

## Phase 2 — Slice 2: expression GROUP BY keys (const, int±k, extract-epoch-invariant) (q34, q35, q18)

### T2.1 — accept const + integer-arithmetic + epoch-invariant extract group keys
#### Why this step
q34 (`GROUP BY 1`), q35 (`GROUP BY ClientIP-1,…`), q18 (`GROUP BY extract(minute FROM EventTime)`) decline as
`target_grouping_expression_or_other`. DataFusion computes all natively; extend the M157 group-expr channel.
#### TDD
- RED (per class, columnar vs heap byte-identical): `GROUP BY 1,col`; `GROUP BY col-1`; `GROUP BY extract(minute FROM ts)`.
- RED negative: `GROUP BY extract(month FROM ts)` DECLINES (epoch-variant, the M157 trap — month diverges); `GROUP BY col+<huge>` near-overflow declines.
- GREEN: extend `GroupFunc` enum ({DateTrunc, ExtractField, IntAddConst, Const}); `classify_target_node` accepts them with the ADR-2 overflow guard + the M157 epoch-invariant unit whitelist for extract; `run_columnar_grouped_aggs` reconstructs them as DataFusion exprs.
#### Concurrency tests
(none — single-threaded planner + decode.)
#### Acceptance criteria
- [ ] const / int±k / extract(minute|hour|second|day) group keys route + A/B byte-identical; month/quarter/year extract declines; overflow-risk declines.

## Phase 3 — validation (measurement-first)

### T3.1 — coverage + A/B on ClickBench 1M
#### Why this step
DoD: measure how many of the 11 flip to pushdown + prove each A/B byte-identical + honest report of what did NOT flip.
#### TDD
- Integration: on the 1M ClickBench load, re-run admit_trace + per-query A/B (symmetric EXCEPT columnar vs heap) for q40/q34/q35/q18; assert they now show CustomScan + diverged=0. Report the overall geomean-vs-ClickHouse delta.
#### Files to edit
`docs/benchmarks/m161-expr-routing-verdict.md` (NEW).
#### Acceptance criteria
- [ ] The routed queries flip non-pushdown→pushdown, A/B diverged=0; honest note of the queries still declined (text MIN/MAX, regexp, HAVING) + realistic coverage gain (~+3-5, not +11).

## Coverage Matrix

| Goal claim | Task |
|---|---|
| Route integer IN-list WHERE | T1.1 |
| Route const/int±k/extract-epoch-invariant GROUP BY | T2.1 |
| Measure coverage gain + A/B byte-identical + honest non-flips | T3.1 |

## Drawbacks & Risks

- **Integer overflow (medium):** `col±k` must decline on possible overflow (ADR-2) — else silent wrong result. Owner: implementer. Mitigation: overflow-impossible guard + negative test.
- **Extract epoch trap (medium):** only epoch-invariant units (minute/hour/second/day) — reuse the M157 whitelist; month/quarter/year decline. Owner: implementer.
- **Compound blockers (low, honest):** flipping these classes unblocks only queries whose ONLY remaining blocker is the class — realistic ~+3-5, not +11. Owner: honest reporting in the verdict.

## Unresolved Questions

- Does q18 flip fully, or does its multi-key + extract combination hit a second blocker? Resolved at T3.1 via admit_trace (measure, don't assume).

## Global Definition of Done

- [x] `/plan-confidence` ≥ SHIPPABLE_WITH_CAVEATS (70, milestone_id M161).
- [x] Routed queries flip to pushdown + A/B byte-identical (diverged=0): q40 (IN-list), q35 (int±k), q18 (extract minute) — each EXPLAIN=Custom Scan + symmetric-EXCEPT=0. Honest non-flips documented (const-key q34 = PG const-elimination; text MIN/MAX, regexp, HAVING out of scope).
- [x] `/code-quality` ∉ {FAIL_HARD, INVALID} — verdict FAIL_SOFT, HARD=0; residual soft cap is `auditor_unavailable_cargo-udeps` (env limitation, cargo-udeps needs nightly; the compiler `dead_code` lint is the real Rust guard and is clean for M161 after removing the one dead field `base_typoid`). Council review pending.
- [ ] Released + M161 checkbox flipped.

## Final Phase: Integration Validation

- [x] ClickBench 1M A/B green for the routed queries + coverage delta measured (32→35/43, +3), committed to `docs/benchmarks/m161-expr-routing-verdict.md`.
