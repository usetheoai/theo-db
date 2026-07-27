---
slug: m166-string-agg
milestone_id: M166
created_at: 2026-07-27
goal: Route ClickBench q29 (SUM(int2_col ± const) wide list) to the columnar CustomScan byte-identical, so it flips non-pushdown → pushdown as measured by the M164-hardened harness; q21/q22 (MIN text) documented as correct collation honest-negatives, q27 (AVG(length)) deferred.
---

# M166 — string aggregates + wide SUM(expr)

## Goal

Route ClickBench q29 (`SELECT SUM(ResolutionWidth), SUM(ResolutionWidth+1), … FROM hits`) to the columnar-agg CustomScan
with a byte-identical A/B (diverged=0) — the wide list of `SUM(int2_col ± int_const)`; success metric: the
`run_m128_clickbench.py --agg` run reports `columnar_agg_routed` for q29 with A/B identical, ratio vs ClickHouse drops
from 567× toward the covered class.

## Context

Discovery (`m166-string-agg-blueprint.md`, council-index-storage + web SOTA) ranked the four M166 queries: **q29 is the
one clean, safe win** — `SUM(int2_col ± const)` is provably overflow-free (`ResolutionWidth` is SMALLINT), reusing the
M161 `IntAddConst` machinery with no new abstraction. **q21/q22 (`MIN(text)`) are correct honest-negatives** — DataFusion
computes byte-minimum, PG computes collation-minimum; a deterministic collation constrains equality not order, so routing
gives an A/B-visible wrong result (safe only under C/POSIX, which ClickBench's default-collation columns are not).
**q27 (`AVG(length(URL))`) is deferred** — routable only under a UTF-8 encoding gate + a new scalar-func-in-agg mechanism.

## Baseline Context

### Files that will be touched

| File | LoC today | Last touch | Why it exists |
|---|---|---|---|
| `theodb_rs/src/am/columnar_agg.rs` | ~2790 | M165 | admit/swap of the columnar aggregate CustomScan (`parse_agg_kind`, agg-arg classification) |
| `theodb_rs/src/am/df_executor.rs` | ~1050 | M165 | DataFusion executor (`AggSpec`, `push_agg_exprs`, `agg_datum`) |
| `benchmarks/columnar_type_ab.py` | ~300 | M165 | type-coverage A/B — gains a `SUM(int2±const)` route case + int4/int8 decline cases |

### Current callers / dependents (real file:line)

- Agg-arg shape check declines a non-`Var` SUM arg at `columnar_agg.rs:935` (trace `agg_over_expression`).
- `parse_agg_kind` (`:627`) maps `(name, vartype)`→kind; `SumInt` → Arrow Int64 → PG int8.
- GROUP BY `IntAddConst` precedent (the exact int2±const arithmetic, gated + delta hi/lo encoded): `columnar_agg.rs:785-860`.
- `AggSpec` variants + `push_agg_exprs` + `agg_datum`: `df_executor.rs:266-289` / `:318-334` / `:837-909`.

### Domain glossary

- **int2±const class**: `SUM(smallint_col + int_const)` — int4 result, per-row overflow impossible (32767 + const « 2³¹), Int64 sum exact.
- **Per-row 22003**: PG raises numeric overflow evaluating `col+k` per row before summing; only reachable for int4-col, so int4-col declines.
- **byte-min vs collation-min**: DataFusion `MIN(Utf8)` = memcmp minimum; PG `min(text)` = collation minimum (q21/q22 divergence).

### Architecture boundaries affected

Read-path admit routing only (`create_upper_paths_hook` + CustomScan). NO page-format / WAL / VACUUM / crash-safety /
upgrade surface. The only correctness surface is byte-identical A/B (a wrong SUM/MIN is A/B-visible), governed by the
fail-closed int2-class gate + the type-coverage A/B (`rules/testing.md §5.1`).

## Prior Art & Related Work

- `.claude/knowledge-base/discoveries/blueprints/m166-string-agg-blueprint.md` (this cycle's discovery).
- M161 `IntAddConst` (the reused int2±const arithmetic + overflow gate).
- M165 q17 honest-negative (the same collation-order class as q21/q22).
- [PostgreSQL collation docs](https://www.postgresql.org/docs/current/collation.html) — min(text) is collation-ordered.

## ADRs

### ADR-1 — q29 fail-closed to the int2±const class
Admit `SUM(Var(int2) ± Const(int))` with int4 result only; decline int4-col (per-row 22003 reachable), int8 result,
non-additive ops. **Alternative rejected:** routing `SUM(int4_col + const)` with a zone-map-max guard — larger, and the
per-row overflow is a real wrong-result-or-missing-error risk; YAGNI (ClickBench's q29 is int2). Reuses M161's proven
`IntAddConst` gate + delta encoding — no new arithmetic mechanism.

### ADR-2 — q21/q22 honest-negative, q27 deferred
`MIN(text)` routes wrong under any non-C collation (byte-min ≠ collation-min); the executor already supports text min/max,
but a C/POSIX-only admit gate would decline ClickBench's default-collation columns (YAGNI to implement). `AVG(length())`
needs a new scalar-func-in-agg mechanism + a UTF-8 encoding gate — deferred as a separate capability. **Alternative
rejected:** routing `MIN(text)` under the existing deterministic-collation gate — WRONG (deterministic constrains
equality, not order; council-index-storage). No wrong result shipped (mandate).

## Dependency Graph

Single substantive task (q29). Phase 2 (benchmark) depends on Phase 1. q21/q22/q27 are documentation-only (no code).

## Phase 1 — q29: SUM(int2±const) aggregate-argument routing

### T1.1 — admit + execute SUM over an int2±const OpExpr arg
#### Why this step
The action: mirror the M161 `IntAddConst` recognition (`columnar_agg.rs:785-860`) into the agg-arg path (`:934`), adding
`AggSpec::SumIntAddConst{col,delta}` that `push_agg_exprs` emits as `sum(cast(col→Int64)+lit(delta))`, decoded via the
existing `SumInt`→Int64→int8 path. The reasoning: q29 declines only because the SUM arg is an OpExpr (`:935`); the exact
safe arithmetic already exists for GROUP BY — relocate it, fail-closed to int2-col/int4-result (ADR-1).

#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs` (agg-arg classification + `AggSpec` encode)
- `theodb_rs/src/am/df_executor.rs` (`AggSpec` variant + `push_agg_exprs` + decode)
- `benchmarks/columnar_type_ab.py` (a `SUM(c2+1)` route case + `SUM(c4+1)`/int8-result decline cases)

#### TDD
- RED (droplet A/B): `SELECT sum(c2+1), sum(c2-2) FROM hits` — currently declines (no Custom Scan).
- GREEN: after the change, `EXPLAIN` shows `Custom Scan (theodb_columnar_agg)` and symmetric-EXCEPT `diverged=0` vs heap.
- RED (type-coverage): `SUM(int2±const)` routes byte-identical; `SUM(int4_col±const)` and an int8-result case DECLINE (fail-closed).

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- q29 routes: `EXPLAIN` of q29 contains `Custom Scan (theodb_columnar_agg)` AND `run_m128_clickbench.py --agg` reports `queries[29].columnar_agg_routed == true` with `result_ab.diverged == 0`.
- `columnar_type_ab.py` gains the SUM-expr cases (int2±const routes diverged=0; int4-col / int8-result decline); harness exit 0.

## Phase 2 — Benchmark validation + honest-negative record

### T2.1 — fresh ClickBench slice + document q21/q22/q27
#### Concurrency tests
(none — single-threaded)
#### Validation
- `run_m128_clickbench.py --agg`: q29 routes, 43/43 A/B byte-identical, its ratio vs ClickHouse drops (567× → covered class); no regression.
- `docs/benchmarks/m166-*-verdict.md` records the q29 delta + the q21/q22 collation honest-negative + q27 deferral. CHANGELOG `[Unreleased]`.

## Coverage Matrix

| Goal claim / DoD item | Task(s) |
|---|---|
| q29 (`SUM(int2±const)`) routes byte-identical | T1.1 |
| Fail-closed (int4-col / int8-result decline) | T1.1 |
| q21/q22 documented as collation honest-negatives | T2.1 (ADR-2) |
| q27 documented as deferred | T2.1 (ADR-2) |
| Benchmark evidence + CHANGELOG | T2.1 |
| Type-coverage A/B extended | T1.1 |

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| int4-col SUM(col+const) admitted by mistake → per-row 22003 lost → wrong result | High | ADR-1 fail-closed gate on int2-col + int4-result; type-coverage `SUM(c4+1)` decline case proves it | me |
| delta hi/lo round-trip bug for negative const (`SUM(col-2)`) | Medium | reuse the proven M161 IntAddConst delta encoding verbatim; type-coverage includes a `col-const` case | me |
| q21/q22 pressure to route under deterministic collation → wrong MIN | High | ADR-2: honest-negative; the gate would be C/POSIX-only and wouldn't fire on ClickBench anyway | me |

## Unresolved Questions

- Whether to pursue q27 (highest leverage, 817×) within M166 or as a follow-up — deferred by ADR-2 (new scalar-func-in-agg
  mechanism + UTF-8 gate); resolved as "follow-up" unless the mechanism proves trivial during T1.1.

## Failure scenarios

(none — no external I/O touched; in-process planner/executor logic over local columnar state.)

## Global Definition of Done

Each criterion verified by the named oracle:
- q29 routes: `EXPLAIN` of q29 has `Custom Scan (theodb_columnar_agg)` AND the harness JSON `queries[29].columnar_agg_routed == true` with `result_ab.diverged == 0`.
- Fail-closed: `columnar_type_ab.py` asserts `SUM(int2±const)` routes (diverged=0) AND `SUM(int4_col+const)` + an int8-result case each `declined` (harness exit 0).
- No regression: `run_m128_clickbench.py --agg` reports 43/43 `result_ab.diverged == 0`.
- Honest-negatives recorded: `docs/benchmarks/m166-*-verdict.md` states q21/q22 collation + q27 deferral; `grep M166 CHANGELOG.md` non-empty.
- Gates: `run_structural.py` ≥ SHIPPABLE_WITH_CAVEATS; `/code-quality` ∉ {FAIL_HARD, INVALID}; `/review` (council-rust-pgrx) READY_TO_MERGE.
