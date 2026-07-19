# M115 Blueprint — columnar-aggregate CustomScan (scanrelid=0) output consumable in subqueries/joins

Deep-research (Staff PG-internals) blueprint. Primary sources: PG17 setrefs.c/plannodes.h/nodeCustom.c/lsyscache.c,
Citus distributed_planner.c, TimescaleDB vector_agg/plan.c.

## Coverage Corner 1 — Integration Tests
Regression tests for ALL four failing shapes (must be byte-identical vs heap + CustomScan engaged): (a)
`SELECT sum(s) FROM (SELECT k, sum(x) s FROM col GROUP BY k) q`; (b) JOIN on the grouped output; (c)
`string_agg(... ORDER BY agg_value)`; (d) scalar `SELECT s+1 FROM (SELECT sum(x) s FROM col) q`; plus one
non-trivial-rtoffset case (CustomScan under a SubqueryScan in the middle of a larger rtable). PLUS: top-level path must
NOT regress.

## Coverage Corner 2 — Dependencies
No new dependency. pgrx 0.19 `pg_sys::{makeVar, makeTargetEntry, makeAlias, RangeTblEntry, RTEKind, INDEX_VAR,
exprType, exprTypmod, exprCollation, lappend, list_length}`.

## Coverage Corner 3 — Tools
pgrx 0.19 / PG 17.10, droplet c-8 for in-PG validation (setrefs behavior only observable in a real PG).

## Coverage Corner 4 — Techniques (the load-bearing research)

### Confirmed root cause (setrefs.c evidence)
A raw `Var` referencing the synthetic `scanrelid=0` relation SURVIVES past `set_customscan_references` into an upper
node, then hits catalog type-resolution with `relid=0` → `ERROR: cache lookup failed for attribute N of relation 0`
(`lsyscache.c:943/1052`). Mechanism: (1) `set_customscan_references` (`setrefs.c:1665`, scanrelid==0 branch)
INDEX_VAR-ifies `plan.targetlist` against `build_tlist_index(custom_scan_tlist)` — top-level works. (2) On subquery
pullup, the outer holds a copy of the inner `Aggref`; `set_upper_references` builds the parent itlist from our
(now INDEX_VAR) `plan.targetlist`; `fix_upper_expr` recurses into the pulled-up `Aggref.args` → the inner `Var` fails
`search_indexed_tlist_for_var` → escapes unfixed → relation-0 lookup. The scalar `s+1` case is identical.

### Why the naive INDEX_VAR-in-plan.targetlist attempt broke top-level
`setrefs` MUST see the REAL expressions in `plan.targetlist` (it builds INDEX_VAR itself); pathkeys expect the real
sortable exprs at create_plan time. Pre-INDEX_VAR-ing → "variable not found in subplan target list" / "could not find
pathkey item to sort".

### The canonical pattern (real extensions)
- **Citus** (`distributed_planner.c`): `custom_scan_tlist` = plain typed `Var`s via `makeVarFromTargetEntry` (NO
  Aggref); `plan.targetlist` = `makeVarFromTargetEntry(INDEX_VAR, tle)`; registers a real `RTE_VALUES` RTE
  (`eref = makeAlias("remote_scan", colnames)`) so columns are catalog-resolvable.
- **TimescaleDB** (`vector_agg/plan.c`): inserts the node POST-planning (after `set_plan_refs`) by swapping an
  existing `Agg` — not available to us without a planner_hook post-pass.
- **Common invariant:** nothing an upper node will re-fix may be an Aggref or a relation-0 Var.

## ADRs
- **ADR-M115-1 (Option A — Citus-exact, chosen):** register a synthetic `RTE_VALUES` RTE (one named column per output)
  in `upper_paths_hook`, carry its RT index; in `plan_custom_path` set `scan.scanrelid` = that index (NOT 0),
  `custom_scan_tlist` = plain typed `Var`s (so `ExecTypeFromTL` builds the scan tupdesc), `plan.targetlist` = plain
  typed Vars (NO Aggref survives). Values computed in the exec callback (unchanged — fill the scan slot). Upper nodes
  resolve against a catalog-describable RTE → subquery/join/agg-ORDER-BY work; top-level unaffected (setrefs takes the
  standard scanrelid>0 branch). **Alternative rejected:** TimescaleDB post-planning swap (needs a planner_hook
  post-pass — bigger, riskier); the naive INDEX_VAR-before-setrefs (broke top-level — proven).
- **ADR-M115-2:** no Aggref in the plan node's tlists — the agg values are already computed in exec; the plan node
  exposes output purely as typed Vars end-to-end (the honest cost the researcher flagged).

## Honest risks
1. RTE registration timing + rtoffset: add the RTE to `root->parse->rtable` in upper_paths_hook (standard place);
   test with non-trivial rtoffset.
2. `nodeCustom.c:75-82`: with `scanrelid>0` AND `custom_scan_tlist != NIL`, the executor uses
   `ExecTypeFromTL(custom_scan_tlist)` + `tlistvarno=INDEX_VAR` — so `plan.targetlist` projects INDEX_VAR against the
   scan slot. Final shape: real scanrelid RTE + `custom_scan_tlist` plain typed Vars + `plan.targetlist` plain Vars
   (no Aggref). Validate the scan slot is still filled correctly by exec.
3. Agg-ORDER-BY pathkeys: ensure the CustomPath pathtarget/pathkeys reference the output Var not the Aggref, so
   `create_plan` finds the pathkey item. Cover with `string_agg(... ORDER BY agg)`.

## Evidence citations
setrefs.c:1665 (scanrelid==0 INDEX_VAR fix) · plannodes.h:685-756 (custom_scan_tlist contract) · nodeCustom.c:75-95
(ExecTypeFromTL/INDEX_VAR) · lsyscache.c:943/1052 (relation-0 error) · Citus distributed_planner.c
makeCustomScanTargetlistFromExistingTargetList(~1668)/makeTargetListFromCustomScanList(~1718)/RemoteScanRangeTableEntry(~2008)
· TimescaleDB vector_agg/plan.c:52-68/619-711 · our columnar_agg.rs:383 (upper_paths_hook)/plan_custom_path.
