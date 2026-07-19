---
slug: m115-columnar-composability
milestone_id: M115
created_at: 2026-07-19
goal: Make the columnar-aggregate CustomScan output byte-identical AND consumable in subqueries/joins/aggregate-ORDER-BY, with no top-level regression.
---

# Plan: M115 — columnar-aggregate CustomScan composability

## Goal

Make the M100 columnar-aggregate `CustomScan` output byte-identical AND consumable inside an enclosing expression
(subquery, join, `string_agg(... ORDER BY agg)`) — eliminating the `cache lookup failed for attribute N of relation 0`
failure — while the top-level `SELECT key, agg FROM t GROUP BY key` path does NOT regress. Verified by an in-PG A/B
(`benchmarks/columnar_composability_ab.py`) that runs all four previously-failing shapes byte-identical vs the heap
with the CustomScan engaged.

## Context

The columnar-aggregate `CustomScan` uses `scanrelid = 0` with `Aggref`s in its target lists (`columnar_agg.rs`,
`plan_custom_path`). It works top-level but fails when its output VALUE is consumed by an upper node, because a `Var`
referencing the synthetic relation-0 survives past `setrefs` into the upper node (blueprint root-cause analysis with
PG17 setrefs.c evidence). The blueprint (`knowledge-base/discoveries/blueprints/m115-columnar-composability-blueprint.md`)
established the Citus-exact fix (ADR-M115-1): a synthetic `RTE_VALUES` RTE + plain typed Vars in the plan node (no
Aggref). A prior naive `INDEX_VAR`-in-`plan.targetlist` attempt broke the top-level path (documented — setrefs must
build INDEX_VAR itself).

## Baseline Context (deep review of current state)

Git sha at plan time: `7be7d6c`.

### Files that will be touched

| File | Role today | Change |
|---|---|---|
| `theodb_rs/src/am/columnar_agg.rs` | `upper_paths_hook` adds the CustomPath; `plan_custom_path` builds the CustomScan with `scanrelid=0`, `plan.targetlist = tlist` (Aggrefs), `custom_scan_tlist = copy(tlist)` (Aggrefs) | Register a synthetic `RTE_VALUES` RTE; carry its RT index; set `scanrelid` = that index; build `custom_scan_tlist` + `plan.targetlist` as plain typed Vars (no Aggref) |
| `benchmarks/columnar_composability_ab.py` | (NEW) | The four failing shapes byte-identical vs heap + CustomScan; top-level regression check |
| `docs/benchmarks/m115-columnar-composability-verdict.{md,json}` | (NEW) | Measured verdict |

### Current callers / dependents

- `plan_custom_path` (`columnar_agg.rs`) — the Path→Plan lowering; its `custom_private` is read by `begin_custom_scan`
  (unchanged — the agg/group/pred blocks stay; a new scanrelid int is prepended-or-appended).
- `begin_custom_scan` / `exec_custom_scan` — fill the scan slot with computed Datums (UNCHANGED — the values are
  computed in exec regardless of the tlist shape).
- `upper_paths_hook` — where the synthetic RTE is registered on `root->parse->rtable`.

### Domain glossary

- **scanrelid=0** — synthetic CustomScan (no underlying relation); `custom_scan_tlist` describes the output tuple.
- **RTE_VALUES RTE** — a synthetic range-table entry (Citus pattern) making the output columns catalog-resolvable so
  upper nodes referencing them do not hit relation-0.
- **INDEX_VAR** — the varno setrefs uses for a scanrelid=0 CustomScan's tlist projecting the scan slot.

### Architecture boundaries affected

Planner integration only (`columnar_agg.rs` `upper_paths_hook` + `plan_custom_path`). No executor change (exec still
fills the scan slot). No new dependency.

## Prior Art & Related Work

- Internal blueprint: `knowledge-base/discoveries/blueprints/m115-columnar-composability-blueprint.md` (PG17 setrefs.c
  + Citus + TimescaleDB primary sources). The M114 verdict (`docs/benchmarks/m114-columnar-aggregate-verdict.md`)
  documented this exact limitation as M115's subject.
