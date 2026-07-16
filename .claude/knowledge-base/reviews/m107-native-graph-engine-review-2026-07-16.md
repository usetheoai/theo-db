---
slug: m107-native-graph-engine
milestone_id: M107
date: 2026-07-16
cycle: review
---

# /review — M107 native graph engine Phase 0 (D3 gate)

**Verdict:** READY_TO_MERGE (2 HIGH transparency fixes applied)

Independent adversarial review (council-benchmark) — the reviewer regenerated the exact deterministic graph, reloaded it into the same live PG 17.10, and re-measured three baseline formulations + a stronger set-hash oracle. Core claim held under attack.

## Verified
- **Not a strawman:** baseline is indexed + ANALYZEd; the reviewer built an "even-fairer" UNION-of-two-directed-scans and measured it ~152ms — the OR-join is not the dominant cost, so splitting it doesn't narrow the gap. Native wins vs the best fair SQL.
- **Oracle semantically sound:** native BFS reachable-set == CTE reachable-set (set-identity confirmed via an independent `bit_xor(hashint8)` re-check, not just count+sum).
- **Build-cost caveat foregrounded, not buried:** the MD verdict leads with the end-to-end number, caveat #1 is CSR-build-dominates → ADR-0048 makes "persist the CSR" binding. The weak number drives the architecture.
- **GO honest, not inflated:** even the modest end-to-end + persist-CSR path is a real, honestly-framed win.

## Findings — all FIXED
- **[HIGH-1]** The `UNION`-dedup fairness variant was cited (~170×) but only ran as prose, not in the harness → **FIXED:** `run_bench.py` now runs BOTH `UNION ALL` and `UNION`-dedup per trial with mean±std JSON rows; oracle PASS on both across all 8 trials. Real numbers: native traverse 106–232× faster than the dedup baseline (was a prose spot-check, now reproducible).
- **[HIGH-2]** The spike was called "the retriever workload"; it measures only the reachable-set `reach` CTE (not the chunk-scoring tail) → **FIXED:** added a § Scope stating plainly it isolates the reachable-set expansion (dominant CTE cost); the real retriever does MORE SQL, so the spike is conservative.
- **[MEDIUM-1]** count+sum oracle not injective → **FIXED:** noted the limitation + set-identity re-check + Phase-1 must use a set-hash.
- **[MEDIUM-2]** materiality shrinks in sparse/low-hub regime → **FIXED:** caveat #2 states the ratio is robust but absolute win is single-digit-ms for sparse graphs (the honest-negative boundary).
- **[LOW-2]** build-ms std loose (4 trials) → noted as caveat #5; advisory for the Phase-1 build milestone (8–10 trials).

## Hard gates
✅ no BLOCKER · ✅ no secrets · ✅ no main commit · ✅ commit-trailer policy honored · ✅ CHANGELOG updated · benchmark reproducible + honest.

**READY_TO_MERGE.** The GO is honest, the oracle sound, the baselines fair and reproducible. This GO authorizes the native-graph-engine follow-on milestones.
