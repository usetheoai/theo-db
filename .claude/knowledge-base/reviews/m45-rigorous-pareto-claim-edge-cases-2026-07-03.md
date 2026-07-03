# Edge Case Review — m45-rigorous-pareto-claim

Date: 2026-07-03
Tasks analyzed: 3 (T1.1 pure logic, T2.1 driver, T3.1 real run)
Cases found: 6 (EDGE: 3, NEGATIVE: 3 | MUST FIX: 1, SHOULD TEST: 3, DOCUMENT: 2)

## MUST FIX

### EC-1: interpolation produces a claim from a non-overlapping frontier
- **Affected task:** T1.1
- **Kind:** NEGATIVE (invalid — no shared recall range)
- **Family:** State
- **Scenario:** if the two frontiers' recall ranges do not overlap (e.g. theodb 0.94–0.99, pgvector 0.86–0.92), a naive `pareto_margin_verdict` could interpolate outside range and emit a fabricated SUPERIOR.
- **Impact:** a false superiority claim — the worst outcome for this milestone (dishonest).
- **Suggested fix:** `pareto_margin_verdict` MUST compute shared levels only within `max(min_recall)` and `min(max_recall)` of the two frontiers; if empty → `verdict=PARITY, reason="no recall overlap"`. Already specified in the plan (Q2 + Deep Dives) — the RED test `test_verdict_parity_when_no_recall_overlap` is the guard. **CONFIRMED covered** — keep it as a hard RED test, not optional.

## SHOULD TEST

### EC-2: single-point frontier (only one ef survived / degenerate build)
- **Affected task:** T1.1
- **Kind:** EDGE (extreme valid — a frontier of length 1)
- **Suggested test:** `test_interpolate_single_point_frontier_only_covers_its_recall` — a 1-point frontier returns that qps only at its exact recall, `None` elsewhere (no crash, no extrapolation).

### EC-3: zero-latency / division guard in QPS
- **Affected task:** T2.1
- **Kind:** NEGATIVE (invalid — mean latency 0 would divide-by-zero into qps)
- **Suggested test:** `test_qps_guards_zero_latency` — if a timed pass reports 0 mean latency (clock granularity), the driver clamps to a tiny epsilon or records the raw latency, never raising ZeroDivisionError. (Mitigation: measure enough queries that mean latency > 0; assert nq ≥ 1.)

### EC-4: identical recall on two adjacent ef points (flat frontier segment)
- **Affected task:** T1.1
- **Kind:** EDGE (boundary — r1==r0)
- **Suggested test:** already in plan (`test_interpolate_equal_recall_no_div_by_zero`). Keep — confirms no ZeroDivisionError on a flat segment.

## DOCUMENT

### EC-5: theodb query-sample cap vs pgvector full sample
- **Kind:** NEGATIVE (measurement bias risk)
- **Accepted risk:** the plan mitigates by using the SAME `queries[:nq]` subset for BOTH indexes at each ef (Deep Dives T2.1). Documented; nq recorded in artifact. No separate test needed — it is a harness invariant, asserted by the structure test measuring both on the identical set.

### EC-6: scale downshift (1M intractable → subsample)
- **Kind:** EDGE (extreme valid — largest tractable N)
- **Accepted risk:** T3.1 allows a documented large subsample if full 1M is impractical; the scale is recorded in the artifact and the M42 scale caveat preserved. Honest, not hidden.

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|------|------|----------|----------|-------------|----------|
| T1.1 | 2 | 1 | 1 | 2 | 0 |
| T2.1 | 0 | 1 | 0 | 1 | 1 |
| T3.1 | 1 | 0 | 0 | 0 | 1 |

**Coverage check:** every task touching an input boundary has both an EDGE and a NEGATIVE case considered. The MUST-FIX (EC-1) is already covered by a planned RED test; EC-2/EC-3 are added as SHOULD-TEST RED tests to T1.1/T2.1.

**Verdict:** PLAN OK — one MUST-FIX confirmed already covered; two SHOULD-TEST tests to add to the TDD lists (EC-2, EC-3). Absorbing into plan v1.1.
