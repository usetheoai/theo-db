# M9 — IVFFlat / IVF vector index: measured recall × QPS (vs HNSW)

**Date:** 2026-06-28 · **Milestone:** M9 (closes `docs/features/03-indice-ivfflat.md` + `04-indice-ivf.md`)
**Status:** measured (reproducible) · **Engine:** `theo-db:dev` (PostgreSQL + pgvector 0.8.x)

> Honesty (CLAUDE.md Rule 5/7): every number below is a real harness run against a live container.
> No "faster than" claim is made — the data shows a **trade-off**, reported as measured.

## What this validates

`docs/features/03` (IVFFlat) and `04` (IVF) were *available* in pgvector (`USING ivfflat`) but never
exercised by our recall@k harness, which only benchmarked HNSW and DiskANN. M9 adds IVFFlat as a
first-class index in the harness (`--index ivfflat` / `--index all`) and measures it on the same
dataset with the same ANN-Benchmarks distance-thresholded recall as HNSW. **Feature 04 (generic "IVF")
is closed by the same validation: pgvector's IVF index *is* IVFFlat — there is no distinct "IVF" access
method to implement.**

## Methodology

- **Harness:** `benchmarks/theodb_bench` (the M2 recall@k harness; distance-thresholded recall, ε=1e-3).
- **Dataset:** synthetic gaussian, `n=5000`, `dim=16`, metric `l2`, `k=10`, `n_queries=100`, `runs=2`
  (best-of-N mean). Ground truth = exact brute-force k-NN (numpy).
- **IVFFlat build:** `lists = n/1000 = 5`; query knob `ivfflat.probes` swept `{1, 5, 10}` (probes clamped
  to `lists`, so probes≥5 scans all clusters). Index forced on (`enable_seqscan=off`) — we measure the
  index, not the planner's small-N seqscan choice (the established pgvector recall-test methodology).
- **HNSW baseline:** `ef_search` swept `{40, 100}` (same harness, same dataset).

### Reproduction

```bash
docker run -d --name m9-it -e POSTGRES_PASSWORD=postgres -p 55473:5432 theo-db:dev   # wait healthy
cd benchmarks && pip install -r requirements.txt
PGHOST=localhost PGPORT=55473 PGUSER=postgres PGPASSWORD=postgres PGDATABASE=postgres \
  python3 -m theodb_bench --index all --n 5000 --dim 16 --n-queries 100 --k 10 --runs 2 --metric l2
# integration test asserting recall + index used:
PGHOST=localhost PGPORT=55473 PGUSER=postgres PGPASSWORD=postgres PGDATABASE=postgres \
  pytest -m integration tests/test_integration.py -k ivfflat -q
```

## Measured results

| Index | Params | recall@10 | QPS | p95 (ms) | build (ms) | index size |
|---|---|---|---|---|---|---|
| HNSW | ef_search=40 | 0.9950 | 3757.9 | 0.547 | 1078 | 1,851,392 B (1.85 MB) |
| HNSW | ef_search=100 | 1.0000 | 2491.6 | 0.703 | 1078 | 1,851,392 B (1.85 MB) |
| IVFFlat | probes=1 | 0.5330 | 4930.6 | 0.358 | 157 | 466,944 B (467 KB) |
| IVFFlat | probes=5 (=lists) | 1.0000 | 1039.5 | 1.418 | 157 | 466,944 B (467 KB) |
| IVFFlat | probes=10 (→clamped 5) | 1.0000 | 976.0 | 1.364 | 157 | 466,944 B (467 KB) |

## Honest analysis (the trade-off, measured)

- **Recall curve is correct and monotone.** IVFFlat at `probes=1` scans 1 of 5 clusters → recall **0.533**;
  at `probes=lists` it scans every cluster → recall **1.0000** (IVFFlat stores full-precision vectors, so
  scanning all lists is exact-among-indexed). This is the expected `recall ↑ with probes` behaviour.
- **IVFFlat builds ~6.9× faster** (157 ms vs 1078 ms) and the **index is ~4× smaller** (467 KB vs 1.85 MB)
  than HNSW on this dataset.
- **At equal high recall (1.0), HNSW serves more queries/sec** (2492 QPS @ ef_search=100) than IVFFlat
  (1040 QPS @ probes=lists) on this dataset. No "IVFFlat is faster" claim is made — the win for IVFFlat
  here is **build time and index size**, not query throughput at high recall.
- **Recommendation (evidence-based):** IVFFlat suits workloads that rebuild indexes often or are memory/disk
  constrained and tolerate a recall/QPS knob (`probes`); HNSW suits read-heavy workloads needing high recall
  *and* high QPS. Both are now first-class in the harness — pick on measured evidence, not folklore.

## Caveats

- Synthetic gaussian + low dim (16) and small `n` (5000) make a coarse `lists=5`; real embedding
  distributions and larger `n` shift the curve. The harness supports real datasets via `--hdf5` for a
  follow-up run; the **methodology and the IVFFlat validation are what M9 delivers** (the absolute
  numbers are dataset-specific, stated here as measured, not as a universal claim).
