---
slug: vecfilter-honest-cost-model
milestone_id: M95
created_at: 2026-07-13
goal: Replace the vecfilter node's forced-selection cost heuristic (`total_cost = min_cost × 0.1`) with an honest cost = bitmap-subplan cost + a selectivity-derived vector-scan cost, so that in a SIFT1M selectivity sweep the planner picks the node ONLY at selectivities where it measurably wins (recall+QPS) and falls back to the native plan where the native plan wins.
---

# M95 — honest cost model for the vecfilter Custom Scan node

## Context

The vecfilter node (M92–M94) forces its own selection: `pathlist_hook` sets `path.total_cost = min_cost × 0.1`
(`customscan.rs:321`), strictly below the cheapest base path, so the planner ALWAYS picks the node when
`theodb.enable_vecfilter` is on — even at loose selectivity where the native post-filter (a plain vector
IndexScan+Filter) or a BitmapHeapScan+Sort would be cheaper. This is the single reason the feature cannot leave
experimental/GUC-off (council LOW-4 in the M94 review; disclosed in every M92/M94 benchmark as "an honest cost model
is a follow-up"). M95 replaces the heuristic so the planner picks the node only where it measurably wins — the
prerequisite for graduating the whole M90–M94 filtered-search investment toward default-capable.

The discovery (council-index-storage, code-grounded against `costsize.c` + our `cost.rs`) reframed the milestone
with one load-bearing insight (its risk R1): the vector-scan cost term is NOT "read the two child path costs and
add" — the child IndexPath's `total_cost` prices the DEFAULT-probe scan and is BLIND to the M91 adaptive probing
(a selective filter probes MORE lists at runtime). So the node's cost must RE-DERIVE the effective probes from the
bitmap selectivity itself. That makes M95 a small NEW honest cost function grounded in the M91 loop, not a wiring
of existing costs.

## Goal

Replace the vecfilter node's forced-selection cost heuristic (`total_cost = min_cost × 0.1`) with an honest cost =
bitmap-subplan cost + a selectivity-derived vector-scan cost, so that in a SIFT1M selectivity sweep the planner
picks the node ONLY at selectivities where it measurably wins (recall+QPS) and falls back to the native plan where
the native plan wins.

## Baseline Context

### Files that will be touched

| File | LoC today | Last touch | Role |
|---|---|---|---|
| `theodb_rs/src/am/customscan.rs` | ~560 | `0f29c7c` (v0.81.0) | the Custom Scan Provider; `pathlist_hook` sets the cost hack at `:320-321` |
| `theodb_rs/src/am/cost.rs` | ~180 | M48 (`amcostestimate`) | the honest visit-ratio cost model to REUSE (its shape + EC-3 fail-safe discipline) |
| `benchmarks/m92_arbitrary_where_bench.py` | ~260 | v0.80.0 | the M92 sweep harness to EXTEND (force-each-plan comparison) |

### Current callers / dependents

| Symbol | Defined | Called by (production) | Called by (tests) |
|---|---|---|---|
| `pathlist_hook` cost block | `customscan.rs:298-321` | the installed `set_rel_pathlist_hook` (`customscan.rs:143`) — invoked by the planner per base rel | `m92_customscan_inert_when_disabled`, `m93_t2_*`, `m94_*` (EXPLAIN assertions) |
| `ivf_visit_ratio` / `scan_visit_ratio` / `ratio_for` | `cost.rs:22,65,52` | `amcostestimate` via `am/mod.rs` | `cost.rs` unit tests (`ivf_ratio_is_probes_over_lists_clamped`, …) |
| `page::peek_magic` / `read_ivf_aq_meta_split*` | `page.rs` | `cost.rs:70-76`, `scan.rs` meta reads | page unit tests |
| `guc::probes` / `guc::over_fetch` | `guc.rs:242,252` | `scan.rs` M91 loop, `cost.rs` | guc tests |

The new `cost.rs` functions (T1.1) get their FIRST production caller in T2.1's `pathlist_hook` rewrite (wiring pillar a). No cross-repo callers — `customscan.rs`/`cost.rs` are internal to `theodb_rs`.

### Current state (from code read)

- `customscan.rs:266-296` — `pathlist_hook` finds `vector_path` (`!pathkeys.is_null() && pathtype == T_IndexScan && param_info.is_null()`) and `bitmap_path` (`T_BitmapHeapScan && param_info.is_null()`), then at `:298-321` hand-rolls a 2-child CustomPath, sets `path.pathkeys = (*vector_path).pathkeys` (`:307` — KEEP), and forces `path.startup_cost = 0.0; path.total_cost = min_cost × 0.1` (`:320-321` — the hack to REPLACE).
- `cost.rs:22-30` — `ivf_visit_ratio(probes, lists) = (probes/lists).min(1.0)`, `lists==0 → 1.0` (the EC-3 fail-safe pattern). `cost.rs:65-82` — `scan_visit_ratio` reads the meta NoLock and degrades to 1.0 on any unreadable/torn meta (NEVER errors — a planner hook that errors aborts ALL query planning). M95's term_V mirrors this discipline exactly.
- `scan.rs:641` (v7) / `scan.rs:507` (v5) — the M91 adaptive loop breaks at `probed >= probes && (!filtering || cands.len() >= rerank_pool)`: under a filter it keeps probing nearest lists until `rerank_pool` matching candidates accumulate. This is what term_V must image.
- `guc.rs:242` `probes()`, `guc.rs:252` `over_fetch()`; `rerank_pool = 64 × over_fetch` (the pool the loop fills).
- pgrx bindings confirmed present: `BitmapHeapPath { path, bitmapqual: *mut Path }`; `IndexPath.indextotalcost`; `get_tablespace_page_costs`; globals `seq_page_cost`/`random_page_cost`/`cpu_operator_cost`/`cpu_tuple_cost`.

### Domain glossary

- **selectivity `s`** — `bitmap_path.rows / rel.tuples`: the planner's own estimate of the matching-tuple fraction.
- **effective probes** — the runtime probe count the M91 loop reaches under a filter: `clamp(max(probes_default, rerank_pool / (s × avg_list_size)), 1, lists)`. Selective `s` → many probes; loose `s` → the default already fills the pool.
- **term B** — the cost of PRODUCING the TIDBitmap membership: the bitmap sub-plan cost, read directly from `bitmapqual.total_cost` (NOT the parent BitmapHeapPath's total, which includes the heap fetch we never perform).
- **term V** — the vector-scan-with-membership cost: `effective_probes × pages_per_list × random_page_cost + candidates × cpu_operator_cost + rerank random reads`.
- **EC-3 fail-safe** — a planner-hook invariant: never `pg_sys::error!` on an unreadable meta; degrade to a conservative cost (fall back to the old heuristic OR refuse the path so the native plan wins).

### Architecture boundaries affected

`customscan.rs` (planner hook) + a new pure function in `cost.rs`. No AM scan change; no page format; no WAL/VACUUM.
The one inherited hard rule: the cost math must be EC-3 fail-safe (never error in a `set_rel_pathlist_hook`).

## Prior Art & Related Work

- **council-index-storage discovery (this milestone)** — the code-grounded blueprint fragment: term B = `bitmapqual.total_cost` (exact, free, avoids double-counting the heap fetch — `costsize.c:1048,1116-1127`); term V = the selectivity-derived formula imaging the M91 loop; pathkeys credit is automatic (the BitmapHeapScan+Sort competitor pays `cost_sort`, `costsize.c:506-515`, we don't); EC-3 fail-safe mandatory; crossover bracketed in `s ∈ [5%, 25%]` from the M92 data (12× QPS margin at 1% collapsing to 1.4× at 5%).
- **`cost.rs` (M48)** — the existing honest visit-ratio model + the EC-3 fail-safe meta-read discipline; M95 reuses its shape and adds `effective_probes` re-derivation (Rule 9 — extend, don't reinvent).
- **Postgres `costsize.c`** — `cost_bitmap_heap_scan` (the B/heap split), `cost_bitmap_tree_node` (`bitmapqual.total_cost` semantics), `cost_sort` (the competitor's Sort we get credited against), the cost globals. Study, not copy.

## ADRs

### ADR M95-1 — term V re-derives effective probes from selectivity (does NOT read the child IndexPath cost)

**Decision:** the vector-scan cost is a NEW pure function `vecfilter_scan_cost(s, lists, tuples, rerank_pool, page costs)` that computes `effective_probes` from the bitmap selectivity and prices page reads + CPU from the cost globals — mirroring the M91 loop, not the child path's `total_cost`.

**Rejected alternative:** *term V = `(*vector_path).total_cost`* — REJECTED (council R1): the child IndexPath cost comes from `genericcostestimate`/our M48 `amcostestimate`, which price the DEFAULT-probe scan and are BLIND to the M91 adaptive probing (a selective filter probes MORE at runtime). Using it would UNDER-cost the node exactly where it works hardest → the planner would over-select it, reproducing the current bug in a subtler form.

### ADR M95-2 — term B = `bitmapqual.total_cost`, never the parent BitmapHeapPath total

**Decision:** the membership-production cost is read directly from `(*(bitmap_path as *mut BitmapHeapPath)).bitmapqual.total_cost`.

**Rejected alternative:** *term B = `bitmap_path.total_cost`* — REJECTED: that INCLUDES `cost_bitmap_heap_scan`'s run_cost (the heap-page fetches + per-tuple CPU, `costsize.c:1070,1099`) — the heap fetch we NEVER perform (we MultiExec the bitmap-producing `.lefttree` for membership only, `customscan.rs:402`). Reading `bitmapqual.total_cost` is exactly the "minus the heap-fetch part" subtraction, for free.

### ADR M95-3 — EC-3 fail-safe: never error, degrade to the conservative heuristic

**Decision:** any unreadable/degenerate input (meta unreadable → `lists == 0`, `tuples <= 0`, null bitmapqual) makes the cost function fall back to a conservative value (the old `min_cost × 0.1`-style forced pick is NOT conservative — instead fall back to NOT adding the node's cheap path, letting the native plan win; OR clamp term_V high).

**Rejected alternative:** *`pg_sys::error!` on unreadable meta* — REJECTED: a `set_rel_pathlist_hook` that errors aborts ALL planning of that query on the instance (a VACUUM momentarily making the meta unreadable would break every query). `cost.rs:65` already honors this; M95 must too.

## Dependency Graph

```
Phase 1 (term B + term V pure fns in cost.rs, unit-tested) ──> Phase 2 (wire into pathlist_hook; remove the hack; EXPLAIN pg_tests) ──> Phase 3 (measurement gate: sweep + force-each-plan) ──> Phase 4 (review + release)
```

## Phase 1 — the honest cost function (pure, unit-tested)

### Task T1.1 — `vecfilter_scan_cost` + `effective_probes` in cost.rs

#### Why this step

**Action:** add to `cost.rs` two pure functions: `effective_probes(s, lists, avg_list_size, rerank_pool, probes_default) -> f64` = `clamp(max(probes_default, rerank_pool / (s × avg_list_size)), 1, lists)`; and `vecfilter_scan_cost(effective_probes, pages_per_list, avg_list_size, rerank_pool, spc_random_page_cost, cpu_operator_cost) -> f64` = `effective_probes × pages_per_list × random + (effective_probes × avg_list_size) × cpu_op + rerank_pool × random + lists × cpu_op`. Both pure (no `Relation`), unit-testable with forged inputs like the existing `cost.rs` tests.

**Reasoning:** ADR M95-1 — this is the honest core the milestone reduces to. Keeping it pure (no FFI) makes it unit-testable and mirrors the existing `ivf_visit_ratio`/`ratio_for` shape (Rule 9). The formula is the algebraic image of the M91 loop (`scan.rs:641`): selective `s` → few matches/list → many probes (costlier, higher recall); loose `s` → `probes_default` fills the pool → cheap.

#### Files to edit

- `theodb_rs/src/am/cost.rs` — add the two functions + unit tests.

#### Deep file dependency analysis

`cost.rs` is imported by `am/mod.rs` (`amcostestimate`). Adding pure functions + tests does not touch `scan_visit_ratio` or `amcostestimate`. No caller changes in this task (wiring is T2.1). The functions become the single source of the cost math T2.1 calls.

#### TDD

```
test_effective_probes_selective_grows (unit): s=0.001, lists=1024, avg_list_size=1000, rerank_pool=64, probes_default=8
  → rerank_pool/(s*avg) = 64/1.0 = 64 probes (> default 8, < lists) → asserts ≈ 64.
test_effective_probes_loose_is_default (unit): s=0.5, avg=1000, rerank_pool=64, probes_default=8
  → 64/(0.5*1000)=0.128 → max(8, 0.128)=8 → asserts == 8 (the default fills the pool, cheap).
test_effective_probes_clamped_to_lists (unit): s=1e-6 → huge → clamped to lists.
test_vecfilter_scan_cost_monotone_in_probes (unit): cost(64 probes) > cost(8 probes) with same other args.
test_effective_probes_degenerate_lists_zero (unit): lists=0 → returns 0.0 or a sentinel the caller treats as fail-safe (NEVER panics/divides-by-zero).
```

#### Concurrency tests

(none — single-threaded pure functions; the planner hook runs in the backend's planning phase, no shared state.)

#### Acceptance criteria

- `assert_eq!(effective_probes(0.5, 1024, 1000.0, 64.0, 8), 8.0)` — loose selectivity returns exactly the default probes.
- `assert!(effective_probes(0.001, 1024, 1000.0, 64.0, 8) >= 60.0)` — 0.1% selectivity returns ≈64 probes (the pool-fill count).
- `assert!(vecfilter_scan_cost(64.0, …) > vecfilter_scan_cost(8.0, …))` — cost strictly increases from 8 to 64 probes with all other args equal.
- `assert_eq!(effective_probes(0.0, 1024, 0.0, 64.0, 8), 0.0)` — degenerate inputs (`s<=0` OR `avg_list_size==0` OR `lists==0`) return the `0.0` fail-safe sentinel and the test completes without a panic.

#### DoD

- `cargo pgrx test cost` green for the 5 new unit tests (droplet).

## Phase 2 — wire into the planner hook

### Task T2.1 — replace the cost hack; read term B + selectivity; EXPLAIN pg_tests

#### Why this step

**Action:** in `pathlist_hook`, after finding `vector_path`+`bitmap_path`: read `s = (*bitmap_path).rows / (*rel).tuples` (clamp to `[ε,1]`); read `term_B = (*(bitmap_path as *mut BitmapHeapPath)).bitmapqual.total_cost`; read the index meta fail-safe (mirror `scan_visit_ratio` — `lists`, `avg_list_size`, `pages_per_list`) via a NoLock open, degrading on any Err; compute `term_V = vecfilter_scan_cost(effective_probes(s,…),…)` with `get_tablespace_page_costs`; set `path.startup_cost = term_B + stage0/1 portion`, `path.total_cost = term_B + term_V`. Remove `startup=0; total=min_cost×0.1`. Keep `path.pathkeys` (`:307`). On ANY fail-safe trip (unreadable meta / null bitmapqual), do NOT add the path (native plan wins) — ADR M95-3.

**Reasoning:** ADRs M95-1/2/3. Setting an honest `total_cost` + keeping `pathkeys` lets the planner's own comparison pick the node only where it wins: against BitmapHeapScan+Sort the competitor pays `cost_sort` (we're credited via pathkeys); against the plain vector IndexScan+Filter the node must win on total_cost (term B + reduced-survivor term V). Removing the hack is the whole behavioral change.

#### Files to edit

- `theodb_rs/src/am/customscan.rs` — `pathlist_hook` cost block (`:298-321`); add a fail-safe meta-read helper (or reuse `page::` readers like `cost.rs:70`).

#### Deep file dependency analysis

`pathlist_hook` is the only caller of the cost block. `path.pathkeys`/`path.rows` stay. The 2-child `custom_paths` construction (`:298-306`) is unchanged. The meta read reuses `page::peek_magic` + `page::read_ivf_aq_meta_split*` (the exact readers `cost.rs:70-76` uses) — Rule 9. No change to `plan_custom_path`/exec/begin.

#### TDD

```
test_m95_node_chosen_at_tight_selectivity (pg_test): a 2000-row table, cat with ~1% matching, vector index, GUC on
  → EXPLAIN shows "Custom Scan (theodb_vecfilter)".
test_m95_node_not_chosen_at_loose_selectivity (pg_test): same table, a predicate matching ~60% of rows, GUC on
  → EXPLAIN does NOT show the node (the native vector IndexScan+Filter / seqscan wins on honest cost).
test_m95_unreadable_meta_falls_back (pg_test OR unit): force the fail-safe path (degenerate meta) → the hook does
  not add the node (native plan), never errors planning.
```

#### Concurrency tests

(none — planner-phase, single backend, no shared mutable state added.)

#### Failure scenarios

- **Unreadable/torn IVF meta during planning (concurrent VACUUM fold):** the fail-safe meta read returns Err → the hook skips adding the node → native plan is used → planning never aborts. Test: `test_m95_unreadable_meta_falls_back`. (This is external-I/O-adjacent — the meta page read — so the scenario is mandatory.)
- **Null `bitmapqual`:** treated as fail-safe (skip the node). Guarded before deref.

#### Acceptance criteria

- `assert!(explain.contains("Custom Scan (theodb_vecfilter)"))` at ~1% matching selectivity with the GUC on.
- `assert!(!explain.contains("theodb_vecfilter"))` at ~60% matching selectivity with the GUC on.
- `grep -c 'min_cost \* 0.1' customscan.rs` returns 0 — the forced-selection literal is deleted.
- `test_m95_unreadable_meta_falls_back` completes with exit 0 (planning did not abort on the degenerate meta).

#### DoD

- `cargo pgrx test m95` green; full suite ≥ 268 tests GREEN (265 + 3), GUC-off byte-identical.
- CHANGELOG `[Unreleased]` updated.

## Phase 3 — measurement gate

### Task T3.1 — SIFT1M selectivity sweep + force-each-plan comparison

#### Why this step

**Action:** extend `m92_arbitrary_where_bench.py` to a sweep `s ∈ {0.1%, 0.5%, 1%, 2%, 5%, 8%, 12%, 15%, 25%, 50%}` (dense around the bracketed crossover `[5%,25%]`). At each `s`: (chosen) GUC on, record whether EXPLAIN picks the node + measure recall+QPS; (force-INLINE) force the node; (force-POST) GUC off native; (force-Bitmap+Sort) force bitmap. Assert the CHOSEN plan's measured (recall,QPS) Pareto-dominates or matches the best forced alternative at every `s`, and the planner's node→native flip falls within one sweep step of the measured crossover. If the model mis-calibrates, tune ≤2 cost constants (KISS) and re-run — the sweep is the calibration oracle.

**Reasoning:** the honest cost is only correct if the planner's pick == the best-measured plan. This is the milestone's explicit "the sweep is the oracle" gate. Honest-negative (if the model cannot discriminate, document why + keep GUC-opt-in) is a valid terminal.

#### Files to edit

- `benchmarks/m92_arbitrary_where_bench.py` — add the sweep + force-each-plan modes (or a sibling `m95_cost_model_bench.py` reusing its fixture loader — Rule 9).

#### Deep file dependency analysis

Reuses the M92 harness's SIFT loader + exact-seqscan ground-truth. The new modes force plans via `SET enable_*` / GUC toggles per run (the M92 harness-bug lesson: reset GUCs explicitly per run).

#### TDD

```
(benchmark, not a unit test — the gate is the measured artifact)
Assertion in the harness: for every s in the sweep, chosen_plan.measured ∈ Pareto-best(forced alternatives);
crossover_planner within 1 step of crossover_measured. Emits docs/benchmarks/m95-cost-model.{md,json}.
```

#### Concurrency tests

(none — the benchmark is a sequential measurement harness.)

#### Acceptance criteria

- `docs/benchmarks/m95-cost-model.{md,json}` produced with real numbers (Rule 5 — no fabrication); the chosen plan matches the measured-best at every sweep point OR an honest-negative is documented with the reason.

#### DoD

- Benchmark artifact committed; the crossover is located and reported.

## Failure scenarios

The only external I/O this plan touches is reading the IVF index meta page during planning (the fail-safe boundary).

| Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|
| Unreadable / torn IVF meta during planning (concurrent VACUUM fold makes `peek_magic`/`read_ivf_aq_meta_split*` return `Err`) | `test_m95_unreadable_meta_falls_back` forces the degenerate meta path (`lists == 0`) | the hook SKIPS adding the node (native plan wins); planning NEVER aborts — `pg_sys::error!` is forbidden in a `set_rel_pathlist_hook` (EC-3, mirrors `cost.rs:65`) |
| Null `bitmapqual` on the BitmapHeapPath | guarded `is_null()` check before deref in T2.1 | fail-safe: skip the node, no deref of a null pointer |
| `rel.tuples == 0` (empty/never-ANALYZEd table) → division by zero in `s` | `effective_probes` degenerate-input unit test (T1.1) | returns the fail-safe sentinel; the hook falls back (no node), never panics/divides-by-zero |

## Phase 4: Integration Validation

- Full `cargo pgrx test pg17` GREEN (≥ 268 tests) on the droplet; GUC-off byte-identical.
- The sweep gate passes (chosen == measured-best across `s`) OR honest-negative documented.
- ADR on the GUC default post-gate (stays opt-in unless the sweep proves clean dominance-selection).
- Review: council-index-storage (cost model) + council-benchmark (the sweep honesty); findings fixed before `/release`.

## Coverage Matrix

| # | Gap / Requirement | Task(s) | Resolution |
|---|---|---|---|
| 1 | Honest term V re-deriving effective probes from selectivity | T1.1 | `effective_probes` + `vecfilter_scan_cost` pure fns (ADR M95-1) |
| 2 | Term B = membership production cost, no heap double-count | T2.1 | read `bitmapqual.total_cost` (ADR M95-2) |
| 3 | Remove the `min_cost × 0.1` forced-selection hack | T2.1 | replaced by `term_B + term_V` |
| 4 | Node chosen only where it wins (tight yes / loose no) | T2.1, T3.1 | EXPLAIN pg_tests + the sweep gate |
| 5 | pathkeys credit (competitor pays the Sort) | T2.1 | keep `path.pathkeys` (already set) |
| 6 | EC-3 fail-safe (never error in the planner hook) | T1.1, T2.1 | degenerate-input sentinel + fail-safe meta read (ADR M95-3) |
| 7 | Measurement proof (chosen == measured-best) | T3.1 | SIFT1M sweep + force-each-plan artifact |
| 8 | Zero regression; GUC-off byte-identical | T2.1, Phase 4 | full suite ≥ 268 GREEN |
| 9 | sign-off council-index-storage + council-benchmark | T3.1 | the councils review T3.1's sweep artifact + the T2.1 cost math at integration validation; findings fixed before `/release` |

**Coverage: 9/9 gaps covered (100%)**

## Drawbacks & Risks

| # | Risk | Severity | Mitigation | Owner |
|---|---|---|---|---|
| 1 | Cost-constant calibration (fudge constants mis-set → crossover shifts) | HIGH | the sweep IS the calibration oracle (T3.1); tune ≤2 constants until chosen==measured-best; KISS (no per-term knob) | impl |
| 2 | Bad/stale stats → wrong `s` → wrong plan | MEDIUM | degrades in the recall-safe direction (over-estimate s → fewer probes → node looks cheaper → picked where INLINE has the higher recall floor); document, don't fix (bad stats hurt every native plan) | impl |
| 3 | The M87 iterative competitor's cost is ALSO probe-blind (both under-priced) | MEDIUM | the blind spot partially cancels in the comparison; M95 fixes the node's term V; leaving M48 as-is is the honest in-scope decision (the node only needs to be more honest about the term that DIFFERS) | impl |
| 4 | Honest-negative: the model may not discriminate cleanly | LOW | honest-negative is a valid terminal — document + keep GUC opt-in; the investment is not lost (M90–M94 still work behind the GUC) | impl |

## Unresolved Questions

- The GUC default post-gate (ON-by-default vs opt-in) is decided by the T3.1 sweep result — resolved at measurement time, recorded in the Phase 4 ADR. Not a blocker for implementation.

## Global DoD

- Full suite ≥ 268 tests, 0 failed (droplet); GUC-off byte-identical.
- No page-format change; `scan.rs` untouched; the cost math is EC-3 fail-safe.
- `customscan.rs` < 800 LoC; `cost.rs` < 350 LoC.
- CHANGELOG `[Unreleased]` updated; benchmark artifact committed.
