# Blueprint: M59 — Anisotropic Vector Quantization + Asymmetric Hashing SIMD (close the ~25× ScaNN QPS gap)

**Slug:** `m59-anisotropic-ah`
**milestone_id:** M59
**Created:** 2026-07-08
**Owner:** paulohenriquevn (Vector/ANN council)
**Rigor profile:** `.claude/rules/discover-phd-rigor.md` (P2 vector pillar — SOTA-anchored, ≥2 primary sources/technique, benchmark-or-`UNBENCHMARKED`)
**Supersedes/deepens:** `m39-pq-product-quantization-blueprint.md` (go/no-go on PQ+ADC — this blueprint resolves the anisotropic-loss weight it left `BLOCKED partial` and specifies the AH SIMD kernel it deferred)
**Direct antecedent:** `docs/adr/0018-m57-sbq-inline-not-superior.md` (SBQ inline MEASURED not-superior — constant-factor, not asymptote; M59 is the real algorithmic axis)

---

## Context — why M59 is the real lever

The North Star (P0 GOTO, `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md`) demands **measured** vector
superiority vs AlloyDB/ScaNN. The gap is quantified: at recall@10 ≥ 0.99 on SIFT1M, ScaNN does **1920 QPS** vs
theodb_ivfflat **78 QPS** — a **~25× throughput gap** (`docs/benchmarks/m33-scann-headtohead.md:22,27`; recall is
PARITY at 0.997 vs 0.992, the gap is purely QPS).

Two prior measurements narrow the search space to exactly one axis:

1. **M36 falsified "distance is the bottleneck"** (`m36-quantization-in-index-blueprint.md:22-35`): full-precision
   f32 distance is only ~14-15% of scan cost; candidate-count + O(C·log C) sort dominate. ScaNN's edge is
   scanning **far fewer candidates** (anisotropic partitioning + SOAR) and never sorting them all.
2. **M57/ADR-0018 falsified "scalar bit-quantization is superior"** (`docs/adr/0018-m57-sbq-inline-not-superior.md:18-19`):
   SBQ inline is recall-neutral but **0.35-0.77× the f32 QPS** in every regime — a constant-factor LOSS. The
   Hamming-walk read-path is more expensive per query, not less. **The ADR explicitly reframes P1/M59 as
   "quantização anisotrópica + Asymmetric Hashing SIMD, não bit-quantization escalar"** (`:45-46`).

So M59 targets the two ScaNN mechanisms that SBQ does NOT have: **(A) an anisotropic (score-aware) codebook** that
preserves top-k ranking per byte far better than isotropic PQ or scalar SBQ, and **(B) an Asymmetric Hashing (AH)
LUT kernel vectorized with in-register SIMD** so per-candidate scoring is a handful of `pshufb` table lookups
instead of a full f32 dot product. Together these are what let ScaNN scan a partition's codes at multi-GB/s.

## Objective

Produce the design decision (with alternatives) for **how to build anisotropic-PQ + AH-SIMD into TheoDB's
existing M35/M51 page-native HNSW AM**, so a subsequent `/to-plan m59-implement` can implement a benchmark-gated
distance path. Honest up-front finding: **no permissive peer ships anisotropic PQ + AH-fastscan we can vendor**
inside a PG extension (ScaNN is a standalone C++/TF library; FAISS is a C++ library; both Apache/MIT and
*studyable/portable* but not drop-in as a `pgrx` crate) — M59 is a **build-from-primary-literature-and-reference-code**
bet, std-only (`std::arch` SIMD), gated by a recall×QPS benchmark vs the f32/SBQ baselines.

---

## Coverage Corner 1 — Integration Tests (how the peers test the quantizer)

### Project A — ScaNN (Apache 2.0 — studyable, portable, `github.com/google-research/google-research/tree/master/scann`)

