---
slug: columnar-groupby-pushdown
created_at: 2026-07-19
goal: Enable a GROUP BY aggregate over a theodb_columnar table to run through the DataFusion vectorized CustomScan, byte-identical to the native plan and measurably faster.
---

# Plan: Columnar GROUP BY pushdown (multi-key, scalar + temporal)

## Goal

Route a `SELECT <keys>, count(*)/sum(float8) FROM <theodb_columnar> GROUP BY <keys>` aggregate through the M100
DataFusion `CustomScan`, emitting a result **byte-identical** to the native plan while the `CustomScan` is engaged
(EXPLAIN) and the vectorized path is **measurably faster** on a 1M-row table — verified by the in-PG A/B
`benchmarks/columnar_groupby_ab.py`.

## Context

The columnar pillar's M100 `CustomScan` (`theodb_rs/src/am/columnar_agg.rs`) currently admits only a **scalar**
aggregate (no GROUP BY — `columnar_agg.rs:208`) and emits exactly **one** result row (`columnar_agg.rs:487-505`,
`st.done` after a single virtual tuple). The zone-map slices (2026-07-18/19) added WHERE predicate pushdown. GROUP BY
is the remaining large columnar capability: it turns the columnar aggregate from a single global scalar into a real
analytical `GROUP BY key, agg` — the canonical OLAP query. This slice adds it, scoped (owner decision 2026-07-19) to
**multi-key grouping over the scalar + temporal types `build_arrow` already supports**, with the existing
`count(*)` / `sum(float8)` aggregates, and **without** a simultaneous WHERE (GROUP BY + WHERE combined is a later
slice — decline to the native plan when both are present, keeping the two axes orthogonal).

## Baseline Context (deep review of current state)

Git sha at plan time: `83303d9`.

### Files that will be touched

| File | LoC | Role today | Change |
|---|---|---|---|
| `theodb_rs/src/am/df_executor.rs` | 384 | Vectorized executor; `run_aggs_on_batch` (`df_executor.rs:229`) does `.aggregate(vec![], exprs)` → one row `Vec<(Datum,bool)>`; `build_arrow` (`df_executor.rs:45`) maps OID→Arrow. | Add a grouped path: `.aggregate(group_exprs, agg_exprs)` → `Vec<Vec<(Datum,bool)>>`; add `arrow_value_to_datum` (reverse of `build_arrow`) for group keys. |
| `theodb_rs/src/am/columnar_agg.rs` | 560 | M100 planner CustomScan; `admit` (`columnar_agg.rs:202`) declines any `groupClause` (`columnar_agg.rs:208`); `exec_custom_scan` (`columnar_agg.rs:487`) emits one row. | Admit a `groupClause` (no WHERE); parse group Vars; carry group layout in `custom_private`; multi-row `exec`. |
| `benchmarks/columnar_groupby_ab.py` | (NEW) | — | 1M-row A/B: grouped result byte-identical (columnar vs heap) + measured speedup + EXPLAIN CustomScan check. |
| `docs/benchmarks/columnar-groupby-verdict.{md,json}` | (NEW) | — | Measured verdict. |

### Current callers / dependents

- `run_aggs_on_batch` (`df_executor.rs:229`) — called by `run_columnar_aggs` (`df_executor.rs:213`) and
  `arrow_cache::run_cache_aggs` (heap M101 path). **Changing its return type would ripple to arrow_cache** → ADR-1
  keeps `run_aggs_on_batch` returning the one-row shape and adds a SEPARATE grouped entry point.
- `run_columnar_aggs` (`df_executor.rs:213`) — called by `columnar_agg.rs:475` (`begin_custom_scan`).
- `ColumnarAggState.result` (`columnar_agg.rs:58`) — read only by `exec_custom_scan` / `end_custom_scan`.

### Domain glossary

- **chunk group** — 10 000-row pruning/decoding granule (`CHUNK_GROUP_ROWS`).
- **group_exprs / agg_exprs** — DataFusion `DataFrame::aggregate(group_exprs, aggr_exprs)` args; the output batch
  columns are ordered `[group_exprs…, aggr_exprs…]`.
