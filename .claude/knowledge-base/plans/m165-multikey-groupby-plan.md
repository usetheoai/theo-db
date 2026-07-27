---
slug: m165-multikey-groupby
milestone_id: M165
created_at: 2026-07-27
goal: Route ClickBench q34 (SELECT const + GROUP BY) and q17 (text GROUP BY under an unordered LIMIT) to the columnar CustomScan, byte-identical A/B, so both flip non-pushdown → pushdown as measured by the M164-hardened harness.
---

# M165 — GROUP BY multi-chave pushdown (q17, q34)

## Goal

Route ClickBench q34 (`SELECT 1, URL … GROUP BY 1, URL`) and q17 (`GROUP BY UserID, SearchPhrase LIMIT 10`) to the
columnar CustomScan with a byte-identical A/B (diverged=0), so both flip non-pushdown → pushdown; success metric: the
`benchmarks/run_m128_clickbench.py --agg` run reports `columnar_agg_routed` for q17 and q34 with A/B identical, and
their measured ratio vs ClickHouse drops from >100× toward the covered class.

## Context

The M165 title said "multi-key GROUP BY", but the discovery blueprint
(`.claude/knowledge-base/discoveries/blueprints/m165-multikey-groupby-blueprint.md`) proved multi-key GROUP BY **already
works** (q16, the identical int+text 2-key grouping, already routes byte-identical — `docs/benchmarks/m153-groupby-text.md:19`).
q17 and q34 decline for two *different* reasons, neither being multi-key. This plan implements the two real fixes.

## Baseline Context

### Files that will be touched

| File | LoC today | Last touch | Why it exists |
|---|---|---|---|
| `theodb_rs/src/am/columnar_agg.rs` | ~2740 | M163/M164 | admit + swap of the columnar aggregate CustomScan (`classify_target_node`, `admit`, `try_swap_agg`) |
| `theodb_rs/src/am/df_executor.rs` | ~1000 | M160 | DataFusion executor for grouped aggregates (`run_columnar_grouped_aggs`) |
| `benchmarks/columnar_type_ab.py` | ~290 | M164 | type-coverage A/B harness — gains a const-output + multi-key-NULL case |

### Current callers / dependents (real file:line)

- `classify_target_node` — `columnar_agg.rs:661`; catch-all `else` that declines q34's `T_Const` output slot at `:947-949`
  (trace `target_grouping_expression_or_other`). Called by the `admit` walk at `:1063-1087`.
- `try_swap_agg` — `columnar_agg.rs:1492`; q17 declines at `:1502-1504` (trace `swap_sorted_text_group_not_resorted`, M153).
- `run_columnar_grouped_aggs` — `df_executor.rs:610`; N-key exec `:648-708`; text-key sort skip at `:723`.
- `arrow_supported_group_type` — `df_executor.rs:258` (already accepts int4=23, text=25).

### Domain glossary

- **AGG_SORTED vs HashAgg**: PG plans a GroupAgg either over sorted input (AGG_SORTED) or via a hash table (HashAgg). The
  M153 guard only fires under AGG_SORTED with a high-card text key.
- **Byte order vs collation order**: our executor emits text groups in byte order; PG under AGG_SORTED expects the key's
  collation order. They coincide only for `C`/`POSIX` collations.
- **Const output slot**: a `SELECT <literal>` puts a `T_Const` node in the target list; it is a projected constant, not a
  group key (PG eliminates constant group keys).
- **Unordered LIMIT**: `... LIMIT k` with no `ORDER BY` returns an arbitrary k rows (SQL-legal non-determinism).

### Architecture boundaries affected

Planner/executor only (`create_upper_paths_hook` + CustomScan). NO page-format, WAL, VACUUM, crash-safety, or upgrade
surface (blueprint § Invariants). The only correctness surface is byte-identical A/B, governed by the M153/M157/M163
per-key guards + the type-coverage A/B (`rules/testing.md §5.1`).

## Prior Art & Related Work