ScaNN's quantizer validity is tested at two levels. The **anisotropic loss** has unit tests over the projection
decomposition (`scann/scann/proto/…` + `scann/scann/utils/…`): the parallel/orthogonal residual split is asserted
to reduce to the isotropic loss when the parallel weight `η = 1`, and to increase the parallel penalty monotonically
as `η > 1`. The **AH scoring** is tested against a brute-force ADC oracle: `LUT16` in-register scoring must equal
the reference float-LUT ADC within the fixed-point quantization tolerance (the LUT is quantized to `int8`/`uint8`
for the `pshufb` path — that requantization is where the tolerance lives). The load-bearing pattern for us: a
`anisotropic_adc_correlates_with_f32_distance` test — the analog of our
`sbq_hamming_correlates_with_f32_distance` (`theodb_rs/src/sbq.rs:280`) — proving the AH-scored order ranks
neighbors like true f32 on a held-out query set.

### Project B — FAISS (MIT — `github.com/facebookresearch/faiss`)

FAISS `IndexPQFastScan` / `IndexIVFPQFastScan` (the "4-bit PQ fastscan", the direct AH-SIMD precedent) is tested by
`tests/test_fast_scan.py` and `tests/test_fast_scan_ivf.py`: it asserts fastscan recall **equals** the reference
`IndexPQ` (non-SIMD ADC) within the LUT-requantization epsilon over a synthetic corpus, across several `M`
(subquantizer count) and `nbits=4`. The `pq4_*` kernels in `faiss/impl/pq4_fast_scan.cpp` +
`faiss/impl/simd_result_handlers.h` have C++ unit coverage that the SIMD accumulate matches the scalar accumulate
bit-for-bit on the packed `int8` LUT. This is exactly the two-tier oracle M59 needs: (1) anisotropic-PQ ADC vs f32
(quantizer validity), (2) AH-SIMD accumulate vs scalar-LUT accumulate (kernel correctness).

### Project C — TheoDB own SBQ (the in-repo test pattern to mirror)

8 `#[pg_test]` in `theodb_rs/src/sbq.rs:247-373`. The ones M59 must reproduce as anisotropic-PQ analogs:
`sbq_hamming_correlates_with_f32_distance` (`:280` — the quantizer-validity gate; becomes
`aq_adc_correlates_with_f32_distance`), `sbq_deterministic_train` (`:331` — anisotropic k-means needs a fixed
seed), `sbq_codebook_roundtrips_through_meta_bytes` (`:310` — the codebook must round-trip through the v3 meta
page), `sbq_knn_smoke` (`:339` — end-to-end), and the typed-error negatives (`:360,367`). The existing on-disk
scan equivalence oracle — `ground_search_matches_brute_exact_knn` (`theodb_rs/src/ann/scan_core.rs:272`) — extends
directly: the AH-walk over the real graph must return the exact-kNN set at high `ef` (recall-neutral survivors +
f32 rerank).

## Coverage Corner 2 — Dependencies

**Zero new heavy dependency (parsimony rung 4/5; PRD D4).** The two axes are:

- **Anisotropic k-means (training):** hand-rolled Lloyd + the anisotropic residual reweighting, std-only, reusing
  `crate::vec` (distance) + `crate::ann::Rng` (seed) — the exact posture the M39 blueprint already committed to
  (`m39-pq-product-quantization-blueprint.md:100-103`) and that pgvectorscale proves is dep-free (it trains SBQ
  with hand-rolled Welford stats, zero clustering crate). The AGPL `k_means` path-crate in vectorchord
  (`vectorchord/Cargo.toml:30`) is **barred by D1** — study-only.
- **AH-SIMD LUT kernel (scan):** `std::arch::x86_64` AVX2 intrinsics (`_mm256_shuffle_epi8` = `pshufb`,
  `_mm256_add_epi8`/`_epi16` for the accumulate), gated by `is_x86_feature_detected!("avx2")` with a scalar
  fallback — **the identical dispatch pattern already in `theodb_rs/src/vec.rs:121,133`** (`l2_sq`, `cosine_terms`,
  `dot` are `#[target_feature(enable = "avx2,fma")]` with a runtime-detected scalar path). No `faiss-sys`, no C++
  link, no BLAS. This is essential complexity (the kernel) with zero accidental dependency surface.

RaBitQ (vectorchord, AGPL) and ScaNN/FAISS source are **read for understanding, never linked/copied** (D1).

## Coverage Corner 3 — Tools

