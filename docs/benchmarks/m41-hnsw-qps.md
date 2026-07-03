# M41 — theodb_hnsw scan QPS optimization: 2.4–3.0× at identical recall

**Date:** 2026-07-03
**Verdict:** **WIN** — `theodb_hnsw` scan QPS improved **2.4–3.0×** at **byte-identical recall**, closing most of the
gap vs `theodb_ivfflat` (from 3–5× slower to ~parity). First positive result after 5 measurement-first negatives.
**Type:** A/B benchmark (the honest oracle; M36 lesson — no internal profiler). Recall unchanged by construction.

## What changed

The on-demand HNSW `traverse` (`theodb_rs/src/am/hnsw_page.rs`) now decodes + scores each visited node **inside
the pinned page scope** (`page::with_page_item`) instead of copying the item bytes out with `to_vec` per node,
and caches `RelationGetNumberOfBlocksInFork` once per query (was called ×2 per node). Motivation: M40 measured
theodb_hnsw 3–5× slower than theodb_ivfflat at matched recall because ivfflat amortizes the pin/lock over a whole
page (SIMD over many vectors) while hnsw paid a fixed per-node cost (bounds-check + pin + lock + **alloc+memcpy** +
unpin) scoring one vector per read. Removing the per-node alloc/copy + the redundant bounds check is the fix.

## A/B measurement (n=50000, dim=64, k=10, 500 queries, 3 runs; `benchmarks/run_m40_carrier.py`)

Baseline = `theo-db:m39` (M40); optimized = `theo-db:m41`. **recall is byte-identical** at every ef (same
traversal order + top-k) — the only variable is QPS.

| ef_search | recall@10 | QPS before (m39) | QPS after (m41) | speedup | p50 before | p50 after |
|---|---|---|---|---|---|---|
| 10 | 0.313 | 1217 | 3538 | **2.9×** | 0.85 ms | 0.27 ms |
| 40 | 0.617 | 566 | 1675 | **3.0×** | 1.76 ms | 0.60 ms |
| 100 | 0.809 | 329 | 865 | **2.6×** | 3.16 ms | 1.16 ms |
| 200 | 0.911 | 214 | 510 | **2.4×** | 5.07 ms | 2.02 ms |

Recall identical across the board (0.3132 / 0.6166 / 0.8092 / 0.9110 in both builds) → the speedup is pure QPS,
not a recall trade. Effect (2.4–3.0×) dwarfs run-to-run variance.

## Gap vs theodb_ivfflat (same run)

| recall | theodb_hnsw QPS (m41) | theodb_ivfflat QPS |
|---|---|---|
| ~0.81 | 865 (ef=100) | ~1000 (interp. probes≈70) |
| ~0.90 | 510 (ef=200) | 609 (probes=100) |

At the high-recall end (~0.90) theodb_hnsw is now ~0.84× ivfflat's QPS (was ~0.35×) — near-parity. On synthetic
random-gaussian (the worst case for a graph) ivfflat is still marginally ahead; on real structured data, where a
graph's traversal advantage shows, this speedup is expected to flip the head-to-head. Honest: the SIFT1M verdict
is still the trustworthy one (M40 caveat) — but the optimization is unambiguous (2.4–3× at identical recall).

## Correctness gate (recall byte-identical)

`benchmarks/tests/test_index_am.py` — **8/8 pass** on `theo-db:m41`, including
`test_hnsw_am_persists_pushes_down_and_recalls` and `test_index_scan_returns_correct_neighbors`. The traverse
produces the same top-k; the optimization touched only the per-node read cost, not the algorithm.

Reproduce: `PGPORT=<port> python3 benchmarks/run_m40_carrier.py --n 50000 --dim 64 --runs 3` on `theo-db:m41`.

## Next (evidence-based)

1. Run this on **SIFT1M** for the trustworthy carrier verdict (M40 caveat) — theodb_hnsw is now competitive.
2. Deferred (YAGNI until measured insufficient): replace the `HashSet<(u32,u16)>` visited set with a bitmap; the
   copy+nblocks fix already delivered 2.4–3×, so the visited-set is not currently the bottleneck.
