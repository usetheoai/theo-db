# M21 — Own ANN index (HNSW + IVFFlat) recall@k parity vs pgvector

**Verdict:** `PARITY_REACHED`  
**Method:** measurement-first (ROADMAP-v2 M21). TheoDB's own Rust HNSW/IVFFlat (`theodb.hnsw_knn` / `theodb.ivfflat_knn`, theodb_rs M21) vs pgvector's `hnsw`/`ivfflat` indexes, recall@10 on an exact brute-force ground truth (`theodb_bench.recall`), distance-thresholded (ADR D2 tolerance band).  
**Config:** n=1500, dim=32, queries=60, runs=3 (mean ± std), tolerance=0.05, ivf lists=38, metric=l2, seed=2026.  
**Hardware/build:** the `theo-db` container image (postgres:17 + pgvector + theodb_rs).  

## Recall@10 (mean ± std over runs)

| algorithm | knob | pgvector | theodb (own) | parity (own ≥ pgvector − tol) |
|---|---|---|---|---|
| hnsw | ef_search=10 | 0.8622 ± 0.0055 | 0.8617 ± 0.0000 | ✅ PASS |
| hnsw | ef_search=40 | 0.9917 ± 0.0000 | 0.9917 ± 0.0000 | ✅ PASS |
| hnsw | ef_search=100 | 1.0000 ± 0.0000 | 1.0000 ± 0.0000 | ✅ PASS |
| hnsw | ef_search=200 | 1.0000 ± 0.0000 | 1.0000 ± 0.0000 | ✅ PASS |
| ivfflat | probes=1 | 0.1606 ± 0.0103 | 0.1767 ± 0.0000 | ✅ PASS |
| ivfflat | probes=8 | 0.6561 ± 0.0170 | 0.6750 ± 0.0000 | ✅ PASS |
| ivfflat | probes=16 | 0.8672 ± 0.0057 | 0.8983 ± 0.0000 | ✅ PASS |
| ivfflat | probes=32 | 0.9978 ± 0.0031 | 1.0000 ± 0.0000 | ✅ PASS |

## Performance (honest)

- Own HNSW batch (build + 60 queries) wall time: **209.1 ms** at ef_search=100. The SQL-callable form rebuilds the in-memory graph per call (measurement-first scope, ADR D1/D3); a persisted on-disk access method (`CREATE INDEX … USING`) is the deferred M21b follow-up.
- pgvector queries use its persisted on-disk index; the comparison here is **recall**, not build/query latency (the latency profiles are not comparable until M21b persists the own index).

## Reproduce

```bash
# against a running theo-db container (PG* env pointing at it):
python3 benchmarks/bench_ann_index.py --n 1500 --dim 32 --nq 60 --runs 3 --tolerance 0.05 --write-doc
# gate as a test:
pytest benchmarks/tests/test_ann_index.py::test_recall_parity_gate -v
```

## Migration decision (coexistence, measurement-first)

Per blueprint ADR D1 + plan ADR D1/D3: the own algorithms **coexist** with pgvector — they read `embedding::real[]` and never touch pgvector's type, operators, or HNSW/IVFFlat indexes; `theodb.embed/hybrid/import` are unaffected. Parity is reached at the operating points marked PASS above, so the own algorithms are a viable opt-in path; the on-disk planner-integrated access method is M21b. No pgvector index is replaced by M21.
