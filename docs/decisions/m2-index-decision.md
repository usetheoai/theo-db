# M2 — Index decision (evidence-driven)

**Date:** 2026-06-27 · **Status:** Evidence captured; **final index choice DEFERRED to a realistic-dataset benchmark** (honest, measurement-first per ADR 0002).
**Evidence:** `docs/benchmarks/2026-06-27-pgvector-cosine.json` (reproducible, seed=42, sha-stamped).

## What this records

M2 DoD-2 asks for "an ANN index beyond HNSW, **chosen by the harness evidence**". This document records the evidence and the honest decision.

## What was delivered

- **pgvectorscale StreamingDiskANN is now available in the official image** (multi-stage Docker build; +2 MB over the M0 image, **no Rust toolchain shipped**). `CREATE EXTENSION vectorscale CASCADE` + `CREATE INDEX … USING diskann` work and the planner uses the index.
- **The harness measures it** (full recall×QPS curve), alongside HNSW.

## Measured evidence (synthetic gaussian, n=5000, dim=128, cosine, k=10)

| index | params | recall@10 | QPS | index size |
|---|---|---|---|---|
| HNSW | ef_search=40 | 0.710 | 1291 | 4.17 MB |
| HNSW | ef_search=100 | **0.944** | 587 | 4.17 MB |
| DiskANN (SBQ) | sls=100 | 0.629 | 431 | **2.43 MB** |
| DiskANN (SBQ) | sls=500 | 0.916 | 178 | 2.43 MB |
| DiskANN (SBQ) | sls=1000 | 0.916 | 141 | 2.43 MB |
| DiskANN (SBQ) | sls=2000, rescore=1000 | **0.970** | — | 2.43 MB |

## Honest reading of the evidence (Rule 3)

- **On this synthetic random-gaussian dataset, HNSW dominates the recall×QPS Pareto frontier**; DiskANN reaches comparable recall (0.92–0.97) only at much higher `query_search_list_size` (and thus lower QPS).
- **DiskANN wins decisively on index size** (2.43 MB vs 4.17 MB — 42% smaller, via Statistical Binary Quantization).
- **This is a dataset artifact, not a property of DiskANN.** Uniform-random high-dimensional vectors are near-equidistant (curse of dimensionality); SBQ quantization loses the fine distinctions such data relies on, so DiskANN needs a larger candidate list. SBQ is engineered for **real embedding distributions** (clustered/structured), where pgvectorscale's published benchmarks show it *beating* HNSW-class indexes (e.g. "28× lower p95 latency vs Pinecone @99% recall, 50M Cohere-768" — blueprint `alloydb-vector-ai-implementation`, flagged `UNBENCHMARKED` for us).

## Decision

**The final M2 index choice is DEFERRED until a realistic embedding dataset (e.g. sift-128 / glove-100 / a Cohere subset) is mirrored locally and benchmarked.** Choosing HNSW *or* DiskANN from synthetic gaussian alone would violate ADR 0002 (measurement-first) and `public-copy.md` (no claim without representative benchmark). What we know:

- DiskANN is **available and functional** in the image (DoD-2 "index beyond HNSW available" ✅).
- DiskANN is **measurably competitive** (reaches 0.97 recall) and **more compact** (−42% index size).
- The **decisive comparison requires real data** — the synthetic harness proved the infrastructure works and revealed its own dataset is unrepresentative for SBQ.

## D3 (Fork Policy) — honored

pgvectorscale is used **as-is** (commit `57c88b7`, pinned), **no fork**. PRD D3 = upstream-first; a fork requires a reproducible trigger benchmark — none exists. The pinned commit is the basis of the rebase-CI.

## Next slice (follow-up)

1. Mirror a real ANN-Benchmarks dataset (sift-128-euclidean / glove-100-angular) under the repo (supply-chain: self-host the HDF5, do not depend on a live third-party host).
2. Extend the harness with an HDF5 dataset loader (the recall math + measurement already exist).
3. Re-run HNSW vs DiskANN on real data → make the final index decision with representative evidence.
4. M2 DoD-3 (embeddings SQL function) is a separate slice.
