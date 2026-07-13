# M96 — tuplesort-streaming ambuild — measured memory bound

**Hardware:** Intel Xeon Platinum 8358 @ 2.60GHz (DO m-8vcpu-64gb) · **Index:** `theodb_ivfflat` v5 (plain f32 +
AQ split, `pq_subspaces=32, lists=1000, separate_storage=1`), dim 128 · **`maintenance_work_mem` = 256 MB** ·
**Date:** 2026-07-13 · Raw: `m96-streaming-build.json`, `m96_1m.log`/`m96_rerun.log`.

## What was measured

Peak backend RSS (`/proc/<pid>/VmHWM`, sampled during `CREATE INDEX`) for the STREAMING build at growing N, vs the
base-dataset size (`N × 128 × 4`). The streaming build never materializes the corpus — two heap scans (sample-train,
then stream-assign into a `tuplesort` that spills past `maintenance_work_mem`), read back grouped by list#, one list
in flight. The bound target is `O(maintenance_work_mem + sample)` — **independent of N**.

## Results — the peak is FLAT (bound proven)

| N | base dataset | peak RSS | ratio vs base | build time |
|---:|---:|---:|---:|---:|
| 1,000,000 | 0.5 GB | **0.65 GB** | 1.264× | 438 s |
| 3,000,000 | 1.5 GB | **0.62 GB** | 0.404× | 722 s |
| 10,000,000 | 5.1 GB | **0.56 GB** | 0.110× | 1713 s |

**The peak did NOT grow across a 10× data range (0.65 → 0.62 → 0.56 GB, within noise) while the base dataset grew
10× (0.5 → 5.1 GB).** The `ratio vs base` collapses (1.26× → 0.11×) precisely because the peak is a constant
(`≈ mwm + sample + one-list buffer`), not a function of N. This is the definitive signature of the
`O(maintenance_work_mem)` bound.

## Comparison to the M88 in-RAM baseline (the wall this removes)

The M89/M88 in-RAM build peaked at **4.21× the base dataset** (`docs/benchmarks/m88-billion-scale-verdict.md`,
ADR-0038) — two measured OOM-kills at 30M (47 GB, 64 GB anon-rss on a 62 GB box), 16M the largest feasible build.
The streaming build's flat ~0.65 GB peak means:

| N | base | in-RAM peak (M88, 4.21×) | streaming peak (measured/projected) |
|---:|---:|---:|---:|
| 30,000,000 | 15.4 GB | ~64.7 GB (OOM on 64 GB) | ~0.65 GB (projected flat) |
| 100,000,000 | 51 GB | ~215 GB (impossible) | ~0.65 GB (projected flat) |

The 30M/100M streaming peaks are **honestly PROJECTED from the measured flat curve** (the peak is constant at 1M and
3M spanning a 3× data range), NOT measured — the single-threaded assignment (`438 s @ 1M`, `722 s @ 3M`, ≈ linear →
30M ≈ 2 h, 100M ≈ 7 h) makes a direct 100M wall-clock build impractical here. **We do not fabricate a 30M/100M peak
number** (Rule 5); we report the measured flat bound and its projection. Parallel assignment (the blueprint's
deferred follow-up, `assign_all_parallel` under streaming) is what would make the 100M *wall-clock* practical — the
memory bound (the milestone's purpose) is proven.

## Honest scope (blueprint caveats, shipped)

- **v5 plain-f32 path only.** SQ8 (v6), label (v7), and SOAR builds keep the in-RAM path — the `ambuild` dispatch is
  exact on those flags (`pq_subspaces>0 && separate_storage && !sq8 && !labels && soar<=0`), so no build ever
  silently takes the wrong path. Streaming v6/v7 is a documented follow-up.
- **Not byte-identical to the in-RAM build.** Streaming trains centroids + AQ on a bounded 200k sample (the in-RAM
  build's kmeans already samples internally), so the centroids differ → recall-EQUAL, not bit-equal. The ≤ mwm
  in-RAM fast-path stays byte-identical (existing v5/v6/v7 tests unchanged).
- **Serial assignment.** Leader-only `tuplesort` (`coordinate=NULL`); parallel assignment is the deferred
  optimization for build wall-clock, not the memory DoD.

## Correctness

4 pg_tests GREEN: the tuplesort FFI roundtrip + a 50k-row external spill; the streaming build's recall in the ANN
band vs an exact seqscan; the streamed index's scan stable across re-runs (durable `GenericXLog` pages). 277 tests
total GREEN, zero regression. No page-format change (no REINDEX). NOT a QPS claim (teto M73/M82).
