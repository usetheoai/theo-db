# M14 — ScaNN-quality / fork-trigger evaluation: measured DiskANN vs the ScaNN-quality bar

**Date:** 2026-06-28 · **Milestone:** M14 (closes `docs/features/05-indice-scann.md`)
**Status:** measured (reproducible) · **Engine:** `theo-db:dev` (PostgreSQL + pgvector + pgvectorscale)
**Decision:** see `docs/adr/0004-scann-fork-decision.md` — **NO-FORK** (DiskANN is the delivered permissive
ScaNN-quality substitute).

> Honesty (CLAUDE.md Rule 5/7): the first-party numbers below are real harness runs; the ScaNN/real-dataset
> reference is **cited**, not reproduced in-repo (synthetic-vs-real caveat stated). We make no claim that the
> literal ScaNN AM itself is delivered — TheoDB ships StreamingDiskANN as the permissive equivalent; the
> literal `theodb_scann` AM is gated.

## The question (PRD fork-gate policy)

spec 05 documents a literal `theodb_scann` access method (Google's ScaNN). Building it is a fork/native-AM —
authorized ONLY when a reproducible benchmark shows the shipped permissive substitute insufficient
(measurement-first, ADR 0002; anti-sunk-cost, CLAUDE.md). The substitute is **StreamingDiskANN**
(pgvectorscale, M2). The question: **does DiskANN already reach ScaNN-quality recall?** If yes → no fork.

**ScaNN-quality bar:** recall@10 ≥ 0.90 at usable QPS — the band ScaNN and StreamingDiskANN occupy on
ann-benchmarks.

## Measured (first-party): DiskANN vs HNSW vs IVFFlat

Reproducible: `cd benchmarks && PGHOST=… bash scann_fork_eval.sh` (n=5000, dim=32, metric cosine, k=10,
runs=2, synthetic gaussian; harness `theodb_bench`, distance-thresholded recall ε=1e-3).

| Index | Params | recall@10 | QPS | p95 (ms) | build (ms) | index size |
|---|---|---|---|---|---|---|
| **DiskANN** | sls=100,rescore=100 | 0.6620 | 273.0 | 5.461 | 4053 | 2,293,760 B |
| **DiskANN** | sls=500,rescore=500 | **0.9310** | 231.6 | 8.323 | 4053 | 2,293,760 B |
| **DiskANN** | sls=1000,rescore=1000 | **0.9860** | 103.1 | 21.991 | 4053 | 2,293,760 B |
| **DiskANN** | sls=2000,rescore=1000 | **0.9860** | 122.3 | 15.853 | 4053 | 2,293,760 B |
| HNSW | ef_search=40 | 0.9710 | 4667.8 | 0.380 | 670 | 2,187,264 B |
| HNSW | ef_search=100 | 1.0000 | 2155.8 | 0.671 | 670 | 2,187,264 B |
| IVFFlat | probes=1 | 0.4480 | 5116.7 | 0.361 | 17 | 786,432 B |
| IVFFlat | probes=5 | 1.0000 | 1258.8 | 1.247 | 17 | 786,432 B |

**Measured result: DiskANN crosses the ScaNN-quality bar — recall@10 = 0.931 at sls=500 and 0.986 at
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

- **DiskANN meets the ScaNN-quality recall bar** (measured 0.986; bar 0.90). Recall rises monotonically with
  `query_search_list_size` (the SBQ candidate list) — the expected DiskANN curve.
- **On this synthetic gaussian, HNSW shows higher QPS** at equal recall (gaussian is unfavorable to DiskANN's
  SBQ; DiskANN's QPS/memory advantage materializes on **real high-dim embeddings at larger scale** — the
  cited pgvectorscale numbers, **UNBENCHMARKED in-repo** on a real dataset). No QPS-superiority claim is made
  here; the load-bearing finding for the fork decision is **recall ≥ the bar**, which holds.
- The harness supports a real dataset via `--hdf5` (glove/sift/cohere) — an honest follow-up to confirm the
  real-data curve; it requires an external download not run in the gate.

## Decision

**NO-FORK.** DiskANN (StreamingDiskANN, pgvectorscale, permissive) reaches the ScaNN-quality recall bar and is
a published SOTA-competitive ANN — it is TheoDB's delivered permissive ScaNN-quality equivalent. A native
`theodb_scann` access method is **not built** (anti-sunk-cost; the substitute covers the need). The
fork-trigger **re-opens** only if a reproducible benchmark shows DiskANN below the bar on a representative
dataset. Full rationale + re-open gate: `docs/adr/0004-scann-fork-decision.md`.
