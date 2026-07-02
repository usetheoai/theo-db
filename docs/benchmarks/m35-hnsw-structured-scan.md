# M35 — theodb_hnsw page-native structured scan (partial-read, à la M31 for the graph)

**Dataset:** sift-128-euclidean (SIFT1M) · n=1,000,000 dim=128 k=10 · queries=1000 (seed 42) · runs=3 · sha=36dd950

**Hardware:** 13th Gen Intel(R) Core(TM) i7-1355U · 12 cores · 15.3 GB RAM · AVX2=True

**Goal:** O(N) whole-blob scan → O(ef·M) on-demand structured traversal for theodb_hnsw

## Recall–QPS frontier at 1M (ef_search sweep)

| params | recall@10 | QPS | p50 (ms) | p95 (ms) | build (ms) | index bytes |
|---|---|---|---|---|---|---|
| ef_search=40 | 0.9272 | 318.9 | 3.15 | 4.781221049415761 | 1050546 | 759250944 |
| ef_search=100 | 0.9789 | 100.4 | 10.06 | 14.072102546560926 | 1050546 | 759250944 |
| ef_search=200 | 0.9926 | 60.4 | 17.56 | 26.671066200651676 | 1050546 | 759250944 |

**vs the M32 O(N) blob scan:** theodb_hnsw blob was **1.6 QPS** (recall 0.9640) at 1M — the M35 structured scan reaches **318.9 QPS** (**~193.7× faster**), the O(N)→O(ef·M) win.

## Flat-in-N — pages read (the O(ef·M) signature)

At a fixed `ef_search=100` the index scan touches: **2742 pages @ 50,000** · **2962 pages @ 200,000** — a **1.08× page-count ratio while N grew 4×** ⇒ **O(ef·M) — flat in N**. Pages read (EXPLAIN BUFFERS) is the true O(ef·M) metric: it stays ~constant while N grows 4×. Wall-clock p50 grows sub-linearly with N (cache misses over a larger index), but the PAGE COUNT — what O(N) vs O(ef·M) is about — is flat.

## Verdict

- QPS ≥ 50 at 1M: **True** (best 318.9 QPS)
- recall preserved (a ≥0.90 recall point exists): **True** (best QPS at recall≥0.90: 318.88464620213966)

## Reproduction

```
PGPORT=<port> python3 benchmarks/run_m35_hnsw.py --n-queries 1000 --runs 3
```
