# M99 — `theodb_columnar` TAM: columnar-vs-heap benchmark (honest, measured)

**Date:** 2026-07-16 · **Milestone:** M99 · **Artifact:** `m99-columnar-tam.json` (raw) · **Harness:** `theodb_rs/isolation/bench.sh`

> **Honest ceiling (Rule 5 / M73/M97 discipline).** M99 ships the columnar **storage substrate** — an own-code
> append-only `TableAmRoutine` with a true column-major on-disk format (per-column zstd chunks + a min/max
> directory) and MVCC delegated to a heap catalog. It does **NOT** yet have projection pushdown, min/max
> skip-pruning consumption, or vectorized execution — those are **M100** (the DataFusion CustomScan). A plain
> `seqscan` therefore decodes **every** column of **every** chunk group and reconstructs full heap tuples, so its
> scan wall-time is **at parity-or-slower than heap, by design**. The measured M99 win is **on-disk size
> (compression)**. This is **not** a performance-superiority claim.

## Scorecard

| Metric | Columnar (`theodb_columnar`) | Heap | Result |
|---|---|---|---|
| **On-disk size** (1M rows, 4 cols) | **6.5 MB** | 60.2 MB | **9.2× smaller** ✅ (the honest win) |
| Full-aggregate scan (mean of 5) | 2331.3 ± 6.4 ms | 87.7 ± 0.8 ms | ~26× slower (expected — no projection/vectorization) |
| GROUP BY category scan (mean of 5) | 2886.7 ± 8.6 ms | 178.7 ± 1.1 ms | ~16× slower (expected) |
| Result-equivalence (aggregates) | — | — | **identical** ✅ |

## Methodology

- **Dataset:** 1,000,000 rows, 4 columns — `id int` (monotonic), `category text` (10 distinct values),
  `measure float8` (= `id * 1.5`), `flag bool`. Same data inserted into a `theodb_columnar` table and a heap table.
- **Compression is dataset-dependent — the 9.2× is NOT a universal multiplier.** All four columns here are
  compression-favorable by construction (10-value category, monotonic `id`, linearly-derived `measure`, boolean
  `flag`). High-entropy data (random UUIDs, uncorrelated floats) compresses far less. The on-disk win is real but
  data-specific.
- **Size scope:** `pg_relation_size('t_col')` measures the columnar MAIN fork only. The MVCC visibility catalog
  `columnar.stripe` (≈1 row per stripe — a single stripe / a few KB at this scale) is a separate heap and is NOT
  included; immaterial to the ratio here, but disclosed.
- **Result-equivalence scope:** the harness cross-checks `count(*)` + `round(sum(measure))` columnar-vs-heap
  (`"t"`); per-group GROUP BY correctness is proven by the isolation suite (`theodb_rs/isolation/columnar_*.spec`),
  not re-measured here.
- **Hardware:** 8 vCPU, 15 GB RAM DigitalOcean droplet; PostgreSQL 17.10 (pgrx-managed), `shared_buffers=1GB`,
  `max_parallel_workers_per_gather=0` (single-threaded, apples-to-apples).
- **Timing:** `EXPLAIN (ANALYZE, TIMING OFF, BUFFERS OFF)` execution time, 1 warm-up discarded + 5 measured runs,
  reported as mean ± population stddev. Heap `VACUUM ANALYZE`d first.
- **Reproduce:** `cargo pgrx install` (build the current .so), then `bash theodb_rs/isolation/bench.sh` on the
  droplet (`N=1000000 RUNS=5` by default).

## Analysis (why columnar is slower here, and why that is correct for M99)

1. **On-disk compression is real and large (9.2×).** The `category` column (`'cat_0'..'cat_9'`) and the monotonic
   `id`/`measure` streams compress well under per-column zstd; column-major packing removes per-row tuple headers.
   This is the deterministic, reproducible M99 win.
2. **Scan is slower because M99 does the maximal amount of work per row:** it `read_chunked` + zstd-decodes **all
   4 columns** of **all 100 chunk groups** (no projection — the aggregate only needs `measure`/`category`, but the
   TAM cannot know that; a plain seqscan receives no projection), then **reconstructs a heap tuple** per row
   (`heap_form_tuple`) which the executor immediately **deforms** again. That form→deform round-trip + full-column
   decode is pure overhead vs heap's direct tuple read.
3. **The scan-speed win lands in M100**, where the DataFusion CustomScan pushes projection + quals into the
   columnar leaf (decode only the needed columns, skip chunk groups via the min/max directory this milestone
   already writes) and runs a vectorized `ExecutionPlan` over Arrow batches — bypassing the per-row form→deform
   entirely. M99 stores what M100 consumes.

## Verdict

M99's columnar substrate is **correct** (result-identical to heap, MVCC-isolation-proven, crash-safe) and
delivers a **9.2× on-disk compression** win. Scan throughput is honestly **not** improved at this milestone — that
is the M100 deliverable. No superiority claim is made over heap scan speed or over AlloyDB's in-core engine.