- **output layout** — the permutation mapping each PG output-target slot → its source batch column (a group key or an
  agg), because PG's target order (e.g. `SELECT sum(x), key`) may differ from DataFusion's `[keys…, aggs…]`.
- **reverse conversion** — Arrow scalar → PG `Datum` for a group-key column (the inverse of `build_arrow`).

### Architecture boundaries affected

- Planner hook + CustomScan exec (`columnar_agg.rs`) and the DataFusion executor (`df_executor.rs`) — the same M100
  seam already crossed. No new module, no new dependency (parsimony rung 4: DataFusion already provides hash
  aggregate). `rules/architecture.md` composition-root discipline unchanged.

## Prior Art & Related Work

- Internal: the M100 CustomScan (`columnar_agg.rs`), the zone-map slices (`docs/benchmarks/columnar-zonemap-verdict.md`,
  `docs/benchmarks/columnar-zonemap-temporal-verdict.md`) — `build_arrow` + `build_filter_expr` are the type-dispatch
  precedent this slice mirrors (reverse direction).
- External: DataFusion `DataFrame::aggregate` (Apache-2.0, the adopted engine — Rule 9; no reimplementation of hash
  aggregate). PostgreSQL CustomScan multi-row emission is the standard `ExecStoreVirtualTuple` loop.

## Objective

Add grouped aggregation to the columnar CustomScan with byte-identical correctness and a measured speedup, reusing
`build_arrow`'s type coverage for both the decoded columns and (reversed) the group-key datums.

## ADRs

### ADR-1 — Separate grouped entry point; do NOT change `run_aggs_on_batch`'s return type

`run_aggs_on_batch` (`df_executor.rs:229`) is shared with the M101 heap path (`arrow_cache::run_cache_aggs`). Adding
a new `run_columnar_grouped_aggs` returning `Vec<Vec<(Datum,bool)>>` keeps the scalar path (and arrow_cache)
untouched. **Alternative rejected:** widen `run_aggs_on_batch` to always return `Vec<Vec<…>>` — ripples into
arrow_cache with no benefit (heap path has no GROUP BY in this slice) and churns a proven path (DRY/YAGNI).

### ADR-2 — Carry an explicit output layout in `custom_private`, do not assume column order

