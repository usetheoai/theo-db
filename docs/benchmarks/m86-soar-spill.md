# M86 — pg_scann SOAR spill: centroid-probe lever (SIFT1M A/B)

**Date:** 2026-07-12 · **Dataset:** SIFT1M (full 1M, official GT) · **Verdict:** `HONEST-NEGATIVE (SIFT1M warm-cache QPS)`

M86 implements **SOAR** (Spilling with Orthogonality-Amplified Residuals, Sun et al. NeurIPS 2023,
[arXiv:2404.00774](https://arxiv.org/abs/2404.00774)): each vector is spilled to a **second** list chosen by the
loss `L(c') = ‖v−c'‖² + λ·⟨v−c', r⟩²/‖r‖²` (`r = v−c₁`), so a query probing **fewer** lists still finds the vector
— attacking the centroid-probe bottleneck. `ivf.rs::with_soar_spill` (~40 LoC), `soar_lambda` reloption; scan
dedup-by-tid reuses the existing `amgettuple` emitted-`HashSet` (no scan change).

## Correctness

**247 pg_tests GREEN** (246 + 1 SOAR test: `ambuild_ivf_soar_spill_scans_high_recall_no_dupes` — asserts no
duplicate ids in the top-k despite each vector living in 2 lists), **0 failed** — zero regression.

## Results

| over_fetch | probes | SOAR recall | SOAR QPS | base recall | base QPS | recall gain | QPS ratio |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 8 | 4 | 0.8905 | 314.6 | 0.7680 | 394.0 | **+0.1225** | 0.80 |
| 8 | 8 | 0.9455 | 261.1 | 0.8870 | 331.2 | **+0.0585** | 0.79 |
| 8 | 16 | 0.9560 | 220.6 | 0.9565 | 306.8 | −0.0005 | 0.72 |
| 8 | 32 | 0.9505 | 186.5 | 0.9775 | 251.2 | −0.0270 | 0.74 |
| 16 | 4 | 0.9030 | 231.9 | 0.7720 | 311.1 | **+0.1310** | 0.75 |
| 16 | 8 | 0.9650 | 181.0 | 0.8940 | 238.9 | **+0.0710** | 0.76 |
| 16 | 16 | 0.9825 | 156.4 | 0.9660 | 214.5 | +0.0165 | 0.73 |
| 16 | 32 | 0.9840 | 125.2 | 0.9875 | 181.0 | −0.0035 | 0.69 |
| 16 | 64 | 0.9820 | 104.3 | 0.9925 | 159.0 | −0.0105 | 0.66 |

**Index size:** SOAR 1051 MB vs base 528 MB (**2×**). Build: SOAR 1012s vs base 833s (+3min for the spill pass).

## Findings (honest)

1. **The centroid-probe lever is REAL.** At LOW probes SOAR reaches materially higher recall: **probes=4 → +0.12**
   (0.89 vs 0.77), probes=8 → +0.06-0.07. Fewer probes reach a given recall — exactly the SOAR promise.
2. **But it does NOT yield a QPS win on SIFT1M warm-cache.** SOAR is slower at **every** point (QPS ratio
   0.66-0.80). Two causes: (a) the index **doubled** (f32 duplicated — see caveat), so each probe reads ~2× the
   candidates; (b) SIFT1M's high-recall bind is the **Stage-2 f32 random-read** (M84/M85), **not** the probe count
   — SOAR attacks the wrong axis for this dataset/regime.
3. **SOAR's recall ceiling is capped ~0.98** (vs base's 0.99+): spilled duplicates occupy rerank-pool slots
   (deduped at emit, but the pool is finite), so at high probes the gain goes negative.

This is the exact **honest-negative the M86 deep research predicted**: *"SOAR helps the probe axis; M84 measured
our SIFT1M bind is the Stage-2 read, not probes — be willing to return honest-negative."*

## Honest caveats

- **The 2× index is an implementation artifact, not inherent to SOAR.** The paper's +7.7-17.3% assumes only the
  **code** is duplicated (f32 stored once globally). Our v5 per-list VECTOR region duplicates the f32 too (a spilled
  vector's full f32 is packed in both its lists). A global-vector-store redesign would fix the storage — **but since
  SOAR gives no SIFT1M QPS win even at +15% (the probe axis isn't the bind), that redesign is deferred (YAGNI)**
  until a regime where probe-reduction pays off (billion-scale / very-high-nlist, M88).
- SOAR's benefit **grows with N and recall** (paper Fig. 10) — it may pay off at billion-scale where many probes
  are needed and memory bandwidth (not the small-N Stage-2 read) dominates. Not measured here.
- The feature ships **opt-in** (`WITH soar_lambda=N`, default 0=off). Correct + tested; its measured SIFT1M value is
  a low-probe recall lever, not a QPS win.

## Verdict

**HONEST-NEGATIVE on SIFT1M warm-cache QPS** (measurement-first, like M82). SOAR is correctly implemented, tested
(247 pg_tests), and demonstrates the real centroid-probe recall lever (+0.12 recall at probes=4) — but on SIFT1M it
does **not** translate to a QPS win, because the probe axis is not this dataset's bind (the Stage-2 read is, which
M85's SQ8 addresses) and the minimal v5-layout impl doubled storage. Ships **opt-in**; the win is projected to
billion-scale (M88). Still class-AlloyDB-in-Postgres; NOT a ScaNN-library beat (M73/ADR-0035). Next: **M87**
(filtered ANN + planner).

See also: `docs/benchmarks/m85-sq8-refine.md`, `docs/benchmarks/m84-recall-confirmation.md`, `docs/research/scann-storage-separation-2026-07.md`.
