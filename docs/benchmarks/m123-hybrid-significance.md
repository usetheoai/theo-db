# M123 — Paired significance of hybrid vs vector on BEIR/SciFact: PARITY (measured)

**Date:** 2026-07-20 · **Box:** DO droplet (32 GB), pgrx-managed PG17, `theodb_rs` (own vector + `ai.hybrid_search_rrf`).
**Dataset:** BEIR **SciFact** test split — 5,183 docs / 300 queries, binary qrels. **Embeddings:** OpenAI
`text-embedding-3-small`, dim 1536 (disk-cached). **Verdict:** the hybrid (BM25/FTS + vector + RRF) lift over
vector-only is **NOT statistically significant on SciFact — parity.** Honest-negative, measured; no overclaim.

## Result

| Retriever | nDCG@10 | Recall@100 |
|---|---|---|
| vector | 0.7296 | 0.9733 |
| fts (lexical) | 0.0703 | 0.0694 |
| **hybrid (RRF, k=60)** | **0.7337** | 0.9733 |

### Paired significance — hybrid vs vector (nDCG@10, pre-declared primary endpoint, n=300)

| Field | Value |
|---|---|
| Δ̄ (mean per-query nDCG@10, hybrid − vector) | **+0.0041** |
| 95% CI (paired bootstrap, percentile) | **[−0.0010, +0.0108]** — straddles 0 |
| p (paired permutation, two-sided) | **0.253** — not significant |
| p (paired t-test, cross-check) | 0.193 (normal-approx) |
| wins / losses / ties | **3 / 1 / 296** |
| Cohen's dz | 0.075 (negligible) |
| seed / n_resamples | 20260720 / 100,000 |

**Interpretation (honest):** the +0.004 mean gain that M53 reported is **within noise** — the 95% CI includes 0
and p ≈ 0.25. The reason is stark in the counts: **296 of 300 queries are ties** (equal nDCG@10 — not necessarily
identical top-10 ordering, but equal at the metric). Only **4 queries are informative** (3 hybrid-wins, 1 loss);
the exact combinatorial two-sided p over the 2⁴=16 sign patterns is 4/16 = 0.25, which the permutation MC estimate
(0.253) converges to — there is simply no power with 4 informative pairs, so "not significant" is unavoidable, not
a modelling artefact. On SciFact the lexical/FTS leg is very weak (nDCG@10 = 0.07), so RRF fusion is dominated by
the strong vector leg and rarely changes the ranking. Per the pre-declared contract (ADR M123-2: nDCG@10 on
SciFact, no k-sweep / dataset-shopping), the honest verdict is **parity** — we do NOT claim hybrid beats vector on
this dataset.

Note on the CI: with only 4 informative pairs the bootstrap distribution is coarse/discrete and this is a
percentile (not BCa) interval — the CI is approximate, but the verdict rests on the permutation p, and any interval
here straddles 0.

## Method

- **Test:** paired permutation / randomization p-value (Smucker/Allan/Carterette CIKM 2007 — the IR-recommended
  test; Wilcoxon/sign rejected) + paired-bootstrap 95% CI on Δ̄ + paired t-test cross-check
  (`benchmarks/theodb_bench/significance.py`, `paired_significance`). numpy does the resampling (deterministic,
  fixed seed); scipy is optional (t-test p; here the normal-approx was used, valid at n=300). 8 unit tests validate
  the test itself (null → p≈1, clear shift → significant, ties counted).
- **Retrieval:** theodb's OWN vector type + `ai.hybrid_search_rrf` (RRF k=60, top=100); deterministic (spread 0.0
  across runs → the only stochastic element is the permutation/bootstrap RNG, seeded).
- **Metric:** nDCG@10 (BEIR's primary), per-query, aligned hybrid vs vector by qid.

## Reproduction

```
# PG with theodb_rs; OPENAI_API_KEY set; benchmarks/ deps (numpy, psycopg2)
python3 benchmarks/run_m53_hybrid_beir.py --dataset scifact --runs 1 --out docs/benchmarks/m123-hybrid-significance.json
```

## Honest caveats

- **SciFact license:** BEIR (Thakur et al. 2021) Appendix E states **CC BY-NC 2.0** (non-commercial); the HF card
  tags `cc-by-sa-4.0` (conflict, unverified uploader tag). Used here **CI-internally only** (downloaded for eval,
  never redistributed in the theo-db image) — the permissive-licence distribution gate covers distributed deps,
  not a CI-downloaded eval set. Do NOT redistribute the corpus; verify the authoritative licence before any reuse.
- **Single dataset:** this is SciFact only. Parity here does NOT prove hybrid never helps — on datasets with a
  stronger lexical signal (e.g. a keyword-heavy corpus) the FTS leg could shift more rankings. A follow-up on
  NFCorpus (graded qrels) or a keyword-heavy set would test that; not run here to avoid dataset-shopping for a
  significant result (ADR M123-2).
- **Candidate-set parity:** the vector leg and the hybrid's vector component retrieve over the same corpus at the
  same top-100 before fusion (no `@@`-filter candidate loss), so the paired comparison is clean — the 296 ties are
  real agreement, not an artifact of a shrunken candidate set. Hard corroboration: **vector Recall@100 == hybrid
  Recall@100 == 0.9733 exactly** — the FTS leg added zero relevant docs to the top-100, so the fusion had the same
  candidates to work with; the parity is genuine, not a candidate-set artefact.
- The absolute nDCG@10 (~0.73) is high because SciFact is a strong dense-retrieval dataset with text-embedding-3;
  this report is about the hybrid-vs-vector DIFFERENCE, not the absolute level.
