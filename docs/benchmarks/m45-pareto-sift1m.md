# M45 — rigorous mean±std recall×QPS Pareto: theodb_hnsw vs pgvector hnsw on SIFT1M

**Verdict:** **PARITY** (effect>variance gate over shared recall levels).  
**Config:** n=1000000, dim=128, nq=500, runs=3 (mean±std), ef_grid=[40, 64, 100, 200, 400], seed=2026, metric=l2, dataset=sift-128-euclidean.hdf5.  
**Build (matched):** m=16, ef_construction=64, max_parallel_maintenance_workers=0 (matched), maintenance_work_mem=2GB (pgvector only; theodb builds in Rust memory). theodb build=271.3s, pgvector build=466.6s.  
**Type:** measurement — the rigorous mean±std the M42 signal (`docs/benchmarks/sift1m-carrier-verdict.md`) lacked. Delivers `public-copy.md` §4 half 1 (reproducible artifact); half 2 (independent third-party reproduction) remains OPEN.

## Pareto frontiers (mean ± std over runs, recall@10 vs exact GT)

### theodb_hnsw

| ef_search | recall@10 | QPS (mean ± std) | nq |
|---|---|---|---|
| 40 | 0.9278 | 294.0 ± 15.5 | 500 |
| 64 | 0.9646 | 178.1 ± 9.8 | 500 |
| 100 | 0.9832 | 139.9 ± 2.8 | 500 |
| 200 | 0.9932 | 43.5 ± 19.1 | 500 |
| 400 | 0.9968 | 44.8 ± 2.8 | 500 |

### pgvector_hnsw

| ef_search | recall@10 | QPS (mean ± std) | nq |
|---|---|---|---|
| 40 | 0.936 | 221.3 ± 9.3 | 500 |
| 64 | 0.9678 | 160.9 ± 5.0 | 500 |
| 100 | 0.9866 | 108.6 ± 1.6 | 500 |
| 200 | 0.9956 | 62.8 ± 1.1 | 500 |
| 400 | 0.9986 | 13.9 ± 8.6 | 500 |

## Matched-recall margin (Pareto interpolation, effect > variance gate)

| shared recall | QPS theodb | QPS pgvector | margin (×) | effect>variance |
|---|---|---|---|---|
| 0.936 | 268.2 | 221.3 | 1.212 | yes |
| 0.9646 | 178.1 | 167.0 | 1.067 | no |
| 0.9678 | 171.5 | 160.9 | 1.066 | no |
| 0.9832 | 139.9 | 118.1 | 1.185 | yes |
| 0.9866 | 107.1 | 108.6 | 0.986 | no |
| 0.9932 | 43.5 | 75.0 | 0.58 | yes |
| 0.9956 | 44.4 | 62.8 | 0.706 | yes |
| 0.9968 | 44.8 | 43.2 | 1.036 | no |

**Honest verdict:** PARITY. A `SUPERIOR` verdict is licensed only when theodb QPS exceeds pgvector's at EVERY shared recall level by >5% AND the gap exceeds the combined std (PRD D3, anti-sunk-cost). `PARITY` = no defensible claim in either direction; `INFERIOR` = pgvector wins.

## The headline: the M42 superiority signal does NOT survive rigor

The M42 verdict (`docs/benchmarks/sift1m-carrier-verdict.md`) reported theodb_hnsw **~1.7–2.8× faster** than
pgvector hnsw at matched recall — on a **200-query best-of-N single run**. Under rigorous mean±std
(500 queries, ≥3 timed runs, exact GT), **that superiority does not reproduce**: the margin is **PARITY**.
theodb_hnsw is faster at low-to-mid recall (1.07–1.21× at recall ≤ 0.983) and pgvector is faster at high
recall (0.58–0.71× at recall ≥ 0.993) — the two frontiers **interleave**, and most crossings sit inside the
combined std. **The honest conclusion: theodb_hnsw and pgvector hnsw are competitive (PARITY) on the
recall×QPS Pareto frontier at 1M SIFT1M — NOT a theodb superiority.** The M42 "1.7–2.8×" is retracted as a
best-of-N + small-sample + warm-cache artifact (the same failure mode that shrank M41's 2.4-3.0× to 1.2-1.5×).

## Cross-run sensitivity (why the verdict is PARITY, not a clean win either way)

Two independent rigorous runs on this machine gave **different verdicts** — run A (1 warmup pass) →
`INFERIOR`; run B (2 warmup passes, tabulated above) → `PARITY`. The margin therefore sits **within
run-to-run measurement noise** on a CPU-contended dev box (concurrent workspace containers). Two operating
points remain visibly noisy even in run B (theodb ef=200: 43.5 **± 19.1**, non-monotonic vs ef=400;
pgvector ef=400: 13.9 **± 8.6**), so the high-recall tail should not be over-read. (The pgvector ef=400
point at recall 0.9986 sits ABOVE the shared overlap band [0.936, 0.9968], so it is excluded as a shared
recall level — but it still bounds the top interpolation, and the effect gate uses the frac-weighted
interpolated std so that noise is carried into the gate, not dropped.) The stable mid-band
points (theodb ef=100: 139.9 ± 2.8 vs pgvector ef=100: 108.6 ± 1.6) are the most trustworthy and show
theodb modestly ahead there — but not by a margin that licenses a public claim. Per-run QPS is recorded in
the `.json` (`qps_runs`) for full transparency.

## Honest bottom line

- **No superiority claim is licensed** (`public-copy.md` §4). The rigorous verdict is **PARITY** — theodb_hnsw
  is a competitive carrier, not a demonstrably faster one, vs the SOTA permissive baseline (pgvector hnsw).
- This **refutes**, honestly, the strongest prior vector-superiority signal (M42). The North Star P0
  (vector superiority vs the reachable SOTA) is **NOT met on recall×QPS at 1M** — it is parity.
- The clear next lever is **theodb_hnsw scan latency + variance reduction** (the `goto-p0-vector-superiority`
  memory names latency as the unmet pillar; this confirms it empirically — theodb's high-recall QPS falls
  off and is noisier than pgvector's).

## Honest caveats
- Single machine, warm cache, local dev CPU — the matched-recall margin with variance is the deliverable; absolute QPS is machine-specific.
- `public-copy.md` §4 **half 2 (independent reproduction) is OPEN** — this artifact is built for it (fixed seed, pinned images, exact command) but is not itself independent.
- theodb_hnsw scan is O(ef·M) since M35+M41 (the M42 200-query cap is lifted here to nq=500).

## Reproduce
```
python3 benchmarks/run_m45_pareto.py --hdf5 benchmarks/.datasets/sift-128-euclidean.hdf5 --nq 500 --runs 3 --port <theo-db:m44 port> --write-doc
```