- `.claude/knowledge-base/discoveries/blueprints/m165-multikey-groupby-blueprint.md` (this cycle's discovery).
- `docs/benchmarks/m153-groupby-text.md` (text GROUP BY routes HASHED; q17 honest-negative + WHY).
- `docs/benchmarks/m161-expr-routing-verdict.md:40-44` (q34 const honest-negative).
- DataFusion composite hash grouping + NULL-grouping gotcha (apache/datafusion#790) — blueprint § SOTA.

## ADRs

### ADR-1 — q17 decided EMPIRICALLY: relax-when-unordered before collation-sort
q17 has no `ORDER BY`, so its `LIMIT` is unordered and the A/B oracle strips the LIMIT (multiset compare). **Hypothesis:**
the M153 guard is over-conservative for the no-ORDER-BY case; relaxing it there routes q17 with a byte-identical A/B and a
SQL-legal live result. Task T2 STARTS by instrumenting the live instance (THEODB_ADMIT_TRACE + a real A/B run) to CONFIRM
the hypothesis before writing code (M152 — instrument, don't guess). **Alternative (fallback):** collation-aware executor
sort via `pg_sys::varstr_cmp(varcollid)` — substantive, council-rust-pgrx FFI review, only if the A/B proves order
matters. **Rejected:** implementing the collation sort blindly (larger, riskier, may be unnecessary).

### ADR-2 — q34 const arm is fail-closed to integer const types
The `T_Const` output arm admits only round-trippable **integer** const types (int2/4/8) initially; float/text/numeric
consts decline (byte-identity risk — same IEEE/collation reasons as M163/M156). **Alternative:** admit all const types —
rejected (float const has the IEEE display risk; text const has collation/encoding risk; YAGNI — ClickBench's only const
is the int4 `1`).

## Dependency Graph

Phase 1 (q34 const arm) and Phase 2 (q17 guard) are **independent** — different code surfaces (admit catch-all vs
try_swap_agg), no barrier. Phase 3 (benchmark validation) depends on both.

## Phase 1 — q34: fail-closed integer `T_Const` output arm

### T1.1 — admit + emit a projected integer constant
#### Why this step
The action: add a `T_Const` arm to `classify_target_node` (~`:947`) producing a new `TargetSlot::ConstOut(datum, typoid)`
+ a `layout` kind, and emit the fixed Datum per row in `run_columnar_grouped_aggs`. The reasoning: q34's sole blocker is
the unhandled const projection column (Baseline `classify_target_node:947`); PG reduces `GROUP BY 1, URL` to single-key
`GROUP BY URL`, which already routes — only the `SELECT 1` output column is unhandled. ADR-2 makes it fail-closed.

#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs` (`classify_target_node` const arm + `layout` kind + arity)
- `theodb_rs/src/am/df_executor.rs` (materialize the const slot per row)
- `benchmarks/columnar_type_ab.py` (a `SELECT <int const>, col, count(*) GROUP BY col` route case)

#### TDD
- RED (droplet A/B): `SELECT 1, URL, count(*) FROM hits GROUP BY 1, URL` — currently declines (no `Custom Scan`).
- GREEN: after the const arm, the query shows `Custom Scan (theodb_columnar_agg)` and symmetric-EXCEPT `diverged=0` vs heap.
- RED (type-coverage): a `SELECT 5, c2, count(*) GROUP BY c2` case in `columnar_type_ab.py` routes + byte-identical; a
  `SELECT 'x', …` (text const) and `SELECT 1.5, …` (float const) case DECLINE (fail-closed).

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- q34 routes with A/B byte-identical measured by `run_m128_clickbench.py --agg` (the M164-hardened harness, not a trivial diverged=0).
- `columnar_type_ab.py` gains the const-output cases (route int const, decline text/float const), all green.

## Phase 2 — q17: text-group emission-order guard (empirical)

### T2.1 — instrument, then apply the confirmed fix
#### Why this step
The action: (a) on the live instance, confirm q17's trace is `swap_sorted_text_group_not_resorted` and run the LIMIT-
stripped A/B with a relaxed guard prototype to see if it is byte-identical; (b) apply the confirmed fix — either relax
the M153 guard for the no-ORDER-BY/unordered-LIMIT case, or (fallback) implement the collation-aware sort. The reasoning:
ADR-1 — decide empirically; the no-ORDER-BY relax is simple and likely sufficient because the A/B strips the LIMIT.

#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs` (`try_swap_agg` guard at `:1502` — relax for the unordered-LIMIT case)
- `theodb_rs/src/am/df_executor.rs` (ONLY in the fallback: collation-aware sort at `:723`)

#### TDD
- RED (droplet A/B): q17 `SELECT UserID, SearchPhrase, count(*) FROM hits GROUP BY UserID, SearchPhrase` (LIMIT-stripped)
  — currently declines.
- GREEN: after the confirmed fix, q17 shows `Custom Scan (theodb_columnar_agg)` and symmetric-EXCEPT `diverged=0` vs heap.
- Guard preserved: a query WITH `ORDER BY <text key>` (order matters, non-C collation) still declines OR sorts correctly
  — the M153 guard must NOT be weakened for the ordered case.

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- q17 routes with A/B byte-identical; a text-group query WITH an ORDER BY on the text key under a non-deterministic
  collation still declines (guard intact for the case it protects).

## Phase 3 — Benchmark validation + release evidence

### T3.1 — fresh ClickBench slice + full A/B
#### Concurrency tests
(none — single-threaded)
#### Validation
- Re-run `benchmarks/run_m128_clickbench.py --n 1000000 --sample systematic --agg` on the droplet: q17 + q34 now route
  (`columnar_agg_routed` includes them), 43/43 A/B byte-identical (`diverged=0`), and their ratio vs ClickHouse drops.
- Record the delta in `docs/benchmarks/` (q17: 115× → covered-class; q34: 152× → covered-class). CHANGELOG `[Unreleased]`.

## Coverage Matrix

| Goal claim / DoD item | Task(s) |
|---|---|
| q34 (`GROUP BY 1, URL`) routes byte-identical | T1.1 |
| q17 (`GROUP BY UserID, SearchPhrase`) routes byte-identical | T2.1 |
| Fail-closed const (int route; text/float decline) | T1.1 |
| M153 guard intact for the ordered text case | T2.1 |
| Benchmark evidence + CHANGELOG | T3.1 |
| Type-coverage A/B extended (const + multi-key NULL) | T1.1 |

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| q17 relax is wrong — order DOES matter → divergence in the live query | High | ADR-1: instrument + live A/B BEFORE coding; keep the guard for the ORDER-BY case; fallback to collation-aware sort | me |
| const arm admits a non-round-trippable type → byte diff | Medium | ADR-2: fail-closed to integer const only; type-coverage A/B declines text/float const | me |
| Multi-key NULL grouping diverges (DataFusion #790 class) | Medium | A/B a multi-key NULL case; DataFusion 54 has the fix, but prove it | me |

## Unresolved Questions

- Whether q17's live `LIMIT 10` (no ORDER BY) returning a different arbitrary 10 rows than PG is acceptable to the owner
  as SQL-legal non-determinism (the A/B, LIMIT-stripped, is identical). Resolved empirically in T2.1; if not acceptable,
  the collation-aware sort fallback makes even the live order match.

## Failure scenarios

(none — no external I/O touched; the change is in-process planner/executor logic over local columnar state, no new
network/DB calls.)

## Global Definition of Done

Each criterion below is verified by the named oracle command; the DoD passes only when every one returns the asserted value.

- q34 routes: `EXPLAIN … GROUP BY 1, URL` contains `Custom Scan (theodb_columnar_agg)` AND `run_m128_clickbench.py --agg` reports `result_ab.diverged == 0` for q34 (verify: the harness JSON `queries[34].columnar_agg_routed == true`).
- q17 routes: `EXPLAIN … GROUP BY UserID, SearchPhrase` contains `Custom Scan (theodb_columnar_agg)` AND the harness reports `result_ab.diverged == 0` for q17 (verify: `queries[17].columnar_agg_routed == true`).
- Const arm fail-closed: `columnar_type_ab.py` asserts an int-const-output case returns `ok` (routed, diverged=0) AND a text-const and float-const case each return `declined` (verify: `pytest`/the harness exit 0, cases as-expected).
- M153 guard intact: a `GROUP BY <text> ORDER BY <text> COLLATE "en_US"` query returns NO `Custom Scan` in EXPLAIN (verify: `plan_routes == False` for that case).
- Benchmark delta recorded: `docs/benchmarks/m165-multikey-groupby-verdict.md` exists with q17/q34 measured ratios before→after AND CHANGELOG `[Unreleased]` has an M165 entry (verify: `grep M165 CHANGELOG.md` non-empty).
- Gates green: `run_structural.py` verdict ≥ SHIPPABLE_WITH_CAVEATS AND `/code-quality` verdict ∉ {FAIL_HARD, INVALID} AND `/review` verdict == READY_TO_MERGE.
