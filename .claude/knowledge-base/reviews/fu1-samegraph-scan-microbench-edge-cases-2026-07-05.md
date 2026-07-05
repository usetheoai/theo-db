# Discover Edge Case Review — fu1-samegraph-scan-microbench

Date: 2026-07-05
Discovery plan analyzed: .claude/knowledge-base/discoveries/plans/fu1-samegraph-scan-microbench-plan.md
Research questions analyzed: 8
Edge cases found: 4 (MUST FIX: 1, SHOULD TEST: 2, DOCUMENT: 1)

## MUST FIX

### EC-1: The discovery researches the REFERENCES but never validates OUR extraction feasibility
- **Affected question:** Q2, Q5
- **Family:** Interpretation / Coverage
- **Scenario:** During `/discover-execute` the questions answer "how pgvectorscale/vectorchord isolate their
  search for benching", but the load-bearing unknown for FU-1 is whether **our** `traverse`
  (`theodb_rs/src/am/hnsw_page.rs`) can have its ground-loop separated from `pg_sys`. The ground loop
  interleaves `neighbors_into(rel,…)` and `load(rel,…)` — and `load` computes the SIMD distance on the pinned
  page bytes (`decode_element` + `score`). If the distance scoring cannot be lifted out of the pinned-page scope,
  the extraction shape (what the `NeighborSource` trait must expose: neighbor addrs only, or addrs + vector
  bytes) changes materially. A blueprint that specs the seam without checking our coupling could be undeliverable.
- **Impact:** The blueprint's central design (the `NeighborSource` DIP seam) would be specified against the
  references' shape, not ours — risking a mid-implement pivot (the exact re-work the goal forbids).
- **Suggested fix:** Add to Q2 and Q5 an explicit method line grepping our own `hnsw_page.rs::traverse` +
  `load` + `neighbors_into` to confirm what the trait must expose (neighbor addrs vs addrs+vector-bytes) and
  that the ground-loop allocation can be parameterized over an in-memory `NeighborSource` that also serves
  vectors. The blueprint's `Techniques` corner MUST name our extraction shape, not just the references'.

## SHOULD TEST

### EC-2: The no-I/O caveat could overstate the production benefit
- **Affected question:** Q3, Q8
- **Suggested halt-loop checkpoint:** Before marking the blueprint's verdict section DONE, assert it quantifies
  or bounds the I/O-amortization caveat — a micro-bench with no page I/O magnifies the allocation share, so the
  reported criterion delta is an UPPER bound on the production QPS benefit, not the production number itself.
  The blueprint must state this explicitly (honesty, `public-copy.md`) and recommend the criterion delta be
  read as "the allocation cost the change removes", paired with a note that production benefit is I/O-amortized.

### EC-3: The fixed-graph fixture must be recall-representative, not degenerate
- **Affected question:** Q3
- **Suggested halt-loop checkpoint:** Before marking Q3 DONE, confirm the blueprint specs a fixture at a scale
  where the M46 effect exists (ef≥200 candidate structures large enough that pre-sizing matters — i.e. a graph
  with enough nodes that `ef*m0` is non-trivial). A 30-node toy graph (like the pg_test) would show ~zero delta.
  The fixture N must be in the regime where rehashing/allocation is a measurable share (the blueprint should
  justify the N it picks against `ef*m0`).

## DOCUMENT

### EC-4: DIP-infeasible fallback = pgvectorscale's copy-plus-equivalence-test pattern
- **Accepted risk:** If EC-1's check finds the ground-loop genuinely cannot be extracted without a large refactor
  (e.g. the SIMD distance is inseparable from the pinned-page scope in a way that forces the bench to duplicate
  logic), the documented fallback is pgvectorscale's own pattern (`benches/lsr.rs` re-implements
  `ListSearchResult` from `src/access_method/graph/mod.rs`) — a bench copy GUARDED by an equivalence test
  (Q7: the benched candidate structure must produce the same visit order as production, reusing M46's
  recall-neutral oracle). This is a known, acceptable SOTA fallback — not the preferred path (divergence risk),
  but a real one. The blueprint may adopt it only WITH the equivalence guard; a naked copy is rejected.

## Summary

| Question | Edges found | MUST FIX | SHOULD TEST | DOCUMENT |
|----------|-------------|----------|-------------|----------|
| Q1 | 0 | 0 | 0 | 0 |
| Q2 | 1 | 1 | 0 | 0 |
| Q3 | 2 | 0 | 2 | 0 |
| Q4 | 0 | 0 | 0 | 0 |
| Q5 | 1 | 1 (shared w/ Q2) | 0 | 0 |
| Q6 | 0 | 0 | 0 | 0 |
| Q7 | 1 | 0 | 0 | 1 |
| Q8 | 1 | 0 | 1 (shared w/ Q3) | 0 |

**Verdict:** DISCOVERY PLAN NEEDS ADJUSTMENT (absorb EC-1 into Q2/Q5 methods + the Techniques deliverable; add
EC-2/EC-3 as halt-loop checkpoints; EC-4 as an ADR fallback).
