# M14 — ScaNN-quality / fork-trigger evaluation: measured DiskANN vs the ScaNN-quality bar

**Date:** 2026-06-28 · **Milestone:** M14 (closes `docs/features/05-indice-scann.md`)
**Status:** measured (reproducible) · **Engine:** `theo-db:dev` (PostgreSQL + pgvector + pgvectorscale)
**Decision:** see `docs/adr/0004-scann-fork-decision.md` — **NO-FORK (provisional)** (DiskANN is the delivered
permissive ScaNN-quality substitute; provisional pending a real-dataset confirmation — see Caveats).

> Honesty (CLAUDE.md Rule 5/7): the first-party numbers below are real harness runs (`runs=3`, `seed=14`); the
> ScaNN/real-dataset reference is **cited**, not reproduced in-repo (synthetic-vs-real caveat stated). We make
> no claim that the literal ScaNN AM itself is delivered — TheoDB ships StreamingDiskANN as the permissive
> equivalent; the literal `theodb_scann` AM is gated.

## The question (PRD fork-gate policy)

spec 05 documents a literal `theodb_scann` access method (Google's ScaNN). Building it is a fork/native-AM —
authorized ONLY when a reproducible benchmark shows the shipped permissive substitute insufficient
(measurement-first, ADR 0002; anti-sunk-cost, CLAUDE.md). The substitute is **StreamingDiskANN**
(pgvectorscale, M2). The question: **does DiskANN already reach ScaNN-quality recall?** If yes → no fork.

**ScaNN-quality bar:** recall@10 ≥ 0.90 at usable QPS — the band ScaNN and StreamingDiskANN occupy on
ann-benchmarks. (This is a RECALL bar; ScaNN's memory/compression features are a separate axis — see Caveats.)

## Measured (first-party): DiskANN vs HNSW vs IVFFlat

Reproducible: `cd benchmarks && PGHOST=… bash scann_fork_eval.sh` (n=5000, **dim=32**, metric cosine, k=10,
**runs=3, seed=14**, synthetic gaussian; harness `theodb_bench`, distance-thresholded recall ε=1e-3). recall
is computed once per built index (deterministic for a fixed seed); QPS is best-of-3.

| Index | Params | recall@10 | QPS | p95 (ms) | build (ms) | index size |
|---|---|---|---|---|---|---|
| **DiskANN** | sls=100,rescore=100 | 0.6640 | 574.5 | 4.167 | 3025 | 2,293,760 B |
| **DiskANN** | sls=500,rescore=500 | **0.9340** | 229.9 | 7.201 | 3025 | 2,293,760 B |
| **DiskANN** | sls=1000,rescore=1000 | **0.9780** | 154.8 | 13.151 | 3025 | 2,293,760 B |
| **DiskANN** | sls=2000,rescore=1000 | **0.9780** | 115.1 | 14.673 | 3025 | 2,293,760 B |
| HNSW | ef_search=40 | 0.9740 | 3878.6 | 0.530 | 735 | 2,179,072 B |
| HNSW | ef_search=100 | 0.9990 | 2019.9 | 0.917 | 735 | 2,179,072 B |
| IVFFlat | probes=1 | 0.4040 | 4900.4 | 0.373 | 33 | 786,432 B |
| IVFFlat | probes=5 | 1.0000 | 1278.5 | 1.184 | 33 | 786,432 B |

**Measured result: DiskANN crosses the ScaNN-quality recall bar — recall@10 = 0.934 at sls=500 and 0.978 at
sls=1000.** So the permissive substitute reaches ScaNN-quality recall on this dataset.

## Reference target (cited, not reproduced in-repo)

- **ScaNN** (Guo et al., 2020; ann-benchmarks `glove-100-angular`): recall@10 in the ~0.90–0.99 band at high
  QPS — the SOTA ANN target spec 05 names. Source: `https://ann-benchmarks.com`.
- **StreamingDiskANN / pgvectorscale** (Timescale benchmark): published recall parity with — and higher QPS
  than — pgvector HNSW at ~99% recall on real embedding datasets (e.g., Cohere/wiki), with lower memory via
  SBQ. Source: `https://github.com/timescale/pgvectorscale` (benchmark post).
- These establish that StreamingDiskANN is a **published, SOTA-competitive** ANN — i.e., a genuine
  ScaNN-quality substitute, not a downgrade.

## Honest analysis

- **DiskANN meets the ScaNN-quality recall bar** (measured 0.978; bar 0.90). Recall rises monotonically with
  `query_search_list_size` (the SBQ candidate list) — the expected DiskANN curve.
- **On this synthetic gaussian, HNSW shows higher QPS** at equal recall (gaussian is unfavorable to DiskANN's
  SBQ; DiskANN's QPS/memory advantage materializes on **real high-dim embeddings at larger scale** — the
  cited pgvectorscale numbers, **UNBENCHMARKED in-repo** on a real dataset). No QPS-superiority claim is made
  here; the load-bearing finding for the fork decision is **recall ≥ the bar**, which holds.

## Caveats (honest — bound the strength of this evidence)

- **Synthetic gaussian at dim=32** — far below real embedding dimensionality (768/1536 for the cited
  ScaNN/Cohere targets). Gaussian is *unfavorable* to DiskANN/SBQ (so crossing the bar here is conservative
  for recall), but the dataset is not representative of production embeddings. The recall bar itself is taken
  from real high-dim ann-benchmarks data. The decision is therefore **provisional**; a real-dataset run
  (harness `--hdf5` glove/cohere) is the honest confirmation (requires an external download, not in the gate).
- **Recall-only bar** — "ScaNN-quality" here means recall@k. ScaNN's anisotropic-hashing (AH) quantizer +
  multi-level trees (spec 05 §quantizers/§levels) are a distinct **memory/compression** axis DiskANN covers
  differently (SBQ). A memory-at-recall gap, if it exists, is NOT captured by this recall bar and is a
  separate future evaluation (see ADR 0004).
- **runs=3, best-of-3 QPS, single seed** — meets a smoke bar; the `/analysis` cycle's ≥3-runs-with-mean±std
  applies to formal trajectory analysis, not this fork-eval smoke.

## Decision

**NO-FORK (provisional).** DiskANN (StreamingDiskANN, pgvectorscale, permissive) reaches the ScaNN-quality
recall bar and is a published SOTA-competitive ANN — it is TheoDB's delivered permissive ScaNN-quality
equivalent. A native `theodb_scann` access method is **not built** (anti-sunk-cost; the substitute covers the
recall need). The fork-trigger **re-opens** on either path in `docs/adr/0004-scann-fork-decision.md` (DiskANN
below the bar on a representative real dataset, OR the north-star vector-superiority bet of ADR 0002). Full
rationale + re-open gates: `docs/adr/0004-scann-fork-decision.md`.
