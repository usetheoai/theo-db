# Review — M161 expression routing (2026-07-27)

**Slug:** m161-expr-routing · **Milestone:** M161 · **Commit under review:** M161 branch on `develop` (post base_typoid cleanup + review fixes).
**Reviewers:** 3 council specialists reading the real code (index-storage, rust-pgrx, benchmark) + `/code-quality`.

**Verdict:** READY_TO_MERGE

0 BLOCKER, 0 HIGH remaining. Every BLOCKER/HIGH raised was fixed AND re-validated by measurement on ClickBench 1M (`docs/benchmarks/m161-artifacts/review-fixes-validation.txt`). LOW/INFO are documented-safe or cosmetic. `/code-quality` = FAIL_SOFT with HARD=0 (soft caps are environment-only: `auditor_unavailable_cargo-udeps` — needs nightly; the compiler `dead_code` lint is the real Rust guard and is clean). Per `code-quality-golden-rule.md § 1`, FAIL_SOFT with HARD=0 does not block.

## Severity matrix

| # | Severity | Reviewer | Finding | Resolution | Verified |
|---|---|---|---|---|---|
| 1 | BLOCKER | index-storage | `col ± k` used the COLUMN type as `out_typoid`, not the operator result type (PG widens `int2±int4→int4`, `int4±int8→int8`) → `i16::try_from` error / wrong type OID | `out_typoid = (*op).opresulttype`; int8 result declined (fail-closed) | `AdvEngineID+5` routes with result type `integer` + `diverged=0`; `UserID+5`/`CounterID+3e9` decline |
| 2 | HIGH | rust-pgrx | Temporal/date columns leaked through the `minmax_kind_of` "integer-class" gate (timestamp→I8, date→I4) → admitted-then-errored | both gates test true integer OIDs `{20,21,23}` | `EventTime IN (…)`, `EventDate IN (…)`, `EventDate+1` all decline to native |
| 3 | HIGH | benchmark | Coverage `35/43` planner-GUC state undisclosed (could be `enable_sort=off`-only) | Method § split: coverage + routing proof are DEFAULT-GUC; `enable_sort=off` is only the A/B full-set-oracle mechanism. Artifacts committed | `routing-proof-default-gucs.txt`: q40/q35/q18 Custom Scan under default GUCs |
| 4 | MEDIUM | index-storage | `int8 ± int8` i64 compute overflow not PG-22003-equivalent | int8 result declined at admit (covered by #1 fix) | `UserID+5` declines |
| 5 | MEDIUM | benchmark | Custom-Scan proof shown for only 1/3; artifacts not archived | all-3 EXPLAINs in `routing-proof-default-gucs.txt`; `m161-artifacts/` committed (coverage + routing + A/B + fixes) | artifacts present |
| 6 | LOW/INFO | all | extract deparse Var type/attno divergence (safe, name-only); `deconstruct_array` detoast (safe for const-folded literals); stale `func 3 Const` comments | documented / comments cleaned | n/a |

## What the reviewers verified as correct (no change needed)

- Channel encode/decode symmetry (6-leaf group-expr, hi/lo i64 incl. negatives), M115 swap count for mixed keys, extract epoch-invariance {minute,hour} (10957-day offset = whole hours/minutes), IN-list `in_list(lits,false)` = PG `ScalarArrayOpExpr useOr`, directory min/max fast-path correctly disabled for IN-list, no new MVCC/pending surface, no panic-across-C (all fallible steps `Option`/`Result<_,String>` → single `pg_sys::error!` sink).

## Hard gates (cycle-review)

- Failing tests on branch: none (A/B green; build clean).
- New secrets: none.
- Direct commit to `main`: no (work on `develop`).
- Co-Authored-By trailer: none.
- CHANGELOG updated: yes (`[Unreleased]` M161 entry).

## Downstream

Proceed to `/release` v0.152.0 (self-merged develop→main PR) + flip M161 checkbox.
