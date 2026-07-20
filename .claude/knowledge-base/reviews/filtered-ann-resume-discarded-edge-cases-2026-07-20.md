# Edge Case Review — M118 filtered-ann-resume-discarded

Date: 2026-07-20
Tasks analyzed: 4 (T1.1, T2.1, T2.2, T3.1)
Cases found: 6 (EDGE: 3, NEGATIVE: 3 | MUST FIX: 2, SHOULD TEST: 3, DOCUMENT: 1)

## MUST FIX

### EC-1: Frontier exhaustion before `max_scan_tuples`
- **Affected task:** T1.1 / T2.1
- **Kind:** NEGATIVE (a termination boundary — the live matches run out before the tuple cap)
- **Family:** State / Timing
- **Scenario:** A highly selective `WHERE` matches fewer live rows than `k`; the resumable frontier empties (every reachable node visited) before `max_scan_tuples` distinct TIDs are emitted. If `next_batch` does not signal "exhausted", the scan loop keeps pulling an empty frontier — a spin or a re-search fallback that defeats the whole point.
- **Impact:** Infinite pull / wasted CPU on the exact selective case M118 targets.
- **Suggested fix:** `next_batch` sets `state.exhausted = true` and returns `[]` when the frontier empties; `amgettuple` treats exhausted-frontier as end-of-scan (no re-search, no re-arm). 3 lines.

### EC-2: `theodb.hnsw_resume_max_mb = 0` semantics undefined
- **Affected task:** T2.2
- **Kind:** NEGATIVE (invalid/edge GUC value)
- **Family:** Input / Format
- **Scenario:** The plan adds the ceiling GUC but does not define `0`. Every sibling GUC (`max_scan_tuples`, `vacuum_fold_max_mb`) uses `0 = disabled`. If left undefined, `0` could mean "fail-safe immediately" (never resume) — silently reverting M118 to re-search-equivalent.
- **Impact:** A misconfigured/zero GUC silently kills the optimization or OOMs, with no clear contract.
- **Suggested fix:** Define `0 = disabled (unbounded, legacy re-search-free resume with no cap)`, consistent with the other GUCs; document in the GUC registration comment. 1 sentence.

## SHOULD TEST

### EC-3: Single-node / `ef=1` resume boundary
- **Affected task:** T1.1
- **Kind:** EDGE (smallest valid graph/breadth)
- **Suggested test:** `test_resume_single_node_index_ef1` — a 1-node index with `ef=1` resumes cleanly (first batch returns the node, second batch returns `[]` exhausted). Assert correct result at the boundary (the single node once, then end).

### EC-4: `max_scan_tuples = 0` disarms resume (regression guard)
- **Affected task:** T2.1
- **Kind:** EDGE (the disarm boundary)
- **Suggested test:** `test_resume_disarmed_when_max_scan_tuples_zero` — with `max_scan_tuples=0`, iterative/resume is NOT armed and the scan returns at most `ef_search` (pre-M52 behavior). Assert byte-identical to the non-iterative path.

### EC-5: Memory-ceiling overflow returns a typed result, no panic across C
- **Affected task:** T2.2
- **Kind:** NEGATIVE (resource exhaustion)
- **Suggested test:** `test_resume_ceiling_overflow_no_panic` — with a tiny `hnsw_resume_max_mb`, a large selective query stops resuming and returns the held candidates; assert **no panic crosses the C boundary** (pgrx: a controlled stop, not an `unwrap`/`error!` mid-heap). Assert the emitted count is bounded, not a crash.

## DOCUMENT

### EC-6: Graph stability of the retained frontier during a scan
- **Kind:** NEGATIVE (concurrent mutation)
- **Accepted risk:** The retained frontier holds node-ids across `amgettuple` calls. A concurrent VACUUM compaction fold takes the advisory **EXCLUSIVE** lock (`am/lock.rs:24`), which waits for the scan's **SHARE** lock — so the page-native graph cannot be compacted out from under an in-flight scan. Concurrent tombstones are filtered by the scan + the executor's MVCC heap recheck. No extra work needed; the existing lock discipline (M26) already makes the retained frontier's node-ids stable for the scan's lifetime. Note this in T2.1 so a reviewer does not re-flag it.

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|------|------|----------|----------|-------------|----------|
| T1.1 | 2 | 1 | 1 | 1 | 0 |
| T2.1 | 1 | 1 | 1 (shared EC-1) | 1 | 1 |
| T2.2 | 0 | 2 | 1 | 1 | 0 |
| T3.1 | 1 | 0 | 0 | 0 | 0 |

**Coverage check:** every task touching an input/state boundary has ≥1 EDGE and ≥1 NEGATIVE considered. T3.1 (benchmark) is measurement — its "boundary" is the recall parity gate (EDGE), already in the plan's honest-HALT criterion.

**Verdict:** PLAN NEEDS ADJUSTMENT — absorb EC-1 and EC-2 (MUST FIX) as sub-steps; add EC-3/4/5 to the relevant tasks' TDD; add the EC-6 note to T2.1.
