---
slug: numeric-output-aggregates
created_at: 2026-07-19
goal: Admit sum(int8) and avg(int2/4/8) in the columnar CustomScan, byte-identical to PostgreSQL's numeric output.
---

# Plan: numeric-output integer aggregates (sum(int8), avg(int2/4/8))

## Goal

Make the M100 columnar `CustomScan` admit `sum(int8)` and `avg(int2/4/8)` — which PostgreSQL returns as `numeric` —
producing a result **byte-identical** to the native plan (proven by `benchmarks/columnar_numeric_agg_ab.py` at
magnitudes exercising PG's data-dependent avg scale 16/8/0), completing the aggregate-type coverage M114 declined.

## Context

M114 declined `avg(int*)`/`sum(int8)` to the native plan (ADR-M114-1) because PG returns exact `numeric` while
DataFusion gives lossy Float64 / overflow-prone Int64. The numeric-output blueprint
(`knowledge-base/discoveries/blueprints/numeric-output-aggregates-blueprint.md`) established, from PG17 source + pgrx
0.19 + DataFusion 54 (empirically validated), that byte-identical IS achievable: DataFusion `sum(cast AS
Decimal128(38,0))` + `count`, then pgrx `AnyNumeric` (whose division IS PG's `numeric_div`) delegates scale to PG.

## Baseline Context (deep review of current state)

Git sha at plan time: `a15081b`.

### Files that will be touched

| File | Role today | Change |
|---|---|---|
| `theodb_rs/src/am/df_executor.rs` | `AggSpec` {CountStar, SumFloat8, SumInt, AvgFloat8}; `agg_expr` (1 expr/spec); `agg_datum` (1 col/spec) | Add `SumInt8Numeric`, `AvgIntNumeric`; a spec may emit N DataFusion columns (avg-int = sum+count); batch→datum consumes each spec's column count; Decimal128→AnyNumeric |
| `theodb_rs/src/am/columnar_agg.rs` | `admit` declines sum(int8)/avg(int*) | Admit `sum(int8)`→kind 4, `avg(int2/4/8)`→kind 5; carry + rebuild |
| `benchmarks/columnar_numeric_agg_ab.py` | (NEW) | Byte-identical A/B at scale-16/8/0 magnitudes + >i64 sum |
| `docs/benchmarks/numeric-output-aggregates-verdict.{md,json}` | (NEW) | Measured verdict |

### Current callers / dependents

- `AggSpec` matches: `agg_expr` (builds 1 expr), `agg_datum` (converts 1 col), `arrow_cache.rs` (heap path builds
  Count/SumFloat8 only), `columnar_agg.rs::begin_custom_scan` (kind→AggSpec). New variants handled in each.
- `run_aggs_on_batch` / `run_columnar_grouped_aggs` — the batch→datum loop assumes 1 col/spec; changes to a
  per-spec column cursor.

### Domain glossary

- **PG data-dependent avg scale** — `avg(int)` result scale = `select_div_scale(sum, count)` = `max(16−qweight·4,0)`,
  shrinking as the sum grows (blueprint). Reproduced by dividing two scale-0 `AnyNumeric`s (== PG `numeric_div`).
- **Multi-column AggSpec** — a spec emitting >1 DataFusion column (avg-int = `sum(cast Decimal128)` + `count`).

### Architecture boundaries affected

Planner admission (`columnar_agg.rs`) + the DataFusion executor (`df_executor.rs`) — the same M100 seam. No new module,
no new dependency (parsimony rung 4).

## Prior Art & Related Work

- Internal blueprint: `knowledge-base/discoveries/blueprints/numeric-output-aggregates-blueprint.md` (PG17 numeric.c +
  pgrx AnyNumeric + DataFusion sum.rs, empirically validated). M114 verdict
  (`docs/benchmarks/m114-columnar-aggregate-verdict.md`) named this as the deferred alternative.
- External: PG17 `numeric.c`/`pg_aggregate.dat`, pgrx 0.19 `numeric_support`, DataFusion 54 `sum.rs`; the Citus
  sum/count decomposition for avg.

## Objective

Ship byte-identical `sum(int8)` and `avg(int2/4/8)` via DataFusion Decimal128 sum + count and pgrx AnyNumeric.

## ADRs

### ADR-N1 — Decimal128 sum + count → AnyNumeric (delegate scale to PG's numeric_div)

`sum(int8)` = `AnyNumeric::from(i128 sum)`; `avg(int)` = `AnyNumeric::from(sum) / AnyNumeric::from(count)`. The pgrx
`AnyNumeric` division IS PG's `numeric_div`, so the data-dependent scale matches byte-for-byte (blueprint, empirically
validated). **Alternative rejected:** DataFusion `avg(Decimal128)` — fixed output scale ≠ PG's `select_div_scale`.

### ADR-N2 — An AggSpec may emit multiple DataFusion columns

`avg-int` decomposes to `sum(cast Decimal128(38,0))` + `count` (2 columns); the batch→datum conversion consumes each
spec's declared column count. **Alternative rejected:** single-column avg — impossible without losing PG's exact scale.

### ADR-N3 — Zero-count guard → SQL NULL

Before the AnyNumeric division, `count == 0` → emit NULL (PG's finalfns return NULL for empty groups; scale-0 zero
division raises division_by_zero).

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| AnyNumeric division scale ≠ PG avg scale | HIGH | blueprint proved they call the SAME `numeric_div`; A/B at scale-16/8/0 magnitudes | impl |
| Int64 sum silently wraps (used by mistake) | HIGH | sum via `cast AS Decimal128(38,0)` only; A/B includes a >i64 sum | impl |
| Multi-column AggSpec breaks the 1-col batch mapping | MEDIUM | per-spec column cursor; A/B byte-identical is the end-to-end check | impl |
| Empty group / all-NULL → wrong (0 vs NULL) | MEDIUM | ADR-N3 zero-count NULL guard; A/B includes an empty/all-NULL group | impl |

## Unresolved Questions

(none — the blueprint resolved the numeric semantics empirically; `sum/avg(numeric)` column input is explicitly
deferred, needing Arrow Decimal128 column decode.)

## Failure scenarios

External engine: DataFusion (in-process; same seam). Decimal128 sum overflow (>1.7e38) → matches PG's i128 boundary
(astronomically unreachable). Zero-count → NULL (ADR-N3). Un-pushable / unsupported → native plan (fail-safe).

## Dependency Graph

Phase 1 (executor: multi-column AggSpec + Decimal128 + AnyNumeric) → Phase 2 (admit) → Phase 3 (A/B).

## Phase 1: Executor — numeric AggSpecs + Decimal128 sum + AnyNumeric

### T1.1 — `AggSpec::SumInt8Numeric` + `AggSpec::AvgIntNumeric`; multi-column exprs + AnyNumeric conversion

#### Objective
Add the two variants; `agg_exprs_for(spec)` returns `sum(cast(col, Decimal128(38,0)))` (SumInt8Numeric, 1 col) and
`[sum(cast …), count(col)]` (AvgIntNumeric, 2 cols); a per-spec column cursor in the batch→datum loop; `agg_datum`
converts Decimal128→`AnyNumeric` (sum) and `AnyNumeric(sum)/AnyNumeric(count)` (avg, zero-count→NULL).

#### Why this step (action + reasoning)
Action: extend the executor's aggregate descriptor + conversion to the numeric path. Reasoning: this is the
byte-identical machinery (ADR-N1/N2/N3) — DataFusion gives the exact Decimal128 sum + count, and pgrx `AnyNumeric`
(== PG `numeric_div`) delegates the scale to PG, so no scale code is written on our side.

#### Evidence
`AggSpec`/`agg_expr`/`agg_datum` in `theodb_rs/src/am/df_executor.rs`; blueprint ADR-N1/N2/N3.

#### Files to edit
- `theodb_rs/src/am/df_executor.rs`

#### Deep file dependency analysis
`agg_expr` (1-expr) → `agg_exprs_for` (Vec). The batch→datum loops in `run_aggs_on_batch` + `run_columnar_grouped_aggs`
track a column offset per spec. `arrow_cache.rs` builds only Count/SumFloat8 (no new variants → unaffected match-wise
via exhaustiveness). `Decimal128Array` + `AnyNumeric` imports added.

#### TDD
- `test_sumint8_numeric_exact` — a `Decimal128(38,0)` array `[27670116110564327421]` (>i64) → `AnyNumeric` datum equals
  the exact numeric (proven via SPI compare in the pg_test / A/B).
- `test_avgint_zero_count_is_null` — count 0 → `(_, true)` (NULL), no division.

#### Concurrency tests
(none — single-threaded)

#### Acceptance Criteria
- SumInt8Numeric → 1 col; AvgIntNumeric → 2 cols; conversion via AnyNumeric; zero-count → NULL.

#### DoD
- `cargo build` clean; unit tests pass (droplet).

## Phase 2: Admission

### T2.1 — `admit` accepts sum(int8) → kind 4, avg(int2/4/8) → kind 5

#### Objective
In `admit`, admit `sum` when the arg Var is `INT8OID` (kind 4) and `avg` when the arg Var is
`INT2OID`/`INT4OID`/`INT8OID` (kind 5), in addition to the existing float8 cases; carry + rebuild the new kinds.

#### Why this step (action + reasoning)
Action: widen the sum/avg arg-type guards. Reasoning: the byte-identical numeric path (Phase 1) makes these shapes
admissible; the aggregate-name + arg-type check is where it's decided (blueprint).

#### Evidence
`admit` sum/avg parsing in `columnar_agg.rs` (the M114 kind 1/2/3 assignment).

#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs`

#### Deep file dependency analysis
`ParsedAgg.kind` 4/5 round-trip in `custom_private` (upper_paths_hook encode + begin_custom_scan rebuild → the new
AggSpec variants). The Agg-swap stash carries them unchanged.

#### TDD
- `test_admit_sum_int8_is_customscan` — `SELECT sum(b) FROM col` (b int8) → CustomScan.
- `test_admit_avg_int_is_customscan` — `SELECT avg(i) FROM col` (i int4) → CustomScan.

#### Concurrency tests
(none — single-threaded)

#### Acceptance Criteria
- sum(int8) + avg(int2/4/8) admitted; the carry round-trips kinds 4/5.

#### DoD
- `cargo pgrx test` green (droplet).

## Phase 3: Measurement (in-PG A/B)

### T3.1 — `benchmarks/columnar_numeric_agg_ab.py` — byte-identical at scale 16/8/0 + >i64

#### Objective
On a 1M-row `theodb_columnar` table vs a heap table, assert `sum(int8)` and `avg(int2/4/8)` byte-identical (as text,
to compare the exact numeric incl. scale) at magnitudes producing avg scale 16 (small), 8 (~1e9), 0 (near-i64), plus a
`sum(int8)` exceeding i64; scalar + GROUP BY; CustomScan engaged; and an empty/all-NULL group → NULL.

#### Why this step (action + reasoning)
Action: an in-PG A/B mirroring `columnar_aggregate_ab.py`, comparing the numeric AS TEXT (exact scale). Reasoning:
Rule 5 — byte-identical vs native is the correctness gate; the multi-magnitude cases prove PG's data-dependent scale
is matched.

#### Evidence
Precedent: `benchmarks/columnar_aggregate_ab.py`.

#### Files to edit
- `benchmarks/columnar_numeric_agg_ab.py` (NEW)
- `docs/benchmarks/numeric-output-aggregates-verdict.{md,json}` (NEW)

#### TDD
- `test_numeric_agg_ab_byte_identical` — each shape's numeric result (as text) equals the heap's; CustomScan engaged.

#### Concurrency tests
(none — single-threaded)

#### Acceptance Criteria
- sum(int8) + avg(int) byte-identical (text) at all magnitudes; CustomScan engaged; empty group → NULL; speedup reported.

#### DoD
- Verdict written with measured numbers.

## Coverage Matrix

| Goal claim / requirement | Task(s) |
|---|---|
| sum(int8) → numeric byte-identical (exact, >i64) | T1.1, T2.1, T3.1 |
| avg(int2/4/8) → numeric byte-identical (scale 16/8/0) | T1.1, T2.1, T3.1 |
| Multi-column AggSpec (avg = sum+count) — ADR-N2 | T1.1 |
| Zero-count → NULL — ADR-N3 | T1.1, T3.1 |
| CustomScan engaged for the new shapes | T2.1, T3.1 |

## Global Definition of Done

- `cargo build` / `cargo pgrx install` clean on the droplet.
- `benchmarks/columnar_numeric_agg_ab.py` reports byte-identical (as text) for sum(int8) + avg(int) at all magnitudes,
  CustomScan engaged, empty group → NULL, with a measured speedup.
- `docs/benchmarks/numeric-output-aggregates-verdict.{md,json}` written.
- `CHANGELOG.md [Unreleased] § Added` updated (Rule 6).
- File-size budget noted (`df_executor.rs` / `columnar_agg.rs` per `rules/architecture.md`).
