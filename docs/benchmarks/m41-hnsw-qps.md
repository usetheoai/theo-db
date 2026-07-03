# M41 — theodb_hnsw scan QPS optimization: 1.2–1.5× at identical recall (honest, variance-controlled)

**Date:** 2026-07-03
**Verdict:** **WIN (modest, real)** — `theodb_hnsw` scan QPS improved **1.2–1.5×** at **byte-identical recall**,
the win growing with `ef_search` (1.46× at ef=200, statistically significant). NOT the 2.4–3.0× a single noisy
cross-session run first suggested — corrected by a 4-sample variance-controlled A/B (measurement-first honesty).
**Type:** A/B benchmark (the honest oracle; M36/M38/M40 lesson — this CPU has large run-to-run variance).

## Honesty note (why the first number was wrong)

An initial single-run cross-session comparison showed 2.4–3.0×. That was **inflated by CPU throttling variance**
(the same trap M38/M40 documented: a single favorable run is not evidence). A rigorous 4-sample alternating A/B
(baseline and optimized measured back-to-back in the same thermal window, 4× each) gives the real number below.
The recall gate (byte-identical) is unaffected — only the QPS multiplier was corrected down.

## What changed

The on-demand HNSW `traverse` (`theodb_rs/src/am/hnsw_page.rs`) now decodes + scores each visited node **inside
the pinned page scope** (`page::with_page_item`, RAII `SharePin` release guard) instead of copying the item bytes
out with `to_vec` per node, and caches `RelationGetNumberOfBlocksInFork` once per query (was ×2 per node).
Motivation: M40 measured theodb_hnsw slower than theodb_ivfflat because ivfflat amortizes the pin/lock over a
whole page (SIMD over many vectors) while hnsw paid a fixed per-node cost (bounds-check + pin + lock +
**alloc+memcpy** + unpin) scoring one vector per read.

## A/B measurement (n=50000, dim=64, k=10, 300 queries; 4 alternating samples, mean±std)

Baseline = `theo-db:m39`; optimized = `theo-db:m41`. **recall byte-identical** at every ef (same traversal + top-k)
— the only variable is QPS.

| ef_search | recall@10 | QPS baseline (mean±std) | QPS optimized (mean±std) | speedup |
|---|---|---|---|---|
| 10 | 0.313 | 2587 ± 250 | 3197 ± 965 | 1.24× (std overlaps — noisy at low ef) |
| 100 | 0.809 | 670 ± 34 | 925 ± 185 | 1.38× |
| 200 | 0.911 | 385 ± 30 | 562 ± 23 | **1.46× (std bands separated — significant)** |

The win **grows with ef** — architecturally consistent: more visited nodes → more per-node `to_vec` copies
eliminated. At ef=200 the std bands (385±30 vs 562±23) do not overlap → the win is real and statistically
significant. At ef=10 (~10-20 nodes) the per-node saving is small relative to fixed query overhead and the noise
band overlaps, so that point is "directionally positive, not conclusive."

Reproduce (rigorous): alternate `run_m40_carrier.py --n 50000 --dim 64 --runs 2` on `theo-db:m39` vs `theo-db:m41`
≥4× each in the same thermal window; compare mean±std. A single cross-session run is NOT reliable on a throttled CPU.

## Correctness gate (recall byte-identical)

`benchmarks/tests/test_index_am.py` — **8/8 pass** on `theo-db:m41` (post-guard rebuild), including
`test_hnsw_am_persists_pushes_down_and_recalls`. The traverse produces the same top-k; the optimization touched
only the per-node read cost, not the algorithm.

## Safety (unsafe pgrx buffer path)

Focused rust-pgrx audit: SOUND — no buffer leak, no borrow-escape (`with_page_item`'s `T` is not
lifetime-parameterized → the closure cannot return the page slice), `Err`-from-closure still releases. The RAII
`SharePin` (`impl Drop { UnlockReleaseBuffer }`) makes release panic-safe by construction, mirroring
pgvectorscale's `LockedBufferShare`.

## Honest bottom line

A real, recall-preserving **1.2–1.5×** QPS win on the theodb_hnsw scan (significant at ef=200), not the inflated
first number. It narrows the gap vs theodb_ivfflat but does not by itself flip the synthetic head-to-head (M40:
ivfflat still ahead on random-gaussian; the trustworthy verdict needs SIFT1M). Modest but honest; the code is
sound and the recall is airtight.

## Next (evidence-based)

1. Run on **SIFT1M** (real structured data, where the graph's advantage shows) for the trustworthy carrier verdict.
2. Deferred (YAGNI): bitmap visited-set — the copy/nblocks fix delivered the measured win; profile before adding more.
