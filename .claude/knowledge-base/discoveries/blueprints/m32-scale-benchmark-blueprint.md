# Blueprint: M32 — Scale benchmark harness (≥1M vectors, QPS head-to-head vs pgvector)

> **Discovery verdict:** SHIPPABLE_WITH_CAVEATS — grounded in the mature in-repo `theodb_bench` harness (the prior
> art is 80% in-repo) + the cloned pgvector reference + an empirical host-feasibility probe. Discovery method here
> is in-repo inventory + empirical feasibility (NOT an external-reference halt-loop) because the harness to extend
> already exists; the true unknowns are the 1M-scale ground-truth strategy, the theodb-AM wiring gap, and whether
> this host can run 1M.

**Slug:** `m32-scale-benchmark` · **Owner:** paulohenriquevn · **Created:** 2026-07-02

## Context

M31/M31b proved `theodb_ivfflat` ≤ pgvector at 100k on distinct data. M32 must produce the **scale evidence**
(≥1M, currently `UNBENCHMARKED`): QPS + p50/p95/p99 + recall@10 + build time + index bytes for
`theodb_ivfflat`/`theodb_hnsw` **vs** pgvector `ivfflat`/`hnsw` on a real public dataset (SIFT1M), reproducible,
mean±std ≥3 runs, honest per-knob verdict with ANN-Benchmarks semantics (no cherry-pick).

## Coverage Corner 1 — Integration Tests

Reuse the mature harness `benchmarks/theodb_bench/` — the M32 gate is a NEW integration test
`benchmarks/tests/test_scale_benchmark.py` that runs the 4-way head-to-head at a **scaled-down but real** N in CI
(the ≥1M full run is an operator-invoked artifact, too heavy for CI). Reused, verified building blocks:

- `theodb_bench.harness.run_benchmark(config, db, out_dir)` — orchestrates load→ground-truth→build→query→measure→
  report; already loops over `config["index_specs"]` (multi-index in one run). `benchmarks/theodb_bench/harness.py:29`.
