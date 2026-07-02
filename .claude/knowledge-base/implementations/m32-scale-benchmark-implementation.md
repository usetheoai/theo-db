---
slug: m32-scale-benchmark
milestone_id: M32
created_at: 2026-07-02
goal: Produce reproducible ≥1M-vector head-to-head scale evidence (theodb vs pgvector) by extending theodb_bench
verdict: IMPLEMENTATION_COMPLETE
---

# M32 — Scale benchmark harness (≥1M, SIFT1M) — implementation summary

## Goal (met)

Reproducible ≥1M-vector 4-way head-to-head (theodb_ivfflat/theodb_hnsw vs pgvector ivfflat/hnsw) with QPS +
p50/p95/p99 + recall@10 + build time + index bytes, mean±std ≥3 runs, hardware cited, honest per-knob verdict.
**MET** — `docs/benchmarks/m32-scale-sift1m.{md,json}` at n=1 000 000, dim=128, 3 runs.

## What shipped

| Task | Change | Files |
|---|---|---|
| T1.1 | `neighbors_ground_truth` (exact GT from HDF5 `neighbors`, 10⁶ ops, chunked, float32 contract) + `load_hdf5_full` (full train + neighbors-GT) | `benchmarks/theodb_bench/recall.py`, `dataset.py` |
| T2.1 | `_theodb_spec` (fixed op-point, l2-only, no ef/probes knob); `--index 4way/theodb_ivfflat/theodb_hnsw`; `--full-train`; cosine skips theodb (no fabricated opclass); harness `full_train` GT branch + report n from corpus | `benchmarks/theodb_bench/__main__.py`, `harness.py` |
| T2.2 | per-spec `query_cap` (theodb_hnsw O(N)-scan tractability), recorded in the result label | `harness.py`, `__main__.py` |
| T3.1 | 4-way scale integration test (real SIFT subsample) + neighbors-GT + query_cap unit tests | `benchmarks/tests/test_scale_benchmark.py` |
| T3.2 | operator driver + the ≥1M artifact | `benchmarks/run_m32_sift1m.py`, `docs/benchmarks/m32-scale-sift1m.{md,json}` |
| T4.1 | honest per-knob verdict + CHANGELOG | `docs/benchmarks/m32-scale-sift1m.md`, `CHANGELOG.md` |

## Evidence (SIFT1M, n=1M, dim=128, k=10, 1000 queries, 3 runs, single-thread builds, i7-1355U)

| index | knob | recall@10 | QPS | p50 ms | build s | size MB |
|---|---|---|---|---|---|---|
| pgvector hnsw | ef=100 | 0.9814 | 237.5 | 4.36 | 292 | 820 |
| pgvector ivfflat | probes=10 | 0.9814 | 242.3 | 4.28 | 61 | 550 |
| **theodb_ivfflat** | fixed | **0.9876** | 30.7 | 32.50 | 297 | **533** |
| theodb_hnsw | fixed [q=50] | 0.9640 | 1.6 | 607 | 903 | 824 |

## Honest verdict

theodb_ivfflat: **recall SUPERIOR (0.9876) + index SUPERIOR (533 MB)**, but **QPS INFERIOR (~8×)** — fixed 100-list
under-partitioning at 1M (no `lists` knob) → scans ~100k candidates vs pgvector's ~10k. theodb_hnsw INFERIOR
(O(N)-per-query blob scan — M31 structured partial-read is ivfflat-only). Build INFERIOR (single-thread scalar).
**North Star vector-superiority pillar NOT met on QPS at scale** — recall parity/superiority only. Named next
levers: (1) configurable ivfflat lists/probes, (2) structured hnsw scan, (3) parallel builds.

## Gates

- Unit: 7 passed (neighbors-GT == brute force; 4way config; l2-only; query_cap); 40 harness tests no regression; ruff clean.
- Integration: `test_4way_scale_harness_runs_on_real_sift` passed (real SIFT n=20k, all 4 AMs via their own index).
- ≥1M artifact committed with mean±std ≥3 runs + hardware + peak RSS (~3.3 GB) + honest verdict + repro command.

## Key engineering findings (measurement-first)

- The neighbors-GT loader is the ≥1M unlock (brute force would be 10¹⁰ / hours).
- Two fairness fixes during the run (honesty): single-thread builds for both (theodb has no parallel path); and
  pgvector ivfflat `lists` must be derived from the REAL train size (1M→lists=1000), not the CLI default (a first
  run built it at lists=5 — caught and corrected before the final artifact).
- theodb_hnsw's O(N)-per-query scan (4.2 s cold / 0.6 s warm at 1M) makes it impractical at scale — surfaced by
  the harness, capped for tractability, reported honestly.

## Not in scope (honest, → future milestones)

- Configurable theodb `lists`/`probes` reloption (the QPS lever). Structured (partial-read) scan for theodb_hnsw.
  Parallel theodb builds. Cosine opclass for theodb AMs (l2-only today — ADR 0010).
- Parallel-build pgvector comparison (held single-threaded here for a fair build-time axis).
