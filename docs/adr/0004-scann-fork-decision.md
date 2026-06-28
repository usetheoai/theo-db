# ADR 0004 — ScaNN access-method fork decision: NO-FORK (DiskANN is the permissive ScaNN-quality substitute)

**Status:** Accepted (provisional — pending a real-dataset `--hdf5` confirmation; see Re-open gate) · **Date:** 2026-06-28 · **Milestone:** M14 · **Deciders:** TheoDB maintainers
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

- **Measured (first-party, reproducible via `benchmarks/scann_fork_eval.sh`, runs=3, seed=14):** DiskANN
  reaches the ScaNN-quality recall bar — recall@10 = **0.934** at `sls=500` and **0.978** at `sls=1000`
  (n=5000, **dim=32, synthetic gaussian** — see Evidence caveats). Asserted by
  `test_diskann_reaches_scann_quality_recall` (bar = 0.90).
- **Cited (reference target):** ScaNN occupies the ~0.90–0.99 recall@10 band on ann-benchmarks; pgvectorscale
  publishes StreamingDiskANN at recall parity with + higher QPS than pgvector HNSW at ~99% recall on real
  embedding datasets. So DiskANN is a published, SOTA-competitive ANN — a genuine ScaNN-quality substitute.

Building a native ScaNN AM before this evidence justifies it would violate measurement-first (ADR 0002) and
anti-sunk-cost (CLAUDE.md): the substitute already covers the need.

## Evidence caveats (bound this decision's strength)

- The first-party numbers are **synthetic gaussian at dim=32** — below real embedding dimensionality
  (768/1536). Gaussian is *unfavorable* to DiskANN/SBQ (so crossing the recall bar here is conservative), but
  not representative; the recall bar itself comes from real high-dim ann-benchmarks data. Hence **Status:
  provisional** until the real-dataset `--hdf5` confirmation below.
- **"ScaNN-quality" is scoped to recall@k here.** ScaNN's anisotropic-hashing (AH) quantizer + multi-level
  trees (spec 05) are a distinct **memory/compression** axis; DiskANN covers it differently via SBQ. This
  decision does NOT claim parity on that axis — a memory-at-recall comparison is a separate evaluation
  (second re-open trigger below).

## Re-open gates (either path re-authorizes a native AM / fork)

1. **Recall-rescue (this milestone's gate):** a **reproducible benchmark** shows DiskANN **below** the
   ScaNN-quality bar (recall@10 < 0.90 at usable QPS) on a **representative real dataset** (glove/cohere via
   `--hdf5`), AND no DiskANN tuning (`query_search_list_size` / `query_rescore`) closes the gap.
2. **North-star superiority (per LOCKED ADR 0002):** this NO-FORK does NOT close the ScaNN-as-PG-AM
   *superiority* bet that ADR 0002 keeps open — a native AM remains authorized if a reproducible benchmark
   shows a **gain over DiskANN** (recall/QPS/memory) that justifies the fork, OR a memory-at-recall gap vs
   ScaNN's AH quantizer that DiskANN/SBQ cannot close. This ADR (subordinate) does not narrow the LOCKED ADR.

Until a gate fires: NO-FORK.

## Consequences

- spec 05's literal `theodb_scann` is **gated, not delivered**; DiskANN is documented as the permissive
  ScaNN-quality equivalent (honest — we never claim the literal ScaNN AM is shipped).
- No new dependency, no native-AM maintenance burden, no fork to rebase.
- **Scoped to recall:** AH-quantizer / multi-level-tree (memory/compression) parity with ScaNN is explicitly
  out-of-scope-by-design here — DiskANN trades a different memory profile via SBQ; a memory-at-recall gap is
  the second re-open trigger, not a silent omission.
- Honest follow-up (non-blocking): a real-dataset `--hdf5` DiskANN run to confirm the real-data recall curve
  (the in-repo measurement is synthetic gaussian; the real-data advantage is cited from pgvectorscale, marked
  UNBENCHMARKED in-repo).

## Alternatives considered

- **Build `theodb_scann` now (literal AlloyDB parity).** Rejected: massive fork, unbenchmarked need, the M6
  rustc/MSRV from-source build blocker precedent; directly violates the fork-gate policy + anti-sunk-cost.
- **Skip the milestone (DiskANN already shipped).** Rejected: spec 05 and the fork-trigger require an
  explicit, auditable, evidence-backed decision — not silence.
- **Building the native AM now regardless of evidence.** Rejected: that is the sunk-cost trap; the two
  re-open gates above (recall-rescue OR the ADR 0002 superiority bet) are the evidence-gated paths to a native AM.
