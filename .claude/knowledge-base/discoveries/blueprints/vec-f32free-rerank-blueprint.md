---
slug: vec-f32free-rerank
date: 2026-07-17
cycle: discover
verdict: SHIPPABLE_WITH_CAVEATS
---

# Blueprint — f32-free rerank (extended multi-bit RaBitQ) + SymphonyQG: the un-measured lever to close the vector QPS gap

Deep research (council-vector-ann + council-performance-simd, R0 web evidence). The "paradigmatically
unreachable" verdict (ADR-0035/0036/0037) is **half-right, half-stale**: correct that beating the ScaNN
*library's* 1920 QPS is unreachable (paradigm + zero MVCC/WAL tax); STALE in that it was measured on a
rerank-bound path — and the **f32-free / rerank-free path was never measured**. That is a real, un-refuted,
mechanism-backed lever worth an estimated **~2.5–4×** (→ AlloyDB-in-Postgres class, ~300–600 QPS @ 0.99 in-RAM).

## Measured state (confirmed from real files by the councils)

- `vec/aq.rs` = TheoDB's anisotropic AQ (ScaNN's Guo 2020 loss); `vec/ah.rs` = AH pshufb FastScan (block32).
  Both already replicate ScaNN's quantization+scan paradigm. Not the gap.
- **v4 interleaved `[ids][f32][codes]`** (`scan.rs:362`): 34 MB paged in per query for a 1 MB AH scan = **32×
  I/O amplification** → AH speedup is theatre (M82: zero QPS gain, 78.5 QPS @ 0.985).
- **v5 storage-separated `[ids][codes]`⊥`[f32]`** (`scan_ivf_aq_split`, `scan.rs:463`) — ALREADY SHIPPED
  (m83-m84): ~1.5 MB Stage-1 I/O (22× less), **96–134 QPS @ 0.99 in-RAM**. Residual bind moves to **Stage-2
  exact-f32 random-read of the rerank survivors** (`scan.rs:562 read_vec_at`) + the centroid probe.
- **v6 SQ8 rerank** (`scan_ivf_aq_split_sq8`, `scan.rs:755`): 4× smaller Stage-2 read but decode-CPU offsets it +
  recall penalty forces wider over_fetch → **no warm-cache QPS win** (m85, honest-negative).
- SOAR (m86), centroid-probe reduction: measured dead-ends for this SIFT1M regime.
- **THE gap: no experiment ever measured a Stage-2 that touches ZERO raw vectors at recall 0.99.**

## Coverage Corner — Techniques (SOTA, cited, Apache-2.0)

- **Extended multi-bit RaBitQ** (Gao & Long, arXiv:2409.09913; base RaBitQ SIGMOD'24 arXiv:2405.12497;
  Apache-2.0 `VectorDB-NTU/RaBitQ-Library`). Verbatim: *"at compression rates 4.4×/4.5×, stably produces >95%
  /99% recall WITHOUT accessing raw vectors for re-ranking"*; *"4/5/7-bit suffices for 90/95/99% recall without
  reranking"*. RaBitQ carries a **proven sharp error bound** → a higher-bit code is reliable enough to be the
  FINAL ranking, no f32 page ever touched. Reimplemented across Faiss/Milvus/Lucene-BBQ/turbopuffer (proof of
  permissive reimplementability). **M74 negative does not apply** — that was 1-bit @ 768d; this is 5–7-bit @ 128d.
