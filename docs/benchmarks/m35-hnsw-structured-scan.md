# M35 — theodb_hnsw page-native structured scan (partial-read, à la M31 for the graph)

**Dataset:** sift-128-euclidean (SIFT1M) · n=1,000,000 dim=128 k=10 · queries=1000 (seed 42) · runs=3 · sha=36dd950

**Hardware:** 13th Gen Intel(R) Core(TM) i7-1355U · 12 cores · 15.3 GB RAM · AVX2=True

**Goal:** O(N) whole-blob scan → O(ef·M) on-demand structured traversal for theodb_hnsw

## Recall–QPS frontier at 1M (ef_search sweep)

| params | recall@10 | QPS | p50 (ms) | p95 (ms) | build (ms) | index bytes |
|---|---|---|---|---|---|---|
| ef_search=40 | 0.9272 | 318.9 | 3.15 | 4.78 | 1050546 | 759250944 |
| ef_search=100 | 0.9789 | 100.4 | 10.06 | 14.07 | 1050546 | 759250944 |
| ef_search=200 | 0.9926 | 60.4 | 17.56 | 26.67 | 1050546 | 759250944 |

**vs the M32 O(N) blob scan (honest, matched recall):** the blob was **1.6 QPS** at recall **0.9640** at 1M (over 50 queries — its O(N) scan is ~4 s/query). At the **matched-recall** operating point `ef_search=100` (recall 0.9789 ≥ the blob's 0.964), the M35 structured scan reaches **100.4 QPS** — **~61.0× faster at preserved recall**. If a recall drop to 0.927 is acceptable (`ef_search=40`), QPS rises to 318.9 (~193.7×). The O(N)→O(ef·M) win.

**Trade-off (honest):** the structured build is ~17.5 min at 1M (single-thread graph construction) — build-once / scan-many. `theodb_hnsw` build got slower vs the blob; the scan is the ~61.0× win.

## Flat-in-N — pages read (the O(ef·M) signature)

Measured on **32-dim seeded-synthetic** vectors at **50,000→200,000** (a scale where builds are cheap — NOT the SIFT1M 128-dim workload of the QPS table above; this demonstrates the page-count invariance, it is not re-validated at 1M). At a fixed `ef_search=100` the index scan touches: **2742 pages @ 50,000** · **2962 pages @ 200,000** — a **1.08× page-count ratio while N grew 4×** ⇒ **O(ef·M) — flat in N**. Pages read (EXPLAIN BUFFERS) is the true O(ef·M) metric: it stays ~constant while N grows 4×. Wall-clock p50 grows sub-linearly with N (cache misses over a larger index), but the PAGE COUNT — what O(N) vs O(ef·M) is about — is flat.

## Verdict

- **recall preserved vs the M32 blob** (≥ 0.964): **True** — matched-recall point `ef_search=100` at recall 0.9789, **100.4 QPS**
- QPS ≥ 50 at the preserved-recall point: **True**
- honest speedup at preserved recall: **~61.0×**; up to ~193.7× if recall 0.927 is acceptable

## Reproduction

```
PGPORT=<port> python3 benchmarks/run_m35_hnsw.py --n-queries 1000 --runs 3
```