- **Criterion micro-bench** for the AH-SIMD kernel latency (LUT-lookups/sec per candidate), mirroring
  pgvectorscale's `benches/distance.rs` PQ-distance section and our own `theodb_rs/src/vec.rs` SIMD benches
  (`cosine_simd_per_candidate_speedup`, `:487`). Same-graph, box-noise-immune (the FU-1
  lesson, `m46-measurement-learnings` memory) — measures the kernel in isolation.
- **End-to-end recall×QPS harness** `benchmarks/theodb_bench/` over **real SIFT1M** (never synthetic — ADR-0012
  data-degeneracy trap; ADR-0018 caveat notes gaussian-mixture absolutes can move). ≥3 runs, mean±std, effect >
  variance (`analysis-golden-rule § A1`; M38 variance lesson). The M33 head-to-head harness
  (`benchmarks/run_m33_scann.py`) is the frontier comparator — re-run ScaNN on the SAME seeded 1000-query subsample.
- **`THEODB_SCAN_PROFILE=1`** (the M36 phase profiler) to confirm the AH kernel actually moves the `score` phase AND
  that the smaller codes cut the `reads` phase (the real ~44-51% cost, `m36-quantization-in-index-blueprint.md:26-27`).

## Coverage Corner 4 — Techniques

### T1 — Anisotropic (score-aware) vector quantization (the codebook)

**What it is.** Classic PQ (Jégou, Douze, Schmid 2011, IEEE TPAMI 33(1):117-128) trains each subspace codebook to
minimize the **isotropic reconstruction error** `‖x − q(x)‖²` — it treats every direction of the residual equally.
ScaNN's insight (Guo, Sun, Lindgren, Geng, Simcha, Chern, Kumar 2020, "Accelerating Large-Scale Inference with
Anisotropic Vector Quantization", ICML / PMLR 119:3887-3896, arXiv:1908.10396) is that for **MIPS / top-k retrieval**
not all residual error matters equally: the component of the residual **parallel to the datapoint direction**
distorts the inner product `⟨q, x⟩` — and hence the ranking of the true near-neighbors — far more than the
orthogonal component. So the loss should penalize parallel error harder.

**The math.** Decompose the residual `r = x − q(x)` relative to the datapoint direction `x̂ = x/‖x‖` into a
parallel part `r_∥ = (r·x̂) x̂` and an orthogonal part `r_⊥ = r − r_∥`. The score-aware anisotropic loss is

```
L(x, q(x)) = h_∥ · ‖r_∥‖²  +  h_⊥ · ‖r_⊥‖²        with   h_∥ / h_⊥ = η  >  1
```

The weight ratio `η` is derived from the expected geometry of query directions: the paper shows the optimal ratio
grows with dimension `d` as `η ∝ (d − 1) · ‖x‖² / t²` for a query threshold `t`, and in practice ScaNN uses a
tunable `anisotropic_quantization_threshold` (`T`) that sets `η` — larger `T` ⇒ larger parallel penalty. `η = 1`
recovers isotropic PQ exactly (the unit-test hook). Training is Lloyd's k-means with the centroid-update and
assignment steps re-derived under this weighted loss (the parallel/orthogonal split makes the optimal centroid a
weighted combination, closed-form per iteration). **Why it beats isotropic PQ/OPQ:** OPQ (Ge, He, Ke, Sun 2013,
CVPR — a learned rotation before PQ) reduces *isotropic* error but is still direction-agnostic; anisotropic
quantization directly optimizes the quantity retrieval cares about (rank-preserving inner product), so at the same
bit-budget it preserves top-k order better. **Published result:** ~**2× QPS at fixed accuracy** vs the
next-fastest of 11 libraries on glove-100-angular (Google Research blog,
`research.google/blog/announcing-scann-efficient-vector-similarity-search/`; corroborated on ann-benchmarks.com).

**Sources (≥2 primary, R2):** Guo et al. 2020 arXiv:1908.10396 (PMLR 119) + Google Research blog; isotropic
baseline Jégou 2011 TPAMI (dl.acm.org/doi/10.1109/TPAMI.2010.57) + Douze et al. 2024 "The Faiss library"
arXiv:2401.08281. The exact `η/T` numeric default is now resolved (the M39 `BLOCKED partial`): ScaNN exposes it as
`anisotropic_quantization_threshold`, and `η = 1` ⇔ isotropic — the tunable is the design knob, not a fixed constant.