- External: Citus `distributed_planner.c` (RTE_VALUES + plain-Var custom_scan_tlist), TimescaleDB `vector_agg/plan.c`
  (post-planning swap — rejected), PG17 `setrefs.c`/`nodeCustom.c`/`plannodes.h`.

## Objective

Restructure the CustomScan plan node to the Citus-exact shape (synthetic RTE + plain typed Vars, no Aggref) so its
output resolves in upper nodes, with no top-level regression and byte-identical results.

## ADRs

### ADR-M115-1 — Citus-exact: synthetic RTE_VALUES + plain typed Vars (no Aggref in the plan node)

Register a synthetic `RTE_VALUES` RTE (one named column per output) in `upper_paths_hook`; `scan.scanrelid` = its RT
index; `custom_scan_tlist` + `plan.targetlist` = plain typed `Var`s built from `exprType/exprTypmod/exprCollation` of
the output tlist; values computed in exec (unchanged). **Alternatives rejected:** (a) TimescaleDB post-`set_plan_refs`
swap — needs a planner_hook post-pass, larger/riskier; (b) hand-built `INDEX_VAR` in `plan.targetlist` before setrefs
— PROVEN to break the top-level path (setrefs must build INDEX_VAR itself; pathkeys fail).

### ADR-M115-2 — No Aggref survives into the plan node's tlists

