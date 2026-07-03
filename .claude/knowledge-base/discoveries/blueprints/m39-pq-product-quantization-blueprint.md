# Blueprint: Product Quantization (PQ) as the vector-superiority lever

**Slug:** `m39-pq-product-quantization`
**milestone_id:** M39
**Created:** 2026-07-03
**Source plan:** `.claude/knowledge-base/discoveries/plans/m39-pq-product-quantization-plan.md`
**Rigor profile:** `.claude/rules/discover-phd-rigor.md` (P2 vector pillar — SOTA-anchored, ≥2 primary sources/technique, benchmark-or-`UNBENCHMARKED`)

## Context

TheoDB's P0 GOTO is vector-superiority vs AlloyDB/ScaNN — today only recall-parity is proven (`docs/adr/0002-north-star-equal-or-superior-to-alloydb.md`). The M38 measurement (`docs/benchmarks/m38-copy-free-scan.md:12-19,36-40`) falsified our SBQ (recall 0.774/0.854/0.947 < 1.0 on real SIFT) and found the scan copy is not the bottleneck — naming **PQ (product quantization) + LUT ADC**, the ScaNN/FAISS technique, as the real remaining algorithmic lever. This blueprint gathers the SOTA evidence to decide whether to build PQ, in what shape, and under which benchmark trigger (measurement-first, PRD D3; `CLAUDE.md § Esforço ≠ Complexidade`).

## Objective