- `theodb_bench.recall.recall_at_k(true_distances, run_distances, k, eps)` — ANN-Benchmarks **distance-thresholded**
  recall (handles ties; the DoD's "ANN-Benchmarks semantics"). `benchmarks/theodb_bench/recall.py:61`.
- `theodb_bench.recall.brute_force_ground_truth(...)` — float32-precision-matched exact k-NN. `recall.py:41`.
- `theodb_bench.metrics.latency_percentiles` (p50/p95/p99/mean/std) + `qps_best_of_n`. `metrics.py:11,26`.
- `theodb_bench.db.VectorDB` — `.build_index(ddl)->seconds`, `.index_size_bytes(name)` (pg_relation_size),
  `.query_topk`, `.assert_index_used`. `db.py:100,131,110,122`.
- `theodb_bench.dataset.load_hdf5_subsample` — ANN-Benchmarks HDF5 (`train`/`test`). `dataset.py:14`.

Test asserts (small real N, both indexes present): every AM produces `0 ≤ recall ≤ 1`, `qps > 0`, `build_ms > 0`,
`index_bytes > 0`; the 4 specs are all exercised; `assert_index_used` passes for each theodb AM (not a seqscan
fallback). Pattern: `benchmarks/tests/test_integration.py:143` (`run_benchmark(config, VectorDB(dsn).connect(), tmp)`).

## Coverage Corner 2 — Dependencies

**No new dependency.** `h5py>=3.10` (BSD-3, dev-only) already present (`h5py 3.16.0` verified) + `numpy 1.26.4`.
Dataset SIFT1M `sift-128-euclidean.hdf5` (~500 MB) fetched from `ann-benchmarks.com` into the gitignored
`benchmarks/.datasets/` (same cache as the existing GloVe-25 127 MB file). SIFT is Euclidean → **l2 metric only**
(matches theodb's `*_l2_ops`; cosine deferred — ADR 0010).

## Coverage Corner 3 — Tools

- ANN-Benchmarks HDF5 layout: `train` (1M×128 base), `test` (10k×128 queries), **`neighbors` (10k×100 precomputed
  ground-truth ids)** — the last is load-bearing for 1M (see Techniques).
- `pg_relation_size(index::regclass)` for index bytes (already wired). `EXPLAIN` for index-usage assertion.
- PG `SET maintenance_work_mem` to let pgvector hnsw build at 1M without spilling pathologically.
- `/usr/bin/time -v` / `docker stats` for peak-RSS capture of the theodb in-memory build (feasibility evidence).

## Coverage Corner 4 — Techniques

**T1 — 1M ground-truth without recomputing 10¹⁰ distances (the scale unlock).** The current harness recomputes
brute-force GT (`recall.py:41`); at 1M base × 10k queries that is 10¹⁰ f32 distances — hours. TWO honest options:
(a) **use the HDF5's precomputed `neighbors`** (ann-benchmarks ships exact GT) — requires loading the FULL 1M train
(no subsample) so `neighbors` ids stay valid, and adapting recall to id-overlap OR recomputing the GT *distances*
only for the true neighbor set (cheap: 10k×100 distances); (b) **subsample queries** (e.g. 1000) and recompute GT
in **chunked numpy** (1000×1M = 10⁹, ~1-2 min, bounded RSS). M32 uses (a) when the full train is loaded, (b) as the
subsample fallback. This is the primary NEW technique vs the existing (small-N) harness.

**T2 — theodb AMs have NO tunable query knob (honest limitation).** pgvector sweeps `ef_search`/`probes`
(`__main__.py:52,66`). theodb `SCAN_PROBES=10` / HNSW `SCAN_EF=64` are **fixed Rust constants** (no GUC/reloption
yet). So M32 reports theodb at its **single fixed operating point** vs pgvector's **sweep** — the per-knob verdict
states this honestly (a configurable knob is a future milestone). This is a real finding, not a defect.

**T3 — theodb in-memory build is the scale ceiling.** `IvfflatIndex::build` / `HnswIndex::build` load the full
corpus into `Vec<Vec<f32>>` in ONE pg backend, then serialize to pages (M26). Est. at 1M×128: ~512 MB corpus (f32)
+ HNSW graph ~256 MB → ~0.8-1 GB per build; k-means (100 centroids × 10 Lloyd × 1M×128) is compute-heavy
(single-thread scalar, ~10-30 min); HNSW build O(N·efc·log N) is the slowest (tens of min). Host has ~7 GB RAM
available (verified `free`) → **feasible but tight + slow**. Mitigations: load corpus float32 (halve numpy RSS),
bound query set, raise `maintenance_work_mem` for pgvector. If a build OOMs, surface honestly + report the ceiling
reached (measurement-first; NEVER fake a 1M number — `public-copy.md`, Rule 3).

**T4 — no cherry-pick (ANN-Benchmarks ethos).** Report the FULL QPS-recall frontier (all sweep points for pgvector,
the single point for theodb) + build time + index bytes; the verdict per (index-family, knob) is `parity` /
`superior` / `inferior` with the number, never the best-only.

## Cross-cutting Comparison

| | Existing harness (M2, GloVe-25 50k) | M32 target (SIFT1M ≥1M) |
|---|---|---|
| Dataset | GloVe-25 (n=50k, dim=25), subsampled | SIFT1M (1M×128), full train |
| Indexes | pgvector hnsw/ivfflat/diskann only | + theodb_ivfflat + theodb_hnsw (4-way) |
| Ground truth | recomputed brute-force | HDF5 `neighbors` (full-train) OR chunked subsample |
| theodb AM knobs | n/a (not wired) | fixed op-point (no sweep — T2) |

## ADRs

### D1 — Extend the existing `theodb_bench` harness; do NOT build a new one
Parsimony rung 4 (reuse installed). The harness already does load/GT/build/query/recall/percentiles/QPS/report.
M32 adds: SIFT1M acquisition, `_theodb_ivfflat_spec`/`_theodb_hnsw_spec` in `__main__.py`, a full-train GT path,
and the scale integration test. Rejected: a standalone 1M script (would duplicate the mature harness).

### D2 — l2-only at 1M (SIFT is Euclidean); cosine deferred
theodb exposes only `*_l2_ops` (ADR 0010). SIFT1M is Euclidean → l2 is the correct + sufficient metric. Cosine
head-to-head waits for theodb cosine opclasses (M-future). Honest scope limit, not a gap.

### D3 — theodb reported at fixed op-point vs pgvector sweep (T2)
No theodb query knob exists. Report the single point honestly rather than fabricate a sweep. A configurable
`probes`/`ef` reloption is a named future milestone.

### D4 — CI runs a scaled real N; the ≥1M full run is an operator artifact
The ≥1M run (tens of minutes to hours) is too heavy for the per-commit gate. The committed test validates the 4-way
harness at a real but smaller N; the 1M evidence is a reproducible operator-invoked artifact in `docs/benchmarks/`
(same pattern as the GloVe-25 artifact). The DoD's "≥1M" is satisfied by the artifact + its reproduction command.

## Recommendations

1. Acquire SIFT1M into `.datasets/` (gitignored); record source URL + SHA256.
2. `dataset.py`: add a full-train loader that also returns the HDF5 `neighbors` GT (T1 path a).
3. `__main__.py`: add `_theodb_ivfflat_spec` + `_theodb_hnsw_spec`; extend `--index` with a `4way` mode + a
   `--full-train` / GT-from-neighbors flag.
4. `benchmarks/tests/test_scale_benchmark.py`: 4-way harness gate at a real smaller N (CI-safe).
5. Run the ≥1M artifact; write `docs/benchmarks/m32-scale-sift1m.{md,json}` with mean±std ≥3 runs, hardware, peak
   RSS, per-knob honest verdict. If 1M OOMs on this host, report the ceiling reached + the exact resource wall.
