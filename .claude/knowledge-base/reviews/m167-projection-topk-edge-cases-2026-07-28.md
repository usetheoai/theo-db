# Edge Case Review — m167-projection-topk

Date: 2026-07-28
Tasks analyzed: 3 (T1.1, T2.1, T3.1)
Cases found: 6 (EDGE: 2, NEGATIVE: 4 | MUST FIX: 3, SHOULD TEST: 2, DOCUMENT: 1)

## Framing

This plan reads as a one-line change (`new(false)` → `new(true)`), and the ADRs are careful. But the
change's real nature is different: **it promotes a code path that has only ever run opt-in into the
default for every `ORDER BY <col> LIMIT k` on a columnar table.** Every shape that reaches
`try_swap_topk` becomes user-facing at once. The guards inside that function are genuinely thorough
(OFFSET, non-Const LIMIT, `LIMIT ALL`, k ≤ 0, k > `i32::MAX`, `numCols != 1`, computed target exprs,
system/whole-row cols, unsupported types, bpchar, non-btree ordering ops, the C/POSIX sort-collation
guard reading `sort.collations[0]` rather than `varcollid`) — the *admission* logic is not where the
risk is. The risk is in the two things the guards do not cover: **resource consumption** and **what the
verification oracles actually compare**.

All three MUST FIX items were verified by reading the code, not inferred from the plan.

## MUST FIX

### EC-1: default-ON makes the documented O(N)-decode-memory trade-off the default
- **Affected task:** T1.1
- **Kind:** NEGATIVE (resource exhaustion under a valid query)
- **Family:** Resource
- **Scenario:** `run_columnar_topk` calls `decode_to_batch` for `{projection ∪ sort key ∪ filter cols}`
  over **all N rows** before DataFusion's bounded-heap TopK ever runs (`df_executor.rs:775`). With no
  `WHERE`, the qual loop in `try_swap_topk` (`columnar_agg.rs:1958–1966`) iterates zero times, `zpreds`
  and `tpreds` stay empty, and the candidate is admitted — so an ordinary
  `SELECT * FROM hits ORDER BY EventTime LIMIT 10` decodes the entire table into one Arrow batch.
  `df_executor.rs:576–581` states this explicitly and justifies it thus: *"the top-k path is therefore
  O(N) memory in the decoded batch — unlike the native top-N heapsort's O(k) — documented as an M158
  trade-off; **gated behind `theodb.enable_columnar_late_mat`, default OFF**."* T1.1 removes precisely
  the mitigation that comment cites. The memory pool is then *sized to fit* the batch
  (`pool_bytes = max(work_mem, batch_bytes*2) + 64 MB`, `:583`), so DataFusion never rejects it — the
  failure is a real backend allocation failure, not a graceful `Resources exhausted`.
- **Impact:** backend OOM on a wide/unfiltered top-k. At the 100M-row scale this project already loads
  (`m162-100m-load-gotchas`), a `SELECT *` batch is tens of GB. An OOM-killed backend takes the
  postmaster into crash recovery and drops every connection — strictly worse than the slow-but-correct
  native plan it replaced. `plan_rows` is set to `k` (`:1990`), so nothing upstream sees the cost.
- **Suggested fix:** in `try_swap_topk`, decline when the estimated decode dwarfs `work_mem` —
  ```rust
  // O(N) decode memory (df_executor.rs:576) — native top-N is O(k); decline when the batch would dwarf work_mem.
  let est_bytes = (*child).plan_rows * (*child).plan_width.max(1) as f64;
  if est_bytes > (pg_sys::work_mem as f64) * 1024.0 * 8.0 { return None; }
  ```
- **Note on ADR-1:** this does **not** contradict ADR-1. That ADR rejected a `plan_rows` **cost** gate,
  and rejected it on *performance* grounds ("the win holds at every measured N"). This is a *resource
  safety* bound on a different axis, on which ADR-1 is silent. If the plan prefers to keep ADR-1
  literally intact, the alternative is to record the memory ceiling as an accepted risk in
  `## Drawbacks & Risks` — but it cannot stay unmentioned, which is its current state.

### EC-2: the "43/43 A/B" no-regression gate does not exercise the top-k path at all
- **Affected task:** T1.1
- **Kind:** NEGATIVE (invalid output undetected by its own oracle)
- **Family:** Boundary (verification boundary)
- **Scenario:** T1.1's acceptance criterion pairs two numbers from **two different executions**.
  `run_m128_clickbench.py:283` strips the trailing LIMIT before the A/B:
  `ab_sql = re.sub(r"\s+LIMIT\s+\d+\s*;?\s*$", "", ...)`. `try_swap_topk` requires the Sort's parent to
  be a `T_Limit` (`columnar_agg.rs:1822–1825`) — with the LIMIT stripped there is no Limit node, the
  swap declines, and the A/B compares *native-on-columnar vs native-on-heap*. On top of that,
  `_canonical` (`:243–246`) **sorts** both sides, so ordering is normalized away even when a LIMIT
  survives. So `result_ab_identical == true` for q23/q24 is true of a query that never routes.
- **Impact:** the plan's headline no-regression evidence — repeated in the Coverage Matrix ("No
  regression (43/43 A/B) → T1.1") and in the Global DoD — is **structurally incapable** of detecting a
  wrong top-k. The only real correctness gate left for the newly-default path would be T2.1's tiny
  synthetic fixture; the 1M/100M scale where the path actually ships would go unverified. This is the
  same blindness ADR-3 correctly identifies for `columnar_type_ab.py`, not carried across to the
  ClickBench oracle.
