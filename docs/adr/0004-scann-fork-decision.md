# ADR 0004 — ScaNN access-method fork decision: NO-FORK (DiskANN is the permissive ScaNN-quality substitute)

**Status:** Accepted · **Date:** 2026-06-28 · **Milestone:** M14 · **Deciders:** TheoDB maintainers
**Related:** ADR 0001 (no engine fork), ADR 0002 (north-star, measurement-first), PRD §15 fork-gate policy
(a fork/native-AM is conditional on a reproducible benchmark), CLAUDE.md (anti-sunk-cost; Esforço ≠ Complexidade)

## Context

`docs/features/05-indice-scann.md` documents a literal `theodb_scann` access method — Google's ScaNN
(anisotropic quantization), the AlloyDB ScaNN index. Implementing it in PostgreSQL is a **fork/native-AM**
(a C++/pgrx ScaNN binding + an index AM). The PRD fork-gate policy authorizes such a fork ONLY when a
**reproducible benchmark** shows the shipped permissive substitute insufficient. TheoDB already ships
**StreamingDiskANN** (`pgvectorscale`, M2) as its permissive ANN.

## Decision

**Do NOT build a native `theodb_scann` access method.** TheoDB's delivered ScaNN-quality index is
**StreamingDiskANN (DiskANN)** — the permissive `pgvectorscale` AM already in the image.

This is anchored on measured + cited evidence (`docs/benchmarks/m14-scann-fork-decision.md`):

- **Measured (first-party, reproducible via `benchmarks/scann_fork_eval.sh`):** DiskANN reaches the
  ScaNN-quality recall bar — recall@10 = **0.931** at `sls=500` and **0.986** at `sls=1000` (n=5000, dim=32,
  cosine, synthetic gaussian). Asserted by `test_diskann_reaches_scann_quality_recall` (bar = 0.90).
- **Cited (reference target):** ScaNN occupies the ~0.90–0.99 recall@10 band on ann-benchmarks; pgvectorscale
  publishes StreamingDiskANN at recall parity with + higher QPS than pgvector HNSW at ~99% recall on real
  embedding datasets. So DiskANN is a published, SOTA-competitive ANN — a genuine ScaNN-quality substitute.

Building a native ScaNN AM before this evidence justifies it would violate measurement-first (ADR 0002) and
anti-sunk-cost (CLAUDE.md): the substitute already covers the need.

## Re-open gate (when this decision flips)

The fork-trigger **re-opens** — i.e., a native `theodb_scann` AM (or a pgvectorscale fork) becomes authorized
— ONLY when a **reproducible benchmark** shows DiskANN **below** the ScaNN-quality bar (recall@10 < 0.90 at
usable QPS) on a **representative real dataset** (e.g., glove/cohere via the harness `--hdf5`), AND no DiskANN
tuning (`query_search_list_size` / `query_rescore`) closes the gap. Until then: NO-FORK.

## Consequences

- spec 05's literal `theodb_scann` is **gated, not delivered**; DiskANN is documented as the permissive
  ScaNN-quality equivalent (honest — we never claim the literal ScaNN AM is shipped).
- No new dependency, no native-AM maintenance burden, no fork to rebase.
- Honest follow-up (non-blocking): a real-dataset `--hdf5` DiskANN run to confirm the real-data recall curve
  (the in-repo measurement is synthetic gaussian; the real-data advantage is cited from pgvectorscale, marked
  UNBENCHMARKED in-repo).

## Alternatives considered

- **Build `theodb_scann` now (literal AlloyDB parity).** Rejected: massive fork, unbenchmarked need, the M6
  rustc/MSRV from-source build blocker precedent; directly violates the fork-gate policy + anti-sunk-cost.
- **Skip the milestone (DiskANN already shipped).** Rejected: spec 05 and the fork-trigger require an
  explicit, auditable, evidence-backed decision — not silence.
- **Encrypted/native ScaNN later regardless of evidence.** Rejected: that is the sunk-cost trap; the re-open
  gate above is the only path back.
