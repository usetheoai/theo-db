# Discover-Confidence — m7-bm25-permissive

**Date:** 2026-06-28
**Verdict:** SHIPPABLE_WITH_CAVEATS (89)
**Blueprint:** .claude/knowledge-base/discoveries/blueprints/m7-bm25-permissive-blueprint.md

## Scores
- research_coverage 100 (4/4) · reference_citations 100 (0 fabricated, 8 local + web verified) · blueprint_completeness 100 · structural_risk 95
- Soft cap: soft_floor_citation_density_low (advisory)

## Key finding (the M7-S2 DoD)
Permissive BM25 alternative IDENTIFIED: **timescale/pg_textsearch** — PostgreSQL License (verbatim from canonical repo), GA v1.3.1 (2026-06-23), true Okapi BM25 (k1=1.2/b=0.75), Block-Max WAND. VectorChord-bm25 = dual AGPLv3/Elastic → BARRED (D1). psql_bm25s (Apache-2.0) = permissive fallback. Native ts_rank/ts_rank_cd confirmed NOT BM25 (cover-density). PG exposes BM25 inputs natively (ts_stat.ndoc + length(tsvector)).

## Recommendation (measurement-first, D3/ADR0002)
Identify pg_textsearch as the permissive alternative (satisfies DoD); gate its distribution-integration on a reproducible recall@k benchmark vs the shipped ts_rank_cd; keep ts_rank_cd+RRF interim; reject SQL-owned BM25 (Rule-9 reinvention of an existing permissive extension).

## Honesty markers
UNVERIFIED: AlloyDB exact-page quote (redirect off-allowlist), pg_bestmatch.rs license. UNBENCHMARKED: BM25-vs-ts_rank_cd recall gain (the gate), SQL-owned perf, psql_bm25s QPS.
