# M9 — IVFFlat / IVF vector index: measured recall × QPS (vs HNSW)

**Date:** 2026-06-28 · **Milestone:** M9 (closes `docs/features/03-indice-ivfflat.md` + `04-indice-ivf.md`)
**Status:** measured (reproducible) · **Engine:** `theo-db:dev` (PostgreSQL + pgvector 0.8.x)

> Honesty (CLAUDE.md Rule 5/7): every number below is a real harness run against a live container.
> No speed-superiority claim is made — the data shows a **trade-off**, reported as measured.

## What this validates

`docs/features/03` (IVFFlat) and `04` (IVF) were *available* in pgvector (`USING ivfflat`) but never
exercised by our recall@k harness, which only benchmarked HNSW and DiskANN. M9 adds IVFFlat as a
first-class index in the harness (`--index ivfflat` / `--index all`) and measures it on the same
dataset with the same ANN-Benchmarks distance-thresholded recall as HNSW. **Feature 04 (generic "IVF")
is closed by the same validation: pgvector's IVF index *is* IVFFlat — there is no distinct "IVF" access
method to implement.**

## Methodology

- **Harness:** `benchmarks/theodb_bench` (the M2 recall@k harness; distance-thresholded recall, ε=1e-3).
- **Dataset:** synthetic gaussian, `n=5000`, `dim=16`, metric `l2`, `k=10`, `n_queries=100`, `runs=3`.
  Ground truth = exact brute-force k-NN (numpy). **Aggregation:** recall is computed once per built index
  (deterministic for a fixed index); QPS = `1 / (best mean query-latency over the 3 runs)`; latency p95 is
  the client-side 95th percentile; build-time / index size are read once per build (`pg_relation_size`).
- **IVFFlat build:** `lists = max(1, n/1000) = 5`; query knob `ivfflat.probes` swept over the distinct
  **clamped** values `{1, 5}` — each raw probe is clamped to `lists` *before* de-duplication, so every row's
  label equals the value actually executed (probes > lists is a no-op in pgvector; an unclamped `probes=10`
  label would silently run `probes=5`). At `probes=lists` all clusters are scanned. Index forced on
  (`enable_seqscan=off`) — we measure the index, not the planner's small-N seqscan choice.
- **HNSW baseline:** `ef_search` swept `{40, 100}` (same harness, same dataset).

### Reproduction

```bash
docker run -d --name m9-it -e POSTGRES_PASSWORD=postgres -p 55473:5432 theo-db:dev   # wait healthy
cd benchmarks && pip install -r requirements.txt
PGHOST=localhost PGPORT=55473 PGUSER=postgres PGPASSWORD=postgres PGDATABASE=postgres \
  python3 -m theodb_bench --index all --n 5000 --dim 16 --n-queries 100 --k 10 --runs 3 --metric l2
# integration test asserting recall + index used:
PGHOST=localhost PGPORT=55473 PGUSER=postgres PGPASSWORD=postgres PGDATABASE=postgres \
  pytest -m integration tests/test_integration.py -k ivfflat -q
```

## Measured results (one representative run)

| Index | Params | recall@10 | QPS | p95 (ms) | build (ms) | index size |
|---|---|---|---|---|---|---|
| HNSW | ef_search=40 | 0.9940 | 4068.1 | 0.362 | 834 | 1,859,584 B (1.86 MB) |
| HNSW | ef_search=100 | 1.0000 | 1890.3 | 0.996 | 834 | 1,859,584 B (1.86 MB) |
| IVFFlat | probes=1 | 0.5680 | 2932.0 | 0.580 | 102 | 458,752 B (459 KB) |
| IVFFlat | probes=5 (=lists) | 1.0000 | 862.8 | 1.776 | 102 | 458,752 B (459 KB) |

## Honest analysis (the trade-off, measured)

- **Recall curve is correct and monotone.** IVFFlat at `probes=1` scans 1 of 5 clusters → recall **0.568**;
  at `probes=lists` it scans every cluster → recall **1.0000** (IVFFlat stores full-precision vectors, so
  scanning all lists is exact-among-indexed). This is the expected `recall ↑ with probes` behaviour, and the
  integration test asserts it (`recalls` non-decreasing across the probes sweep).
- **Index size: IVFFlat is ~4× smaller** (459 KB vs 1.86 MB) than HNSW on this dataset — a stable result
  (the same ~4× ratio reproduced across runs).
- **Build time: IVFFlat's build completes in less wall-clock time** (this run: 102 ms vs HNSW 834 ms). The
  *direction* is stable (IVFFlat's build consistently finishes ahead of HNSW's on this data), but the
  **exact ratio is machine-load dependent** — across repeated runs we saw IVFFlat-to-HNSW build ratios from
  ~1.8× to ~8× as background load varied. We therefore do not state a fixed multiplier as a property of the
  index; we report the per-run numbers and the stable direction.
- **At equal high recall (1.0), HNSW serves more queries/sec** (1890 QPS @ ef_search=100) than IVFFlat
  (863 QPS @ probes=lists) on this dataset. The win for IVFFlat here is **build time and index size**, not
  query throughput at high recall.
- **Recommendation (evidence-based):** IVFFlat suits workloads that rebuild indexes often or are memory/disk
  constrained and tolerate a recall/QPS knob (`probes`); HNSW suits read-heavy workloads needing high recall
  *and* high QPS. Both are now first-class in the harness — pick on measured evidence, not folklore.

## Caveats

- Synthetic gaussian + low dim (16) and small `n` (5000) give a coarse `lists=5`; real embedding
  distributions and larger `n` shift the curve. The harness supports real datasets via `--hdf5` for a
  follow-up run; the **methodology and the IVFFlat validation are what M9 delivers** (the absolute
  numbers are dataset- and machine-specific, stated here as measured, not as a universal claim).
- **Degenerate `lists=1`** (when `n < 1000`): a single cluster means every probe scans the whole table —
  IVFFlat degrades to an exact full scan (recall ≈ 1.0, no approximation). The harness floors `lists` at 1
  (never the invalid `lists=0`) and collapses the probes sweep to a single point; such a row is exact-via-
  fullscan, not a meaningful ANN operating point. The default `n=5000` (lists=5) is unaffected.
