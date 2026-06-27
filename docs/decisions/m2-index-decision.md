# M2 — Index decision (evidence-driven)

**Date:** 2026-06-27 · **Status:** Evidence captured; **final index choice DEFERRED to a realistic-dataset benchmark** (honest, measurement-first per ADR 0002).
**Evidence:** `docs/benchmarks/2026-06-27-pgvector-cosine.json` (reproducible, seed=42, sha-stamped).

## What this records

M2 DoD-2 asks for "an ANN index beyond HNSW, **chosen by the harness evidence**". This document records the evidence and the honest decision.

## What was delivered

- **pgvectorscale StreamingDiskANN is now available in the official image** (multi-stage Docker build; +2 MB over the M0 image, **no Rust toolchain shipped**). `CREATE EXTENSION vectorscale CASCADE` + `CREATE INDEX … USING diskann` work and the planner uses the index.
- **The harness measures it** (full recall×QPS curve), alongside HNSW.

## Measured evidence (synthetic gaussian, n=5000, dim=128, cosine, k=10)

Every row below is produced by the harness (`--index both`, seed=42, runs=3 best-of-N) and lives in
the reproducible artifact — there are **no hand-run, off-harness numbers** in this table.

| index | params | recall@10 | QPS | p95 | index size |
|---|---|---|---|---|---|
| HNSW | ef_search=40 | 0.731 | 854 | 3.16 ms | 4.17 MB |
| HNSW | ef_search=100 | **0.940** | 563 | 4.47 ms | 4.17 MB |
| DiskANN (SBQ) | sls=100, rescore=100 | 0.629 | 193 | 8.37 ms | **2.43 MB** |
| DiskANN (SBQ) | sls=500, rescore=500 | 0.914 | 162 | 12.17 ms | 2.43 MB |
| DiskANN (SBQ) | sls=1000, rescore=1000 | **0.971** | 88 | 24.79 ms | 2.43 MB |
| DiskANN (SBQ) | sls=2000, rescore=1000 | 0.971 | 56 | 33.61 ms | 2.43 MB |

> **Methodology note (Rule 3).** Earlier drafts capped `diskann.query_rescore` at 500 while sweeping
> `query_search_list_size` higher — that asymmetry froze DiskANN's recall at a fake 0.916 plateau while
> QPS still fell. The harness now scales `rescore` with `sls` up to pgvectorscale's engine ceiling
> (`query_rescore` max = 1000), so every point is a real (recall, QPS) pair. The top of the DiskANN
> curve (recall 0.971) is now measured **with QPS** and reproducible from the harness, not a manual
> psql one-off. The flat top (0.971 at both sls=1000 and sls=2000) is the genuine engine ceiling
> (rescore saturates at 1000), not a harness artifact.

## Honest reading of the evidence (Rule 3)

- **On this synthetic random-gaussian dataset, HNSW dominates the measured recall×QPS band.** At comparable recall, HNSW is multiples faster: at recall ≈0.91–0.94, HNSW does ~560 QPS (ef=100) versus DiskANN's 162 QPS (sls=500). DiskANN does reach a *higher* recall (0.971) than any HNSW config we swept, but only at 88 QPS — and HNSW above ef=100 was not swept, so the >0.94 region is simply unmeasured for HNSW, not won by DiskANN.
- **DiskANN wins decisively on index size** (2.43 MB vs 4.17 MB — 42% smaller, via Statistical Binary Quantization) and reaches the highest measured recall (0.971).
- **This is a dataset artifact, not a property of DiskANN — on two axes.** (1) *Distribution:* uniform-random high-dimensional vectors are near-equidistant (curse of dimensionality); SBQ quantization loses the fine distinctions such data relies on, so DiskANN needs a larger candidate list. (2) *Scale:* DiskANN is a disk-resident, billion-scale algorithm; at n=5000 the whole dataset is in memory and the streaming/graph-on-disk advantage is irrelevant, so SBQ is pure downside here. SBQ is engineered for **real embedding distributions at scale** (clustered/structured), where pgvectorscale's published benchmarks show it *beating* HNSW-class indexes (e.g. "28× lower p95 latency vs Pinecone @99% recall, 50M Cohere-768" — blueprint `alloydb-vector-ai-implementation`, flagged `UNBENCHMARKED` for us).

## Decision

**The final M2 index choice is DEFERRED until a realistic embedding dataset (e.g. sift-128 / glove-100 / a Cohere subset) is mirrored locally and benchmarked.** Choosing HNSW *or* DiskANN from synthetic gaussian alone would violate ADR 0002 (measurement-first) and `public-copy.md` (no claim without representative benchmark). What we know:

- DiskANN is **available and functional** in the image (DoD-2 "index beyond HNSW available" ✅).
- DiskANN is **measurably competitive** (reaches 0.971 recall @88 QPS, reproducible from the harness) and **more compact** (−42% index size).
- The **decisive comparison requires real data** — the synthetic harness proved the infrastructure works and revealed its own dataset is unrepresentative for SBQ.

## D3 (Fork Policy) — honored

pgvectorscale is used **as-is** (commit `57c88b7`, pinned), **no fork**. PRD D3 = upstream-first; a fork requires a reproducible trigger benchmark — none exists. The pinned commit is the basis of the rebase-CI.

## D1 (License) — release-gate obligation (honest debt)

pgvectorscale's top-level license is **The PostgreSQL License** (verified: `.claude/knowledge-base/references/pgvectorscale/LICENSE`, commit `57c88b7`) — D1-clean at the project level. But `vectorscale.so` statically links pgvectorscale's transitive Rust crate tree, and *that* code ships in the image. `Cargo.lock` carries no license fields, so a **`cargo-deny`/`loop-check-licence` sweep over the pinned crate set is a mandatory pre-release gate** (PRD §11) before this image is part of any distribution. This is M2 DoD-2 (a dev image, `theo-db:dev`) — not a release — so the sweep is tracked here as an explicit obligation, not silently assumed clean.

## Build reproducibility — pinned

The multi-stage build is pinned on every axis for reproducible artifacts: base image by digest (shared `ARG BASE_IMAGE` across builder + runtime), pgvector by SHA (`586e7515`), pgvectorscale by commit (`57c88b7`), `cargo-pgrx` by version (`0.16.1`, `--locked`), Rust toolchain by version (`1.91.0`), and `cargo pgrx install --locked` so the compiled crate set matches the pinned `Cargo.lock`.

## Next slice (follow-up)

1. Mirror a real ANN-Benchmarks dataset (sift-128-euclidean / glove-100-angular) under the repo (supply-chain: self-host the HDF5, do not depend on a live third-party host).
2. Extend the harness with an HDF5 dataset loader (the recall math + measurement already exist).
3. Re-run HNSW vs DiskANN on real data → make the final index decision with representative evidence.
4. M2 DoD-3 (embeddings SQL function) is a separate slice.
