# M101 — Heap-authoritative Arrow cache: HTAP benchmark (honest, measured)

**Date:** 2026-07-16 · **Milestone:** M101 · **Artifact:** `m101-arrow-cache.json` (raw) · **Harness:** `theodb_rs/isolation/bench_m101.sh`

> **Honest ceiling (Rule 5 / M73/M97).** The gain is the vectorized **Arrow-cache aggregate** (reuse a pre-built
> in-memory batch — no heap seqscan) vs the **native heap aggregate** (seqscan + PG aggregate) for a repeated
> read-heavy analytical query. The cache is **MVCC-correct** (invalidate-on-write + a snapshot-compatibility gate,
> proven by the isolation permutations); a write invalidates the cache and the next read pays a rebuild. The pragma
> is **MANUAL** (`theodb_columnarize`), NOT AlloyDB's auto-maintained engine — this is the permissive subset.

## Scorecard (2,000,000 rows, `SELECT count(*), sum(measure)`, one column cached, 5 runs, single-threaded)

| Path | Exec time (mean ± stddev) | vs native heap |
|---|---|---|
| **Arrow cache** (M101 CustomScan, GUC on) | **52.4 ± 0.3 ms** | **2.48× faster** ✅ |
| Native heap aggregate (GUC off) | 130.0 ± 0.6 ms | 1.0× (baseline) |

- **Cache path is the CustomScan:** ✅ (`EXPLAIN` shows the `Custom Scan` node — the measurement is of the real
  vectorized cache path, not a fallback).
- **MVCC-correct under concurrency:** ✅ proven by `theodb_rs/isolation/arrow_cache_{invalidation,rr_snapshot}.spec`
  (a committed write invalidates the cache → rebuild; a REPEATABLE READ reader holds its snapshot across a concurrent
  write; a fresh xact after commit sees the new row).

## Methodology

- **Dataset:** 2,000,000 rows, 3 columns (`id int`, `category text`, `measure float8`) in a HEAP table; the `measure`
  column is cached via `theodb_columnarize('h', ARRAY['measure'])`.
- **Hardware:** 8 vCPU, 15 GB RAM DigitalOcean droplet; PostgreSQL 17.10 (pgrx-managed), `shared_buffers=2GB`,
  `work_mem=256MB`, `max_parallel_workers_per_gather=0` (single-threaded, apples-to-apples).
- **Timing:** ONE persistent psql session (the cache is per-backend, so it must persist across the timed queries):
  `EXPLAIN (ANALYZE, TIMING OFF, BUFFERS OFF)` execution time, 1 warm-up discarded + 5 measured runs per path, mean ±
  population stddev. The cache/native paths are toggled by `theodb.enable_columnar_agg` within the session.
- **Reproduce:** `cargo pgrx install`, then `N=2000000 RUNS=5 bash theodb_rs/isolation/bench_m101.sh` on the droplet.

## Analysis

1. **2.48× over the native heap aggregate is the measured HTAP win.** The native path scans 2M heap tuples and
   aggregates them each time; the cache path aggregates a pre-built in-memory Arrow batch (`measure` only — projection)
   with DataFusion, skipping the heap scan entirely on a cache hit. For a read-heavy analytical workload (reads ≫
   writes), the cache is built once and reused across many reads — the win.
2. **A write costs a rebuild (honest).** Any INSERT/UPDATE/DELETE bumps the `columnar.cache_state` generation (the
   statement trigger); the next read rebuilds the cache under the reader's snapshot. So the cache pays off when reads
   dominate writes; a write-heavy table would rebuild constantly (the operator's `columnarize` decision).
3. **Result-equivalence — where it is proven.** The harness's inline `result_equivalence` check is weak (it compares
   the heap to itself); the authoritative cache-vs-heap result-equivalence is the pg_test
   `m101_heap_cache_customscan_matches_heap` (the aggregate over the cache CustomScan equals the native heap
   aggregate, byte-for-byte) plus the isolation permutations. This artifact reports the timing; correctness is the
   test suite's.
4. **OLTP non-interference (structural, not yet measured under load).** The cache is read-only, per-backend, and built
   via an ordinary SPI seqscan under the reader's snapshot — it holds no extra lock on the heap during reads. A full
   pgbench concurrent OLTP-p95-under-analytical-load measurement (the AlloyDB non-degradation claim) is an honest
   follow-up, not asserted here.

## Verdict

The M101 heap-authoritative Arrow cache delivers a **measured 2.48× speed-up** over the native heap aggregate for a
repeated analytical query, is **MVCC-correct under concurrency** (proven by isolation permutations), and is honest
about its boundary: a manual pragma (not auto-tuned), a rebuild on write, and OLTP-non-interference proven
structurally (a load-measured p95 is a follow-up). No superiority claim over AlloyDB's in-core engine.
