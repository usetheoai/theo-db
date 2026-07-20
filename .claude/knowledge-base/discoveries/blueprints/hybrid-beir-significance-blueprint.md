# Blueprint — M123 paired significance for hybrid vs vector on BEIR

Date: 2026-07-20 · Source: `/roadmap-feature hybrid-beir-significance` discover (council-benchmark, web-evidenced).

## Bottom line

The M53 harness already collects the per-query arrays the test needs (`theodb_bench/hybrid.py:83` emits
`per_query[name] = {qids, ndcg10:[...], recall100:[...]}` behind `return_per_query=True`), but
`run_m53_hybrid_beir.py` reports only the MEAN and never captures them. M123 is genuinely just adding a paired
significance test over arrays that already exist — no new retrieval work. M53's own doc (§4) flags this exact
follow-up.

## The correct test

Matched-pairs design (same queries, one nDCG@10 per query per system):

- **Headline p-value = paired permutation / randomization test** (Smucker, Allan & Carterette, CIKM 2007 —
  "recommend the randomization test as the preferred test in all cases … the Wilcoxon and sign tests should no
  longer be used"; PDF https://maroo.cs.umass.edu/pub/web/getpdf.php?id=744). Recipe: sign-flip the per-query
  difference `d_i = ndcg_hybrid_i − ndcg_vector_i` under label exchangeability, recompute mean(d), two-sided
  p = fraction of permutations with |mean| ≥ |observed|. B = 100,000 (or exact if 2^n small).
- **95% CI on Δ̄ = paired bootstrap** (percentile/BCa on the per-query differences) — the interval strength.
- **Paired t-test = agreeing cross-check** (`scipy.stats.ttest_rel`; pytrec_eval's own significance example uses it).
- Wilcoxon/sign REJECTED — discard magnitude, must drop tied (Δ=0) queries.
- Honest nuance (Urbano, Marrero, Martín, SIGIR 2013): bootstrap/t/wilcoxon also perform well; do not overclaim
  permutation as uniquely correct. → report permutation p + bootstrap CI + t-test together.

**Rule 9 / parsimony:** `scipy.stats.permutation_test(..., permutation_type='samples')`, `scipy.stats.bootstrap`,
`scipy.stats.ttest_rel` do exactly this — do not reimplement statistical tests. scipy is BSD-3, dev-only (D1
gates *distributed* deps; dev-only is exempt). Seed the RNG + record it.

## The honest report shape (anti p-hack — load-bearing)

On ONE pre-declared primary endpoint (nDCG@10 on the pre-declared dataset):

| Field | Why |
|---|---|
| n (queries) | denominator; "significant" on n=50 is weak |
| Δ̄ (mean per-query diff, nDCG@10 points) | the effect size that matters in IR |
| 95% CI (paired bootstrap) | a CI straddling 0 = parity regardless of p |
| p (permutation, with B + seed) | the test |
| wins / losses / ties | exposes whether Δ̄ is a broad shift or a few big wins masking many small losses |
| Cohen's dz (optional) | cross-dataset comparability |

- **Pre-declare k=10 + dataset** (nDCG@10 is BEIR's fixed primary metric). Do NOT sweep k∈{1,3,5,10,100} and
  report the significant one; do NOT run 5 datasets and headline the one that reached p<0.05.
- If multiple metrics/datasets tested → Holm/Bonferroni correction, stated.
- **If not significant → say "parity" and stop** (matches M53's existing honest posture; honest-negative accepted).

## Datasets (small, license-flagged)

- **SciFact** (300 test queries / 5,183 docs, binary qrels) — ALREADY WIRED in M53. **License FLAG:** BEIR paper
  Appendix E = CC BY-NC 2.0 (non-commercial); HF card `BeIR/scifact` = cc-by-sa-4.0 (conflict, unverified
  uploader tag). CI-internal use OK (D1 gates *distributed* deps, not a CI-downloaded eval set); do NOT
  redistribute the corpus; flag it in the artifact. → **primary** (smallest, fastest, already wired).
- NFCorpus (323 q / 3,633 docs, 3-level graded qrels — exercises nDCG graded gain) — secondary; license also
  unverified. ArguAna / CQADupStack = CC BY 4.0 / Apache-2.0 if strictly-permissive required.

## BEIR protocol + pitfalls

- nDCG@10 primary (BEIR §4, pytrec_eval `ndcg_cut.10`); load the `test` split; unjudged docs = grade 0 (default).
- RRF is rank-only (k=60 — the value M53 uses); feed the ranked doc-id list to the metric, deterministic
  tie-break by doc-id (M53 spread is 0.0 — keep it). Report tie count.
- **Candidate-set parity (the M53 §3 trap):** the `@@` filter dropped ~93% relevant → confounds ranker quality
  with candidate-set size. For a clean paired test the vector leg and the hybrid's vector component must generate
  candidates over the SAME corpus at the SAME top-N before fusion.
- The metric is deterministic (M53 spread 0.0) → the ONLY stochastic element is the permutation/bootstrap
  resampling; pin + record its seed and B so p and CI reproduce exactly.

## Sources (primary, verified)

- Smucker/Allan/Carterette CIKM 2007 — https://maroo.cs.umass.edu/pub/web/getpdf.php?id=744 · DOI 10.1145/1321440.1321528
- Urbano/Marrero/Martín SIGIR 2013 — https://julian-urbano.info/publications/057-comparison-optimality-statistical-significance-tests-information-retrieval-evaluation.html
- BEIR (Thakur et al. NeurIPS 2021) — https://arxiv.org/abs/2104.08663 (§4 nDCG@10, Table 1 sizes, Appendix E licenses)
- pytrec_eval — https://github.com/cvangysel/pytrec_eval (+ statistical_significance.py example)
- BEIR repo — https://github.com/beir-cellar/beir

## Flagged / unverifiable

- Fuhr "Some Common Mistakes in IR Evaluation" (SIGIR Forum 2018) — every mirror 404'd; canonical CI/effect-size/
  p-hacking cite. The report contract stands on Smucker + Urbano alone.
- SciFact license conflict (BEIR paper CC BY-NC 2.0 vs HF cc-by-sa-4.0) — CI-internal use fine; verify before any
  redistribution.

## Local anchors

- `benchmarks/run_m53_hybrid_beir.py` — driver (run() at :73; calls `run_three_retrievers` at :124 WITHOUT
  return_per_query → M123 must pass it + capture `_per_query`).
- `benchmarks/theodb_bench/hybrid.py:83` — per-query emit; `metrics.py` (`ndcg_at_k`/`recall_at_n`).
- `benchmarks/requirements.txt` — add `scipy` (BSD-3, dev-only).
- `docs/benchmarks/m53-hybrid-beir.md` §4 — lists this significance test as the open follow-up.
