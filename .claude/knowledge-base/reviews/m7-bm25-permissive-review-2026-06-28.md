# Review — M7-S2 Permissive BM25 (identify · prove · measure)

**Slug:** m7-bm25-permissive
**Date:** 2026-06-28
**Verdict:** READY_TO_MERGE (after fixes)
**Plan:** `.claude/knowledge-base/plans/m7-bm25-permissive-plan.md` (SHIPPABLE 96.4)
**Discovery:** `.claude/knowledge-base/discoveries/blueprints/m7-bm25-permissive-blueprint.md` (SHIPPABLE_WITH_CAVEATS 89)
**Code-quality:** `.claude/knowledge-base/audits/m7-bm25-permissive-code-quality-2026-06-28.md` (PASS)
**Commits:** `bd0f9a7` (T1.1 ADR+sweep) · `9b3d53c` (T2/T3 image+measure+CI) · `7a738c3` (review fixes)

## Process

5 specialist agents in parallel (license/security · measurement-correctness · cross-validation · architecture
· — discovery already gated separately). Tally: license + architecture + cross-validation = READY_TO_MERGE;
measurement-correctness = NEEDS_FIXES. **Zero BLOCKER / zero HIGH.** All MEDIUM/LOW addressed + re-verified live.

## Findings & resolution

| # | Sev | Finding | Resolution | Verify |
|---|---|---|---|---|
| 1 | MEDIUM | Report cited the Recall@100 delta (1.0 vs 0.3125) as BM25-ranker superiority, but it is an asymmetry artifact: `bm25_query` has no `WHERE @@` filter (returns all docs on a 12-doc corpus → 1.0 trivially) while `fts_query` boolean-filters | Report § "Honest reading" now discloses the asymmetry explicitly + states nDCG@10 (0.9546 vs 0.5143) is the only load-bearing ranking signal; "do not cite Recall@100 as evidence" | report diff |
| 2 | MEDIUM | `pg_textsearch_available()` docstring claimed it proves the lib is preloaded, but it only checked `pg_available_extensions` (control file) — would not skip on an image started without the preload | Gate now checks BOTH `pg_available_extensions` AND `shared_preload_libraries` (honest); docstring corrected | shipped-image skip-path green (3 skipped) |
| 3 | MEDIUM | Failure-scenario #3 (empty-content index) declared in the plan but untested | Added `test_bm25_empty_content_index_and_nonmatch` (build over `''` doesn't crash; non-matching query doesn't error) — honestly asserts the top-k-no-filter behavior (not "returns no row") | bm25 image 3 passed |
| 4 | LOW | Report stated k1=1.2/b=0.75 without in-code provenance | Cited the live index-build NOTICE `Using index options: k1=1.20, b=0.75` | report diff |
| 5 | LOW | VectorChord-bm25 license fetched from moving `main` (weakens reproducibility) | Pinned to tag `0.3.0` (re-verified = AGPL/Elastic); §(c) advisory-exit comment added | sweep verdict green |
| 6 | LOW | `bm25_query` lacked the `IS NOT NULL` guard `vector_query_docs` has | Added `WHERE {text_col} IS NOT NULL` | tests green |

Reviewer-confirmed strengths (no action): pg_textsearch LICENSE **live-verified = PostgreSQL License** (not
the restrictive Timescale License); dual-license trap handled (AGPL/Elastic checked before permissive →
barred); `fail=1` on unexpected-AGPL; verdict + build pinned to the same `v1.3.1`; shipped `Dockerfile`
**verified unchanged** (measurement-first honored); the `fts Recall@100=0.3125` number independently
re-derived from qrels (numbers are real, not fabricated); BM25 functional proof + ordering test-proven;
backward-compat (include_bm25 default off → M7-S1 path unchanged, 11 hybrid tests green); BM25F correctly
deferred (no code).

## Hard gates (cycle-review)

| Gate | Status |
|---|---|
| Tests passing on branch | PASS — 69 unit + BM25 (3+1skip) + M7-S1 hybrid 11 (no regression) |
| No secrets committed | PASS — no secrets in this slice; `.env` gitignored |
| No direct commit to `main` | PASS — develop |
| No authorship trailer (user policy) | PASS |
| CHANGELOG updated | PASS — `[Unreleased]` M7-S2 (identified + measured, NOT "adopted") |
| No unbenchmarked perf claim | PASS — report states measured numbers only + caveats fixture; `grep faster than|outperforms` = 0 |
| D1 (no AGPL in distribution) | PASS — pg_textsearch only in throwaway image; license verdict reproducible |

## Verdict

READY_TO_MERGE. The ROADMAP M7-S2 DoD ("alternativa permissiva a BM25 full-text identificada") is met with
evidence: **pg_textsearch (PostgreSQL License)** identified (ADR 0003), license verdict **reproducible** in
`license-sweep.sh` (pg_textsearch permissive / VectorChord-bm25 AGPL-barred, live-verified), BM25 **functionally
proven** on the TheoDB engine, and **measured** vs ts_rank_cd (nDCG@10 0.9546 vs 0.5143, honestly framed).
Measurement-first honored: the shipped distribution is unchanged; adoption is a future ADR gated on a
real-corpus measurement. BM25F deferred (ADR 0003 §D4). No BLOCKER/HIGH; all MEDIUM/LOW resolved + re-verified.
