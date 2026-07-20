# M125 — Hybrid vs vector on a lexical-favoring set (NFCorpus): SIGNIFICANT (measured)

**Date:** 2026-07-20 · **Box:** DO droplet, pgrx-managed PG17, `theodb_rs` (own vector + `ai.hybrid_search_rrf`).
**Dataset:** BEIR **NFCorpus** test split — 3,633 docs / 323 queries (medical, exact-term). **Embeddings:** OpenAI
`text-embedding-3-small`, dim 1536. **Verdict:** on a lexical-favoring set where the shipped `ts_rank` lexical leg
is alive, **hybrid (vector + FTS + RRF) SIGNIFICANTLY beats vector-only on nDCG@10 (ranking quality)** — resolving the H6 risk left open by
M123's SciFact parity. Honest: the gain is small and regime-dependent.

## Result

| Retriever | nDCG@10 | Recall@100 |
|---|---|---|
| vector | 0.3845 | 0.3619 |
| fts (ts_rank lexical leg) | **0.2076** | 0.1019 |
| **hybrid (RRF, k=60)** | **0.3950** | 0.3674 |

### Three paired comparisons (nDCG@10, n=323, permutation p + bootstrap 95% CI)

| Comparison | Δ̄ | 95% CI | p (permutation) | wins/losses/ties | verdict |
|---|---|---|---|---|---|
| **hybrid − vector** (the claim) | **+0.0105** | **[+0.0027, +0.0188]** | **0.0099** | 55 / 49 / 219 | **SIGNIFICANT** (p<0.05 AND CI excludes 0) |
| hybrid − fts | +0.1873 | [+0.1601, +0.2156] | <1e-4 | 191 / 23 / 109 | SIGNIFICANT (fusion pulls far above the lexical leg alone) |
| fts − vector | −0.1768 | [−0.2075, −0.1469] | <1e-4 | 46 / 183 / 94 | fts loses to vector (as expected — ts_rank ≪ dense) |

## Interpretation — H6 resolved (and the M123 confound explained)

- **Hybrid significantly improves ranking quality (nDCG@10) on a lexical-favoring workload.** hybrid − vector is +0.0105 nDCG@10 with
  p=0.0099 and a CI that excludes 0 — a real, if small, lift. This is the regime the IR literature predicts hybrid
  wins (dense misses exact terms; BM25/FTS recovers them; RRF fuses the complementary rankings).
- **The M123 SciFact parity is now explained, not a contradiction.** On SciFact the lexical leg was effectively
  dead (296/300 ties, dense-strong). Here the **fts leg is alive (mean nDCG@10 = 0.2076)** and fusion moves ~1/3
  of queries (219 ties, 104 changed). So M123's parity was "dense-strong set + a lexical leg that rarely reorders"
  — NOT "hybrid never helps."
- **The `ts_rank ≠ BM25` confound is bounded, not fatal.** Even the *ts_rank* lexical leg (no BM25 IDF/length-norm)
  is strong enough here to produce a significant fusion gain. A true BM25 leg (`pg_textsearch`) would likely widen
  it — that remains the tracked product follow-up, but the value-prop is already validated with what ships.

**Honest positioning (defensible under public-copy.md):** *"Hybrid retrieval (vector + FTS, RRF-fused)
significantly improves ranking quality (nDCG@10) on lexical/exact-match workloads (measured on NFCorpus: Δ nDCG@10 = +0.0105,
p=0.0099, paired permutation). On dense-strong workloads (SciFact, M123) it is at parity. Superiority is
dataset-dependent."* Never claim "hybrid beats dense" unqualified — BEIR shows dense wins on FiQA/ArguAna.

## Method

- `benchmarks/theodb_bench/significance.py::paired_significance` (reused unchanged from M123) — paired permutation
  p + bootstrap 95% CI + t-test; numpy resampling, fixed seed 20260720, B=100,000. `_paired_sig`
  (`run_m53_hybrid_beir.py`) now reports the three comparisons + the fts leg's mean nDCG@10 so a parity is
  attributable. 9 unit tests validate the test + the three-comparison wiring.
- Retrieval: theodb own vector + `ai.hybrid_search_rrf` (RRF k=60, top=100), deterministic (spread 0.0).

## Reproduction

```
# PG with theodb_rs; OPENAI_API_KEY set; benchmarks/ deps (numpy, psycopg2)
python3 benchmarks/run_m53_hybrid_beir.py --dataset nfcorpus --runs 1 --out docs/benchmarks/m125-hybrid-lexical.json
```

## Honest caveats + follow-ups

- **Small effect:** +0.0105 nDCG@10 is significant but small (219/323 ties). It is a measured lift, not a
  transformation. Cohen's dz is modest.
- **Reproducibility (M125 review LOW):** p/CI reproduce on a full re-run (fixed seed 20260720, B=100,000). The
  harness now also persists the aligned per-query nDCG@10 arrays under `significance.per_query_ndcg10` so a third
  party can recompute p/CI **offline** without re-embedding — this JSON predates that field; the next run of the
  harness carries it. The three diagnostic legs' Monte-Carlo p is floored at 1/(B+1) ≈ 1e-5 (never exactly 0).
- **NFCorpus license:** the BEIR authors report no license; the HF `BeIR/nfcorpus` card tags `cc-by-sa-4.0` (an
  unverified uploader tag). Used **CI-internally only** (downloaded for eval, not redistributed) — the
  permissive-licence distribution gate covers distributed deps, not a CI-downloaded eval set. Do not redistribute
  the corpus; verify before any reuse.
- **Deferred follow-ups (tracked, honest):** (1) a true **BM25 leg** via `pg_textsearch` to A/B against `ts_rank`
  (would likely widen the gain — a product lever, not a correctness gap); (2) **Touché-2020** (`webis-touche2020`,
  CC BY 4.0, the strongest BM25≫dense set) — its 382K-doc OpenAI embed is a long/costly run impractical for a CI
  benchmark, deferred to a dedicated embed budget. NFCorpus (feasible) was the pre-declared measurable.
