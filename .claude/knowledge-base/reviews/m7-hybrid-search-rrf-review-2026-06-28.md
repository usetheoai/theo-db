# Review — M7-S1 Hybrid Search (FTS + vector + RRF)

**Slug:** m7-hybrid-search-rrf
**Date:** 2026-06-28
**Verdict:** READY_TO_MERGE (after fixes)
**Plan:** `.claude/knowledge-base/plans/m7-hybrid-search-rrf-plan.md` (SHIPPABLE_WITH_CAVEATS 84.8)
**Code-quality:** `.claude/knowledge-base/audits/m7-hybrid-search-rrf-code-quality-2026-06-28.md` (PASS)
**Commits:** `5f7acb1` (T1.1) · `1c2e095` (T2/T3) · `d00e330` (review fixes)

## Process

5 specialist agents in parallel (architecture · test-auditor · security · cross-validation · SQL/RRF correctness).
Initial tally: architecture READY; the other 4 NEEDS_FIXES (2 HIGH + several MEDIUM). All addressed and
re-verified live.

## Findings & resolution

| # | Sev | Finding | Resolution | Verify |
|---|---|---|---|---|
| 1 | HIGH | FTS CTE had no `ORDER BY` before `LIMIT` → arbitrary subset of FTS matches kept when matches > `per_leg_limit` (latent relevance bug; CI missed it because corpus < per_leg_limit) | Added `ORDER BY ts_rank_cd(...) DESC` before `LIMIT` in the fts CTE (`sql/40:80`) | smoke + 26 integration green |
| 2 | HIGH | Plan ACs + failure-scenario claimed an endpoint-skip test/path that did not exist | Shipped behavior is a typed error at the SQL layer when `theodb.embedding_endpoint` is unset (vector leg calls `theodb.embed` → `22023`); added `test_hybrid_unconfigured_endpoint_raises_typed_error`; reconciled plan ACs to the real behavior (Rule 3) | `pytest -k endpoint` green |
| 3 | MEDIUM | `plainto_tsquery($2)` omitted `'english'` → stemming mismatch vs the indexed column + the Python FTS leg (D1 drift) | Pinned `plainto_tsquery('english', $2)` in `@@`, `ts_rank_cd`, and the new ORDER BY | integration green |
| 4 | MEDIUM | SQL fused `ORDER BY score DESC` only; Python twin `rrf_fuse` breaks ties `id ASC` → divergent ordering under tied scores (D2 parity) | Added `, id ASC` to the SQL ORDER BY; added `test_rrf_fuse_tie_break_is_id_asc` | unit + integration green |
| 5 | MEDIUM | `ai.hybrid_search_rrf` granted to PUBLIC while `theodb.embed` is REVOKEd → inconsistent privilege; latent SSRF if ever made SECURITY DEFINER | `REVOKE ALL ON FUNCTION ... FROM PUBLIC` + SECURITY-INVOKER note (`sql/40`) | function loads; tests (superuser) green |
| 6 | MEDIUM | Negative-case lens absent at unit layer (typed-error guards untested) | Added `test_rrf_fuse_rejects_nonpositive_k`, `test_ndcg_at_k_rejects_nonpositive_k`, `test_recall_at_n_rejects_nonpositive_n` | unit green |
| 7 | MEDIUM | `test_three_retrievers` could pass vacuously for the hybrid leg | Assert `results["hybrid"]["ndcg10"] > 0` AND `recall100 > 0` directly | integration green |
| 8 | MEDIUM | SQL boundary validation: only `k=0` tested (3 of 4 branches uncovered) | Added invalid `per_leg_limit`, invalid `result_limit`, both-query-args-NULL tests | integration green |
| 9 | LOW | RRF tie-break determinism unproven; hash golden unpinned; both-legs-empty + nDCG truncation untested | Added tie-break, `hash_token` golden (`787029587`), both-legs-empty, nDCG-truncation tests | unit + integration green |
| 10 | LOW | RUM escape-hatch documentation thin | Covered in the spec note + report; GIN-default is implemented; RUM remains a documented opt-in | n/a |

DRY note (architecture MEDIUM): the Python `rrf_fuse` is a test-only twin; the production hybrid path uses
the SQL function exclusively (`db.hybrid_rrf_docs`), so there is one runtime fusion source of truth (D2).
Both now share the same tie-break (`id ASC`), removing the divergence risk.

## Hard gates (cycle-review)

| Gate | Status |
|---|---|
| Tests passing on branch | PASS — 69 unit + 26 integration |
| No secrets committed | PASS — staged `sk-proj` matches = 0; `.env` gitignored |
| No direct commit to `main` | PASS — develop |
| No authorship trailer (user policy) | PASS — none |
| CHANGELOG updated | PASS — `[Unreleased]` M7-S1 |
| No unbenchmarked perf claim | PASS — report states hybrid TIES vector on the synthetic fixture; real-win deferred out-of-CI |

## Verdict

READY_TO_MERGE. Both HIGH findings fixed and re-verified live; all MEDIUM/LOW addressed. The slice ships
`ai.hybrid_search_rrf` (correct, injection-safe, least-privilege) + a BEIR-style recall eval with measured
numbers, honest about the synthetic-fixture tie. Caveat carried forward (not blocking): the decision-grade
real-embedding-model hybrid-win eval is an out-of-CI follow-up (the M7-S1 report documents this); BM25
permissive is M7-S2.