DataFusion emits `[group_cols…, agg_cols…]`; PG's output target (`reltarget->exprs`) may interleave (`SELECT sum(x),
key`). Carry, per output slot, a `(src_kind, src_idx)` pair (0=group→batch col `src_idx`; 1=agg→batch col
`ngroup+src_idx`) so `exec` fills each slot from the correct batch column. **Alternative rejected:** assume PG target
== DataFusion order — silently wrong for any query that lists an aggregate before a key (a correctness bug, not a
perf bug).

### ADR-3 — Build group-key datums in the executor per-query memory context

`sum`/`count` return by-value Datums (int8/float8), but a **text/varlena group key** datum is palloc'd by-reference and
must survive across `exec` calls until emitted. Switch to `estate->es_query_cxt` while materializing the result rows
so every group-key datum lives for the whole scan. **Alternative rejected:** materialize into a transient context —
the varlena datum would dangle after the context resets (use-after-free). By-value-only key types would dodge this but
would drop text/temporal keys from scope (the owner chose the full type set).

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Column-order mapping wrong → aggregates/keys swapped in output | HIGH | ADR-2 explicit layout carry; TDD asserts `SELECT sum(x), key … GROUP BY key` (agg-before-key) byte-identical | impl |
| Text/varlena group-key datum dangles after context reset | HIGH | ADR-3 build in `es_query_cxt`; TDD includes a `text` group key over multiple emitted rows | impl |
| Reverse Arrow→Datum conversion wrong for a type (sign, tz, bytes) | MEDIUM | `arrow_value_to_datum` mirrors `build_arrow` exactly; TDD covers int/float/bool/text/timestamptz/date keys; A/B byte-identical vs heap | impl |
| GROUP BY + WHERE both present routed to a path that ignores one | MEDIUM | admit declines when `groupClause` AND `baserestrictinfo` both present → native plan (correct); TDD asserts the combined query is NOT a CustomScan | impl |
| NULL group key mis-grouped (Arrow null vs PG NULL) | MEDIUM | emit `is_null=true` for a null group cell; TDD includes a column with NULLs producing a NULL group; A/B byte-identical | impl |

## Unresolved Questions

(none — every decision is resolved at plan time; GROUP BY+WHERE, grouping expressions like `date_trunc`, and
`avg`/`sum(int)` are explicitly out of scope and decline to the native plan.)

## Failure scenarios

External engine touched: **DataFusion** (in-process vectorized aggregate; the same seam as M100).

- **Unsupported group-key type** (e.g. `numeric`, an array, a grouping expression `date_trunc(...)`): `admit` returns
  `None` → native plan runs the GROUP BY correctly (unpruned, un-vectorized). Test: `SELECT key, sum(x) … GROUP BY
  key` on a `numeric` key is NOT a CustomScan and still returns the right result.
- **DataFusion resource exhaustion** (too many groups > `work_mem`): the existing `GreedyMemoryPool`
  (`df_executor.rs:257`) returns a typed `ResourcesExhausted` → clean SQL error (not OOM). Test: covered by the
  existing memory-pool discipline; the grouped path reuses the same pool.
- **Empty table / all-filtered**: zero result rows → `exec` returns `ExecClearTuple` on the first call (cursor at
  end). Test: GROUP BY over an empty columnar table returns zero rows, byte-identical to heap.

## Dependency Graph

Phase 1 (executor grouped path) → Phase 2 (planner admit + multi-row exec) → Phase 3 (A/B measurement). Phase 2
depends on Phase 1's `run_columnar_grouped_aggs`; Phase 3 depends on both.

## Phase 1: Executor grouped aggregate + reverse Arrow→Datum

### T1.1 — `arrow_value_to_datum`: reverse conversion for group keys (the inverse of `build_arrow`)

#### Objective
Add `arrow_value_to_datum(array, row, typoid) -> Result<(Datum, bool)>` covering every OID `build_arrow` handles
(int2/4/8, float4/8, bool, text, timestamp/timestamptz/date), returning `(_, true)` for a null cell.

#### Why this step (action + reasoning)
Action: implement a per-type Arrow-array downcast → PG `Datum`. Reasoning: GROUP BY output rows carry the group-key
VALUES, which live as Arrow cells in the aggregate result batch; converting them back to PG Datums is the new machinery
GROUP BY needs and nothing else provides. It mirrors `build_arrow` (`df_executor.rs:45`) exactly in the reverse
direction, so the type coverage and byte encoding stay consistent (ADR-3 datum-lifetime handled by the caller).

#### Evidence
`build_arrow` type dispatch at `theodb_rs/src/am/df_executor.rs:49-96`; `into_datum` usage precedent at
`df_executor.rs:284`.

#### Files to edit
- `theodb_rs/src/am/df_executor.rs`

#### Deep file dependency analysis
Pure function over an `&dyn Array` + `typoid` + `row`; no PG relation handle needed. Called only by T1.2. Uses
`pgrx::IntoDatum` (already imported via `pgrx::prelude::*`).

#### Pseudo-code / Signatures
```rust
fn arrow_value_to_datum(arr: &dyn Array, row: usize, typoid: u32) -> Result<(pg_sys::Datum, bool), String> {
    if arr.is_null(row) { return Ok((pg_sys::Datum::from(0usize), true)); }
    let d = match typoid {
        23 => arr.as_any().downcast_ref::<Int32Array>()?.value(row).into_datum(),
        20 => …Int64…, 21 => …Int16…, 700 => …Float32…, 701 => …Float64…, 16 => …Boolean…,
        25|1042|1043 => …StringArray → String → text datum…,
        1114|1184 => Int64→timestamptz datum (μs), 1082 => Int32→date datum,
        other => return Err(format!("unsupported group key oid {other}")),
    };
    Ok((d.ok_or("datum")?, false))
}
```

#### TDD
- `test_arrow_value_to_datum_int_roundtrip` — Given an `Int32Array [7, null, -3]`, When converting rows 0/1/2 for oid
  23, Then `(datum(7), false)`, `(_, true)`, `(datum(-3), false)`. (Standalone-provable logic where feasible;
  full-type coverage via the Phase 3 A/B on real columns.)
- `test_arrow_value_to_datum_unsupported_type_errs` — Given oid `1700` (numeric), Then `Err` (fail-fast typed).

#### Concurrency tests
(none — single-threaded)

#### Acceptance Criteria
- Every `build_arrow`-supported OID has a reverse arm; an unsupported OID returns a typed `Err` (not a panic).
- Null cell → `(_, true)`.

#### DoD
- `cargo pgrx test` (droplet) green for the new unit test; `cargo build` clean.

### T1.2 — `run_columnar_grouped_aggs`: grouped DataFusion path → `Vec<Vec<(Datum,bool)>>`

#### Objective
Add a grouped executor: decode (group cols ∪ agg cols), `.aggregate(group_exprs, agg_exprs)`, convert the multi-row
batch to `Vec<Vec<(Datum,bool)>>` ordered by an explicit **output layout**.

#### Why this step (action + reasoning)
Action: a new `run_columnar_grouped_aggs(rel, group_cols:&[(String,u32)], aggs, layout:&[(u8,usize)])`. Reasoning:
this is the executor half of GROUP BY; keeping it separate from `run_aggs_on_batch` honors ADR-1 (arrow_cache
untouched). The `layout` argument implements ADR-2 (each output slot maps to a batch column), so the result rows are
emitted in the PG target order regardless of DataFusion's `[keys…,aggs…]` order.

#### Evidence
`.aggregate(vec![], exprs)` at `df_executor.rs:267`; batch→row conversion at `df_executor.rs:274-294`;
`decode_to_batch` projection at `df_executor.rs:116-141`.

#### Files to edit
- `theodb_rs/src/am/df_executor.rs`

#### Deep file dependency analysis
Reuses `decode_to_batch` (project group cols ∪ sum cols; `predicates=&[]`, `skip=false` — no WHERE in this slice),
`count`/`sum`/`col` builders, the `HeldInterrupts`+`GreedyMemoryPool`+`target_partitions=1` runtime
(`df_executor.rs:251-260`), and `arrow_value_to_datum` (T1.1). Returns the multi-row shape consumed by T2.3.

#### Pseudo-code / Signatures
```rust
pub(super) unsafe fn run_columnar_grouped_aggs(
    rel, group_cols: &[(String,u32)], aggs: &[AggSpec], layout: &[(u8,usize)],
) -> Result<Vec<Vec<(pg_sys::Datum,bool)>>, String> {
    let batch = decode_to_batch(rel, &sum_names(aggs)+group names, &[], false)?;
    // df.aggregate(group_cols.map(col), agg exprs aliased a0..); collect
    // out cols: [group_0..group_{g-1}, a0..a{n-1}]; for each result row r, for each layout (kind,idx):
    //   kind 0 → arrow_value_to_datum(batch group col idx, r, group_typoid[idx])
    //   kind 1 → existing agg conversion on col (g+idx), row r
}
```

#### TDD
- `test_run_columnar_grouped_shape` — Given a 3-group columnar table `GROUP BY k`, Then result has 3 rows, each with
  `[key, sum]` in target order. (Full assertion via the Phase 3 in-PG A/B; the unit test asserts row count + shape.)

#### Concurrency tests
(none — single-threaded)

#### Acceptance Criteria
- Returns one inner Vec per group; inner Vec length == output slot count; slot order == `layout`.
- Zero groups → empty outer Vec.

#### DoD
- `cargo build` clean; grouped path compiles and is called by T2.

## Phase 2: Planner admit + multi-row exec

### T2.1 — `admit` accepts a `groupClause` (no WHERE); parse group keys + build the output layout

#### Objective
Widen `admit` to accept a grouped aggregate: walk `output_rel->reltarget->exprs`, classify each as a bare group `Var`
(supported type) or an `Aggref`, build the `(group_cols, aggs, layout)` triple; decline if a WHERE is also present or
any target expr is neither.

#### Why this step (action + reasoning)
Action: remove the blanket `groupClause` decline (`columnar_agg.rs:208`) and add group-key parsing. Reasoning: the
planner hook is where the query shape is inspected; PG guarantees every non-aggregate target of a GROUP BY query is a
grouping key, so classifying `T_Var` as a key and `T_Aggref` as an agg is sound. Declining GROUP BY + WHERE keeps the
slice orthogonal (owner scope). A bare-`Var` requirement (not `date_trunc(...)`) keeps it to column keys.

#### Evidence
Blanket decline at `columnar_agg.rs:208-215`; agg parsing at `columnar_agg.rs:236-278`; `extract_all_predicates` at
`columnar_agg.rs:192`; supported OIDs = `build_arrow` set (`df_executor.rs:49-96`).

#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs`