### T2 — Asymmetric Hashing (AH) / LUT SIMD (the scan-time scorer)

**What it is.** "Asymmetric Hashing" is ScaNN's name for asymmetric-distance-computation (ADC) scoring: the query
stays full-precision, the corpus is quantized to codes, and per query you precompute one **lookup table**
`LUT[i][j] = partial_score(x^i, c_{i,j})` of shape `m × k*` (`m` subspaces, `k*` centroids each). The score of any
corpus code is then `Σ_{i=1..m} LUT[i][code_i]` — `m` table lookups + `m−1` adds, **no per-candidate multiply, no
decode**. This is exactly the ADC skeleton the M39 blueprint described (`:63`); AH is that skeleton scored with a
SIMD kernel.

**The SIMD trick (the ScaNN/FAISS "4-bit fastscan" / LUT16).** The naïve ADC does `m` scattered `float` gathers per
candidate — cache-and-latency-bound, the reason plain PQ-ADC is not fast. ScaNN and FAISS both use **4-bit
subquantizers (`nbits=4`, k*=16)** so each subspace's 16 LUT entries fit in a **16-byte register lane**, and the
`pshufb` instruction (`_mm256_shuffle_epi8`) does **16 parallel table lookups in one instruction**: the 4-bit codes
of 16 (AVX2) / 32 (AVX-512) candidates index the in-register LUT simultaneously. The partial scores are quantized
to `int8` (the LUT is requantized per query to fit the byte lane — the source of the small tolerance) and
accumulated with `_mm256_add_epi8` / widened to `epi16` to avoid overflow. Corpus codes are stored in a
**transposed / interleaved "SIMD-friendly" block layout** (blocks of 32 vectors, subquantizer-major) so one load
feeds one `pshufb`. Throughput: multiple **billions of LUT lookups/sec**, i.e. per-candidate scoring becomes a
handful of cycles — this is the concrete mechanism behind the 25× the M36 profile said we could not get from
narrowing the f32 distance alone (because AH also collapses the `score` phase to near-free AND shrinks the bytes/
candidate that dominate `reads`).

**Peer symbols (concrete):** FAISS `faiss/impl/pq4_fast_scan.cpp` (`pq4_lookup`, `pq4_accumulate_loop`),
`faiss/impl/simd_result_handlers.h`, `IndexPQFastScan` / `IndexIVFPQFastScan`; ScaNN
`scann/scann/hashes/asymmetric_hashing2/` (`LUT16`, `PackedDataset`, the `pshufb` scoring in
`internal/lut16_*_avx2.inc` / `avx512`). Both Apache/MIT — the algorithm and layout are studyable and portable to
`std::arch`; **no code is copied**, the kernel is re-derived in Rust.

**Sources (≥2 primary, R2):** André, Kermarrec, Le Scouarnec 2015 "Cache locality is not enough: High-performance
nearest neighbor search with product quantization fast scan" (VLDB — the original `pshufb` 4-bit PQ scan);
Douze et al. 2024 "The Faiss library" arXiv:2401.08281 (fastscan §); Guo et al. 2020 arXiv:1908.10396 (AH/LUT16).

---

## Cross-cutting comparison (each cell cited or `UNBENCHMARKED`)

recall@10; QPS relative to our f32 HNSW; bytes/vector at dim=128; scan-time scorer. Anchored to M33/M38/ADR-0018.

