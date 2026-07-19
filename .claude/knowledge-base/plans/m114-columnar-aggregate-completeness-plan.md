---
slug: m114-columnar-aggregate-completeness
milestone_id: M114
created_at: 2026-07-19
goal: Broaden the columnar CustomScan to admit GROUP BY+WHERE combined, avg(float8), and sum(int2/int4), byte-identical to the native plan, declining the numeric-output shapes.
---

# Plan: M114 — columnar analytical aggregate completeness

## Goal

Broaden the M100 columnar `CustomScan` to admit `GROUP BY` combined with a `WHERE`, `avg(float8)`, and
`sum(int2/int4)` — each producing a result **byte-identical** to the native plan (proven by the in-PG A/B
`benchmarks/columnar_aggregate_ab.py`) while the numeric-output shapes (`avg(int*)`, `sum(int8)`, `sum(float4)`,
`avg(float4)`) DECLINE to the native plan.

## Context

The M100 CustomScan (`theodb_rs/src/am/columnar_agg.rs`) admits only `count(*)`/`sum(float8)`, and its grouped path
declines a simultaneous `WHERE` (`columnar_agg.rs:326`) and its `agg_datum` hardcodes `count→Int64`/`sum→Float64`
(`df_executor.rs`). The M114 blueprint (`knowledge-base/discoveries/blueprints/m114-columnar-aggregate-completeness-blueprint.md`)
established, from PostgreSQL docs (E1) + DataFusion 54 source (E2/E3) + the Citus pattern (E5), the exact byte-identical
strategy per aggregate shape. This slice implements the SHIP set and declines the numeric-output set honestly.

## Baseline Context (deep review of current state)

Git sha at plan time: `4491f43`.

### Files that will be touched

| File | Role today | Change |
|---|---|---|
| `theodb_rs/src/am/df_executor.rs` | `AggSpec` = {CountStar, SumFloat8}; `agg_datum` hardcodes int8/float8; `run_columnar_grouped_aggs` passes no filter | Add `SumInt`, `AvgFloat8` AggSpec variants; `agg_datum` emits the PG output type per variant (ADR-M114-2); grouped path accepts predicates + filter |
| `theodb_rs/src/am/columnar_agg.rs` | `admit` accepts only `count`/`sum(float8)`; declines GROUP BY+WHERE (`columnar_agg.rs:326`); sum arg guard is `FLOAT8OID`-only | Accept `avg(float8)` + `sum(int2/int4)`; accept GROUP BY+WHERE (reuse `extract_all_predicates`); carry the new agg kinds |
| `benchmarks/columnar_aggregate_ab.py` | (NEW) | 1M A/B: byte-identical per shipped shape + EXPLAIN decline for the declined shapes |
| `docs/benchmarks/m114-columnar-aggregate-verdict.{md,json}` | (NEW) | Measured verdict |

### Current callers / dependents

- `AggSpec` (`df_executor.rs`) — consumed by `run_aggs_on_batch`, `run_columnar_grouped_aggs`, and constructed in
  `columnar_agg.rs::begin_custom_scan` (kind→AggSpec) + `arrow_cache.rs` (heap path). New variants must be handled in
  every match.
- `agg_datum` (`df_executor.rs`) — called by `run_aggs_on_batch` (row 0) and `run_columnar_grouped_aggs` (per row).
- `admit` (`columnar_agg.rs`) — the `ParsedAgg{kind,attno}` carry: new kinds (2=sum_int, 3=avg_float8) round-trip via
  `custom_private` and are rebuilt in `begin_custom_scan`.

### Domain glossary

- **AggSpec** — the executor's aggregate descriptor (df_executor). **ParsedAgg.kind** — the plan-time int carried in
  `custom_private` (0=count, 1=sum_float8; M114 adds 2=sum_int→int8, 3=avg_float8→float8).
- **PG output type** — the type PostgreSQL's aggregate returns (E1): `sum(int2/4)→int8`, `avg(float8)→float8`.

### Architecture boundaries affected

Planner admission (`columnar_agg.rs`) + the DataFusion executor (`df_executor.rs`) — the same M100 seam. No new
module, no new dependency (parsimony rung 4).

## Prior Art & Related Work