#### Deep file dependency analysis
`admit`'s return type grows to include `group_cols: Vec<(i32 attno, u32 typoid)>` + `layout: Vec<(u8,usize)>`.
Callers: `upper_paths_hook` (`columnar_agg.rs:317`) → `plan_custom_path` carry. A new helper
`arrow_supported_type(typoid)->bool` lists the `build_arrow` OIDs.

#### TDD
- `test_admit_groupby_single_key_is_customscan` — `SELECT k, sum(x) FROM col GROUP BY k` → EXPLAIN shows
  `Custom Scan (theodb_columnar_agg)`.
- `test_admit_groupby_with_where_declines` — `SELECT k, sum(x) FROM col WHERE x>0 GROUP BY k` → NOT a CustomScan
  (native plan), result still correct.
- `test_admit_groupby_expr_key_declines` — `GROUP BY date_trunc('day', ts)` → NOT a CustomScan.

#### Concurrency tests
(none — single-threaded)

#### Acceptance Criteria
- Grouped aggregate over supported key types with no WHERE → admitted with a correct layout.
- GROUP BY + WHERE, grouping expression, or unsupported key type → `None` (native plan).

#### DoD
- `cargo pgrx test` green for the three admit tests.

### T2.2 — Carry the group layout in `custom_private`; parse it in `begin_custom_scan`

