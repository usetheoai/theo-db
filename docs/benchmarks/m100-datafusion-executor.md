# M100 — DataFusion vectorized CustomScan aggregate: benchmark (honest, measured)

**Date:** 2026-07-16 · **Milestone:** M100 · **Artifact:** `m100-datafusion-executor.json` (raw) · **Harness:** `theodb_rs/isolation/bench_m100.sh`

> **Honest ceiling (Rule 5 / M73/M97).** The gain claim is the **vectorized DataFusion CustomScan vs the M99
> row-at-a-time TAM seqscan on the SAME columnar data** — the two ways to aggregate a `theodb_columnar` table. vs a
> heap row-store is **context only** (different storage). This is **not** a superiority claim vs AlloyDB's in-core
> engine. Slice 1 covers `count(*)` / `sum(float8)` without GROUP BY / WHERE; GROUP BY + filter pushdown widen the
> gain in later slices.

## Scorecard (2,000,000 rows, `SELECT count(*), sum(measure)`, 5 runs, single-threaded)

| Path | Exec time (mean ± stddev) | vs M99 seqscan |
|---|---|---|
| **Vectorized columnar** (M100 CustomScan, GUC on) | **531.2 ± 3.0 ms** | **9.89× faster** ✅ |
| M99 seqscan columnar (GUC off) | 5251.2 ± 16.1 ms | 1.0× (baseline) |
| Heap row-store (context) | 147.3 ± 0.6 ms | — |

- **Vectorized path is the CustomScan:** ✅ (`EXPLAIN` confirms the `Custom Scan (theodb_columnar_agg)` node — the
  measurement is of the real vectorized path, not a fallback).
- **Result-equivalence:** ✅ the vectorized aggregate equals the heap aggregate (`count`/`sum` identical).

## Methodology

- **Dataset:** 2,000,000 rows, 5 columns (`id int`, `category text`, `bucket int`, `flag bool`, `measure float8`).
  Same data in a `theodb_columnar` table and a heap table.
- **Hardware:** 8 vCPU, 15 GB RAM DigitalOcean droplet; PostgreSQL 17.10 (pgrx-managed), `shared_buffers=2GB`,
  `work_mem=256MB`, `max_parallel_workers_per_gather=0` (single-threaded, apples-to-apples).
- **Timing:** `EXPLAIN (ANALYZE, TIMING OFF, BUFFERS OFF)` execution time, 1 warm-up discarded + 5 measured runs,
  mean ± population stddev. The vectorized/seqscan paths are toggled by `theodb.enable_columnar_agg`.
- **Reproduce:** `cargo pgrx install`, then `N=2000000 RUNS=5 bash theodb_rs/isolation/bench_m100.sh` on the droplet.

## Analysis

1. **9.89× over the M99 seqscan is the real M100 win, and it is measured.** The M99 seqscan decodes *all* 5 columns
   of every chunk group and reconstructs a full heap tuple per row (`heap_form_tuple`) which the executor then
   deforms — pure overhead. The M100 CustomScan decodes *only* the `measure` column (projection pushdown, Phase B),
   builds an Arrow array, and runs a vectorized DataFusion aggregate — no per-row form/deform. Same storage, same
   data, ~10× less work.
2. **Heap is still faster than the vectorized columnar for this simple aggregate (147 ms vs 531 ms), and that is
   honest.** A heap `count/sum` reads tuples directly with zero decode; the columnar path pays a decode + Arrow
   build cost that a single narrow aggregate does not amortize. The columnar-vectorized advantage grows with wider
   projections, more columns pruned, GROUP BY (vectorized hash aggregate), and larger-than-RAM scans — none of which
   this minimal slice exercises. No superiority over heap (or AlloyDB) is claimed.
3. **pg_duckdb comparison:** not installed on the droplet; the harness runs without it and this is disclosed rather
   than fabricated. The single-planner distinction from pg_duckdb (one plan vs two engines, ADR-0023) is an
   architecture property, not a number this benchmark asserts.

## Verdict

The M100 DataFusion CustomScan is **correct** (result-identical to heap, EXPLAIN-visible single-plan node, safety
discipline: `HeldInterrupts` + a `work_mem` MemoryPool that errors-not-panics + `target_partitions=1` Send-pinning)
and delivers a **measured 9.89× speed-up over the M99 row-at-a-time seqscan** on columnar-resident data. The gain is
real and honest; it is not a claim of superiority over heap scan or AlloyDB.