- **Suggested fix:** state in T1.1 that the 43/43 ClickBench A/B is LIMIT-stripped (hence a *storage*
  oracle, not a top-k oracle), and make the top-k correctness gate explicit: run T2.1's
  `topk_ab_check` against the 1M `hits` / `hits_heap` pair as an acceptance criterion of T1.1.

### EC-3: `EXPLAIN VERBOSE` on the 105-column `hits` — the M131 deparse hazard, now on by default
- **Affected task:** T1.1
- **Kind:** NEGATIVE (unkillable hang during plan printing)
- **Family:** Format
- **Scenario:** T1.1's TDD and acceptance criteria specify plain `EXPLAIN` only. This repo has a
  documented precedent for exactly this node family: `run_m128_clickbench.py:336–341` records that on
  the real 105-col `hits`, `EXPLAIN` of a query with `ORDER BY <aggregate>` *"recursed forever in
  ruleutils' `resolve_special_varno` deparse … `statement_timeout` could not interrupt it because it
  happened during plan PRINTING"* (fixed in M131, #135). M166 re-verified `EXPLAIN VERBOSE` for its new
  node for this reason. The top-k node builds a `custom_scan_tlist` of fresh base Vars with
  `scanrelid = 0` (`columnar_agg.rs:1874–1893`, `:2003`) — the same deparse surface — and, because the
  GUC defaulted OFF and *the harness never set it*, this node has plausibly never been
  `EXPLAIN VERBOSE`d on the wide table.
- **Impact:** any user (or any tool that auto-EXPLAINs) running `EXPLAIN VERBOSE` on a routed q23/q24
  shape could hit a hang that `statement_timeout` cannot cancel. Availability, not just cosmetics.
- **Suggested fix:** change T1.1's TDD/acceptance from `EXPLAIN` to `EXPLAIN (VERBOSE, COSTS OFF)` on
  q23 **and** q24 against the real 105-column `hits`, asserting it returns.

## SHOULD TEST

### EC-4: the top-k A/B compares sort keys only — blind to key↔payload misalignment
- **Affected task:** T2.1
- **Kind:** NEGATIVE
- **Scenario:** ADR-3 compares the sort-key **multiset**, which correctly neutralizes tie-row
  nondeterminism. But late materialization's signature defect is precisely that the payload
  materialized for the k survivors gets associated with the wrong key — `run_columnar_topk` re-locates
  output columns **by name** in the post-sort batch (`df_executor.rs:790–800`), so a schema/index
  mismatch there yields correct keys with wrong payload rows. A key-multiset oracle passes that
  unchanged. T2.1 is the task whose stated purpose is "locks the contract"; as designed it locks the
  ordering contract but not the materialization contract.
- **Suggested test:** `test_topk_unique_key_full_row_identical` — add a fourth case whose sort key is
  **unique** in the fixture (no ties ⇒ full-row comparison is deterministic) and compare **whole rows**,
  not just the key. Assert byte-identical rows between columnar and heap arms.

### EC-5: `topk_int_order` is vacuous if k ≥ fixture row count
- **Affected task:** T2.1
- **Kind:** EDGE (boundary of a valid input)
- **Scenario:** `columnar_type_ab.py` builds a small synthetic fixture — the catalog cross-product, one
  row per (col-index) (`:89`, `:95–110`). If the chosen `LIMIT k` is ≥ the loaded row count, the LIMIT
  never cuts, the top-k degenerates to "return everything", and the case passes while proving nothing
  about the k-boundary. The row count is data-dependent and can drift as columns are added.
- **Suggested test:** `test_topk_case_k_is_strictly_less_than_fixture_rows` — assert in the pure-logic
  tier that the declared k for `topk_int_order` is `< nrows`, so the case fails loudly if the fixture
  ever grows past it rather than silently going vacuous.

## DOCUMENT

### EC-6: default-ON is validated only in the serial plan shape
- **Kind:** EDGE
- **Accepted risk:** `run_m128_clickbench.py:344` sets `max_parallel_workers_per_gather = 0`, so the
  T1.1 no-regression run never plans a parallel shape, while a real default-ON user has parallelism
  enabled. The new node copies `parallel_safe` from the Sort it replaces and sets
  `parallel_aware = false` (`columnar_agg.rs:1988–1989`) — the identical pattern the shipped agg node
  has used since M110 (`:1730–1731`), so this is not novel mechanism. Acceptable to note rather than
  test now; worth one line in `## Drawbacks & Risks` so the gap is a decision, not an oversight.

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|------|------|----------|----------|-------------|----------|
| T1.1 | 1 | 3 | 3 | 0 | 1 |
| T2.1 | 1 | 1 | 0 | 2 | 0 |
| T3.1 | 0 | 0 | 0 | 0 | 0 |

**Coverage check:** T1.1 and T2.1 each have both lenses considered. T3.1 is a documentation/verdict
task with no input boundary of its own — the EDGE/NEGATIVE lenses do not apply; its correctness is
inherited from T1.1/T2.1. Noted rather than padded.

**Verdict:** PLAN NEEDS ADJUSTMENT

Three MUST FIX items, none of which requires new abstraction: EC-1 is a 3-line guard (or an explicit
accepted-risk row), EC-2 and EC-3 are corrections to T1.1's acceptance criteria. EC-2 is the most
consequential — it is not a missing edge case but a claim in the Global DoD that the cited oracle
cannot support, and `/plan-confidence` will not catch it (M2 is structural; it does not read harness
source).
