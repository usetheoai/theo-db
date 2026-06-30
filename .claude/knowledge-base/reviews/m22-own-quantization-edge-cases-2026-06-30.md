# Discover Edge Case Review — m22-own-quantization

Date: 2026-06-30
Discovery plan analyzed: .claude/knowledge-base/discoveries/plans/m22-own-quantization-plan.md
Research questions analyzed: 7
Edge cases found: 4 (MUST FIX: 1, SHOULD TEST: 1, DOCUMENT: 2)

The plan is well-formed (paths validated, 4 corners, 4 ADRs, budget + stop conditions). Findings below are the
ones not yet foreseen for `/discover-execute`.

## MUST FIX

### EC-1: Q6 license lookup misses workspace-inherited license
- **Affected question:** Q6 (deps — licenses)
- **Family:** Method
- **Scenario:** Verified: `vectorchord/crates/rabitq/Cargo.toml` uses `version.workspace = true` (and almost
  certainly `license.workspace = true`). Q6's Fase A greps the CRATE manifest for `license` → returns nothing →
  the license would be wrongly recorded as "unknown".
- **Impact:** false "unknown license" for the RaBitQ datapoint; the D1 AGPL gate can't be evaluated.
- **Suggested fix:** Q6 Fase A also greps the WORKSPACE root `vectorchord/Cargo.toml` `[workspace.package]`
  `license` (and pgvectorscale's workspace root if applicable).

## SHOULD TEST

### EC-2: Q3 assumes a rerank pass that may not exist as a named symbol in the SBQ module
- **Affected question:** Q3 (recall-vs-memory tradeoff / rerank)
- **Suggested halt-loop checkpoint:** before marking Q3 DONE, assert the blueprint states **honestly whether
  pgvectorscale SBQ does a full-precision rerank** (Fase A found `num_neighbors` knobs but no `rerank` symbol in
  `sbq/*.rs`) — if rerank is absent/elsewhere, say so and treat "own quantizer + optional rerank" as a TheoDB
  design choice for M22, not a borrowed fact. Do not fabricate a rerank path that isn't in the source.

## DOCUMENT

### EC-3: memory metric for the SQL-callable form is a computed formula, not `pg_relation_size`
- **Accepted risk:** `theodb_bench/db.py:131 index_size_bytes` uses `pg_relation_size` — valid only for a real
  on-disk index (M22b). The M22 SQL-callable gate measures **bytes/vector** via the quantized-size formula
  (f32 = 4·dim vs SBQ = ceil(dim·bits/8)) computed in-process. Document in Q4/Q7 that the memory gate compares
  the computed bytes/vector (own vs pgvectorscale SBQ formula at matched bits/dim), not `index_size_bytes`.

### EC-4: "retain pgvectorscale" is a VALID outcome (anti-sunk-cost)
- **Accepted risk:** already encoded in ADR D4. Per M22 DoD (`ROADMAP-v2.md:135`), if own quantization cannot
  reach recall parity at a comparable memory profile, the blueprint may legitimately recommend **retain
  pgvectorscale** — the milestone delivers the measurement, not a regression. The recommendation must keep this
  option live, never force substitution.

## Summary

| Question | MUST FIX | SHOULD TEST | DOCUMENT |
|----------|----------|-------------|----------|
| Q3 | 0 | 1 (EC-2) | 0 |
| Q4 | 0 | 0 | 1 (EC-3) |
| Q6 | 1 (EC-1) | 0 | 0 |
| Q7 | 0 | 0 | (EC-3 shared) |
| (cross) | 0 | 0 | 1 (EC-4) |

**Verdict:** DISCOVERY PLAN NEEDS ADJUSTMENT (1 MUST FIX — Q6 workspace license; absorbed into plan v1.1 + checkpoints)