| Method | recall@10 | QPS relative | bytes/vec (dim=128) | scan scorer |
|---|---|---|---|---|
| f32 HNSW (ours, baseline) | 1.0 (SIFT real, `m38`) / 0.99 reachable (`m33:27`) | 1× | 512 (`4·dim`) | exact f32 dot (SIMD, `vec.rs`) |
| SBQ inline (ours) | recall-neutral vs f32 (`ADR-0018:23`) | **0.35-0.77×** (SLOWER, `ADR-0018:25-29`) | 16 (`⌈dim·1/8⌉`) | Hamming-walk + f32 rerank |
| PQ-8bit isotropic (M39) | `UNBENCHMARKED` on our stack | ADC skeleton; no LUT16 | m (m=16→16) | scattered float ADC |
| **anisotropic-PQ + AH-SIMD (M59)** | `UNBENCHMARKED` on our stack; ScaNN precedent recall-parity at 25× QPS (`m33`) | ScaNN: ~2× vs next-fastest lib (research.google); vs OUR baseline the target is closing 25× — `UNBENCHMARKED` | m/2 (4-bit, m=16→8) | **`pshufb` LUT16 (near-free) + f32 rerank** |
| ScaNN (the target) | 0.997 @ leaves=100 (`m33:22`) | **1920 QPS** (25× ours, `m33:22,27`) | learned | anisotropic AH LUT16 |

Signal: AH-SIMD + anisotropic codebook is the **only** method with a SOTA precedent of recall-parity at ~25× QPS
(the exact gap M59 must close), and the only one that attacks BOTH measured bottlenecks (M36: `reads` via 8 bytes/
vec 4-bit codes; `score` via near-free `pshufb`). SBQ is falsified as a performance path (ADR-0018).

---

## ADRs

### ADR-1 — Build anisotropic-PQ + AH-SIMD as a NEW opclass/reloption on the EXISTING M35 HNSW AM (recommended)

**Decision.** Do **not** replace the SBQ-inline path, and do **not** add a new access method. Add anisotropic-PQ +
AH-SIMD as a **new build knob on the existing `theodb_hnsw` AM** — a reloption `WITH (pq_subspaces = M, pq_bits = 4,
aq_threshold = T)` alongside the current `WITH (sbq_bits = N)` (`theodb_rs/src/am/options.rs:19-33` is the exact
precedent; the shared `TheodbIvfflatOptions` struct already carries `sbq_bits` and each AM reads only its option).
This lands as **meta-page layout v3**, coexisting with v1 (f32) and v2 (SBQ) exactly as v2 coexisted with v1:

