# M115 — columnar-aggregate CustomScan composability: verdict

**Date:** 2026-07-19 · **Module:** `theodb_rs/src/am/columnar_agg.rs` (Agg-swap: `upper_paths_hook` stashes `admit`,
`planner_hook` swaps a normal `Agg` → our `CustomScan` post-`set_plan_refs`), `am/df_executor.rs` (ascending group-key
sort of the swapped grouped result). Own-code; blueprint:
`knowledge-base/discoveries/blueprints/m115-columnar-composability-blueprint.md`. Plan: `m115-columnar-composability`.

**What is measured** (DigitalOcean **c-8** dedicated, PG 17.10 + pgrx 0.19.0): a 1M-row `theodb_columnar` table vs an
identical heap table. The four shapes that used to fail with `cache lookup failed for attribute N of relation 0`
(consuming a columnar-aggregate output VALUE inside an enclosing expression) are run byte-identical vs the heap with
the columnar CustomScan engaged; a JOIN on the grouped output; and a top-level regression check. Reproduce:
`benchmarks/columnar_composability_ab.py`. Raw: `m115-columnar-composability-verdict.json`.

**Goal:** the columnar-aggregate output is byte-identical AND consumable in subqueries/joins/aggregate-ORDER-BY, with
no top-level regression. **Result: MET.**

## The fix (Agg-swap — TimescaleDB pattern)

The bug was a pre-existing M100 limitation: the columnar aggregate was a `scanrelid=0` CustomScan carrying `Aggref`s in
its tlists; subquery pullup inlined an `Aggref` into an upper node, leaving a `Var` pointing at "relation 0". Three
targeted CustomPath-level fixes hit a fundamental tension (real exprs → pathkeys OK but Aggref leaks; plain Vars →
pathkeys break) — proven empirically. The fix is a **rearchitecture** (blueprint ADR-M115-1): `admit` now STASHES its
result at `upper_paths_hook` (no CustomPath added), `standard_planner` builds a **normal `Agg`** (whose output the
parent references as plain Vars — no Aggref leaks), and a `planner_hook` **swaps that Agg → our CustomScan**
post-`set_plan_refs`. The swapped node's tlist is plain typed Vars (no Aggref), so nothing is re-fixed and nothing
leaks. The swapped grouped result is ascending-sorted by the group keys (Rust-side, over the small grouped output) to
reproduce a `GroupAgg`'s output-order guarantee (a `SORTED` text-key GroupAgg is left native — collation-order safety).

## Result — composability (the four previously-failing shapes) + join

| shape | byte-identical vs heap | CustomScan engaged |
|---|:---:|:---:|
| `sum(s) FROM (SELECT k, sum(x) s … GROUP BY k)` (subquery over grouped) | ✅ | YES |
| `string_agg(s ORDER BY s) FROM (grouped)` (aggregate + ORDER BY the agg value) | ✅ | YES |
| `s+1 FROM (SELECT sum(x) s …)` (scalar over subquery) | ✅ | YES |
| `count(*) FROM (grouped)` | ✅ | (native; result correct) |
| JOIN on the grouped output (matched groups) | ✅ 100/100 | — |

All previously raised `cache lookup failed for attribute N of relation 0` (crashing even EXPLAIN); now byte-identical.

## Result — NO regression

| suite | outcome |
|---|---|
| Top-level `GROUP BY k [ORDER BY k]` | CustomScan engaged, byte-identical (order preserved) |
| M114 aggregate A/B (`columnar_aggregate_ab.py`) | avg(float8) 9.53×, sum(int4) 11.43×, sum(int2) 12.69×, **GROUP BY+WHERE 5.89×** — all CustomScan + byte-identical; declines native + correct |
| GROUP BY A/B (`columnar_groupby_ab.py`) | int 6.06×, multi-key 4.57×, temporal(date) 9.87×, agg-before-key 6.19× — all CustomScan + byte-identical |

## Verdict (honest)

- **GOAL MET.** The columnar-aggregate output is now consumable in subqueries, joins, and aggregate-ORDER-BY —
  byte-identical to the native plan — closing the pre-existing M100 composability limitation documented by M114. The
  Agg-swap rearchitecture introduces **no regression** to M114 (aggregate breadth), the GROUP BY pushdown, or the
  top-level path (all re-validated columnar + byte-identical).
- The fix is the TimescaleDB `vector_agg` pattern (swap a planned `Agg`), reached after three empirically-disproven
  CustomPath-level attempts — the honest, non-workaround path (blueprint + memory `m115-composability-blocked`).

## Review fixes (pre-merge review found + fixed 4 blockers/highs)

A pre-merge review (council-rust-pgrx) caught real correctness gaps the first evidence pass missed (it had disabled
parallelism); all are fixed + re-validated:

- **B1 — partial-aggregate double-swap under a parallel plan:** `try_swap_agg` now declines any non-`AGGSPLIT_SIMPLE`
  Agg (a `Finalize`/`Partial` split), so a parallel plan never swaps a partial transvalue. Validated: forced-parallel
  `sum(x)` and `sum(s) FROM (grouped)` byte-identical (`M115_PARALLEL`). In practice the `theodb_columnar` TAM does not
  parallelize, so the columnar CustomScan still engages under default parallelism (no perf regression).
- **B2 — hardcoded ASC sort vs `ORDER BY … DESC`:** a `SORTED` GroupAgg is admitted ONLY when its input Sort is
  exactly ASC nulls-last (checked via `get_ordering_op_properties`); DESC / nulls-first / text → native. Validated:
  `GROUP BY k ORDER BY k DESC` byte-identical (`M115_DESC`).
- **B3 — same-OID stash cross-match:** the swap now matches a stash entry by base-table OID AND shape (group-key count
  + output arity), so a scalar Agg cannot bind a grouped `Admitted` (or vice-versa).
- **H1 — stash not restored on a planner longjmp:** a `Drop`-based `StashGuard` restores the enclosing run's stash even
  when `standard_planner` `ereport`s (pgrx converts the longjmp to an unwind).

## Caveats (honest)

- A `GroupAgg` (`AGG_SORTED`) over a **text** group key is left to the native plan (PG's collation order differs from
  the byte-wise sort — a safety decline, not a defect; `HashAgg` text keys still swap). `MIXED` (grouping sets) is out
  of scope.
- The swap matches a stashed `admit` to the planned Agg by the base table's OID (first unconsumed) — robust for the
  admitted single-table aggregate shapes; unusual multi-aggregate-same-table queries fall back to native if the match
  is ambiguous.