Decide **go/no-go on building PQ+ADC for TheoDB, and the minimal integration shape**, so a subsequent `/to-plan` (M39-implement) can implement a benchmark-gated PQ distance path. Key finding up front: **no permissive peer ships PQ today** (pgvectorscale removed it — SBQ only; vectorchord's RaBitQ is AGPL) — so PQ is a build-from-primary-literature bet, std-only, gated by a recall×QPS benchmark vs SBQ.

## Coverage Corner 1 — Integration Tests

### Project A — pgvectorscale (SBQ, PostgreSQL License — borrowable pattern)

The scan-time quantizer test/integration flow that PQ must mirror: build materializes the quantized code per node (`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/sbq/storage.rs:253-268`, `create_node`); scan quantizes the query once (`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/sbq/mod.rs:145-148`) then computes candidate distance over codes via `calculate_bq_distance` → XOR (`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/sbq/mod.rs:150-158`); exact f32 rerank runs after (`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/sbq/storage.rs:304-328`, `get_full_distance_for_resort`).

### Project B — TheoDB own SBQ (the test pattern PQ must reproduce)

Our SBQ has 8 `#[pg_test]` in `theodb_rs/src/sbq.rs:186-292`. The load-bearing one is **`sbq_hamming_correlates_with_f32_distance` (`theodb_rs/src/sbq.rs:219`)** — the "quantizer-validity" gate proving the quantized distance orders neighbors like real f32. PQ needs the analog `pq_adc_correlates_with_f32_distance`. Others to mirror: deterministic train (`theodb_rs/src/sbq.rs:249` — k-means needs a fixed seed), memory-formula (`theodb_rs/src/sbq.rs:196`), boundary encode (`theodb_rs/src/sbq.rs:203`), end-to-end knn smoke (`theodb_rs/src/sbq.rs:257`), and typed-error negatives (`theodb_rs/src/sbq.rs:278,285`).

### Project C — vectorchord (RaBitQ, AGPL — study-only, NOT borrowable)

Scan distance is a binary accumulate + metadata post-process (`.claude/knowledge-base/references/vectorchord/crates/index/src/accessor.rs:412-432`). AGPL header (`.claude/knowledge-base/references/vectorchord/crates/index/src/accessor.rs:1-13`) — read for understanding, never copied (D1).

## Coverage Corner 2 — Dependencies

### Project A — pgvectorscale

Trains its quantizer with hand-rolled incremental (Welford) statistics in std, **zero clustering crate** (`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/sbq/quantize.rs:115-148`). Direct precedent that a scan-time quantizer needs no external dep.

### Project B — TheoDB own SBQ

`theodb_rs/src/sbq.rs:32-60` (`train`) is std-only (single mean/std pass). A `crate::ann::Rng` is already available (used in tests, `theodb_rs/src/sbq.rs:224`) — enough to seed a deterministic k-means. **Decision: PQ's Lloyd k-means is hand-rolled std-only, reusing `crate::vec` distance + `Rng`. Zero new dependency** (mirrors the M22 zero-new-dep posture; parsimony rung 4/5).

### Project C — vectorchord

Uses a path-crate `k_means = { path = "./crates/k_means" }` (`.claude/knowledge-base/references/vectorchord/Cargo.toml:30`), but that crate is **AGPL/ELv2** (`.claude/knowledge-base/references/vectorchord/crates/k_means/src/flat.rs:1-13`) → **barred by D1** (Apache/MIT/BSD only). External MIT/Apache crates (`linfa-clustering`, `kmeans`) exist but pull `ndarray`/`rand` transitives for ~50 lines of Lloyd — rejected by the parsimony ladder unless a hand-roll proves inferior recall.

## Coverage Corner 3 — Tools

### Project A — pgvectorscale

Criterion micro-benches for distance kernels (`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/benches/distance.rs:1-5,341-357`), including a **"PQ distance on few dimensions" section** (`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/benches/distance.rs:176-196`) — kernel latency, not recall.

### Project B — TheoDB M38 harness

End-to-end recall×QPS over real SIFT (`docs/benchmarks/m38-copy-free-scan.md:12,63-66`): recall@10 of the quantized knn vs exact seqscan (f32 baseline recall=1.0), QPS via `benchmarks/run_m36_scan.py`. The M38 honesty lesson (`docs/benchmarks/m38-copy-free-scan.md:36-40`): variance dominates on a throttled CPU (~±50% between runs) → any PQ harness needs **≥3 runs, mean±std, effect > variance** (`analysis-golden-rule § A1`).

### Project C — vectorchord

Not read for tooling (AGPL, study-only) — out of scope for a borrowable harness.

## Coverage Corner 4 — Techniques

### Product Quantization (PQ) — subspace codebooks + ADC LUT

A vector `y ∈ R^D` is split into `m` disjoint sub-vectors of dim `D/m`. Each subspace `i` has an independent codebook `C_i` of `k*` centroids trained by **k-means (Lloyd)** on that subspace; typically `k*=256` → 8 bits/subquantizer. The code of `y` is the tuple of nearest-centroid indices, `m·log2(k*) = 8m` bits/vector. **ADC (asymmetric distance)** keeps the query full-precision: per query, precompute one lookup table `LUT[i][j] = ‖x^i − c_{i,j}‖²` of shape `m × k*`; then approximate distance to any corpus code is `d(x,y)² ≈ Σ_{i=1..m} LUT[i][q_i(y^i)]` — `m` lookups + `m−1` adds, no decode, no multiply, LUT cache-resident at `k*=256`. Unlike scalar quantization (SBQ: 1 threshold/dim), PQ quantizes groups of dimensions jointly, preserving intra-subspace correlation — why it loses less ranking-info per byte. **Sources:** Jégou, Douze, Schmid 2011, IEEE TPAMI 33(1):117-128 (`https://dl.acm.org/doi/10.1109/TPAMI.2010.57`); Douze et al. 2024 "The Faiss library" (`https://arxiv.org/pdf/2401.08281` — ADC formula, LUT `m×256`, `8m`-bit codes).

### ScaNN anisotropic quantization — score-aware codebook loss

ScaNN keeps PQ's ADC skeleton but replaces the isotropic reconstruction loss with an **anisotropic / score-aware loss**: decompose the residual `r = x − q(x)` into parallel `r_∥` (along the datapoint) and orthogonal `r_⊥`, and penalize `r_∥` more (`L = h_∥‖r_∥‖² + h_⊥‖r_⊥‖²`, `h_∥ > h_⊥`). For MIPS, parallel error distorts high inner products (the top-k neighbors) disproportionately, so preserving ranking means penalizing `r_∥`. This is a **different codebook training**, same scan-time LUT. **Sources:** Guo et al. 2020 ICML/PMLR 119:3887-3896 (`https://arxiv.org/abs/1908.10396`); Google Research blog (`https://research.google/blog/announcing-scann-efficient-vector-similarity-search/`). Published result: **~2× QPS at fixed accuracy vs the next-fastest of 11 libraries on glove-100-angular** (research.google blog; ann-benchmarks.com). Exact `h_∥/h_⊥` numeric weight: **BLOCKED partial** — the PMLR PDF was not text-extractable this run; the qualitative claim + 2× QPS are sourced.

### RaBitQ (vectorchord) — binary codes + per-vector correction (SOTA cross-check)

RaBitQ uses binary codes (1 bit/dim after a random rotation) plus per-code scalar correction factors, recovering distance analytically (`.claude/knowledge-base/references/vectorchord/crates/index/src/accessor.rs:412-432` accumulate + `half_process_l2s`). No LUT, no per-subspace k-means. Differs from PQ (codebooks) fundamentally; AGPL → informs the tradeoff table only, not borrowed.

## Cross-cutting Comparison

recall@10 (k=10); QPS relative to f32; bytes/vector at dim=128; build cost. Each cell cited or `UNBENCHMARKED`.

| Method | recall@10 | QPS relative | bytes/vector (dim=128) | build cost |
|---|---|---|---|---|
| full-precision f32 | **1.0000** (SIFT real 120k, `docs/benchmarks/m38-copy-free-scan.md:12`) | 1× baseline | 512 (`4·dim`) | none |
| SBQ (ours) | 0.774 / 0.854 / 0.947 (b=1/2/4, over_fetch=40, SIFT real 120k; `docs/benchmarks/m38-copy-free-scan.md:12-19`) | `UNBENCHMARKED` (M38: no reliable end-to-end win) | 16/32/64 (`⌈dim·bits/8⌉`, `theodb_rs/src/sbq.rs:89`) | O(N·dim), 1 pass (cheap) |
| PQ 8-bit (m subq, k*=256) | `UNBENCHMARKED` for our config (FAISS: "90%+" SIFT1M, qualitative) | ScaNN: ~2× at fixed accuracy, glove-100-angular (research.google) | m bytes (`8m` bits); m=16→16 | O(iters·N·k*·(dim/m))×m (expensive) |
| RaBitQ (vectorchord) | `UNBENCHMARKED` (AGPL, study-only) | `UNBENCHMARKED` | ~dim/8 + `[f32;4]` meta/vec (`accessor.rs:412-432`) | rotation+encode/vector; no per-subspace k-means |

Signal: **full-precision** is the only proven recall-1.0 (M38); **SBQ falsified** at recall<1.0 (M38); **PQ** is the only method with a SOTA precedent of QPS-win at fixed recall (ScaNN) but `UNBENCHMARKED` on our stack; **RaBitQ** is AGPL → study-only.

## ADRs

### D1 — Build PQ+ADC std-only, benchmark-gated (PRD D3, anti-sunk-cost)

**Decision:** Build a `PqQuantizer` std-only (Lloyd k-means per subspace, hand-rolled, reusing `crate::vec` distance + `crate::ann::Rng`; `theodb_rs/src/sbq.rs:32-60,224` precedent), integrated as a distance branch at `theodb_rs/src/am/scan.rs:198` with the ADC LUT precomputed once per query in `amrescan`/`scan_ivf_structured`, mirroring the SBQ pipeline train→encode→quantized-rank→f32-rerank (`theodb_rs/src/sbq.rs:114-174`). Codebook persisted with the index meta.

**Rationale:** PQ is the only lever with a theoretical basis (per-subspace joint quantization preserves ranking better than the scalar quantization that already failed) AND a SOTA precedent of QPS-win at fixed recall (ScaNN, research.google). No permissive peer ships PQ (pgvectorscale removed it; vectorchord RaBitQ is AGPL) — so it is a build-from-literature bet.

**Alternatives considered:** (1) adopt MIT `kmeans`/`linfa` crate — rejected (transitive deps for ~50 lines; parsimony rung 4); (2) port RaBitQ — rejected (AGPL, D1 license gate); (3) keep SBQ only — rejected (falsified by M38, cannot sustain a superiority claim).

**Gate (PRD D3):** PQ merges ONLY if the Q6 harness proves, with ≥3 runs mean±std on SIFT, that **PQ beats SBQ at equal QPS/bytes (or QPS at equal recall)**, effect > measurement variance. If PQ does not beat SBQ, do NOT merge — record as next-discovery seed (the next lever becomes ScaNN's anisotropic loss over the same PQ skeleton, not vanilla PQ).

**Consequences:** high effort (codebooks + LUT SIMD + codebook persistence in AM pages + recall gate at 1M scale), essential complexity justified by the P0 need; risk that PQ also falls below f32+IVFFlat at recall 1.0 on our corpus — mitigated by the pre-merge benchmark gate.

### D2 — Zero new dependency (Lloyd hand-rolled)

**Decision:** Hand-roll Lloyd's k-means in std, reusing existing `crate::vec` and `crate::ann::Rng`. **Rationale:** pgvectorscale trains its quantizer with no clustering crate (`.../sbq/quantize.rs:115-148`); the AGPL `k_means` crate (`vectorchord/Cargo.toml:30`) is barred by D1. **Consequences:** ~40-60 lines of essential code; deterministic with a fixed seed (needed for the `pq_train_deterministic` test analog).

## Recommendations for the project

1. **Go — conditionally.** Proceed to `/to-plan m39-pq-implement` for a minimal PQ+ADC, but treat the recall×QPS win as `UNBENCHMARKED` until measured. Do NOT claim superiority before the D1 gate passes.
2. **Minimal integration shape (2 points, no new AM, no new dep):** (a) encode at build — `PqQuantizer.train` (k-means per subspace) + `encode` (argmin per sub-vector → m bytes), writing m bytes/entry in the IVFFlat page instead of `dim·4` f32 (mirror `SbqQuantizer`, `theodb_rs/src/sbq.rs:23-92`); (b) ADC LUT at scan — precompute `LUT[m][256]` once per query in `scan_ivf_structured`, swap `theodb_rs/src/am/scan.rs:198` for `Σ LUT[i][code[i]]` when the index is PQ, keeping heapify + exact f32 rerank (top-`k·over_fetch`) identical.
3. **Mandatory tests:** the `pq_adc_correlates_with_f32_distance` quantizer-validity gate (analog of `theodb_rs/src/sbq.rs:219`) + deterministic-train + end-to-end knn smoke + typed-error negatives.
4. **Benchmark first (D1 gate):** reuse `benchmarks/run_m36_scan.py` skeleton over SIFT, ≥3 runs mean±std, gate PQ>SBQ at fixed recall before any merge or claim.
5. **Fallback if PQ underperforms:** the next lever is ScaNN's anisotropic loss over the same PQ skeleton — not a new index.

## Blocked questions (if any)

- ScaNN exact anisotropic loss weight `h_∥/h_⊥` — BLOCKED partial (PMLR PDF not text-extractable this run). Qualitative mechanism + 2× QPS claim are sourced with ≥2 primary references; the exact weight is a next-discovery seed, not required for the go/no-go decision.

## Halt-loop progress (audit trail)

- Q1 PQ algorithm — done (Jégou 2011 + FAISS 2024)
- Q2 ScaNN + tradeoff table — done (Guo 2020 + research.google; anisotropic weight BLOCKED partial)
- Q3 peer integration — done (pgvectorscale borrowable, vectorchord AGPL study-only; neither ships PQ)
- Q4 our SBQ test+scan integration — done (`scan.rs:198` integration point; 8 tests)
- Q5 deps — done (std-only Lloyd, zero new dep; AGPL k_means barred)
- Q6 harness — done (SIFT recall×QPS, ≥3 runs mean±std, PQ>SBQ gate)

## Related

- Source plan: `.claude/knowledge-base/discoveries/plans/m39-pq-product-quantization-plan.md`
- Prior measurement that named PQ: `docs/benchmarks/m38-copy-free-scan.md`
- Existing quantizer (pattern to mirror): `theodb_rs/src/sbq.rs`
- Scan integration point: `theodb_rs/src/am/scan.rs:198`, `theodb_rs/src/vec.rs:167`
- Rigor profile: `.claude/rules/discover-phd-rigor.md`