The aggregate values are computed in the exec callback; the plan node exposes output purely as typed Vars end-to-end,
so no Aggref can be inlined into an upper node by subquery pullup (the root cause). This is the honest cost of the
correct fix (a larger change than swapping one tlist).

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| RTE registration / rtoffset wrong under a nested subquery | HIGH | register on `root->parse->rtable` (standard); A/B includes a CustomScan under a SubqueryScan in a larger rtable | impl |
| `nodeCustom.c` scanrelid>0 + custom_scan_tlist interaction fills the wrong tupdesc | HIGH | pair real scanrelid with non-NULL `custom_scan_tlist` (plain Vars) so `ExecTypeFromTL` supplies the tupdesc; validate the scan slot on the droplet | impl |
| Agg-ORDER-BY pathkey item not found at create_plan | MEDIUM | pathtarget/pathkeys reference the output Var, not the Aggref; A/B covers `string_agg(... ORDER BY agg)` | impl |
| Top-level path regression | HIGH | A/B asserts top-level GROUP BY still CustomScan + byte-identical (the naive attempt's failure mode) | impl |

## Unresolved Questions

(none — the approach is fixed by the blueprint; the empirical setrefs/nodeCustom behavior is validated iteratively on
the droplet, which is the only place PG planner behavior is observable.)

## Failure scenarios

External engine: PG planner/executor (in-process). The fix is planner-structural; a wrong tlist shape manifests as a
planner error (`variable not found` / `pathkey item`) or the relation-0 exec error — both caught by the A/B running the
four shapes + top-level. If the correct shape cannot be achieved without a planner_hook post-pass, that is surfaced
honestly (do not ship a half-fix that silently declines composable queries).

## Dependency Graph

Phase 1 (RTE + plain-Var plan node) → Phase 2 (A/B validation of all shapes + top-level regression).

## Phase 1: Citus-exact plan node

### T1.1 — Register synthetic RTE + build plain-typed-Var custom_scan_tlist / plan.targetlist

#### Objective
In `upper_paths_hook`, register a synthetic `RTE_VALUES` RTE (named column per output), carry its RT index in
`custom_private`; in `plan_custom_path`, set `scan.scanrelid` = that index, build `custom_scan_tlist` and
`plan.targetlist` as plain typed Vars (no Aggref) from the output tlist's `exprType/exprTypmod/exprCollation`.

#### Why this step (action + reasoning)
Action: restructure the plan node to the Citus shape. Reasoning: the blueprint's setrefs.c analysis proves that an
Aggref (or relation-0 Var) surviving into an upper node is the root cause; the only fix that keeps the top-level path
working (setrefs builds INDEX_VAR itself) AND resolves upper references is a catalog-describable RTE + plain Vars
(ADR-M115-1). The exec callback already computes the values, so the plan node needs no Aggref.

#### Evidence
`plan_custom_path` (`columnar_agg.rs`), `upper_paths_hook`; blueprint setrefs.c:1665 / nodeCustom.c:75 / Citus
distributed_planner.c.

#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs`

#### Deep file dependency analysis
`begin_custom_scan`/`exec_custom_scan` unchanged (exec fills the slot). `custom_private` gains a `scanrelid` int; the
agg/group/pred blocks are unchanged. The synthetic RTE is added to `root->parse->rtable`.

#### TDD
- `test_composability_subquery_agg` — `SELECT sum(s) FROM (SELECT k, sum(x) s FROM col GROUP BY k) q` runs without
  the relation-0 error and equals the heap (asserted by the Phase 2 A/B; a `#[pg_test]` asserts no error + equality).
- `test_composability_toplevel_no_regression` — `SELECT k, sum(x) FROM col GROUP BY k` stays a CustomScan + correct.

#### Concurrency tests
(none — single-threaded planner/exec)

#### Acceptance Criteria
- No Aggref in `plan.targetlist` / `custom_scan_tlist`; a synthetic RTE is registered; `scanrelid` = its index; the
  four failing shapes execute without the relation-0 error; top-level unchanged.

#### DoD
- `cargo build` / `cargo pgrx install` clean on the droplet; the four shapes + top-level validated.

## Phase 2: Measurement (in-PG A/B)

### T2.1 — `benchmarks/columnar_composability_ab.py` — all shapes byte-identical + top-level regression

#### Objective
On a 1M-row `theodb_columnar` table vs a heap table, run the four previously-failing shapes (subquery-over-agg, join
on grouped output, `string_agg(... ORDER BY agg)`, scalar `s+1`) + one non-trivial-rtoffset case, asserting
byte-identical results with the CustomScan engaged; and assert the top-level GROUP BY path is unchanged
(byte-identical + CustomScan).

#### Why this step (action + reasoning)
Action: an in-PG A/B mirroring `columnar_groupby_ab.py`. Reasoning: Rule 5 — the fix is a correctness claim proven by
byte-identical vs the native plan across the failing shapes; the top-level regression check guards the naive attempt's
failure mode.

#### Evidence
Precedent: `benchmarks/columnar_groupby_ab.py`.

#### Files to edit
- `benchmarks/columnar_composability_ab.py` (NEW)
- `docs/benchmarks/m115-columnar-composability-verdict.{md,json}` (NEW)

#### TDD
- `test_composability_ab_byte_identical` — each shape's result equals the heap's; top-level unchanged.

#### Concurrency tests
(none — single-threaded benchmark)

#### Acceptance Criteria
- Every shape byte-identical + CustomScan; top-level byte-identical + CustomScan; no relation-0 error anywhere.

#### DoD
- Verdict written with measured evidence.

## Coverage Matrix

| Goal claim / requirement | Task(s) |
|---|---|
| Synthetic RTE + plain-Var plan node (no Aggref) — ADR-M115-1/2 | T1.1 |
| Subquery-over-agg / join / agg-ORDER-BY / scalar s+1 consumable, byte-identical | T1.1, T2.1 |
| No top-level regression | T1.1, T2.1 |
| CustomScan engaged for all shapes | T2.1 |

## Global Definition of Done

- `cargo build` / `cargo pgrx install` clean on the droplet.
- `benchmarks/columnar_composability_ab.py` reports byte-identical for the four failing shapes + top-level, CustomScan
  engaged, no relation-0 error.
- `docs/benchmarks/m115-columnar-composability-verdict.{md,json}` written with measured evidence.
- `CHANGELOG.md [Unreleased] § Added` / `§ Fixed` updated (Rule 6).
- File-size budget noted (columnar_agg.rs already > 700 LoC — no net growth beyond the fix; documented debt).