- Internal blueprint: `knowledge-base/discoveries/blueprints/m114-columnar-aggregate-completeness-blueprint.md`
  (PG E1 / DataFusion E2-E4 / Citus E5). The ad-hoc GROUP BY + zone-map slices
  (`docs/benchmarks/columnar-groupby-verdict.md`, `docs/benchmarks/columnar-zonemap-verdict.md`) are the direct precedent.
- External: PostgreSQL aggregate function table (E1), DataFusion 54 `sum.rs`/`average.rs`/`dataframe/mod.rs`
  (E2/E3/E4), Citus `multi_logical_optimizer.c` rettype-driven casting (E5).

## Objective

Ship the byte-identical aggregate shapes (GROUP BY+WHERE, avg(float8), sum(int2/int4)); decline the numeric-output
shapes to the native plan with an ADR naming the deferred alternative.

## ADRs

### ADR-M114-1 — Ship byte-identical shapes; DECLINE numeric-output shapes to native

Ship GROUP BY+WHERE, `avg(float8)`, `sum(int2/int4)`. DECLINE `avg(int*)`, `sum(int8)`, `sum(float4)`, `avg(float4)`.
Rationale (E1/E2/E3/E5): PG `avg(int*)`/`sum(int8)`→`numeric`, but DataFusion yields lossy Float64 / overflow-prone
Int64 — a byte-identical numeric needs a Decimal128/`AnyNumeric` accumulator or the Citus sum/count decomposition,
disproportionate accidental complexity for M114. `sum(float4)`/`avg(float4)` differ from PG at the ULP (f4 vs f64
accumulation). **Alternative rejected (deferred):** Decimal128 accumulator + `AnyNumeric` datum (pgrx 0.19) — a real
follow-up, out of scope. Declining to the native plan is correct (Rule 3), not a defect: the native plan computes the
exact PG result.

### ADR-M114-2 — `agg_datum` emits the PG output type per-AggSpec variant

Stop hardcoding `count→Int64`/`sum→Float64`; each `AggSpec` variant carries its PG output type (CountStar/SumInt→int8
via `Int64Array`; SumFloat8/AvgFloat8→float8 via `Float64Array`). Mirrors Citus's rettype-driven cast (E5). **Alternative
rejected:** resolve the full `Aggref.aggtype` at exec — unnecessary generality (YAGNI); the four variants enumerate the
whole shipped output-type space.

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Integer-sum overflow (sum(int8)→numeric) shipped by mistake | HIGH | admit guards sum to INT2OID/INT4OID only; sum(int8)/sum(float4) → decline; A/B EXPLAIN asserts decline | impl |
| avg(int) shipped as lossy float | HIGH | admit guards avg to FLOAT8OID only; avg(int*)/avg(float4) → decline; EXPLAIN asserts decline | impl |
| skip×group×filter interaction drops a group | MEDIUM | reuse the admission-filter invariant (Filter is the row authority); A/B with a partial-overlap-group + WHERE shape | impl |
| New AggSpec kind mis-parsed in custom_private round-trip | MEDIUM | round-trip the kind int; A/B byte-identical is the end-to-end check | impl |

## Unresolved Questions

(none — every decision is resolved by the blueprint; the numeric-output shapes are explicitly declined with a deferred
alternative named in ADR-M114-1.)

## Failure scenarios

External engine: **DataFusion** (in-process; same seam as M100).

- **Un-pushable WHERE alongside GROUP BY:** `extract_all_predicates` returns `None` → admit declines → native plan
  applies the WHERE correctly. Test: `SELECT k, sum(x) FROM col WHERE x*2>0 GROUP BY k` (non-pushable qual) is NOT a
  CustomScan.
- **Declined aggregate type** (avg(int), sum(int8), sum(float4)): admit returns None → native plan computes the exact
  result. Test: each declined shape is NOT a CustomScan and still returns the correct value.
- **DataFusion resource exhaustion:** the existing `GreedyMemoryPool` returns a typed error (unchanged).

## Dependency Graph

Phase 1 (executor AggSpec + agg_datum) → Phase 2 (admit new shapes + GROUP BY+WHERE) → Phase 3 (A/B measurement).

## Phase 1: Executor — new AggSpec variants + per-type agg_datum + grouped filter

### T1.1 — `AggSpec::SumInt` + `AggSpec::AvgFloat8`; `agg_datum` emits PG output type per variant

