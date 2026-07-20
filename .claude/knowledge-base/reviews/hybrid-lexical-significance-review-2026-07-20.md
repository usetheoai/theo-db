# Review — M125 hybrid lexical-heavy significance

**Date:** 2026-07-20 · **Slug:** hybrid-lexical-significance · **Milestone:** M125
**Verdict:** READY_TO_MERGE

## Scope

Adversarial review (council-benchmark lens: "você mediu ou está supondo?") of the M125 slice: the three-comparison
paired-significance wiring (`run_m53_hybrid_beir.py`), the reused test (`significance.py`), the 10 unit tests, and
the measured NFCorpus artifact (`docs/benchmarks/m125-hybrid-lexical.{md,json}`) whose verdict is **SIGNIFICANT**.

## Consolidated findings

| # | Severity | Finding | Resolution |
|---|---|---|---|
| 1 | MEDIUM | Public-positioning quote said "improves **recall**" but significance was measured on **nDCG@10** (Recall@100 is a separate, untested column). | **FIXED** — every occurrence in the report + the sanctioned quote now reads "improves ranking quality (nDCG@10)". |
| 2 | LOW | Per-query nDCG@10 arrays not persisted → p/CI recomputable only by re-embedding, not offline from the artifact. | **FIXED (going forward)** — harness now persists `significance.per_query_ndcg10` (+ a unit test); this JSON predates the field, noted honestly in the report caveats. No droplet re-run for a LOW (numbers reproduce via seed). |
| 3 | LOW | Diagnostic legs' Monte-Carlo p rendered "0.0000"/"p=0" (a permutation p is floored at 1/(B+1), never 0). | **FIXED** — report table + caveats now render "<1e-4". |
| 4 | LOW | `_paired_sig` guards key-presence not content; three comparisons aligned independently with no n-match assertion. | **ACCEPTED (inherited from M123, unreachable)** — the harness always builds all three legs over the full qid set; documented, not a regression. |

## What the review verified (measured, not supposed)

- **Wiring correct to 6 dp.** `_align` aligns by qid regardless of retriever order (proven by the shuffled-order
  test); `_one_sig` passes `(x,y)` as `(a,b)` so all three signs are right (hybrid−vector +, hybrid−fts +,
  fts−vector −); recovered t=2.5592 / p_ttest=0.01049 match the JSON exactly. No fabrication.
- **SIGNIFICANT is the honest pre-declared call.** p_perm=0.0099 < 0.05 AND ci95_low=+0.0027 > 0 — both legs of the
  AND-rule hold. Framing is honest (dz=0.14 = below Cohen's "small"; "modest" is not an overclaim; the quote
  forbids "hybrid beats dense" unqualified).
- **Confound resolution is evidence-backed, not post-hoc.** The SciFact-parity attribution is supported by three
  independent measured facts (fts leg alive 0.2076; hybrid−fts hugely significant; 104/323 reordered) + matches the
  cited IR literature. fts−vector = −0.1768 fits RRF-complementarity, not contradicts it.
- **Anti-p-hack clean.** NFCorpus + nDCG@10 + the decision rule were pre-declared (ADR M125-2); the blueprint
  pre-registered the expected +0.006 effect and +0.0105 landed in that ballpark; NFCorpus is the *harder*
  BM25≈dense synergy set (conservative, not cherry-picked); only ONE comparison is confirmatory (no multiple-testing
  inflation on the headline).

## Gate check

- No BLOCKER, no HIGH. One MEDIUM (fixed), three LOW (2 fixed, 1 accepted-inherited). Per `cycle-review.md`:
  `READY_TO_MERGE` (no BLOCKER, ≤ 2 HIGH). 
- 10/10 unit tests green. CHANGELOG `[Unreleased]` updated. No secrets. No Co-Authored-By.

## Verdict

**READY_TO_MERGE.** The result is measured, internally consistent, honestly framed, and the one metric-noun
imprecision the reviewer flagged for external use is corrected. H6 (hybrid AT_RISK from the M123 analysis) is
resolved with evidence.
