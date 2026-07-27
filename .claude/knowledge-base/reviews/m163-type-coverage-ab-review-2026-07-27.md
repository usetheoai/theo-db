# Review — M163 type-coverage A/B + float GROUP BY decline

**Date:** 2026-07-27 · **Slug:** m163-type-coverage-ab · **Commits:** 43c9add, 97597e4 (branch `develop`).
**Verdict: READY_TO_MERGE** (no BLOCKER; the one HIGH raised was fixed and re-validated live).

## Reviewers (domain specialists, fresh eyes on the real code)

### council-rust-pgrx — the float-decline guard (`columnar_agg.rs`)
Verdict: **CORRECT, COMPLETE, SAFE.** No BLOCKER/HIGH/MEDIUM.
- `matches!((*var).vartype, FLOAT4OID | FLOAT8OID)` covers exactly the two float OIDs in the admitted set
  (`arrow_supported_group_type` = `21|23|20|700|701|16|25|1043|1114|1184|1082`); `numeric`/arrays decline one line earlier.
- No alternate leak: the FuncExpr branch requires a timestamp base; the OpExpr branch is gated to true-integer OIDs; a
  cast around a float is not a bare `Var` → final `else` declines. `classify_target_node` is the sole group-key gate.
- `(*var)` deref is safe — `var` is proven non-null + `T_Var` before the guard; no UB across the C boundary.
- Declining is the correct fail-closed remedy (mirrors M154 float `COUNT(DISTINCT)`); no correctness lost (a float group
  key is byte-identical only when the column has neither −0.0/+0.0 co-occurrence nor NaN — unprovable at plan time).
- LOW (accepted): stylistic `matches!` vs M154's `==` — not worth a rebuild. INFO: min/max-float *aggregate* NaN is a
  separate surface (M105), not this GROUP-BY guard.

### council-benchmark — the harness (`columnar_type_ab.py`) + verdict honesty
Verdict after fixes: **sound and honest.**
- **HIGH (fixed):** routing was EXPLAIN-ed on the bare query but divergence measured on a CTE/set-op-wrapped query — a
  wrapper barrier could stop the pushdown from firing inside the comparison, silently comparing native-on-columnar vs
  native-on-heap. **Fix:** `ab_check` now takes routing evidence from `EXPLAIN ANALYZE CREATE TEMP TABLE _ab_on AS <sql>`
  — the same execution that materializes the compared data. Re-validated live: every route case routes AND diverged=0 on
  the materialized-arm comparison, empirically confirming the pushdown fires in the executed query.
- **MEDIUM (fixed):** rot-guard now matches `\bcol\b` word-boundaries (substring was trivially true for 1-char columns).
- **LOWs (addressed/documented):** tz ≥2 distinct instants; int8 INT_MIN edge; text-collation-default limitation stated;
  `plan_routes` presence-anywhere caveat commented; int4 INT_MIN honestly omitted (the `c4-1` route case would underflow).
- Confirmed sound: positive control flows through `ab_check`; a silently-declined route case fails loudly; `EXCEPT ALL`
  is the correct multiset operator; the `-0.0` catalog edge is a real sign-bit; 20/20 matches with no arithmetic spin.

## Gate summary
- **code-quality:** FAIL_SOFT with HARD=0 — caps are 100% environmental (`cargo-udeps` cannot compile pgrx in this env;
  `symbol_fab_unverifiable` has no network). No real dead code or fabrication. Does not block per golden-rule § 1 (env caps).
- **Evidence:** `docs/benchmarks/m163-type-coverage-verdict.md` — 20/20 cases as-expected, positive control diverged=2, exit 0.
- **CHANGELOG:** `[Unreleased]` has the Added (harness) + Fixed (float GROUP BY) entries.

Handoff: proceed to `/release` (v0.154.0), self-merge, flip M163 checkbox.