#### Objective
Add `SumInt(String)` (int8 output) and `AvgFloat8(String)` (float8 output) to `AggSpec`; build their DataFusion exprs
(`sum(col)`, `avg(col)`); make `agg_datum` downcast per variant (Int64→int8 for Count/SumInt, Float64→float8 for
SumFloat8/AvgFloat8).

#### Why this step (action + reasoning)
Action: extend the executor's aggregate descriptor + result conversion. Reasoning: this is the byte-identical output
machinery for the two new numeric shapes (ADR-M114-2). `sum(int2/4)` coerces to Arrow Int64 (E2) = PG int8, and
`avg(float8)` yields Arrow Float64 (E3) = PG float8, so the downcast per variant is exact.

#### Evidence
`AggSpec` + `agg_datum` + expr builders in `theodb_rs/src/am/df_executor.rs`; blueprint E2/E3.

#### Files to edit
- `theodb_rs/src/am/df_executor.rs`

#### Deep file dependency analysis
`AggSpec` matches in `run_aggs_on_batch`, `run_columnar_grouped_aggs`, `arrow_cache.rs`, `columnar_agg.rs::begin`.
Every match arm must handle the two new variants (exhaustiveness = compiler-enforced).

#### TDD
- `test_agg_datum_sumint_emits_int8` — an `Int64Array [42]` for a `SumInt` spec → `into_datum` as int8 `(42, false)`.
- `test_agg_datum_avgfloat8_emits_float8` — a `Float64Array [2.5]` for `AvgFloat8` → float8 `(2.5, false)`.

#### Concurrency tests
(none — single-threaded)

#### Acceptance Criteria
- `SumInt`/`AvgFloat8` build the right DataFusion expr; `agg_datum` emits int8 for Count/SumInt and float8 for
  SumFloat8/AvgFloat8; a null cell → `(_, true)`.

#### DoD
- `cargo build` clean; the two unit tests pass (droplet `cargo pgrx test` OR standalone where feasible).

### T1.2 — Grouped path accepts predicates + filter (GROUP BY + WHERE)

#### Objective
Thread `predicates: &[ZonePredicate]` into `run_columnar_grouped_aggs`: project predicate columns, decode with
zone-map skip, and `df.filter(build_filter_expr(...))?.aggregate(group_exprs, agg_exprs)` (filter before aggregate).

#### Why this step (action + reasoning)
Action: give the grouped executor the same WHERE machinery the scalar path has. Reasoning: E4 confirms
`filter().aggregate()` filters before grouping (= SQL WHERE…GROUP BY); the zone-map skip is only an admission filter,
the DataFusion Filter is the final row authority — so GROUP BY+WHERE stays byte-identical.

#### Evidence
`run_columnar_grouped_aggs` + `build_filter_expr` + `decode_to_batch` predicate projection in `df_executor.rs` (E4).

#### Files to edit
- `theodb_rs/src/am/df_executor.rs`

#### Deep file dependency analysis
`run_columnar_grouped_aggs` caller = `columnar_agg.rs::begin_custom_scan` (grouped branch) — passes the parsed
predicates + skip GUC. `decode_to_batch` already projects predicate columns.

#### TDD
- `test_grouped_with_filter_shape` — grouped decode with a predicate filters rows before aggregating (asserted end-to-end
  by the Phase 3 A/B; unit asserts the filter expr is applied).

#### Concurrency tests
(none — single-threaded)

#### Acceptance Criteria
- Grouped path applies the zone-map skip + the DataFusion Filter; empty filter → unfiltered (backward compatible).

#### DoD
- `cargo build` clean.

## Phase 2: Admission — new shapes + GROUP BY+WHERE

### T2.1 — `admit` accepts avg(float8), sum(int2/int4); GROUP BY + WHERE

#### Objective
In `admit`: accept `avg` when the arg Var is `FLOAT8OID` (kind 3); accept `sum` when the arg Var is
`INT2OID`/`INT4OID` (kind 2) in addition to `FLOAT8OID` (kind 1); remove the grouped `WHERE` decline and run
`extract_all_predicates` in the grouped branch (decline if any qual is un-pushable). Keep declining `avg(int*)`,
`sum(int8)`, `sum(float4)`, `avg(float4)` (→ None → native).

#### Why this step (action + reasoning)
Action: widen the admission guard per the blueprint's SHIP set. Reasoning: the aggregate-name + arg-type checks are
where byte-identity is decided (E1-E5); declining the numeric-output shapes here keeps the native plan authoritative
for them (ADR-M114-1).

