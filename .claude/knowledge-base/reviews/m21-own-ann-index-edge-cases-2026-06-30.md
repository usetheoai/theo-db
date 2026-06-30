# Discover Edge Case Review — m21-own-ann-index

Date: 2026-06-30
Discovery plan analyzed: .claude/knowledge-base/discoveries/plans/m21-own-ann-index-plan.md
Research questions analyzed: 7
Edge cases found: 5 (MUST FIX: 1, SHOULD TEST: 2, DOCUMENT: 2)

## MUST FIX

### EC-1: Q4 vectorchord method targets the wrong file for the amhandler
- **Affected question:** Q4 (tools — AM registration)
- **Family:** Method
- **Scenario:** Q4's Fase A greps `vectorchord/src/index/mod.rs` + `vectorchord/src/index/opclass.rs` for `amhandler` / `CREATE ACCESS METHOD`. Verified: vectorchord's handler is NOT there — it lives at `vectorchord/src/index/vchordrq/am/mod.rs:983`-area (`_vchordrq_amhandler`) and `vectorchord/src/index/vchordg/am/mod.rs:111` (`_vchordg_amhandler`), with the SQL in `vectorchord/sql/install/vchord--1.1.0.sql:983`. The planned grep returns empty → Q4's vectorchord datapoint is BLOCKED for the wrong reason (false "not found").
- **Impact:** Q4 loses its second source (PhD-rigor R2 ≥2 sources); blueprint's AM-registration section thins to pgvectorscale-only.
- **Suggested fix:** Change Q4 Fase A vectorchord target to `vectorchord/src/index/vchordrq/am/mod.rs` + `vectorchord/sql/install/vchord--1.1.0.sql` (grep `amhandler|CREATE ACCESS METHOD|OPERATOR CLASS`); keep `vchordrq/opclass.rs` for the opclass binding.

## SHOULD TEST

### EC-2: Q1/Q2 scope-creep into WAL / parallel-build / vacuum
- **Affected question:** Q1 (HNSW), Q2 (IVFFlat)
- **Suggested halt-loop checkpoint:** `hnswbuild.c` carries 37 `GenericXLog`/parallel hits; before marking Q1/Q2 DONE, assert the answer covers the **in-memory algorithm + scan path only** and explicitly defers WAL logging / parallel build / vacuum to "implementation concern, noted not detailed" — a measurement-first M21 needs the algorithm + recall knobs, not the durability machinery (that is implementation-time, YAGNI for the blueprint).

### EC-3: "recall@k parity" is undefined as exact vs tolerance-band
- **Affected question:** Q5 (recall determinants)
- **Suggested halt-loop checkpoint:** before marking Q5 DONE, assert the blueprint states parity as a **tolerance band** — "recall@k within eps of pgvector at matched `hnsw_ef_search` / `ivfflat_probes`", reusing `theodb_bench/recall.py:61 recall_at_k`'s eps semantics — NOT bit-exact result-set identity. HNSW build is order-dependent (`hnsw.c:201` entry-point/layer math is non-deterministic across build orders), so demanding identical neighbor sets is physically wrong; the M20 ADR D3 tolerance lesson applies.

## DOCUMENT

### EC-4: pgvectorscale/vectorchord implement DiskANN/RaBitQ, NOT HNSW/IVFFlat
- **Accepted risk:** The two Rust references do not contain HNSW or IVFFlat — pgvectorscale is DiskANN+SBQ, vectorchord is RaBitQ (vchordrq) + graph-quant (vchordg). They are the source for the **Rust/pgrx AM scaffolding** (Q3 storage/hooks, Q4 registration, Q6 deps, Q7 tests) ONLY. pgvector (C) is the **sole** source for the HNSW/IVFFlat **algorithm** (Q1/Q2) and the recall knobs (Q5). The blueprint must not conflate "how pgvectorscale stores a graph" with "the HNSW algorithm". This split is already reflected in the question→reference mapping; documenting it here so `/discover-execute` does not borrow DiskANN graph details as if they were HNSW.

### EC-5: measurement-first — "keep pgvector" is a VALID blueprint outcome
- **Accepted risk:** Per M21 DoD (`ROADMAP-v2.md:124`) and ADR D3 of the plan, if the evidence suggests an own AM cannot reach recall parity within reasonable effort, the blueprint's Recommendation may legitimately propose **coexistence with pgvector index retained** (own AM as opt-in, gated, or deferred) — that is anti-sunk-cost, not failure. The blueprint must keep this as a live option in the coexistence-vs-substitution ADR, never force a "we will substitute" conclusion the evidence does not support.

## Summary

| Question | Edges found | MUST FIX | SHOULD TEST | DOCUMENT |
|----------|-------------|----------|-------------|----------|
| Q1 | 1 | 0 | 1 (EC-2) | 0 |
| Q2 | 1 | 0 | 1 (EC-2) | 0 |
| Q3 | 1 | 0 | 0 | 1 (EC-4) |
| Q4 | 1 | 1 (EC-1) | 0 | 0 |
| Q5 | 1 | 0 | 1 (EC-3) | 0 |
| Q6 | 0 | 0 | 0 | 0 |
| Q7 | 1 | 0 | 0 | 1 (EC-4) |
| (cross) | 1 | 0 | 0 | 1 (EC-5) |

**Verdict:** DISCOVERY PLAN NEEDS ADJUSTMENT (1 MUST FIX — Q4 vectorchord path; absorbed into plan v1.1)