#### Objective
Extend the `custom_private` IntList with the group block `[ngroup, (attno,typoid)×ngroup, noutput, (kind,idx)×noutput]`
and parse it in `begin_custom_scan`, then call `run_columnar_grouped_aggs`.

#### Why this step (action + reasoning)
Action: mirror the existing carry pattern (`columnar_agg.rs:433` parses `[mode,relid,nagg,(kind,attno)×,npred,…]`).
Reasoning: plan-time→exec state travels through `custom_private` (the proven M100 side-channel); the group block
extends it without a new mechanism (parsimony). `begin` resolves the group column NAMES from attnos via the tupdesc
(same as sum-col resolution at `columnar_agg.rs:444`).

#### Evidence
IntList build at `plan_custom_path`/`upper_paths_hook` (`columnar_agg.rs:317-400`); parse at `columnar_agg.rs:432-468`.

#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs`

#### Deep file dependency analysis
`ColumnarAggState.result` (`columnar_agg.rs:58`) changes to `*mut Vec<Vec<(Datum,bool)>>`; add `cursor: usize`
(`create_custom_scan_state` at `columnar_agg.rs:402` inits it). `begin` builds the result via
`run_columnar_grouped_aggs` (grouped) or the existing `run_columnar_aggs` wrapped as a single-row outer Vec
(non-grouped), inside `es_query_cxt` (ADR-3).

#### TDD
- `test_groupby_carry_roundtrip` — the layout carried plan-time is parsed identically at exec (asserted indirectly by
  the A/B byte-identical result; a `#[pg_test]` asserts a 2-key group returns correct rows).

#### Concurrency tests
(none — single-threaded)

#### Acceptance Criteria
- Group block round-trips; `begin` builds `Vec<Vec<…>>` in `es_query_cxt`.

#### DoD
- `cargo build` clean.

### T2.3 — Multi-row `exec_custom_scan` (cursor)

#### Objective
Emit one result row per `exec` call via a cursor; `ExecClearTuple` when the cursor reaches the end.

#### Why this step (action + reasoning)
Action: replace the single-row `st.done` gate (`columnar_agg.rs:490-504`) with a `cursor` over `Vec<Vec<…>>`.
Reasoning: GROUP BY produces N rows and the executor pulls one slot per `exec` call; a cursor is the minimal change
(the non-grouped path is just the N=1 case, unifying both). `rescan` resets the cursor to 0.

#### Evidence
Current single-row emit at `columnar_agg.rs:487-505`; `rescan` at `columnar_agg.rs:517-522`.

