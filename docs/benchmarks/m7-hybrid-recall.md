# M7-S1 — Hybrid search (FTS + vector + RRF) recall, measured

> **Measured, not asserted** (CLAUDE.md TheoDB rule 5 / `.claude/rules/public-copy.md` §4-§5). The numbers
> below come from an actual run of the 3-retriever eval against a live `theo-db:dev` container — no
> estimates, no fabrication. Reproduction commands at the bottom.

## What is measured

The eval (`benchmarks/theodb_bench/hybrid.py::run_three_retrievers`) loads a labelled corpus into a
`documents(doc_id, content, text_tsv GENERATED, embedding)` table (GIN on `text_tsv`), then scores three
retrievers with the BEIR methodology (Thakur et al. 2021, `arxiv.org/abs/2104.08663`):

- **vector** — `pgvector` `<=>` (cosine) top-100
- **fts** — PostgreSQL native FTS (`plainto_tsquery` + `ts_rank_cd`, GIN) top-100
- **hybrid** — `ai.hybrid_search_rrf` (RRF fusion, k=60 — Cormack et al. 2009)

Metrics: **nDCG@10** (primary) and **Recall@100** (secondary), averaged over the queries.

## Dataset (CI fixture — deterministic, offline)

A small hand-labelled **synthetic** corpus (`benchmarks/theodb_bench/beir.py::synthetic_dataset`): 12 docs,
4 queries, graded qrels across two topics (databases, cooking). Embeddings are produced by a deterministic
**feature-hashed lexical embedder** (`lexical_embed`, dim 16) so the run is fully reproducible with **no
embedding-endpoint dependency** (plan ADR D4). This fixture exists to make the eval reproducible in CI — it
is **not** a decision-grade benchmark of real-world hybrid quality (see § Honest caveats).

## Results (run 2026-06-28 against `theo-db:dev`, PG 17.10 + pgvector 0.8.3)

| Retriever | nDCG@10 | Recall@100 |
|---|---|---|
| vector | 0.8311 | 1.0000 |
| fts | 0.5143 | 0.3125 |
| **hybrid (RRF, k=60)** | **0.8311** | **1.0000** |

## Honest reading of these numbers

- **Hybrid ties pure-vector here; it does not beat it on this fixture.** The reason is the fixture, not the
  method: the deterministic CI embedder is itself **lexical** (feature-hashed token counts), so the "vector"
  leg already captures keyword overlap and saturates Recall@100 at 1.0 — leaving no gap for the FTS leg to
  fill. RRF correctly **never drags the strong vector ranks down** (rank-based fusion), so hybrid ≥ vector
  on every metric, but with no headroom to exceed it on this corpus.
- **Where the hybrid win is expected to materialize** is the regime BEIR documents: a *real* dense embedding
  model (semantic, not lexical) misses exact-term / rare-token matches that BM25/FTS catches — there RRF
  fusion of a semantic vector leg + a lexical FTS leg beats either alone. Proving that requires a real
  embedding model (`theodb.embed` over a configured endpoint) on a harder, real corpus — the **out-of-CI
  real-BEIR slice**, tracked as the next step. No such number is claimed here until it is measured.

## Reproduce

```bash
docker build -t theo-db:dev .
docker run -d --name hyb -e POSTGRES_PASSWORD=postgres -p 5432:5432 theo-db:dev   # wait for healthy
cd benchmarks && pip install -r requirements.txt
PGHOST=localhost PGPORT=5432 PGUSER=postgres PGPASSWORD=postgres PGDATABASE=postgres \
  python3 - <<'PY'
from theodb_bench.db import VectorDB
from theodb_bench.beir import EMBED_DIM, lexical_embed, synthetic_dataset
from theodb_bench.hybrid import run_three_retrievers
db=VectorDB("host=localhost port=5432 dbname=postgres user=postgres password=postgres").connect()
db.ping(); db.ensure_extension()
for n,m in run_three_retrievers(db, synthetic_dataset(), lexical_embed, EMBED_DIM).items():
    print(f"{n:8s} nDCG@10={m['ndcg10']:.4f} Recall@100={m['recall100']:.4f}")
PY
```
