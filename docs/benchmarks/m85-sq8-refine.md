# M85 — pg_scann v6 SQ8-refine: memory-optimized rerank tier (SIFT1M A/B)

**Date:** 2026-07-11 · **Dataset:** SIFT1M (full 1M, official GT) · **Verdict:** `GO — memory win (3.5× smaller); honest-partial on warm-cache QPS`

M85 adds an **SQ8 refine tier** to the storage-separated AM: the v6 scan reranks survivors on SQ8 codes
(`dim` B/vec = 128 B) instead of raw f32 (`dim·4` = 512 B), motivated by M84's finding that the Stage-2 f32
random-reads erode the speedup at the high-recall frontier. Single-tier, FAISS `QT_8bit` per-dim min/max,
asymmetric (query stays f32, decode-then-metric).

## Method

Same-data A/B (M46): `sift6` (v6, `separate_storage=1, refine=1`, SQ8 rerank) vs `sift5` (v5, `separate_storage=1`,
f32 rerank), identical 1M data, lists=500, pq_subspaces=32. Swept `over_fetch` × probes. recall@10 vs official GT,
QPS best-of-3.

## Correctness

**246 pg_tests GREEN** (238 + 6 new `sq8.rs` unit tests + 2 v6 `#[pg_test]`), **0 failed** — zero regression. New
`sq8.rs` quantizer (~90 LoC, no library — Rule 9): per-dim min/max train, encode f32→i8, decode+metric.

## The decisive result — memory

| Index | Size | Rerank region |
|---|---|---|
| **v6 SQ8** (`sift6_idx`) | **153 MB** | 128 B/vec (SQ8) |
| v5 f32 (`sift5_idx`) | 528 MB | 512 B/vec (f32) |

**v6 is 3.5× smaller** — the AlloyDB SQ8-default footprint advantage, correctly reproduced. Build: v6 823s, v5 833s
(SQ8 encode is negligible).

## Recall × QPS

| over_fetch | probes | v6 recall | v6 QPS | v5 recall | v5 QPS | recall loss | per-config QPS ratio |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 8 | 32 | 0.9635 | 346.9 | 0.9775 | 271.5 | 1.40% | 1.28× |
| 16 | 32 | 0.9705 | 280.9 | 0.9875 | 182.7 | 1.70% | 1.54× |
| 32 | 32 | 0.9745 | 189.4 | 0.9935 | 134.0 | 1.90% | 1.41× |
| 32 | 64 | 0.9785 | 156.5 | 0.9980 | 114.7 | 1.95% | 1.36× |
| 32 | 128 | 0.9790 | 118.9 | 0.9985 | 96.3 | 1.95% | 1.24× |

## Findings (honest)

1. **Memory: 3.5× smaller index** — the decisive, unambiguous win.
2. **Recall loss ~1.4-2.0%** (ε ≤ 2%, within the FAISS/AlloyDB-predicted SQ8 range). v6 reaches a ~0.979 recall
   ceiling (SQ8 is approximate) vs v5's 0.9985.
3. **QPS-at-MATCHED-recall is flat-to-marginal on warm-cache 1M.** Per-config v6 is 1.15-1.54× faster (128 B not
   512 B), but its ~1.5% recall penalty forces a larger `over_fetch` to reach the same recall, and the SQ8 decode
   adds CPU — so at matched recall v5 (exact f32) is competitive-to-better. Example: at recall ~0.975, **v5 gives
   271 QPS (of=8, probes=32) vs v6 189 QPS (of=32, probes=32)**. This confirms the M85 deep-research caveat: *"the
   win is I/O, not arithmetic; at warm cache the SQ8 decode CPU offsets the fewer-bytes saving, so net QPS can be flat."*

## Honest caveats

- **Warm-cache 1M is the wrong regime to show SQ8's QPS win.** The f32 (512 MB) fits in 16 GB RAM, so v5's 512-B
  reads are buffer hits, and the SQ8 4×-fewer-bytes advantage is a buffer-access saving offset by decode CPU. The
  QPS/I/O win becomes **physical at billion-scale (M88)**, where the 3.5×-smaller SQ8 index fits in RAM and the f32
  index spills to disk — that is where SQ8-default (AlloyDB) pays off. M85 measures the **memory** win now and
  projects the QPS win to M88.
- SQ8 is SIFT-friendly (small-range int components); on wide float embeddings recall loss may exceed 2% — a
  float-corpus re-measure is owed (Rule 5).
- v6 is **opt-in** (`WITH refine=1`); v5 (exact f32) remains the default for recall-critical / warm-cache-QPS use.
  v6 is the **memory-optimized / billion-scale** variant.

## Verdict

**GO — memory win.** SQ8 refine delivers a correct, tested, measured **3.5× memory reduction** at **ε ≤ 2% recall**
— the AlloyDB SQ8-default profile. Its QPS payoff is billion-scale (M88), not warm-cache 1M (honestly reported, not
overclaimed). The feature is 100% functional (246 tests) and opt-in. Still class-AlloyDB-in-Postgres; NOT a
ScaNN-library beat (paradigm tax, M73/ADR-0035). Next: **M86** (SOAR spill — fewer probes for the same recall).

See also: `docs/benchmarks/m84-recall-confirmation.md`, `docs/research/scann-storage-separation-2026-07.md`.