#### Evidence
`admit` agg parsing + the `FLOAT8OID`-only guard + the grouped `WHERE` decline in `columnar_agg.rs`; blueprint mismatch
matrix.

#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs`

#### Deep file dependency analysis
`ParsedAgg.kind` new values 2/3 round-trip in `custom_private` (upper_paths_hook build + begin_custom_scan parse). The
grouped branch's `preds` now comes from `extract_all_predicates` (currently `Vec::new()`), carried like the scalar
path.

#### TDD
- `test_admit_avg_float8_is_customscan` — `SELECT avg(x) FROM col` (x float8) → CustomScan.
- `test_admit_sum_int_is_customscan` — `SELECT sum(i) FROM col` (i int4) → CustomScan.
- `test_admit_avg_int_declines` — `SELECT avg(i) FROM col` (i int4) → NOT a CustomScan (numeric output).
- `test_admit_sum_int8_declines` — `SELECT sum(b) FROM col` (b int8) → NOT a CustomScan.
- `test_admit_groupby_where_is_customscan` — `SELECT k, sum(x) FROM col WHERE k>=0 GROUP BY k` → CustomScan.

#### Concurrency tests
(none — single-threaded)

#### Acceptance Criteria
- avg(float8) + sum(int2/4) + GROUP BY+WHERE admitted; avg(int*)/sum(int8)/sum(float4)/avg(float4) declined; the carry
  round-trips the new kinds.

#### DoD
- `cargo pgrx test` (droplet) green for the admit tests.

## Phase 3: Measurement (in-PG A/B)

### T3.1 — `benchmarks/columnar_aggregate_ab.py` — byte-identical per shipped shape + decline proof

#### Objective
On a 1M-row `theodb_columnar` table vs an identical heap table, assert byte-identical results for `avg(x)`,
`sum(int4_col)`, `sum(int2_col)`, and `GROUP BY k … WHERE …` (incl. a partial-overlap group + WHERE), CustomScan
engaged; and assert `avg(int)`, `sum(int8)`, `sum(float4)` are NOT CustomScans (declined) and still correct. Measure
the speedup for the shipped shapes.

#### Why this step (action + reasoning)
Action: an in-PG A/B mirroring `columnar_groupby_ab.py`. Reasoning: Rule 5 — byte-identical vs the native plan is the
correctness gate; the EXPLAIN-decline checks prove the numeric-output shapes fall back correctly (no silent wrong
result).

#### Evidence
Precedent: `benchmarks/columnar_groupby_ab.py`, `columnar_zonemap_ab.py`.

#### Files to edit
- `benchmarks/columnar_aggregate_ab.py` (NEW)
- `docs/benchmarks/m114-columnar-aggregate-verdict.{md,json}` (NEW)

#### TDD
- `test_aggregate_ab_byte_identical` — each shipped shape's result equals the heap's; each declined shape is native +
  correct.

#### Concurrency tests
(none — single-threaded)

#### Acceptance Criteria
- Every shipped shape byte-identical + CustomScan; every declined shape native + correct; speedup reported.

#### DoD
- Verdict written with measured numbers + honest decline list.

## Coverage Matrix

| Goal claim / requirement | Task(s) |
|---|---|
| avg(float8) admitted, byte-identical | T1.1, T2.1, T3.1 |
| sum(int2/int4) admitted, byte-identical (no overflow) | T1.1, T2.1, T3.1 |
| GROUP BY + WHERE combined, byte-identical | T1.2, T2.1, T3.1 |
| agg_datum emits PG output type per variant (ADR-M114-2) | T1.1 |
| DECLINE avg(int*), sum(int8), sum(float4), avg(float4) → native | T2.1, T3.1 |
| CustomScan engaged for shipped shapes; native for declined | T3.1 |

## Global Definition of Done

- `cargo build` / `cargo pgrx install` clean on the droplet.
- `benchmarks/columnar_aggregate_ab.py` reports byte-identical for every shipped shape + native for every declined
  shape, with a measured speedup.
- `docs/benchmarks/m114-columnar-aggregate-verdict.{md,json}` written with measured numbers.
- `CHANGELOG.md [Unreleased] § Added` updated (Rule 6).
- File-size budget: `columnar_agg.rs` / `df_executor.rs` stay under 700 LoC each (per `rules/architecture.md`).