- **Element tuple:** the trailing bytes after the f32 vector hold the **`m/2`-byte 4-bit PQ code** instead of the
  SBQ code (`elem_size(dim, code_len)` already parameterizes the trailing length,
  `theodb_rs/src/am/hnsw_page.rs:59-64`; `code_bytes: &b[end..]` already exposes it, `:234`). The f32 vector STAYS
  inline (needed for the exact rerank — the survivors' true distance, the ADR-0018 recall-recovery pattern).
- **Meta page:** persist the anisotropic codebook (`m × k* × (dim/m)` centroids + the `aq_threshold`) via the same
  versioned `[bits/kind][cb_len:u32][codebook]` trailer the SBQ codebook already uses (`hnsw_page.rs:113-141`,
  `HNSW_STRUCT_VERSION_SBQ` → add `HNSW_STRUCT_VERSION_AQ = 3`). The codebook round-trips through
  `to_meta_bytes`/`from_meta_bytes` (the M22 pattern, `sbq.rs:101-146`).
- **Scan seam:** the walk-scoring branch is **already isolated** — `traverse` reconstructs the query code once and
  scores the walk by the cheap code distance, then f32-reranks the survivors (`hnsw_page.rs:925-947` for SBQ;
  `scan_core::ground_search_nodes` returns raw nodes precisely so the caller can re-score, `scan_core.rs:101-107`).
  M59 swaps the per-candidate `hamming_bytes` (`hnsw_page.rs:851`) for the **AH LUT16 `pshufb` accumulate**, with
  the LUT precomputed once per query right where `qcode_owned` is built (`hnsw_page.rs:927-946`). The
  `NeighborSource` seam does NOT change — the AH kernel is a new `dist`-equivalent over the inline 4-bit code.

**Rationale.** (1) The M35/M51 layout was **designed** for exactly this — a trailing per-node code + a versioned
codebook in the meta + a walk-scoring branch that already separates cheap-code-distance from f32-rerank. The
integration surface is ~1 new file (`aq.rs`, mirroring `sbq.rs`) + the AH kernel in `vec.rs` + one branch in
`traverse`. Essential complexity only (CLAUDE.md § Esforço ≠ Complexidade). (2) A **reloption/opclass** (not a
replacement) keeps SBQ/f32 indexes byte-identical and lets the benchmark compare all three on the same AM — the
honest A/B the D3 gate needs. (3) It reuses the AVX2 dispatch pattern of `vec.rs` verbatim (`is_x86_feature_detected`
+ scalar fallback) — no new dependency, no new AM handler, no planner cost-model change.

**Alternatives considered.**
- **(b) Replace SBQ-inline with anisotropic-PQ+AH** — rejected: destroys the v2 format that ADR-0018 explicitly
  kept as an experiment base; forces a REINDEX of every existing index; and forecloses the honest 3-way benchmark.
  Anti-goal (removing a working format for a not-yet-measured one violates measurement-first + anti-sunk-cost).
- **(c) A brand-new `theodb_scann` access method** — rejected (YAGNI + KISS): a new AM means a new handler, new
  build/scan/cost callbacks, new WAL/crash-safety surface (M48), all to host a distance kernel that fits in the
  existing scan seam. The M36/M57 evidence says the lever is the **scorer + codebook**, not the index structure —
  HNSW-with-AH is exactly ScaNN's own shape (partition + AH), and our HNSW already prunes candidates well
  (theodb HNSW ~1.2× pgvector QPS, ADR-0018:42). No second AM is justified.
- **(a-lite) Fold AH into the IVFFlat AM instead of HNSW** — deferred: IVFFlat scans a whole partition (the AH
  fastscan's ideal batch layout — contiguous codes → one `pshufb` sweep), which is closer to ScaNN's leaf scan and
  may extract MORE of the kernel's throughput than HNSW's pointer-chasing walk. This is a **real open question**
  (see Risks R2 / Unresolved Q1): the AH kernel is layout-sensitive and HNSW's per-node scoring may not feed
  `pshufb` as efficiently as a contiguous IVF list. Recommended path: build the kernel + codebook once
  (`aq.rs`/`vec.rs`), wire it into HNSW first (smallest diff, reuses the M51 seam), **measure**, and only add the
  IVF batch-scan variant if the HNSW walk under-feeds the SIMD kernel.

**Gate (PRD D3, anti-sunk-cost).** Merges ONLY if the harness proves, ≥3 runs mean±std on **real SIFT1M**, that
anisotropic-PQ+AH beats the f32 HNSW baseline on **QPS at recall ≥ 0.99** (the M33 operating point), effect >
variance. If it beats SBQ but not f32-at-0.99, that is an honest partial (record as claim-bounded, not
"superiority"). If it does not close a material fraction of the 25×, record the honest-negative and the next seed
(DiskANN-style disk-resident + SOAR partitioning — a complementary axis, not a substitute).

### ADR-2 — Anisotropic k-means hand-rolled, std-only, deterministic (zero new dep)

**Decision.** Hand-roll the anisotropic Lloyd's k-means in `aq.rs`, reusing `crate::vec` + `crate::ann::Rng`,
with a fixed seed for `aq_train_deterministic`. **Rationale.** pgvectorscale trains dep-free; the AGPL `k_means`
crate is barred (D1); the anisotropic centroid update is a ~60-line closed-form weighted mean, not worth an
`ndarray`/`linfa` transitive tree (parsimony rung 4). **Consequences.** Essential ~60-100 LoC; the `η = 1` isotropic
reduction is the unit-test correctness hook.

### ADR-3 — 4-bit subquantizers (`k*=16`) for the LUT16 `pshufb` path

**Decision.** Use `nbits=4` (16 centroids/subspace) so the LUT fits a 16-byte lane and the AH kernel is
`_mm256_shuffle_epi8`, matching ScaNN/FAISS fastscan. **Rationale.** 8-bit PQ (k*=256) forces scattered float
gathers (slow — the reason plain ADC is not fast); 4-bit is the SOTA sweet spot that unlocks the in-register
lookup. **Consequences.** Slightly coarser codebook (16 vs 256 centroids) — the anisotropic loss + f32 rerank of
survivors recovers recall; `m/2` bytes/vector (half the SBQ 1-bit budget at m=dim/8). Requantizing the LUT to
`int8` per query introduces a bounded tolerance (the FAISS test epsilon) — asserted, not ignored.

---

## Recommendations (for the human / next `/to-plan`)

1. **Go — conditionally, on the recommended shape (ADR-1a).** Proceed to `/to-plan m59-implement` for
   anisotropic-PQ + AH-SIMD as a **new reloption/opclass on the existing M35 HNSW AM** (meta layout v3), NOT a new
   AM and NOT a replacement of SBQ. The integration surface is one new domain file (`aq.rs`), the AH kernel in
   `vec.rs`, and one scan-branch swap in `hnsw_page.rs::traverse`.
2. **Two-part build, kernel first, measured between.** (i) `aq.rs`: anisotropic Lloyd k-means (`η`-weighted,
   std-only) + 4-bit encode + codebook meta round-trip (mirror `sbq.rs`). (ii) `vec.rs`: `ah_lut16` AVX2 kernel
   (`_mm256_shuffle_epi8` accumulate, scalar fallback, `is_x86_feature_detected` dispatch) + criterion micro-bench.
   Wire into HNSW `traverse` (swap `hamming_bytes` at `hnsw_page.rs:851`, precompute LUT at `:927`), keep the f32
   rerank of survivors.
3. **Mandatory tests** (mirror `sbq.rs` + FAISS two-tier oracle): `aq_adc_correlates_with_f32_distance`
   (quantizer validity), `ah_simd_matches_scalar_lut` (kernel correctness within LUT-requant epsilon),
   `aq_train_deterministic`, `aq_codebook_roundtrips_through_meta_bytes`, end-to-end `ground_search` == brute-kNN
   at high ef (extend `scan_core.rs:272`), typed-error negatives.
4. **Benchmark first, D3-gated.** `benchmarks/theodb_bench/` over **real SIFT1M**, ≥3 runs mean±std, re-run ScaNN
   on the same seeded subsample (`run_m33_scann.py`). Gate: **QPS at recall ≥ 0.99 vs f32 baseline**, effect >
   variance, before ANY superiority claim (Regra 5 / `public-copy.md § 4`). Confirm with `THEODB_SCAN_PROFILE=1`
   that AH moves both `score` and `reads`.
5. **If HNSW under-feeds the kernel, add the IVF batch-scan variant (ADR-1 a-lite), not a new AM.** The contiguous
   IVF-list layout is the AH fastscan's ideal batch — a measured fallback, not speculation.
6. **Honest scope:** this is the algorithm axis, not plumbing. It may not close the full 25× alone; disk-resident /
   SOAR partitioning is a **complementary** next lever, tracked as a seed — never claimed as done here.

## Blocked / resolved questions

- **[RESOLVED]** ScaNN anisotropic weight `η` (M39 left `BLOCKED partial`): it is the tunable
  `anisotropic_quantization_threshold T`; `η = 1` ⇔ isotropic PQ; larger `T` ⇒ larger parallel penalty. Sourced
  from arXiv:1908.10396 + ScaNN API. The exact per-dataset default is a tuning knob, not a fixed constant — not
  required for the go/no-go.
- **[OPEN — Unresolved Q1]** Does HNSW's per-node pointer-chasing walk feed `pshufb` as efficiently as a contiguous
  IVF-list sweep? Layout-sensitive; resolved by measurement in the implement cycle (ADR-1 a-lite is the fallback).

## Related

- Antecedent go/no-go (deepened here): `.claude/knowledge-base/discoveries/blueprints/m39-pq-product-quantization-blueprint.md`
- Reframing decision: `docs/adr/0018-m57-sbq-inline-not-superior.md`
- The measured gap: `docs/benchmarks/m33-scann-headtohead.md`
- Scan phase breakdown (bottlenecks): `.claude/knowledge-base/discoveries/blueprints/m36-quantization-in-index-blueprint.md`
- Layout + scan seam: `theodb_rs/src/am/hnsw_page.rs` (element/meta/traverse), `theodb_rs/src/ann/scan_core.rs` (NeighborSource)
- Quantizer pattern to mirror: `theodb_rs/src/sbq.rs`; SIMD dispatch pattern: `theodb_rs/src/vec.rs`
- Reloption precedent: `theodb_rs/src/am/options.rs`
- Rigor profile: `.claude/rules/discover-phd-rigor.md`
