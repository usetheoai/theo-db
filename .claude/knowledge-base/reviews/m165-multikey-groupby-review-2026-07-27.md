# Review — M165 GROUP BY multi-chave (q34 const-out + q17 honest-negative)

**Date:** 2026-07-27 · **Slug:** m165-multikey-groupby · **Commit:** 873d53c (branch develop).
**Verdict: READY_TO_MERGE** (council-rust-pgrx sign-off; no BLOCKER/HIGH; one pre-existing MEDIUM as a backlog note).

## Discovery correction (avoided re-work)

The milestone premise ("multi-key GROUP BY") was FALSE — multi-key GROUP BY already routes byte-identical (q16 proof,
`docs/benchmarks/m153-groupby-text.md:19`). The two blockers were different:
- **q34**: an unhandled `T_Const` output column (`SELECT 1`). Fixed (const-out arm).
- **q17**: a correct M153 honest-negative (text GROUP BY under AGG_SORTED with a non-Sort parent). Kept as honest-negative.

## council-rust-pgrx review of the applied diff (commit 873d53c)

**Verdict: READY_TO_MERGE — the code is sound.** Traced all six flagged paths on the real committed code:

- **Negative int8 hi/lo round-trip (primary suspect): CORRECT.** Same split-pack pattern as M161 IN-list / M157 delta
  (both signed). Hand-traced `-1` and `INT64_MIN`: `lo as u32 as i64` zero-extends the low word, `(hi as i64)<<32`
  sign-extends — sign-agnostic by construction, so the positive-only type-coverage test exercised the same path.
- **`deparse_safe_tlist` kind=3 `copyObjectImpl`: CORRECT.** Deep-copies the `Const` (descriptor byte-identical),
  `makeTargetEntry` preserves resname/resjunk/resno; a bare `Const` is untouched by `setrefs.c` — position/type
  alignment held; no dangling/double-free.
- **int2/int8 boundary, backward-compat `if i < n` decode, empty `const_outs`: CORRECT** (no truncation, no off-by-one).
- **Arity/numCols: CORRECT and fail-closed** — const counts toward `layout`/`out_arity` but not `group_cols`/`numCols`;
  any mismatch declines rather than mis-emits (bonus: if PG ever failed to eliminate the const group key, it declines).
- **No new panic-across-C surface** (`?`/`ok_or`, no unwrap).

**MEDIUM (pre-existing since M157, widened marginally by M165 — backlog, NOT a merge blocker):** the admission↔Agg-node
binding (`columnar_agg.rs:1538-1543`) matches on `(table_oid, groupkeycount, arity)`, not layout composition. Two grouped
columnar-agg subqueries over the same table with colliding `(groupkeycount, arity)` could bind the wrong admission →
silent wrong result. ClickBench is single-agg-per-statement so the A/B is blind to it. Fix: fold a layout-shape
discriminant (hash of layout kinds) into the stash match key. Tracked as a follow-up.

## Empirical validation (droplet, 1M ClickBench + synthetic type-coverage)

- **q34**: `EXPLAIN` = `Custom Scan (theodb_columnar_agg)`; A/B symmetric-EXCEPT `diverged=0` (byte-identical).
- **Type-coverage A/B: 26/26** as-expected, positive control diverged=2 — const_out int2/int4/int8 route (diverged=0);
  const float/text/NULL decline (fail-closed). The M164 §5.1 gate.
- **q17**: under `enable_sort=on` (benchmark condition) PG plans `GroupAggregate → Sort`, no Custom Scan → declines
  (honest-negative, correct).

## Gates

- `/code-quality`: FAIL_SOFT HARD=0 (env-only cargo-udeps/symbol_fab; no real defect).
- `/review` (council-rust-pgrx): READY_TO_MERGE.
- Benchmark: ClickBench --agg run in progress for q34's ratio + no-regression → verdict doc.

Handoff: `/release` (v0.156.0), self-merge, flip M165. File the MEDIUM as a follow-up backlog issue.