- **SymphonyQG** (Gou/Gao/Xu/Long, arXiv:2411.12229, SIGMOD'25; Apache-2.0 `gouyt13/SymphonyQG`). Names the exact
  bind: NGT-QG "entails a re-ranking step … which introduces extra random memory accesses"; SymphonyQG "avoids
  the explicit re-ranking step and refines the graph to align with FastScan" — RaBitQ codes contiguous per-vertex,
  out-degree a multiple of 32 (FastScan batch), scored in-traversal, **no rerank pass**. Reported **1.5–4.5× QPS
  @95% vs best baselines, 3.5–17× vs HNSWlib** — new SOTA over the HNSW+PQ class TheoDB is stuck in.
- **MVCC-free per-backend snapshot (M108 pattern for vectors)** — the highest-ceiling systems lever: build once →
  per-backend deserialized `Rc<AnnSnapshot>` (centroids+block32 codes+multi-bit RaBitQ, NOT the 512 MB f32),
  keyed by `(oid, built_at)` epoch → scan runs in-process (no ReadBufferExtended/LockBuffer/copy-out per query),
  batch-visibility on final top-k only. Reproduces the M75 in-memory 5–7× regime as a durable cache. Proven
  crash-safe by construction (bytea-in-heap, M108 review).

## ADRs

- **ADR-1 — f32-free Stage-2 via extended multi-bit RaBitQ (the E1 spike, do first).** Swap `scan_ivf_aq_split`'s
  Stage-2 (`read_vec_at` f32 + exact metric, `scan.rs:562`) for a 5–7-bit RaBitQ code on a DEDICATED page, scored
  with the RaBitQ estimator — no f32 touched. **Own-code reimplementation from arXiv:2409.09913** (the algorithm
  is Apache-2.0; the vendored `rabitq-rs` tree was DELETED in ADR-0046 after the M74 1-bit memory-only verdict —
  so this is a from-scratch multi-bit implementation like `aq.rs`/`ah.rs` were, NOT extending existing code; ZERO
  new deps per D1/D4). *Rejected:* SQ8 rerank (v6, measured no-win — decode CPU + recall penalty); 1-bit RaBitQ
  (M74/ADR-0036, tops at 98.4%, needs f32 rerank). *Mechanism:* deletes the Stage-2 random-read that binds v5 at
  0.99 → the batched-AH Stage-1 (already 6–12× faster, m83) becomes the whole hot path.
- **ADR-2 — SymphonyQG QG index as a new scan mode (E2, if E1 GO).** RaBitQ-codes-in-graph, FastScan traversal,
  no rerank. Reuses `ah.rs`+`aq.rs`; delta = graph layout (degree-32). *Rejected:* HNSW+PQ (the stuck class).
- **ADR-3 — honest ceiling.** Target = ~2.5–4× over v5 (AlloyDB-in-Postgres class), NOT a ScaNN-library beat
  (paradigm + MVCC/WAL tax remain, arXiv:2603.23710 = 84% cycles system overhead). *Rejected:* claiming ScaNN
  parity (measured false, ADR-0035 stands). Positioning per public-copy.md.

## Measurement plan (M46 same-data A/B — the only way to earn the claim)

- **E1 (cheapest, surgical):** extended RaBitQ 5-bit + 7-bit rerank codes on a dedicated page in
  `scan_ivf_aq_split`, f32-FREE Stage-2. Same 1M SIFT table, lists=1000, A/B vs v5-f32-rerank. Sweep probes×bits.
  Report recall@10 vs official GT, QPS best-of-3, **and buffers_per_query** (cache-independent I/O proof). Warm
  AND cold cache. Real-AM (not in-memory — the M75/M82 lesson). **Gate: ≥2× v5 QPS at matched recall 0.99 with a
  confirmed drop in f32 buffer accesses.** Honest-negative is a valid terminal (extends ADR-0035 a 4th time).
- **E2 (primary bet, if E1 GO):** SymphonyQG-layout QG index, degree 32, RaBitQ codes contiguous per-vertex,
  FastScan traversal, no rerank. A/B vs best v5/E1 @ 0.99, same metrics.

## Honest caveats

- The reachable win is ~2.5–4×, NOT a ScaNN beat. The MVCC/WAL/heap tax (~4–6×) is structural for a transactional
  PG extension (arXiv:2603.23710). Claiming otherwise is barred (Rule 5 / public-copy.md).
- Needs a fresh build/bench droplet + the SIFT1M dataset (the pillar droplet was destroyed).
- Extended RaBitQ multi-bit is a **from-scratch own-code quantizer** (arXiv:2409.09913, Apache-2.0 algorithm) —
  the RaBitQ vendored tree was deleted (ADR-0046). A real multi-week implementation like the AQ/AH pillar, not a
  parameter flip. Path: new `vec/rabitq.rs` (multi-bit codebook + geometric estimator + error bound) → dedicated
  rerank-code page → f32-free Stage-2 in `scan_ivf_aq_split` → SIFT1M same-data A/B (m84 harness) on a fresh droplet.