#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs`

#### Deep file dependency analysis
`exec` reads `st.result[st.cursor]`, fills the slot, `cursor += 1`; `end` frees the outer Box; `rescan` sets
`cursor=0`. The scan slot tupdesc (group cols + agg cols) is already the CustomScan output target — no descriptor
change.

#### TDD
- `test_groupby_multirow_emit` — a 3-group aggregate returns exactly 3 tuples then EOF (asserted by the A/B row count
  + a `#[pg_test]` counting rows).
- `test_groupby_empty_returns_zero_rows` — GROUP BY over an empty columnar table returns 0 rows.

#### Concurrency tests
(none — single-threaded)

#### Acceptance Criteria
- N groups → N tuples then `ExecClearTuple`; empty → 0 tuples; rescan re-emits from row 0.

#### DoD
- `cargo pgrx test` green.

## Phase 3: Measurement (in-PG A/B)

### T3.1 — `benchmarks/columnar_groupby_ab.py` — byte-identical + measured speedup + EXPLAIN

#### Objective
On a 1M-row `theodb_columnar` table vs an identical heap table, run `SELECT key, sum(x), count(*) FROM t GROUP BY key`
(and a temporal `GROUP BY d`, a multi-key `GROUP BY a,b`, and the agg-before-key `SELECT sum(x), key … GROUP BY key`),
asserting the full grouped result set is byte-identical to the heap plan, the columnar plan is a CustomScan (EXPLAIN),
and measuring the wall-clock speedup.

#### Why this step (action + reasoning)
Action: an in-PG A/B mirroring `columnar_zonemap_ab.py`. Reasoning: Rule 5 — the speedup is a claim only with a
reproducible artifact under `docs/benchmarks/`; byte-identical vs the native plan is the correctness gate; the
agg-before-key case proves ADR-2 (column-order mapping).

#### Evidence
Precedent: `benchmarks/columnar_zonemap_ab.py`, `benchmarks/columnar_zonemap_ts_ab.py`.

#### Files to edit
- `benchmarks/columnar_groupby_ab.py` (NEW)
- `docs/benchmarks/columnar-groupby-verdict.{md,json}` (NEW)

#### TDD
- `test_groupby_ab_byte_identical` — the A/B asserts, per query shape, the sorted grouped result set from the columnar
  CustomScan equals the heap result set exactly (ORDER BY key for a deterministic compare).

#### Concurrency tests
(none — single-threaded)

#### Acceptance Criteria
- Every query shape: byte-identical result set (columnar == heap), CustomScan engaged, speedup reported.

#### DoD
- Verdict written with measured numbers; honest caveat on the regime.

## Coverage Matrix

| Goal claim / requirement | Task(s) |
|---|---|
| Reverse Arrow→Datum for group keys (all supported types) | T1.1 |
| Grouped DataFusion aggregate → multi-row result in target order | T1.2 (layout ADR-2), T2.2 |
| admit a GROUP BY (no WHERE); decline combined / expr keys / unsupported types | T2.1 |
| Carry group layout plan-time→exec | T2.2 |
| Multi-row tuple emission + empty + rescan | T2.3 |
| Text/varlena group-key datum lifetime | T2.2 (ADR-3 es_query_cxt) |
| NULL group key handling | T1.1 (null cell), T3.1 (A/B) |
| Byte-identical vs native + measured speedup + CustomScan engaged | T3.1 |
| Column-order mapping (agg-before-key) correctness | T1.2, T3.1 |

## Global Definition of Done

- `cargo build` / `cargo pgrx install` clean on the droplet; no new clippy errors.
- `cargo pgrx test` green for all new `#[pg_test]`s.
- `benchmarks/columnar_groupby_ab.py` reports byte-identical result sets + CustomScan engaged for every query shape,
  with a measured speedup.
- `docs/benchmarks/columnar-groupby-verdict.{md,json}` written with measured numbers + honest caveat.
- `CHANGELOG.md [Unreleased] § Added` updated (Rule 6).
- File-size budget: `columnar_agg.rs` and `df_executor.rs` stay under 700 LoC each (per `rules/architecture.md`
  guidance; current 560 / 384).
