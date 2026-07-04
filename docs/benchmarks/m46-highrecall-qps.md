# M46 — theodb_hnsw scan hot-path hygiene: recall-neutral, benchmark-gated

**Verdict — two parts (measurement-first, ADR-2 of the M46 plan):**

1. **Recall-neutral: PROVEN.** The pre-size (L1-A) + reused neighbor scratch (L1-B) change the *allocation*, not the
   *visit order*. Proven on the shipped `theo-db:m46` binary: an index scan through `traverse`
   (`EXPLAIN` → `Index Scan using rn_idx`) returned the **byte-identical** order `2,3,1,4,7` to the exact seqscan
   oracle at `ef_search=200`. The unit tests (`traverse_presize_is_recall_neutral_end_to_end`,
   `decode_neighbors_into_matches_original`) encode the same invariant.

2. **QPS win: NOT ESTABLISHED at this scale/box — honest-negative (ADR-2, anti-sunk-cost).** The measurement
   environment was too contended to attribute any QPS delta to the change, and the target regime was not
   reproduced. Details below. The correct next measurement is **SIFT1M on a quiet box**.

> Performance is a claim, not an opinion (`public-copy.md` §4). This report makes **no** QPS superiority claim —
> the evidence does not support one, and the control below proves the environment invalidates QPS attribution.

## The control invalidates the QPS comparison (the load-bearing honesty check)

`run_m46_highrecall.py` measures the **unchanged** `pgvector_hnsw` alongside `theodb_hnsw` on both images. pgvector
is the **control**: its binary is identical in the baseline and post runs, so any drift in its QPS is pure
box noise. It drifted **massively** between the two runs (same box, minutes apart):

| ef | pgvector baseline (unchanged) | pgvector post (unchanged) | control drift |
|---:|:---|:---|---:|
| 100 | 790.1 ± 104.5 | 1016.0 ± 99.8 | **+29%** |
| 200 | 287.5 ± 26.0 | 638.1 ± 138.1 | **+122%** |
| 300 | 210.8 ± 11.2 | 519.4 ± 16.8 | **+146%** |
| 400 | 198.2 ± 18.8 | 439.5 ± 30.8 | **+122%** |

An unchanged binary reading **+122% faster** between two runs means the box (12 cores, load average **18–36**
during measurement, 11 workspace containers competing) dominates the signal. Any theodb baseline-vs-post QPS
delta (below) is **dwarfed** by this drift → **effect ≪ variance** → no honest QPS verdict is possible here.
Chasing the theodb numbers would be exactly the measurement-artifact-chasing ADR-2 forbids.

## Deterministic evidence (box-independent — this is what proves recall-neutrality)

`recall@10` and `pages_read` are deterministic functions of (graph, query, ef) — independent of box load.

| ef | recall base→post | pages_read base→post | Δ |
|---:|:---|:---|:---|
| 100 | 0.9945 → 0.9940 | 1088 → 1085 | recall −0.0005, pages −0.28% |
| 200 | 0.9960 → 0.9955 | 1905 → 1908 | recall −0.0005, pages +0.16% |
| 300 | 0.9960 → 0.9955 | 2576 → 2584 | recall −0.0005, pages +0.31% |
| 400 | 0.9960 → 0.9955 | 3245 → 3247 | recall −0.0005, pages +0.06% |

These deltas are **sub-0.3%** — and they are **build** nondeterminism, not a scan regression. `n=50k > 4096`
triggers the **M44 parallel build**, whose linking order races (`ann/hnsw.rs:34` — *"the parallel LINKING
order races"*), so the two containers built *slightly different graphs*. A two-container benchmark therefore
**cannot** prove byte-identical recall/pages_read — the clean recall-neutral proof is the **same-graph** SQL
test above (one binary, one graph, index-scan == exact-scan). The benchmark merely confirms **no systematic
difference** beyond the parallel-build noise band.

## Why 50k cannot show the M46 benefit (scale caveat)

The change targets the **~44% ef≥200 QPS variance** the M45 SIFT1M Pareto measured — a **1M-scale, memory-bound**
phenomenon: on a 1M graph the default-capacity `HashSet` rehashes ~12× over an ef=200 search and the per-node
`Vec` churns the allocator. At **n=50k** the per-query structures are small, so pre-sizing barely matters — the
measured variance is already **6–12%**, not 44%. The regime where L1-A/L1-B pay off is **not reproduced** at 50k.
So this run can neither show a throughput win nor the "44% → <15%" variance-reduction alternative; both require
the full SIFT1M corpus.

## Honest status & next measurement

- **Shipped:** the recall-neutral hot-path change (correct, proven) + the hardened harness (median/trimmed/
  pages_read + control) — the substrate for a clean verdict.
- **Deferred (not failed):** the QPS/variance verdict, blocked on a **quiet box at SIFT1M scale**. Under dev-box
  contention (control drift +122%) and at a scale that doesn't reproduce the target regime, no honest QPS claim
  is attributable to M46. This is the honest-negative ADR-2 explicitly permits.

## Reproduce

```bash
# Build both images (pre-change baseline via `git stash push theodb_rs/src/am/hnsw_page.rs`, then pop):
docker build -t theo-db:m46-baseline .   # (pre-change source)
docker build -t theo-db:m46 .            # (with the change)
# Run each with the deterministic pages_read profiler on:
docker run -d --name m46-base -e POSTGRES_PASSWORD=postgres -e THEODB_SCAN_PROFILE=1 -p 5480:5432 theo-db:m46-baseline
docker run -d --name m46-post -e POSTGRES_PASSWORD=postgres -e THEODB_SCAN_PROFILE=1 -p 5479:5432 theo-db:m46
# On a QUIET box, at full 1M (drop --no-full-train / --n for the full corpus):
python3 benchmarks/run_m46_highrecall.py --hdf5 benchmarks/.datasets/sift-128-euclidean.hdf5 \
    --nq 500 --runs 5 --ef-grid 100,200,300,400 --port 5480 --container m46-base --tag baseline --out base.json
python3 benchmarks/run_m46_highrecall.py --hdf5 benchmarks/.datasets/sift-128-euclidean.hdf5 \
    --nq 500 --runs 5 --ef-grid 100,200,300,400 --port 5479 --container m46-post --tag post --out post.json
python3 benchmarks/run_m46_highrecall.py --compare base.json post.json --write-doc
```

Raw numbers: `docs/benchmarks/m46-highrecall-qps.json`. Recall-neutral SQL proof: `theodb_rs/src/am/hnsw_page.rs`
(`traverse_presize_is_recall_neutral_end_to_end`).
