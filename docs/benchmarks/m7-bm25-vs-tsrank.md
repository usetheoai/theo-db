# M7-S2 — BM25 (pg_textsearch) vs ts_rank_cd, measured

> **Measured, not asserted** (CLAUDE.md TheoDB rule 5 / `rules/public-copy.md` §4-§5). The numbers below come
> from an actual run of the recall harness against a live build of **pg_textsearch v1.3.1** (PostgreSQL
> License) on `theo-db:dev`. This is the **measurement-first gate** (ADR 0002 / ADR 0003) that informs whether
> pg_textsearch should be adopted into the shipped distribution. No piece is shipped on the strength of a
> spec; it is shipped (later) on the strength of this measurement.

## What is measured

The M7-S1 recall harness (`benchmarks/theodb_bench/hybrid.py::run_three_retrievers`, `include_bm25=True`)
loads a labelled corpus into a `documents` table and scores four retrievers with the BEIR methodology
(Thakur et al. 2021, nDCG@10 primary + Recall@100):

- **vector** — `pgvector` `<=>` (cosine) top-100
- **fts** — PostgreSQL native `ts_rank_cd` + GIN (the lexical leg shipped in M7-S1) top-100
- **bm25** — **pg_textsearch** Okapi BM25 (`content <@> 'query'`, k1=1.2/b=0.75) top-100
- **hybrid** — `ai.hybrid_search_rrf` (RRF of vector + ts_rank_cd, k=60)

## Dataset (CI fixture — deterministic, offline)

The hand-labelled synthetic corpus (`benchmarks/theodb_bench/beir.py::synthetic_dataset`): 12 docs, 4
queries, graded qrels (databases vs cooking). Embeddings via the deterministic feature-hashed `lexical_embed`
(no embedding endpoint). Reproducible in CI; **not** a decision-grade real-world benchmark (see § Honest reading).

## Results (run 2026-06-28, pg_textsearch v1.3.1 on theo-db:dev, PG 17.10)

| Retriever | nDCG@10 | Recall@100 |
|---|---|---|
| vector (pgvector `<=>`) | 0.8311 | 1.0000 |
| **fts (ts_rank_cd — shipped M7-S1)** | **0.5143** | **0.3125** |
| **bm25 (pg_textsearch)** | **0.9546** | **1.0000** |
| hybrid (RRF vector+ts_rank_cd) | 0.8311 | 1.0000 |

## Honest reading

- **BM25 measurably beats the shipped `ts_rank_cd` lexical leg on this fixture** — nDCG@10 **0.9546 vs 0.5143**
  and Recall@100 **1.0000 vs 0.3125**. The direction is unambiguous: as a *lexical* ranker, pg_textsearch's
  true Okapi BM25 is far stronger than PostgreSQL's cover-density `ts_rank_cd` here. This is the expected
  result — `ts_rank_cd` is not BM25 — and it is now **measured**, not assumed.
- **Caveat (fixture, not the field):** this is a small synthetic lexical corpus. The magnitude of the gain on a
  real heterogeneous corpus (BEIR `scifact`/`nfcorpus`, real embeddings) is the **decisive follow-up** before
  adoption. The synthetic embedder also makes `vector`/`hybrid` saturate Recall@100 at 1.0 (little headroom),
  so the BM25-vs-vector comparison here is not the point — the **BM25-vs-ts_rank_cd lexical comparison** is.
- **No superiority claim ships unqualified** (rule 5): the result motivates adoption but the
  distribution-integration decision is taken in a future ADR once a real-corpus measurement confirms the gain
  justifies the build dependency + `shared_preload_libraries` constraint.

## Decision status (measurement-first)

| Question | Status |
|---|---|
| Permissive BM25 piece identified? | ✅ pg_textsearch (PostgreSQL License) — ADR 0003 |
| BM25 functionally proven on the TheoDB engine? | ✅ live build + ranked query + this measurement |
| BM25 > ts_rank_cd on the lexical leg? | ✅ measured here (synthetic); real-corpus confirm = follow-up |
| Adopt pg_textsearch into the shipped image? | ⏳ future ADR, gated on a real-corpus measurement (build dep + shared_preload_libraries to weigh) |
| BM25F (multi-field)? | ❌ deferred (ADR 0003 §D4 — single-field schema, YAGNI) |

## Reproduce

```bash
docker build -f packaging/Dockerfile.bm25 -t theo-db-bm25 .
docker run -d --name bm25 -e POSTGRES_PASSWORD=postgres -p 5432:5432 theo-db-bm25 \
  -c shared_preload_libraries=pg_textsearch                       # wait for healthy
cd benchmarks && pip install -r requirements.txt
PGHOST=localhost PGPORT=5432 PGUSER=postgres PGPASSWORD=postgres PGDATABASE=postgres \
  python3 - <<'PY'
from theodb_bench.db import VectorDB
from theodb_bench.beir import EMBED_DIM, lexical_embed, synthetic_dataset
from theodb_bench.hybrid import run_three_retrievers
db=VectorDB("host=localhost port=5432 dbname=postgres user=postgres password=postgres").connect()
db.ping(); db.ensure_extension()
for n,m in run_three_retrievers(db, synthetic_dataset(), lexical_embed, EMBED_DIM, include_bm25=True).items():
    print(f"{n:8s} nDCG@10={m['ndcg10']:.4f} Recall@100={m['recall100']:.4f}")
PY
```
