# Discover-Confidence — m7-hybrid-search-rrf

**Date:** 2026-06-28
**Verdict:** SHIPPABLE_WITH_CAVEATS (89)
**Blueprint:** .claude/knowledge-base/discoveries/blueprints/m7-hybrid-search-rrf-blueprint.md

## Scores
- research_coverage: 100 (4/4 corners)
- reference_citations: 100 (0 fabricated — 13 local + 3 allowlisted web verified)
- blueprint_completeness: 100 (all mandatory sections + 3 ADRs)
- structural_risk: 95
- Soft cap: soft_floor_citation_density_low (advisory; below 1.0/200w)

## Frontier rigor (R1/R2/R3)
- R1 SOTA anchor: AlloyDB `ai.hybrid_search` cited per technique.
- R2 ≥2 primary sources: Cormack 2009 (RRF, k=60) + BEIR (arxiv 2104.08663) + field witnesses (paradedb hybrid.rs, supabase FTS).
- R3 benchmark-or-UNBENCHMARKED: 1 honest UNBENCHMARKED marker — no hybrid recall measured on TheoDB yet; M2 harness is pure-vector (E4 gap → HIGH-priority M7-S1 task).

## Key decisions carried into planning
- D1: PG-native FTS (GIN + ts_rank) default; RUM as escape hatch.
- D2: RRF k=60 default (exposed as param); manual-SQL MVP first, `ai.hybrid_search` native fn as thin wrapper (KISS).
- D3: borrow RRF technique from the paper; paradedb (AGPL) is field witness only — never vendored (D1).

## Caveat for downstream
The recall-win claim is UNBENCHMARKED — M7-S1 implementation MUST extend the recall harness to a BEIR-style hybrid eval before any performance claim (CLAUDE.md rule 5 / public-copy.md).
