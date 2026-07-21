# Blueprint — #135 root cause: columnar-agg CustomScan EXPLAIN deparse infinite recursion (NOT a planner hang)

> Discover executed 2026-07-21 via LIVE gdb backtrace of the hung backend on the droplet (measurement-first,
> discover-first per the M131 DoD). Feeds M131. The #135 hypothesis ("O(cols²) planner loop on wide mixed-type
> tables") is FALSIFIED by the backtrace.

## Context

M131 fixes #135. #135 reported an "18-minute uninterruptible planner hang" on `theodb.enable_columnar_agg=on` over
the wide ClickBench `hits` (105 col, 27 TEXT), and hypothesized an O(cols²) loop in the CustomScan path/cost
creation, killable only by a server restart (statement_timeout doesn't fire during planning). The suggested fix was
"profile the planner cost hook + a width/type guard".

## Empirical root cause (falsifies the #135 hypothesis)

Reproduced on the current build (`.so` Jul 20; unchanged M115 Agg-swap code): running EXPLAIN over all 43 ClickBench
queries with `enable_columnar_agg=on`, **exactly 2 hang** — **Q16** (`SELECT UserID, COUNT(*) FROM hits GROUP BY
UserID ORDER BY COUNT(*) DESC LIMIT 10`) and **Q33** (`… GROUP BY WatchID, ClientIP ORDER BY c DESC`). A plain
`GROUP BY userid` (no ORDER BY) plans in **27 ms**; Q16 **executes** (no EXPLAIN) in **0.537 s** with correct results.

**gdb backtrace of the hung Q16 backend (3 samples, identical):**

```
#0  check_stack_depth / get_tle_by_resno   (parse_relation.c)
#1  resolve_special_varno                  (ruleutils.c:7699)
#2  resolve_special_varno                  (ruleutils.c:7674)   ← recursing into itself
#3  get_variable                           (ruleutils.c:7430)
#4  deparse_expression_pretty              (ruleutils.c:3648)
#6  show_sort_group_keys ("Sort Key")      (explain.c:2794)
#7  show_sort_keys
#8  ExplainNode → ExplainPrintPlan → ExplainOnePlan → EXPLAIN
```

**Conclusion:** the hang is **NOT in the planner and NOT in execution** — it is **EXPLAIN plan-printing** (deparse of
the Sort node's sort keys). It is an **EXPLAIN-only, ORDER-BY-aggregate-triggered infinite recursion**, unrelated to
table width or TEXT columns (that correlation was coincidental — the ClickBench queries that ORDER BY an aggregate
happen to run on `hits`).

## The defect (exact locus)

`theodb_rs/src/am/columnar_agg.rs::try_swap_agg`:
- L641 `plan.targetlist = plain_var_tlist(tlist)` — `Var(INDEX_VAR, resno)` (correct: the executed output).
- **L658 `cscan.custom_scan_tlist = plain_var_tlist(tlist)`** — ALSO `Var(INDEX_VAR, resno)` ← **the bug**.

For a `scanrelid=0` CustomScan, ruleutils resolves an upper Sort key's Var by walking
`plan.targetlist[resno]`→`INDEX_VAR`→`custom_scan_tlist[resno]`. Our `custom_scan_tlist[resno].expr` is
`Var(INDEX_VAR, resno)` → `resolve_special_varno` follows it back into `custom_scan_tlist[resno]` → the same
INDEX_VAR → **infinite recursion**. `resolve_special_varno` terminates only when the resolved expr is a
**non-special-varno** expression (a real-varno Var → callback, or a non-Var expr it can deparse).

Why it slipped through: the M115 Agg-swap inserts the CustomScan **post-`set_plan_refs`**, so `set_customscan_references`
never processes `custom_scan_tlist` (which is why a self-referential INDEX_VAR never crashed setrefs). The ONLY
consumer of `custom_scan_tlist` for this post-setrefs node is EXPLAIN deparse — and only when a Sort/ORDER-BY above
the CustomScan references the aggregate output.

## Coverage Corner 1 — Integration tests

The regression test must be a **wide-table GROUP BY with ORDER BY on the aggregate under EXPLAIN** (the exact trigger),
plus real execution correctness. Both Q16 (single-key + ORDER BY agg + LIMIT) and Q33 (multi-key + ORDER BY agg)
shapes. The existing `benchmarks/run_m128_clickbench.py` already provides the 43-query A/B oracle — re-run with agg ON.

## Coverage Corner 2 — Dependencies

None new. Fix is in existing Rust (`columnar_agg.rs`) using `pg_sys` (pgrx) only.

## Coverage Corner 3 — Tools

gdb (backtrace of the hung backend — the decisive tool), perf (available). `cargo pgrx install` to rebuild the
extension on the droplet. The M128 ClickBench harness for the accelerated A/B.

## Coverage Corner 4 — Techniques

**The fix**: build a **self-contained, deparse-safe `custom_scan_tlist`** where each entry's expr is a NON-special
expression so `resolve_special_varno` terminates:
- **Group-key columns** → a plain `Var(base_rel_varno, group_attno)` (real varno = the original `scanrelid`; the RTE
  is still in `rtable` even though the child scan PLAN node was dropped). ruleutils deparses the column name.
- **Aggregate columns** → the `Aggref` (deparses as `count(*)`, `sum(col)`, …). count(*) has no args; for
  sum/avg/min/max the single arg Var must be a base-rel Var (real varno), NOT the post-setrefs `OUTER_VAR` (which
  references the dropped child → would re-introduce a crash). Rebuild the arg Var from the admission metadata
  (`adm.aggs[i].attno` + the base-rel varno).

`plan.targetlist` stays `plain_var_tlist` (INDEX_VAR) — the executed output is unchanged (M115 invariant preserved:
no Aggref in the executed tlist). Only `custom_scan_tlist` (deparse metadata) changes.

**Prior art:** PostgreSQL `setrefs.c::set_customscan_references` + `ruleutils.c::resolve_special_varno` (the contract
that `custom_scan_tlist` entries describe the output columns as real expressions); Citus / TimescaleDB grouped
CustomScan set `custom_scan_tlist` to real base-rel Vars + Aggrefs for exactly this reason.

## ADRs

### ADR-1 — the fix is `custom_scan_tlist` content, NOT a planner-latency guard

**Decision:** fix the self-referential `custom_scan_tlist` (build real base-rel Vars + Aggrefs). Do NOT add the
"planner-latency width/type guard" the #135 issue suggested — the backtrace proves there is no planner-cost pathology
to guard against (the cost is copied from the standard_planner Agg, L646-647; planning is cheap).

**Rationale (Rule 3 honesty):** guarding a planner hang that does not exist would be cargo-cult defense. The real
defense-in-depth is the regression test (EXPLAIN of ORDER-BY-aggregate over the swapped CustomScan) + the byte-identical
accelerated ClickBench A/B.

**Alternatives rejected:** (a) width/type planner guard — REJECTED (guards a non-existent pathology; the hang is in
EXPLAIN deparse). (b) `custom_scan_tlist = tlist` (the post-setrefs Agg tlist) — REJECTED (its group-key Vars are
`OUTER_VAR` referencing the dropped child → deparse would deref a null lefttree). (c) disabling the swap when an
ORDER BY references the aggregate — REJECTED (loses the acceleration on exactly the ClickBench queries that need it).

## Verdict

Root cause is **precisely** identified with a live backtrace. The fix is a localized change to `custom_scan_tlist`
construction in `try_swap_agg`. The #135 "planner hang / O(cols²) / wide-table" framing is falsified — record that
honestly in the fix + close #135 with the real cause. DoD item "EXPLAIN GROUP BY … agg on plans in <1s" already
holds for non-ORDER-BY queries; the fix extends it to the ORDER-BY-aggregate shapes (Q16/Q33).
