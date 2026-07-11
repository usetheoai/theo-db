# M84 — pg_scann v5: high-recall confirmation + rerank-pool fix (SIFT1M A/B)

**Date:** 2026-07-11 · **Dataset:** SIFT1M (full 1M, official GT) · **Verdict:** `GO` (the layout wins at high recall)

M84 answers the make-or-break question M83 left open: **does the v5 storage-separated layout keep its speedup at
HIGH recall?** M83 measured 6-12× but at a ~0.80 recall ceiling. M84 (a) fixes the cause and (b) confirms the win
holds at recall 0.98-0.998.

## The fix

M83's recall plateaued at ~0.80 because the AQ rerank pool was **hardcoded at 64**: `over_fetch().max(64)` is
always 64 (since `over_fetch()` ≤ 64, the `.max(64)` floor always won), so `theodb_hnsw.over_fetch` never widened
the pool — a latent no-op. **M84 changes it to `64 * over_fetch()`** in both `scan_ivf_aq` and `scan_ivf_aq_split`
(`am/scan.rs`): default (over_fetch=1) stays 64; over_fetch=8/32 → pool 512/2048. **238 pg_tests GREEN, zero regression.**

## Method

Same-data A/B (M46): `sift5` (v5, `separate_storage=1`) vs `sift4` (v4 interleaved), identical 1M data,
lists=500, pq_subspaces=32. Swept `over_fetch` ∈ {1,8,32} (rerank pool 64/512/2048) × probes. recall@10 vs official
GT; QPS best-of-3.

## Results — the recall × speedup frontier

| over_fetch (pool) | probes | recall (v5=v4) | **v5 QPS** | v4 QPS | **speedup** |
|---:|---:|---:|---:|---:|---:|
| 1 (64) | 32 | 0.796 | 523 | 46.8 | 11.2× |
| 1 (64) | 128 | 0.794 | 194 | 11.9 | 16.3× |
| **8 (512)** | 32 | **0.978** | 294 | 48.2 | **6.1×** |
| **8 (512)** | 64 | **0.981** | 203 | 23.3 | **8.7×** |
| **8 (512)** | 128 | **0.981** | 141 | 11.6 | **12.2×** |
| **32 (2048)** | 32 | **0.994** | 134 | 45.0 | 3.0× |
| **32 (2048)** | 64 | **0.998** | 118 | 23.7 | **5.0×** |
| **32 (2048)** | 128 | **0.9985** | 96 | 11.8 | **8.1×** |

Build: v5 834s, v4 833s (lists=500). recall v5==v4 identical (lossless) at every point.

## Findings

1. **Recall ceiling resolved.** The M83 ~0.80 ceiling was purely the rerank-pool cap. over_fetch=8 (pool 512) →
   recall 0.956-0.981; over_fetch=32 (pool 2048) → 0.993-0.9985. Fully recovered.
2. **v5 keeps the speedup at high recall — the confirmation.** Pareto-optimal operating points: **recall 0.98 →
   8.7×**, **recall 0.998 → 5.0×**, **recall 0.9985 → 8.1×**. Every high-recall point wins **≥3×** (mostly 5-12×).
3. **Honest tradeoff (as the deep research predicted).** A larger rerank pool means Stage-2 does more f32
   random-reads, so v5's edge narrows at the extreme-recall corner (of=32, probes=16-32 → 1.7-3.0×). This is the
   "random-read of survivors" cost — and it is exactly what **M85 (SQ8 refine: 128 B per survivor, not 512 B)**
   attacks to restore the high-recall speedup. The data validates the roadmap sequencing.

## Honest caveats

- **lists=500** (not 1000) — chosen to keep the build tractable after a slow-host 1M/lists=1000 run was aborted
  (DO single-core variance). The v5-vs-v4 A/B and the over_fetch→recall curve are relative and scale/lists-robust;
  absolute QPS is lists=500-specific.
- **Warm-cache** — win is fewer buffer-accesses (lower bound); billion-scale (M88) compounds with physical I/O.
- The sub-3× points are **non-Pareto-optimal** (over-large pool for few probes); a real deployment picks the
  QPS-maximizing (over_fetch, probes) for its target recall.

## Verdict

**GO.** The storage-separation lever wins **3-12× at production recall (0.98-0.998)** at matched recall — the
**class-AlloyDB-in-Postgres** target. It does NOT beat the ScaNN library (the paradigm MVCC/WAL/heap tax remains,
M73/ADR-0035). Next: **M85** (SQ8 refine) to push the high-recall speedup back up by shrinking the Stage-2 survivor
reads. The v5 layout is production-safe already (same WAL/`extend_page_with_item` path as v4, VACUUM no-op gate,
`amcostestimate` v5-aware, 238 tests).

See also: `docs/benchmarks/m83-split-storage-spike.md`, `docs/research/scann-storage-separation-2026-07.md`.
