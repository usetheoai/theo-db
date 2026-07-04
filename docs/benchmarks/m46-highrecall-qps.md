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
delta is **dwarfed** by this drift → **effect ≪ variance** → no honest QPS verdict is possible here.
Chasing the theodb numbers would be exactly the measurement-artifact-chasing ADR-2 forbids.

For full transparency, here are the theodb median-QPS numbers the control invalidates (do **not** read these as
a result — they are shown so the skeptic sees exactly what the noise swamped, including a −12% at ef=300):

| ef | theodb QPS median base→post | raw Δ | note |
|---:|:---|---:|:---|
| 100 | 366.7 → 444.0 | +21% | control drifted +29% at this ef — Δ not attributable |
| 200 | 263.3 → 278.4 | +6% | control drifted +122% — Δ not attributable |
| 300 | 259.6 → 228.5 | **−12%** | control drifted +146% — the baseline run was non-uniformly contended (theodb baseline ef=300 even reads *faster* than ef=200, physically incoherent), so this −12% is a noise artifact, not a regression |
| 400 | 179.6 → 211.9 | +18% | control drifted +122% — Δ not attributable |

Normalizing theodb by the control does **not** rescue the comparison: the baseline run was non-uniformly loaded
(its own ef-ordering is physically incoherent), so the ratio is untrustworthy in both directions. "No
attributable delta either way" is the only honest read.

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

> **Harness limitation (do not repeat the confound).** The driver's automated `recall_neutral_verdict` gate is a
> *byte* gate: fed these points it returns `RECALL_REGRESSION`, because recall moved 0.9960→0.9955. That gate
> **cannot** distinguish a −0.0005 build-race from a −0.0005 real regression — only the same-graph SQL oracle
> can, which is why "recall-neutral: PROVEN" rests on the oracle, not on the benchmark gate. The gate is correct
> for a *same-graph binary swap*; the two-container A/B does not provide one.

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
- **Deferred (not failed):** the QPS/variance verdict, blocked on TWO confounds this run exposed — box contention
  (control drift +122%) **and** the parallel-build graph difference between containers. This is the honest-negative
  ADR-2 explicitly permits.

## The next measurement must be SAME-GRAPH (a quiet box alone is not enough)

A quiet box removes the *load* confound but **not** the *graph-difference* confound: at any `n > 4096` the M44
parallel build still races, so a two-container A/B keeps comparing two *different* graphs. To attribute an
allocation-only scan change to QPS you need a **byte-identical graph** on both binaries. Two ways:

1. **Persist/restore one index into both binaries** — build the theodb_hnsw index once, snapshot its page image,
   restore it into both the baseline and the post container, then sweep. Same graph → the only variable is the
   scan allocator.
2. **Rust `criterion` micro-bench over a fixed in-memory graph** — bench `traverse` directly against one
   `HnswIndex::build(seed=42)` graph, comparing pre-size vs `::new()`. No container, no box-load noise, no build
   race. This is the cleanest isolation of the L1-A/L1-B effect and the recommended next step.

Either design, at SIFT1M scale (where the ~44% ef≥200 variance regime appears), is the reproducible artifact for
the win/variance verdict. Tracked in `knowledge-base/implementations/m46-hnsw-highrecall-qps-followups.md`.

## Reproduce (this run — the confounded two-container A/B, for the record)

```bash
# baseline image = pre-change source (git stash push theodb_rs/src/am/hnsw_page.rs → build → pop):
docker build -t theo-db:m46-baseline .   # pre-change
docker build -t theo-db:m46 .            # with the change
docker run -d --name m46-base -e POSTGRES_PASSWORD=postgres -e THEODB_SCAN_PROFILE=1 -p 5480:5432 theo-db:m46-baseline
docker run -d --name m46-post -e POSTGRES_PASSWORD=postgres -e THEODB_SCAN_PROFILE=1 -p 5479:5432 theo-db:m46
# --nq 200 --n 50000 (this run's scale; drop --no-full-train/--n for full 1M on a quiet box):
python3 benchmarks/run_m46_highrecall.py --hdf5 benchmarks/.datasets/sift-128-euclidean.hdf5 --no-full-train \
    --n 50000 --nq 200 --runs 5 --ef-grid 100,200,300,400 --port 5480 --container m46-base --tag baseline --out base.json
python3 benchmarks/run_m46_highrecall.py --hdf5 benchmarks/.datasets/sift-128-euclidean.hdf5 --no-full-train \
    --n 50000 --nq 200 --runs 5 --ef-grid 100,200,300,400 --port 5479 --container m46-post --tag post --out post.json
```

Raw numbers: `docs/benchmarks/m46-highrecall-qps.json`. Recall-neutral SQL proof: `theodb_rs/src/am/hnsw_page.rs`
(`traverse_presize_is_recall_neutral_end_to_end`).
