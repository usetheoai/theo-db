# Blueprint — M125 hybrid on a lexical-heavy set (resolve H6)

Date: 2026-07-20 · Source: council-ai-in-db discover (web-evidenced).

## Core finding

M123 measured PARITY on SciFact because SciFact is **dense-strong with a near-parity lexical leg** (BEIR: BM25
0.665 vs TAS-B 0.643). To test the hybrid value-prop fairly, use a set where BM25 **decisively** beats dense.
Two confounds decide the outcome, not just the dataset.

## Where BM25 beats dense on BEIR (Thakur 2021, nDCG@10)

| dataset | BM25 | TAS-B | BM25−dense | regime |
|---|---|---|---|---|
| **Touché-2020** | 0.367 | 0.162 | **+0.205** | BM25 ≫ ALL dense (strongest lexical) |
| TREC-COVID | 0.656 | 0.481 | +0.175 | strong lexical (custom license ⚠) |
| **NFCorpus** | 0.325 | 0.319 | +0.006 | BM25≈dense (synergy test) |
| SciFact (excluded) | 0.665 | 0.643 | +0.022 | dense-strong — M123's parity set |
| FiQA / ArguAna | — | — | −0.06 / −0.11 | **dense wins** — wrong regime, do NOT use |

Source: https://arxiv.org/abs/2104.08663 (ar5iv full text, Table 3). BEIR narrative: dense underperforms on
"task-shifts like Touché-2020". **Honest correction:** ArguAna is dense-FAVORING despite being "argument
retrieval" — do not pick it.

## Dataset recommendation

- **Touché-2020** (`webis-touche2020`, **CC BY 4.0** ✓, 49q / 382K corpus) — PRIMARY (strongest BM25≫dense). 49
  queries is enough at Δ≈+0.2. **Practical caveat:** 382K-corpus OpenAI embed is a long/costly run (rate-limited).
- **NFCorpus** (3.6K corpus / **323q**, license: authors report none; HF tags cc-by-sa — flag for loop-check-licence,
  CI-internal only) — SECONDARY, fastest + best statistical power; synergy test (BM25≈dense).
- Avoid TREC-COVID (custom license), SCIDOCS (GPL), FiQA/ArguAna (dense wins).

## The load-bearing confound: ts_rank_cd ≠ BM25

TheoDB's shipped lexical leg is Postgres `ts_rank_cd` (`hybrid.rs:15,:42`) — NOT BM25 (no IDF saturation, no
length norm). A true BM25 leg exists (`hybrid.rs:64-86`, `USING bm25`) but needs `pg_textsearch` — "not present on
the shipped image" (`hybrid.rs:176`). So a null result is AMBIGUOUS: "hybrid doesn't help" vs "our ts_rank leg is
too weak". **M125 must disambiguate** — measure the fts leg's own contribution (its nDCG + hybrid-vs-fts), and if
the ts_rank leg is the bottleneck, that's a PRODUCT finding (ship a true BM25 leg), not "hybrid is worthless".

## RRF + honest expectation

RRF k=60 is Cormack 2009 (pilot-optimal — `hybrid.rs:343` matches). Bruch 2023 (arXiv:2210.11934): lexical+dense
are complementary (lexical=exact-match, dense=paraphrase); RRF is parameter-sensitive; a learned convex-combo
beats RRF. **Honest expectation:** the hybrid gain is real but **dataset-dependent, not universal** — large on
lexical-heavy sets, small/parity on synergy sets. **Modern-embedding caveat (inference):** BEIR's dense numbers
are 2021 (TAS-B); TheoDB uses 2024 OpenAI embeddings (stronger) → the BM25−dense gap will be SMALLER than the table.

## Honest positioning (whatever M125 measures)

- Significant hybrid win on the lexical set → "hybrid significantly improves recall on lexical/exact-match
  workloads (measured); parity on dense-strong; superiority is dataset-dependent" (defensible under public-copy.md).
- Flat with ts_rank but the fts leg is weak → "the shipped ts_rank leg is the bottleneck; a true BM25 leg
  (pg_textsearch) is the product follow-up" — not a retraction.
- Flat everywhere → drop the "hybrid improves retrieval" line until re-measured (honest-negative).
- NEVER claim "hybrid beats dense" unqualified — false on FiQA/ArguAna.

## Sources (primary, verified)

- BEIR (Thakur 2021): https://arxiv.org/abs/2104.08663 · https://github.com/beir-cellar/beir
- RRF (Cormack 2009): http://cormack.uwaterloo.ca/cormack/cormacksigir09-rrf.pdf
- Hybrid fusion analysis (Bruch 2023): https://arxiv.org/abs/2210.11934

## Flags

- Touché-2020 shallow-judgment caveat (relaunched 2021/2022) — could not confirm a clean primary URL; treat as
  known-but-unverified when reading Touché absolute scores.
- Modern-embedding attenuation is an inference (not measured) — Touché stays the right pick but calibrate.

## Local anchors

- `theodb_rs/src/hybrid.rs` — ts_rank leg :15/:42, bm25 leg :64-86, extension-absent guard :176, k=60 :343.
- `benchmarks/theodb_bench/significance.py` (reuse as-is), `beir.py` (loader downloads any `{name}.zip`), `hybrid.py`
  (`_RETRIEVERS = vector/fts/hybrid` — the fts leg's per-query arrays already flow).
