---
slug: m59-anisotropic-ah
milestone_id: M59
created_at: 2026-07-08
goal: Add anisotropic-PQ + AH-SIMD as a new reloption on the existing theodb_hnsw AM and prove recall@10 ≥ 0.99 is preserved end-to-end on real SIFT1M, with QPS measured head-to-head vs SBQ (M57) and the ScaNN gap (M33).
---

# Plan: M59 — Anisotropic Vector Quantization + Asymmetric-Hashing SIMD (close the ScaNN QPS gap)

> **Version 1.0** — M59 adds a **new build knob** `WITH (pq_subspaces = M, pq_bits = 4, aq_threshold = T)` to the existing `theodb_hnsw` access method (meta layout **v3**, coexisting with v1 f32 + v2 SBQ), backed by (1) a new domain file `aq.rs` that trains an **anisotropic (score-aware) product quantizer** — 4-bit subquantizers, 16 centroids each — mirroring `sbq.rs`, and (2) an **Asymmetric-Hashing LUT16 SIMD kernel** in `vec.rs` (`_mm256_shuffle_epi8` / `pshufb`, scalar fallback, same `is_x86_feature_detected!` dispatch the M58 kernels use). The scan `traverse` gains a branch that scores the walk with the near-free AH LUT then reranks survivors by exact f32. It matters because the ~25× QPS gap vs ScaNN at recall 0.99 (M33) is the North-Star P0 lever, and both prior narrower bets were **measured** dead ends (M36: f32 distance is only ~15% of scan cost; ADR-0018: scalar SBQ is 0.35–0.77× f32 QPS, a LOSS). The expected outcome is a **benchmark-gated** merge: it merges only if it beats the f32 baseline on QPS-at-recall-0.99 (PRD D3, anti-sunk-cost); otherwise an honest-negative ADR records the result and the next seed.

## Goal

> "Enable `theodb_hnsw` index owners to build an anisotropic-PQ + AH-SIMD index (`WITH (pq_subspaces=M, pq_bits=4, aq_threshold=T)`) so that top-k ANN recall is preserved while per-candidate scoring collapses to in-register LUT lookups, measured by the SIFT1M end-to-end harness reporting **recall@10 ≥ 0.99** with QPS recorded head-to-head vs SBQ (M57) and ScaNN (M33)."

## Context

The North Star (P0 GOTO, `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md`) demands **measured** vector superiority. The gap is quantified: at recall@10 ≥ 0.99 on SIFT1M, ScaNN does **1920 QPS** vs theodb ~78 QPS — a **~25× throughput gap** where recall is already at parity (`docs/benchmarks/m33-scann-headtohead.md:22,27`, cited in the M59 blueprint `Context`).

Two prior measurements narrow the axis to exactly one lever. M36 falsified "distance is the bottleneck" (full-precision f32 distance is ~14–15% of scan cost; candidate-count + sort dominate). M57/ADR-0018 falsified "scalar bit-quantization is superior" — SBQ inline is recall-neutral but **0.35–0.77× the f32 QPS** in every regime, a constant-factor LOSS; the ADR explicitly reframes M59 as "quantização anisotrópica + Asymmetric Hashing SIMD, não bit-quantization escalar" (`docs/adr/0018-m57-sbq-inline-not-superior.md:45-46`, per blueprint `Context`).

So M59 targets the two ScaNN mechanisms SBQ lacks: **(A)** a score-aware anisotropic codebook that preserves top-k ranking per byte, and **(B)** an AH LUT kernel vectorized with `pshufb` so per-candidate scoring is a handful of table lookups instead of an f32 dot. The M35/M51 page-native HNSW layout was **designed** for exactly this drop-in: a trailing per-node code, a versioned codebook in the meta, and a walk-scoring branch that already separates cheap-code-distance from f32-rerank (blueprint ADR-1). This plan is the algorithm axis only — plumbing and a new AM are explicitly out of scope.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `theodb_rs/src/am/aq.rs` (NEW) | 0 | — | (file to be created — the anisotropic PQ quantizer, mirroring `sbq.rs`) | — |
| `theodb_rs/src/sbq.rs` | 374 | `2376077` (2026-07-07) | The M22/M51 SBQ quantizer: `train`/`quantize`/`to_meta_bytes`/`from_meta_bytes`, `hamming_bytes`, 8 `#[pg_test]`. The pattern `aq.rs` mirrors. | UNCHANGED — `aq.rs` is a sibling, never a modification of `sbq.rs`. The v2 SBQ format stays byte-identical. |
| `theodb_rs/src/vec.rs` | 515 | `dc5f561` (2026-07-07) | Distance kernels + the AVX2+FMA SIMD dispatch (`simd_x86` mod, `l2_sq`/`cosine_terms`/`dot`, `available()`, `force_for_test`). | The M20 SQL-callable ops (`l2_distance`/`inner_product`/`cosine_distance`) stay byte-parity with pgvector; the M58 kernels stay callable; the `simd_x86` dispatch pattern is reused verbatim (new kernel, no refactor of existing ones). |
| `theodb_rs/src/am/hnsw_page.rs` | 2006 | `a3a4747` (2026-07-07) | M35 page-native layout: `elem_size(dim, code_len)` (`:60`), `decode_element` exposing `code_bytes` (`:234`), meta v1/v2 codec (`encode_meta`/`decode_meta` `:118,145`), `load` scoring branch (`:842`), `traverse` (`:907`) with the SBQ `qcode` path (`:927,981`), `pack_at(idx, base, sbq_bits)` (`:363`). | v1 (f32) + v2 (SBQ) meta versions stay readable; `elem_size`/`pack_at` analytic addresses stay correct; the tombstone bytes (`E_DELETED`/`E_VERSION` `:39,40`) stay untouched; a v3 index adds a branch, never rewrites the v1/v2 paths. |
| `theodb_rs/src/am/options.rs` | 131 | `8676aac` (2026-07-06) | The reloption surface: `TheodbIvfflatOptions` `#[repr(C)]` struct (`:30`) carrying `lists`+`sbq_bits`; `init()` `add_int_reloption` (`:42`); `amoptions` parse table (`:68`); `sbq_bits_from_relation` (`:120`). | The existing `lists`/`sbq_bits` options + their `_from_relation` resolvers stay byte-identical; new options are ADDED to the same shared struct (each AM reads only what it uses). |
| `theodb_rs/src/am/guc.rs` | 243 | `a3a4747` (2026-07-07) | Scan-time GUCs incl. `theodb_hnsw.over_fetch` (`:33`, `over_fetch()` `:235`); `init()` registers them (`:133`). | Existing GUCs unchanged; a new `aq_rerank` / reuse of `over_fetch` is additive. |
| `theodb_rs/src/am/build.rs` | 443 | `8dc2c7d` (2026-07-08) | `ambuild_hnsw` reads `sbq_bits_from_relation` and calls `pack_sbq(&idx, sbq_bits)` (`:117-118`); the VACUUM fold `vacuum_rebuild_hnsw_structured` re-packs preserving `meta.sbq_bits` via `pack_at(&idx, base, meta.sbq_bits)` (`:319,329`). | The f32/SBQ build + fold paths stay byte-identical; the AQ build/fold is a new branch that preserves the same crash-safe fold contract (`pack_at` position-independence). |
| `theodb_rs/src/ann/scan_core.rs` | 337 | `cabb437` (2026-07-07) | The `NeighborSource` DIP seam (`:24`) + `ground_search`/`ground_search_nodes` (`:83,101`); the D3 oracle `ground_search_matches_brute_exact_knn` (`:272`). | The seam does NOT change (blueprint ADR-1): the AH kernel is a new `dist`-equivalent inside `load`, not a new trait method. The recall-neutral dedup-before-load contract holds. |
| `theodb_rs/src/lib.rs` | ~40 (mod list) | `2376077` (2026-07-07) | Declares `mod sbq;` (`:35`), `mod am;`, `mod vec;`. | Add `mod aq;` (or `am::aq`) alongside; no other module reordered. |
| `benchmarks/theodb_bench/` (bench harness) | (existing) | — | Real-SIFT1M recall×QPS harness (M33/M45/M57 comparator; `benchmarks/run_m33_scann.py` re-runs ScaNN on the same seeded subsample). | Bench is validation, not shipped code — it exercises the built index, never links into the crate. |
| `theodb_rs/src/am/mod.rs` (opclass SQL) | (existing) | — | `theodb_hnsw` amhandler + opclass `extension_sql` (`:55-65`). | If a distinct opclass is chosen it is ADDED; the default L2 opclass stays; existing indexes are byte-identical. |
| `CHANGELOG.md` | (existing) | — | The public change contract (Unbreakable Rule 6). | `[Unreleased]` gets one `Added` entry for the AQ+AH reloption. |
| `docs/benchmarks/m59-anisotropic-ah.md` (NEW) | 0 | — | (file to be created — the D3-gate benchmark artifact + verdict) | — |
| `docs/benchmarks/m59-anisotropic-ah.json` (NEW) | 0 | — | (file to be created — raw ≥3-run mean±std numbers) | — |
| `docs/adr/0019-m59-anisotropic-ah-outcome.md` (NEW) | 0 | — | (file to be created — records the go/partial/honest-negative verdict + next seed) | — |

Every file listed in any `#### Files to edit` block below appears in this table. `(NEW)` rows are expected.

### Current callers / dependents

- **Symbol:** `pack_sbq(idx, sbq_bits)` / `pack_at(idx, base, sbq_bits)` in `theodb_rs/src/am/hnsw_page.rs:354,363`
  - **Callers (production):** `theodb_rs/src/am/build.rs:118` (`ambuild_hnsw`), `theodb_rs/src/am/build.rs:319,329` (`vacuum_rebuild_hnsw_structured`), `theodb_rs/src/am/hnsw_page.rs:348` (`pack`).
  - **Callers (tests):** `theodb_rs/src/am/hnsw_page.rs:1226` (`pack_sbq_writes_codebook_and_matching_codes`).
  - Resolution: M59 adds an AQ-aware pack path (either an `AqParams` argument or a sibling `pack_aq`) so v3 is packed; the v1/v2 signatures stay callable. `grep -rn 'pack_sbq\|pack_at' theodb_rs/src` was used to enumerate.
- **Symbol:** `decode_element` → `ElementView.code_bytes` in `theodb_rs/src/am/hnsw_page.rs:215,234`
  - **Callers (production):** `load` (`:836`), the rerank loop (`:993`), the SBQ-scan tests. code_bytes already carries the trailing per-node code for ANY code kind — v3 reuses it verbatim.
  - **External (public API):** no — all `pub(crate)`.
- **Symbol:** `over_fetch()` in `theodb_rs/src/am/guc.rs:235`
  - **Callers (production):** `theodb_rs/src/am/hnsw_page.rs:986` (SBQ rerank widening). v3 reuses the same GUC for the AH rerank pool.
- **Symbol:** `sbq_bits_from_relation` in `theodb_rs/src/am/options.rs:120`
  - **Callers (production):** `theodb_rs/src/am/build.rs:117`. M59 adds sibling resolvers `pq_subspaces_from_relation` / `pq_bits_from_relation` / `aq_threshold_from_relation` in the same file; existing callers untouched.

### Domain glossary

- **AQ (anisotropic quantization)** — a score-aware product quantizer whose k-means loss penalizes the residual component **parallel** to the datapoint direction harder than the orthogonal one (weight ratio `η > 1`), preserving inner-product ranking (blueprint T1).
- **AH (Asymmetric Hashing)** — ScaNN's name for asymmetric-distance-computation (ADC): query stays f32, corpus is quantized to codes, per-query one **LUT** `[m × k*]` is precomputed and a corpus code scored as `Σ LUT[i][code_i]` — no per-candidate multiply, no decode (blueprint T2).
- **LUT16 / pshufb** — with 4-bit subquantizers (`k*=16`) each subspace's 16 LUT entries fit a 16-byte lane; `_mm256_shuffle_epi8` (`pshufb`) does 16/32 parallel table lookups in one instruction (blueprint T2).
- **η / aq_threshold (T)** — the anisotropic parallel/orthogonal weight ratio; `η = 1` recovers isotropic PQ exactly (the unit-test hook); larger T ⇒ larger parallel penalty (blueprint T1, `[RESOLVED]` question).
- **m (pq_subspaces)** — number of subquantizers; the vector is split into `m` subspaces of `dim/m` dims each, one 4-bit code per subspace ⇒ `m/2` bytes/vector.
- **Meta layout v3** — the third `theodb_hnsw` meta format (v1 f32, v2 SBQ, v3 AQ); a versioned trailer holds the AQ codebook, coexisting with v1/v2 (blueprint ADR-1).
- **Rerank** — after the cheap AH walk selects survivors, re-score them by exact f32 (the inline vector stays in the tuple) to recover recall — the ADR-0018 recall-recovery pattern.
- **Wiring triad** — caller + integration test + runtime metric; here the runtime metric is the existing `THEODB_SCAN_PROFILE=1` `pages_read`/`score` log (`hnsw_page.rs:1009`).

### Architecture boundaries affected

Per `rules/architecture.md § 1` (interface → application → domain → infrastructure):

- **Domain (no I/O):** `aq.rs` (the quantizer + anisotropic k-means) and the `vec.rs` AH kernel are pure domain — no `pg_sys`, testable in isolation (mirrors why `scan_core.rs` forbids `pg_sys`, `:8`). This respects DIP: the domain declares the math; infrastructure (pages) consumes it.
- **Infrastructure (persistence):** `hnsw_page.rs` v3 meta codec + element `code_bytes` and `build.rs` pack/fold are the adapters that persist the domain codebook. The `NeighborSource` seam (`scan_core.rs`) is the DIP boundary the scan crosses **without change** — the AH kernel plugs into `load`, not the trait.
- **Interface (DDL/GUC):** `options.rs` reloption + `guc.rs` scan knob are the interface surface. Boundary crossing direction: interface → infrastructure → domain (build reads the reloption, trains the domain quantizer, persists via infra). No inner layer imports an outer one.

## Prior Art & Related Work

- **Internal blueprint** — `.claude/knowledge-base/discoveries/blueprints/m59-anisotropic-ah-blueprint.md` (this plan's source): ADR-1 (new reloption on M35 AM, recommended, §"ADR-1"), ADR-2 (hand-rolled anisotropic k-means, §"ADR-2"), ADR-3 (4-bit LUT16, §"ADR-3"), T1 (anisotropic loss math, §"Coverage Corner 4 — Techniques"), T2 (AH LUT16 pshufb, same section).
- **Internal blueprint** — `.claude/knowledge-base/discoveries/blueprints/m39-pq-product-quantization-blueprint.md` (the deepened antecedent — the ADC skeleton + the `BLOCKED partial` η weight this plan resolves), cited by the M59 blueprint `Supersedes/deepens`.
- **Internal ADR** — `docs/adr/0018-m57-sbq-inline-not-superior.md` (the reframing decision that made M59 the real algorithmic axis), cited by the M59 blueprint `Direct antecedent`.
- **Internal benchmark** — `docs/benchmarks/m33-scann-headtohead.md` (the measured ~25× gap + the ScaNN comparator harness), cited by the M59 blueprint `Related`.
- **In-repo pattern to mirror** — `theodb_rs/src/sbq.rs` (the quantizer shape: train/quantize/meta round-trip + 8 pg_tests) and `theodb_rs/src/vec.rs:95-226` (the `simd_x86` AVX2 dispatch the AH kernel copies).
- **External literature** — Guo et al. 2020, "Accelerating Large-Scale Inference with Anisotropic Vector Quantization", PMLR 119:3887-3896 (arXiv:1908.10396) — the anisotropic loss + η definition (blueprint T1). André et al. 2015, "Cache locality is not enough: High-performance nearest neighbor search with product quantization fast scan", VLDB — the original `pshufb` 4-bit PQ scan (blueprint T2). Douze et al. 2024, "The Faiss library" (arXiv:2401.08281) — the fastscan §, the two-tier oracle test pattern (blueprint T2, Coverage Corner 1 Project B).
- **Rules cited** — `rules/architecture.md § 1` (domain/infra separation for `aq.rs`/`vec.rs`), `rules/parsimony-ladder.md` rung 4 (reuse the installed SIMD dispatch + the M35 seam, add zero heavy dependency), `rules/testing.md § 4.1` (edge vs negative cases in the TDD sections).

## Objective

- [ ] Sub-goal 1 — `aq.rs`: anisotropic Lloyd k-means (η-weighted), 4-bit encode, codebook meta round-trip, deterministic train (mirrors `sbq.rs`, ADR-2/ADR-3).
- [ ] Sub-goal 2 — `vec.rs`: AH LUT16 kernel (`_mm256_shuffle_epi8` accumulate, scalar fallback, `is_x86_feature_detected!` dispatch) matching the scalar LUT within the requant epsilon (T2/ADR-3), + criterion micro-bench.
- [ ] Sub-goal 3 — meta layout v3 persistence: AQ codebook in a versioned meta trailer + `m/2`-byte codes inline in the element tuple, round-tripping through `pack_at`/`decode_meta`, backward-compatible with v1/v2.
- [ ] Sub-goal 4 — scan wiring: `traverse` branch precomputes the query LUT once, scores the walk by AH, reranks survivors by exact f32; reloption + GUC exposed.
- [ ] Sub-goal 5 — Integration Validation + D3-gated benchmark: recall@10 ≥ 0.99 preserved end-to-end on real SIFT1M; QPS recorded vs SBQ (M57) and ScaNN (M33) → `docs/benchmarks/m59-anisotropic-ah.{md,json}` + outcome ADR.

## ADRs

### D1 — Add AQ+AH as a NEW reloption on the existing `theodb_hnsw` AM (meta layout v3), never a new AM or an SBQ replacement

- **Decision.** A build reloption `WITH (pq_subspaces=M, pq_bits=4, aq_threshold=T)` on `theodb_hnsw`, landing as meta **v3** coexisting with v1 (f32) and v2 (SBQ), reusing `elem_size(dim, code_len)`'s trailing-code slot and the versioned meta trailer.
- **Rationale.** The M35/M51 layout already isolates every hook: `code_bytes` (`hnsw_page.rs:234`), the versioned meta codec (`:118-159`), the walk/rerank split (`traverse:981-1002`), and the `NeighborSource` seam that does NOT need to change (`scan_core.rs`). The reloption precedent is exact (`options.rs:30-33` shares a `#[repr(C)]` struct across both AMs, each reading only its option). This is essential complexity only — one new domain file + one kernel + one scan branch (CLAUDE.md § Esforço ≠ Complexidade; `rules/parsimony-ladder.md` rung 4).
- **Alternatives considered.** (a) **Replace SBQ-inline with AQ** — REJECTED: destroys the v2 format ADR-0018 explicitly kept as an experiment base, forces REINDEX of every SBQ index, and forecloses the honest 3-way benchmark (measurement-first + anti-sunk-cost, CLAUDE.md). (b) **A brand-new `theodb_scann` access method** — REJECTED (YAGNI/KISS, `rules/parsimony-ladder.md` rung 1): a new AM means new build/scan/cost/WAL callbacks to host a distance kernel that fits the existing seam; M36/M57 evidence says the lever is the scorer+codebook, not the index structure.
- **Consequences.** Enables a same-AM 3-way benchmark (f32 vs SBQ vs AQ). Constrains: v3 must keep v1/v2 readable (the `decode_meta` version switch); a v3 fold must re-train the codebook (like the SBQ fold, `build.rs:319`).

### D2 — Anisotropic k-means hand-rolled, std-only, deterministic (zero new dependency)

- **Decision.** Hand-roll the anisotropic Lloyd's k-means in `aq.rs`, reusing `crate::vec` (distance) + `crate::ann::Rng` (seed), fixed seed for the deterministic-train test. 4-bit subquantizers, 16 centroids/subspace.
- **Rationale.** pgvectorscale trains its quantizer dep-free; the AGPL `k_means` crate is barred by PRD D1; the anisotropic centroid update is a ~60–100-LoC closed-form weighted mean, not worth an `ndarray`/`linfa` transitive tree (`rules/parsimony-ladder.md` rung 4). The `η = 1` isotropic reduction is the built-in correctness hook (blueprint ADR-2, T1 `[RESOLVED]`).
- **Alternatives considered.** (a) **Vendor a clustering crate** — REJECTED: adds a transitive dependency surface for ~60 LoC of essential math, and the only score-aware option (vectorchord's `k_means`) is AGPL (barred, PRD D1). (b) **Isotropic PQ only (skip the anisotropic reweighting)** — REJECTED: isotropic PQ is direction-agnostic and does not preserve top-k inner-product ranking as well at the same bit budget (blueprint T1: OPQ reduces isotropic error but stays direction-agnostic) — it would leave the core mechanism ScaNN uses on the table.
- **Consequences.** Enables deterministic, reproducible training with a tunable `η`. Constrains: k-means quality depends on init + iterations; the f32 rerank of survivors is the recall safety net for a coarse 16-centroid codebook.

### D3 — 4-bit subquantizers (`k*=16`) so the AH kernel is the LUT16 `pshufb` path

- **Decision.** `pq_bits = 4` (16 centroids/subspace) so each subspace LUT fits a 16-byte lane and the AH kernel is `_mm256_shuffle_epi8`, matching ScaNN/FAISS fastscan; the LUT is requantized to `int8` per query.
- **Rationale.** 8-bit PQ (`k*=256`) forces scattered float gathers — the reason plain ADC is not fast; 4-bit is the SOTA sweet spot that unlocks the in-register lookup (blueprint ADR-3; T2). `m/2` bytes/vector (half the SBQ 1-bit budget at `m=dim/8`) also cuts the `reads` phase M36 measured as ~44–51% of scan cost.
- **Alternatives considered.** (a) **8-bit PQ (k\*=256)** — REJECTED: no `pshufb` in-register lookup (256 > 16-byte lane), so scoring stays scattered-gather bound — defeats the whole AH throughput mechanism (blueprint ADR-3). (b) **No LUT requantization (f32 LUT)** — REJECTED: an f32 LUT cannot feed `pshufb` (byte-lane instruction); the int8 requant is what makes the SIMD path possible — its bounded tolerance is asserted (the FAISS test epsilon), not ignored (blueprint ADR-3, T2).
- **Consequences.** Enables billion-lookups/sec scoring. Constrains: a coarser 16-centroid codebook (recovered by the anisotropic loss + f32 rerank) and a bounded int8-requant tolerance that the kernel-correctness test asserts.

### D4 — Wire AQ into HNSW first; keep an IVF batch-scan variant as a measured fallback, not a speculative build (Unresolved Q1)

- **Decision.** Build the kernel + codebook once and wire it into the HNSW `traverse` walk first (smallest diff, reuses the M51 seam). Only if the HNSW per-node walk under-feeds `pshufb` (measured) do we add an IVF contiguous batch-scan variant — never a new AM.
- **Rationale.** The AH kernel is layout-sensitive: HNSW pointer-chasing may not feed `pshufb` as efficiently as a contiguous IVF list (blueprint ADR-1 a-lite / Risks R2 / Unresolved Q1). Resolving this by speculation would violate YAGNI; resolving it by measurement is the measurement-first mandate.
- **Alternatives considered.** (a) **Build the IVF batch-scan variant up front** — REJECTED (YAGNI, `rules/parsimony-ladder.md` rung 1): it is speculation until the HNSW measurement shows under-feeding; the contiguous-list variant is a real follow-up seed, not a plan-time bet. (b) **Only ever HNSW, never IVF** — REJECTED: forecloses the documented fallback if HNSW under-feeds the kernel; kept as a measured branch, not built now.
- **Consequences.** Enables the smallest first diff + an honest measured decision. Constrains: the recall×QPS harness must include the `THEODB_SCAN_PROFILE=1` `score`/`reads` split so under-feeding is observable.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| The AH kernel may not close a material fraction of the 25× on HNSW's pointer-chasing walk (under-feeds `pshufb`) — the plan could land correct-but-not-superior. | High | D3-gate is honest: if it beats SBQ but not f32-at-0.99, record a claim-bounded partial (not "superiority"); IVF batch-scan (D4) is the measured next lever. `THEODB_SCAN_PROFILE=1` confirms whether `score` actually collapsed. | Vector council |
| `unsafe` AVX2 intrinsics (`_mm256_shuffle_epi8` + int8 accumulate) risk OOB / overflow across the C boundary if the LUT/code lengths disagree. | High | Enforce the length invariant ALWAYS (not debug-only) at the dispatch boundary, exactly as `l2_dist_from_bytes` does (`vec.rs:258`); widen int8→int16 accumulate to avoid overflow (blueprint T2); scalar fallback is the correctness oracle; `is_x86_feature_detected!` gate (`vec.rs:121`). | Perf/SIMD council |
| int8 LUT requantization introduces a scoring tolerance that could silently drop recall. | Medium | Assert AH-SIMD == scalar-LUT within the requant epsilon (kernel-correctness test, FAISS two-tier oracle) AND assert AQ-ADC correlates with f32 order (quantizer-validity test); the f32 rerank of survivors recovers final recall. | Vector council |
| Meta v3 + a coarse 16-centroid codebook could regress recall vs the 0.99 gate. | Medium | The exact-f32 rerank keeps the inline vector (ADR-0018 recovery pattern, `traverse:993`); over_fetch GUC widens the rerank pool; end-to-end `ground_search == brute-kNN` at high ef is the recall gate (extends `scan_core.rs:272`). | Vector council |
| A v3 fold (VACUUM compaction) must re-train the codebook or corrupt the index (as SBQ does). | Medium | Mirror the SBQ fold: `vacuum_rebuild_hnsw_structured` re-packs preserving the code kind (`build.rs:319,329`); a round-trip-through-fold test asserts a v3 index survives a compaction. | Index/storage council |
| The build existing HNSW recall ceiling (~0.96–0.974 at 100k–500k, `build.rs:15-21`) may cap the AQ index below 0.99 regardless of the quantizer. | Medium | This is a scan/graph issue orthogonal to AQ (the `build.rs` note flags a suspected scan bug for M60); the benchmark measures AQ vs the SAME f32 baseline, so the delta is quantizer-attributable even if the absolute ceiling is graph-limited. Record honestly. | Vector council |

## Unresolved Questions

- Q1 — Does HNSW's per-node pointer-chasing walk feed `pshufb` as efficiently as a contiguous IVF-list sweep, or does it under-feed the SIMD kernel (blueprint Unresolved Q1)? Resolved by measurement in Phase 5; D4 (IVF batch-scan) is the fallback.
- Q2 — What `aq_threshold` (T ⇒ η) default best trades recall vs speed on SIFT1M? The blueprint resolves that `η=1` ⇔ isotropic and T is a tunable knob, but the per-dataset default is a Phase-5 sweep, not a fixed constant.
- Q3 — Does the existing graph recall ceiling (`build.rs:15-21`, suspected M60 scan bug) prevent the AQ index from reaching 0.99 absolute even when the quantizer is faithful? If so, the plan records recall-at-parity-with-the-f32-baseline rather than absolute 0.99, and flags the graph ceiling as the blocking dependency.
- Q4 — Is a distinct SQL opclass required, or does the default `theodb_hnsw` L2 opclass + reloption suffice (as SBQ needed no new opclass)? Resolved at implement time by whether the codebook needs an opclass-bound support proc; default assumption is reloption-only (like SBQ).

## Dependency Graph

```
Phase 1 (aq.rs codebook) ──▶ Phase 2 (vec.rs AH kernel) ──▶ Phase 3 (v3 persistence) ──▶ Phase 4 (scan wiring)
        │                            │                                                         │
        │ (domain, no I/O)           │ (domain, no I/O)                                        ▼
        └────────────┬───────────────┘                                              Phase 5 (Integration + benchmark)
                     ▼
      Phase 1 and Phase 2 are independently testable (pure domain) and MAY be
      developed in parallel; both are prerequisites for Phase 3.
      Phase 3 (persistence) blocks Phase 4 (scan reads v3). Phase 4 blocks Phase 5.
```

Sequential blockers: 3→4→5. Parallelizable: 1 ∥ 2 (both pure domain, no shared file).

---

## Phase 1: Anisotropic PQ codebook (`aq.rs`)

**Objective:** a std-only, deterministic anisotropic product quantizer — train, 4-bit encode, codebook meta round-trip — mirroring `sbq.rs`, with the `η=1` isotropic reduction as the correctness hook.

### T1.1 — `AqQuantizer::train` (anisotropic Lloyd k-means, η-weighted)

#### Objective
Train `m` subspace codebooks (16 centroids each) minimizing the score-aware anisotropic loss `L = h∥·‖r∥‖² + h⊥·‖r⊥‖²`, deterministic under a fixed seed.

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — creates `theodb_rs/src/am/aq.rs` with `AqQuantizer { m, k, centroids, aq_threshold }` and `train(corpus, m, bits, aq_threshold, seed)` running Lloyd's k-means per subspace under the anisotropic residual reweighting (blueprint T1 math).
2. **Why it is necessary now** — the codebook is the root dependency (Phases 2–4 all consume it); it is the score-aware mechanism SBQ lacks (D2). It must exist and be deterministic before any encode/scan can be built. Motivated by Baseline row `aq.rs (NEW)` + ADR D2.

#### Evidence
`sbq.rs:30-60` (`SbqQuantizer::train` — the deterministic-train shape to mirror); blueprint T1 §"The math" (`L = h∥·‖r∥‖² + h⊥·‖r⊥‖²`, `η = h∥/h⊥ > 1`, `η=1` ⇔ isotropic); ADR D2.

#### Files to edit
```
theodb_rs/src/am/aq.rs — NEW: AqQuantizer struct + train() (anisotropic k-means)
theodb_rs/src/lib.rs — add `mod aq;` (or `am::aq`) next to `mod sbq;` (:35)
theodb_rs/src/am/aq.rs — RED pg_tests: aq_train_deterministic, aq_eta_one_reduces_to_isotropic
```

#### Deep file dependency analysis
- `aq.rs` (new) — depends on `crate::vec` (residual norms) + `crate::ann::Rng` (seed), exactly as `sbq.rs` uses `crate::ann`. No downstream file depends on it yet (Phase 2+ will).
- `lib.rs` — adds one `mod` line; no caller of `sbq` is affected.

#### Deep Dives
- Data structures: `AqQuantizer { m: usize, bits: u8, aq_threshold: f32, sub_dim: usize, centroids: Vec<Vec<f32>> }` — `m × k*` centroids of `dim/m` dims each (`k* = 1<<bits = 16` at bits=4).
- Algorithm (per subspace): standard Lloyd (assign→update) but the **update** uses the anisotropic weighted mean: decompose each residual `r = x − q(x)` into `r∥ = (r·x̂)x̂`, `r⊥ = r − r∥` (`x̂ = x/‖x‖`), and weight parallel error by `η`. `η = 1` ⇒ ordinary mean (isotropic) — the reduction hook.
- Invariants (from Baseline): deterministic under a fixed seed (`aq.rs (NEW)` mirrors `sbq_deterministic_train`); no `pg_sys` reference (domain-layer invariant, `architecture.md § 1`).
- Edge cases: empty corpus → zero centroids (no panic); fewer distinct points than `k*` → duplicate/empty centroids handled (no div-by-zero on an empty cluster).

#### Pseudo-code / Signatures
```pseudocode
struct AqQuantizer { m, bits, aq_threshold, sub_dim, centroids: Vec<Vec<f32>> }  // m*k* centroids

fn train(corpus: &[Vec<f32>], m, bits, aq_threshold, seed) -> AqQuantizer
  k = 1 << bits                          // 16 at bits=4
  sub_dim = dim / m
  for s in 0..m:                         // per subspace
    pts = corpus.map(|v| v[s*sub_dim .. (s+1)*sub_dim])
    cents = kmeans_pp_init(pts, k, Rng(seed ^ s))
    repeat ITERS:
      assign each pt to nearest cent
      for each cluster: cent = anisotropic_weighted_mean(pts, aq_threshold)  // η=1 ⇒ plain mean
    store cents
# Example (η=1, one subspace, k=2): points {[0],[2]} → centroids ≈ {[0],[2]} (isotropic mean)
```

#### Tasks
1. Create `aq.rs` with the struct + `train` signature.
2. Implement per-subspace Lloyd with `kmeans++` init (seeded `Rng`).
3. Implement `anisotropic_weighted_mean` with the parallel/orthogonal split; `η=1` path = plain mean.
4. Add `mod aq;` in `lib.rs`.

#### TDD
```
RED:     aq_train_deterministic() — two trains on the same corpus+seed produce identical centroids (mirror sbq_deterministic_train, sbq.rs:331). MUST fail before train exists.
RED:     aq_eta_one_reduces_to_isotropic() — with aq_threshold set so η=1, centroids equal a plain-PQ k-means run within eps (the isotropic reduction hook, blueprint T1).
RED:     aq_train_empty_corpus_no_panic() — empty corpus → empty/zero codebook, no panic (edge case).
GREEN:   Implement AqQuantizer::train.
REFACTOR: Extract the per-subspace slicing helper if it clarifies; else "None expected".
VERIFY:  cargo pgrx test --package theodb_rs (aq_* tests)
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `aq_train_deterministic` + `aq_eta_one_reduces_to_isotropic` + `aq_train_empty_corpus_no_panic` green.
- [ ] `aq.rs` references no `pg_sys` (domain-layer invariant, `architecture.md § 1`).
- [ ] Complexity — each function in `aq.rs` has cyclomatic complexity ≤ 10, measured by `cargo clippy -p theodb_rs -- -W clippy::cognitive_complexity` reporting zero `cognitive_complexity` warnings on `aq.rs`.
- [ ] Coverage — the `aq_*` tests cover ≥ 90% of `aq.rs` lines (critical paths `train`/`encode` 100%), measured by `cargo pgrx test --package theodb_rs` passing with the `aq_*` subset green (coverage inspected via the test run's assertions over train/encode/serialize).
- [ ] Lint — `cargo clippy --package theodb_rs -- -D warnings` exits 0 with zero warnings on `aq.rs`.
- [ ] Size — `wc -l theodb_rs/src/am/aq.rs` reports ≤ 500 (per `rules/architecture.md`).

#### DoD (Definition of Done)
- [ ] All tasks completed and validated.
- [ ] `cargo pgrx test --package theodb_rs aq_train_deterministic aq_eta_one_reduces_to_isotropic aq_train_empty_corpus_no_panic` exits 0.
- [ ] `cargo clippy --package theodb_rs -- -D warnings` exits 0.
- [ ] `wc -l theodb_rs/src/am/aq.rs` reports ≤ 500 (per `rules/architecture.md`).

### T1.2 — `AqQuantizer::encode` (4-bit codes) + codebook meta round-trip

#### Objective
Encode a vector into an `m/2`-byte 4-bit PQ code and serialize/deserialize the codebook to LE bytes for the v3 meta page (typed `Err` on truncation, never a panic).

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — adds `encode(v) -> Vec<u8>` (nearest-centroid per subspace, two 4-bit codes packed per byte), `bytes_per_vector(dim, m)`, `to_meta_bytes`/`from_meta_bytes` (mirroring `sbq.rs:99-146`), and `dim()`.
2. **Why it is necessary now** — the persistence phase (Phase 3) needs the code bytes + a round-tripping codebook; the scan (Phase 4) needs `encode(query-subspace)`-free LUT building but the corpus codes come from here. Motivated by Baseline row `sbq.rs` (the pattern) + ADR D3.

#### Evidence
`sbq.rs:63-146` (`quantize`, `bytes_per_vector`, `to_meta_bytes`, `from_meta_bytes`, the truncation `Err` at `:118`); `sbq_codebook_roundtrips_through_meta_bytes` (`sbq.rs:310`); `sbq_codebook_from_bytes_rejects_truncated` (`:323`); ADR D3 (4-bit, `m/2` bytes).

#### Files to edit
```
theodb_rs/src/am/aq.rs — add encode(), bytes_per_vector(), to_meta_bytes(), from_meta_bytes(), dim()
theodb_rs/src/am/aq.rs — RED pg_tests: aq_codebook_roundtrips_through_meta_bytes, aq_codebook_from_bytes_rejects_truncated, aq_bytes_per_vector_formula, aq_adc_correlates_with_f32_distance
```

#### Deep file dependency analysis
- `aq.rs` — extends the struct from T1.1 with encode/serialize; consumed by Phase 3 (`hnsw_page.rs` v3 pack) and Phase 4 (scan). No caller yet outside its own tests.

#### Deep Dives
- Data structures: code = `m` 4-bit indices packed two-per-byte → `⌈m/2⌉` bytes; meta bytes = `[bits:u8][m:u32][sub_dim:u32][aq_threshold:f32][centroids: m*k*·sub_dim f32 LE]`.
- Algorithm: `encode` = per subspace, argmin over the 16 centroids (reuse `crate::vec` L2 on the sub-slice); pack.
- Invariants: `from_meta_bytes(to_meta_bytes()) == identity` (round-trip, mirrors `sbq.rs:310`); exact-length validation before decode (`Err` not panic — Rule 8, `sbq.rs:118`).
- Edge cases (edge vs negative, `rules/testing.md § 4.1`): edge — `m` odd ⇒ the last byte holds one 4-bit code (high nibble zero); negative — truncated meta bytes ⇒ typed `Err`.

#### Pseudo-code / Signatures
```pseudocode
fn encode(&self, v: &[f32]) -> Vec<u8>
  code = vec![0u8; ceil(m/2)]
  for s in 0..m:
    idx = argmin_c ‖ v[sub s] - centroid[s][c] ‖²        // 0..16
    if s even: code[s/2] |= idx           else: code[s/2] |= idx << 4
  code
fn to_meta_bytes(&self) -> Vec<u8>   // [bits][m][sub_dim][aq_threshold][centroids…]
fn from_meta_bytes(b) -> Result<Self,String>   // exact-len check → typed Err (sbq.rs:118 pattern)
```

#### Tasks
1. Implement `encode` (argmin + nibble packing).
2. Implement `bytes_per_vector` + `dim`.
3. Implement `to_meta_bytes` / `from_meta_bytes` with exact-length validation.

#### TDD
```
RED:     aq_bytes_per_vector_formula() — bytes = ceil(m/2); asserted for a few (dim,m) (mirror sbq_bytes_per_vector_formula, sbq.rs:257).
RED:     aq_codebook_roundtrips_through_meta_bytes() — to_meta_bytes → from_meta_bytes yields byte-identical centroids AND identical encode() (mirror sbq.rs:310).
RED:     aq_codebook_from_bytes_rejects_truncated() — a byte short ⇒ Err, never panic (negative case, sbq.rs:323).
RED:     aq_adc_correlates_with_f32_distance() — QUANTIZER-VALIDITY oracle: the f32-near half of a corpus has LOWER mean AH-ADC score than the f32-far half (analog of sbq_hamming_correlates_with_f32_distance, sbq.rs:280).
GREEN:   Implement encode + serialization.
REFACTOR: "None expected" (or extract the nibble packer).
VERIFY:  cargo pgrx test --package theodb_rs (aq_* tests)
```

#### Concurrency tests
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] The 4 RED tests pass: `cargo pgrx test --package theodb_rs aq_bytes_per_vector_formula aq_codebook_roundtrips_through_meta_bytes aq_codebook_from_bytes_rejects_truncated aq_adc_correlates_with_f32_distance` exits 0.
- [ ] Round-trip is byte-exact — `aq_codebook_roundtrips_through_meta_bytes` asserts `from_meta_bytes(to_meta_bytes()) == identity` AND identical `encode()`; truncation returns a typed `Err` (asserted by `aq_codebook_from_bytes_rejects_truncated`).
- [ ] Lint — `cargo clippy --package theodb_rs -- -D warnings` exits 0 on `aq.rs`.
- [ ] Size — `wc -l theodb_rs/src/am/aq.rs` reports ≤ 500.

#### DoD
- [ ] `cargo pgrx test --package theodb_rs` exits 0; `cargo clippy --package theodb_rs -- -D warnings` exits 0; every changed file's `wc -l` is ≤ 500.

---

## Phase 2: Asymmetric-Hashing LUT16 SIMD kernel (`vec.rs`)

**Objective:** a per-query LUT16 build + an AVX2 `_mm256_shuffle_epi8` accumulate kernel (scalar fallback, runtime dispatch) that scores a corpus code within the int8-requant epsilon of the scalar LUT, with a criterion micro-bench.

### T2.1 — `ah_lut16` scalar path + LUT builder (the correctness oracle)

#### Objective
Precompute the per-query LUT (`m × 16` int8 partial scores) and score a corpus code by `Σ LUT[i][code_i]` in a scalar loop — the oracle the SIMD path must match.

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — adds to `vec.rs` a `build_lut16(query, &AqQuantizer) -> Lut16` (per subspace, the `dim/m` partial distance from the query sub-vector to each of 16 centroids, requantized to int8) and `ah_score_scalar(&Lut16, code_bytes) -> i32`.
2. **Why it is necessary now** — the scalar path is the correctness oracle for the `unsafe` SIMD path (T2.2); building the oracle first is the RED before the SIMD GREEN. Motivated by Baseline row `vec.rs` + ADR D3 + blueprint T2.

#### Evidence
`vec.rs:275-330` (the scalar-from-bytes kernels that are the fallback pattern); blueprint T2 §"What it is" (`LUT[i][j] = partial_score`, score `= Σ LUT[i][code_i]`); ADR D3 (int8 requant tolerance).

#### Files to edit
```
theodb_rs/src/vec.rs — add Lut16, build_lut16(), ah_score_scalar()
theodb_rs/src/vec.rs — RED pg_tests: ah_lut_score_equals_naive_adc, ah_lut_requant_bounded
```

#### Deep file dependency analysis
- `vec.rs` — adds the AH LUT builder/scorer alongside the M58 kernels; consumed by T2.2 (SIMD) and Phase 4 (`traverse`). The M20 operators + M58 kernels are untouched (Baseline invariant).
- Depends on `AqQuantizer` (Phase 1) to read centroids for the LUT.

#### Deep Dives
- Data structures: `Lut16 { m: usize, tables: Vec<i8> }` (`m·16` int8) + a per-query `scale`/`bias` for the requant (so the accumulated int scores map back to f32 order).
- Algorithm: per subspace, compute the `dim/m`-dim partial distance query→centroid for all 16 centroids (f32), find per-subspace min/max, linearly requantize to int8; store. Score = sum of `m` int8 lookups (widen to i32).
- Invariants (`rules/testing.md § 4.1`): scalar score preserves the ORDER of the naive f32 ADC (the ranking is what matters, not the absolute value); the requant tolerance is bounded and asserted.
- Edge cases: `m` odd (last nibble); a subspace with all-equal partials (min==max ⇒ requant scale guard, no div-by-zero).

#### Pseudo-code / Signatures
```pseudocode
struct Lut16 { m, tables: Vec<i8> }   // m*16
fn build_lut16(query: &[f32], q: &AqQuantizer) -> Lut16
  for s in 0..m:
    parts_f32[c] = ‖ query[sub s] - centroid[s][c] ‖²   for c in 0..16
    (lo,hi) = (min,max) parts_f32
    tables[s*16 + c] = requant_i8(parts_f32[c], lo, hi)
fn ah_score_scalar(lut: &Lut16, code: &[u8]) -> i32
  acc=0; for s in 0..m: idx = nibble(code, s); acc += lut.tables[s*16+idx] as i32
  acc
```

#### Tasks
1. Add `Lut16` + `build_lut16` (per-subspace requant).
2. Add `ah_score_scalar` (nibble unpack + int accumulate).
3. Add a naive f32-ADC reference in the test module (the order oracle).

#### TDD
```
RED:     ah_lut_score_equals_naive_adc() — over a random corpus, ah_score_scalar RANKS codes identically to the naive f32-ADC (Σ partial f32) — order-preserving (the FAISS/ScaNN validity property, blueprint T2). MUST fail before build_lut16 exists.
RED:     ah_lut_requant_bounded() — the int8 requant error per subspace is ≤ the declared epsilon (edge: min==max subspace does not panic).
GREEN:   Implement build_lut16 + ah_score_scalar.
REFACTOR: "None expected".
VERIFY:  cargo pgrx test --package theodb_rs (ah_* tests)
```

#### Concurrency tests
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `cargo pgrx test --package theodb_rs ah_lut_score_equals_naive_adc ah_lut_requant_bounded` exits 0 (order-preservation + bounded requant asserted).
- [ ] Lint — `cargo clippy --package theodb_rs -- -D warnings` exits 0 on the new `vec.rs` functions.
- [ ] Size — `wc -l theodb_rs/src/vec.rs` reports ≤ 500; since it is 515 LoC today, the AH LUT builder+scorer land in a `vec::ah` submodule file (`theodb_rs/src/vec/ah.rs`) so no single file exceeds 500 (verified by `wc -l`).

#### DoD
- [ ] `cargo pgrx test --package theodb_rs` exits 0; `cargo clippy --package theodb_rs -- -D warnings` exits 0; `wc -l theodb_rs/src/vec/ah.rs` ≤ 500.

### T2.2 — `ah_score` AVX2 `pshufb` accumulate + runtime dispatch (SIMD, `unsafe`)

#### Objective
Score corpus codes with `_mm256_shuffle_epi8` (16-parallel LUT lookups) + int8→int16 accumulate, dispatched via `is_x86_feature_detected!` with the scalar fallback, matching the scalar oracle within eps.

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — adds `simd_x86::ah_score` (`#[target_feature(enable = "avx2")]`) and a dispatcher `ah_score(lut, codes) -> ...` reusing the `simd_x86::available()`/`force_for_test` machinery from the M58 kernels.
2. **Why it is necessary now** — this is the throughput mechanism (the near-free `score` phase); it must exist for the scan (Phase 4) and the benchmark (Phase 5). It is built AFTER its scalar oracle (T2.1) so correctness is provable. Motivated by Baseline row `vec.rs` (dispatch pattern `:95-226`) + ADR D3 + Risk "unsafe AVX2".

#### Evidence
`vec.rs:95-160` (the `simd_x86` mod: `available()`, `force_for_test`, `#[target_feature]`, the length-invariant `assert_eq!` at `:258`); blueprint T2 §"The SIMD trick" (`_mm256_shuffle_epi8` = 16 parallel lookups, int8→int16 accumulate, transposed block layout); ADR D3; Risk "unsafe AVX2 intrinsics".

#### Files to edit
```
theodb_rs/src/vec.rs — add simd_x86::ah_score (avx2 pshufb) + the ah_score dispatcher (length-invariant enforced always)
theodb_rs/src/vec.rs — RED pg_tests: ah_simd_matches_scalar_lut (both dispatch branches, across m + tail), ah_simd_no_overflow
```

#### Deep file dependency analysis
- `vec.rs` — the SIMD kernel sits in the existing `simd_x86` mod (reuses `available()`); the dispatcher mirrors `l2_dist_from_bytes` (`:257`) including the ALWAYS-on length assert. Consumed by `traverse` (Phase 4).

#### Deep Dives
- Algorithm: interleave/transpose the codes into 4-bit nibble lanes; broadcast the per-subspace 16-byte LUT; `_mm256_shuffle_epi8` gathers 16/32 partials in one instr; `_mm256_add_epi8` then widen (`_mm256_add_epi16`) to avoid overflow across `m` subspaces (blueprint T2).
- Invariants (Baseline `vec.rs` cell): the length invariant (`code lanes` × `m`) enforced ALWAYS (release too), exactly `l2_dist_from_bytes:258`; `available()` gates the `unsafe`; scalar fallback is bit-comparable in ORDER.
- Edge cases (edge vs negative): edge — `m` not a multiple of the lane width (tail handling, like `l2_sq` tail `:152`); negative — mismatched LUT/code length ⇒ typed `Err`/assert at the boundary, never OOB in the `unsafe` block.

#### Pseudo-code / Signatures
```pseudocode
#[target_feature(enable = "avx2")] unsafe fn ah_score(lut: &Lut16, codes_block: &[u8], out: &mut [i32])
  # for a block of up to 32 codes, subquantizer-major transposed:
  for s in 0..m:
    tab = _mm256_broadcastsi128(lut.tables[s*16 .. s*16+16])   # 16-byte LUT lane
    lo/hi nibbles = unpack codes_block for subspace s
    partial = _mm256_shuffle_epi8(tab, nibbles)                # 16/32 parallel lookups
    acc = _mm256_adds_epi16(acc, widen(partial))               # saturating widen accumulate
  store acc -> out
fn ah_score(lut, codes_block, out)  # dispatcher: assert len invariant ALWAYS; available()? avx : scalar
```

#### Tasks
1. Implement `simd_x86::ah_score` (transpose + `pshufb` + widened accumulate).
2. Implement the `ah_score` dispatcher with the always-on length assert + `available()` gate.
3. Wire `force_for_test(bool)` coverage of BOTH branches (reuse the M58 pattern).

#### TDD
```
RED:     ah_simd_matches_scalar_lut() — for both forced branches (avx/scalar) and several m + a non-lane-multiple tail, SIMD score ORDERS codes identically to ah_score_scalar within the requant eps (the FAISS pq4 bit-for-bit accumulate oracle, blueprint Coverage Corner 1 Project B). MUST fail before ah_score exists.
RED:     ah_simd_no_overflow() — a worst-case all-max-LUT block does not overflow the int8→int16 accumulate (negative/edge).
GREEN:   Implement simd_x86::ah_score + dispatcher.
REFACTOR: "None expected".
VERIFY:  cargo pgrx test --package theodb_rs (ah_simd_* tests)
```

#### Concurrency tests
```
(none — single-threaded; the AVX2 cache atomic follows the existing `AVX2_FMA` idempotent pattern, vec.rs:100)
```

#### Acceptance Criteria
- [ ] `cargo pgrx test --package theodb_rs ah_simd_matches_scalar_lut ah_simd_no_overflow` exits 0 (both `force_for_test` branches + the tail covered).
- [ ] The length invariant is a release `assert_eq!` (not `debug_assert!`) at the dispatcher boundary — verified by `grep -n 'assert_eq!' theodb_rs/src/vec/ah.rs` showing the code/LUT-length guard, mirroring `vec.rs:258`.
- [ ] The `unsafe` block is guarded by `simd_x86::available()` — verified by `grep -n 'available()' theodb_rs/src/vec/ah.rs` preceding every `_mm256_shuffle_epi8` call site.
- [ ] Lint — `cargo clippy --package theodb_rs -- -D warnings` exits 0 on the AH functions.
- [ ] Size — `wc -l theodb_rs/src/vec/ah.rs` reports ≤ 500.

#### DoD
- [ ] `cargo pgrx test --package theodb_rs` exits 0; `cargo clippy --package theodb_rs -- -D warnings` exits 0; every changed file's `wc -l` is ≤ 500.

### T2.3 — Criterion micro-bench for the AH kernel (per-candidate cost)

#### Objective
Measure AH-SIMD vs scalar per-candidate scoring cost (LUT-lookups/sec), same-graph/box-noise-immune, and record the ratio for the benchmark doc.

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — adds a `#[pg_test]` micro-bench (mirroring `cosine_simd_per_candidate_speedup`, `vec.rs:487`) that times `ah_score` under both forced branches and logs + writes the ratio.
2. **Why it is necessary now** — the kernel latency is the isolated evidence that AH collapses the `score` phase; measured in isolation before the end-to-end benchmark confounds it with reads (M46 measurement lesson, `m46-measurement-learnings` memory). Motivated by Baseline row `vec.rs` (the M58 micro-bench precedent) + blueprint Coverage Corner 3.

#### Evidence
`vec.rs:486-514` (`cosine_simd_per_candidate_speedup` — the micro-bench shape, black_box, log + `/build/target` write, non-flaky "not slower" guard); blueprint Coverage Corner 3 §"Criterion micro-bench".

#### Files to edit
```
theodb_rs/src/vec.rs — add ah_simd_per_candidate_speedup pg_test (mirror cosine_simd_per_candidate_speedup)
```

#### Deep file dependency analysis
- `vec.rs` test module — a self-contained timing test; no production caller. Reuses `force_for_test`/`reset_for_test`.

#### Deep Dives
- Invariants: assert only "SIMD not slower" (non-flaky regression guard, `vec.rs:513`); the magnitude is reported, not gated on timing.
- Edge cases: n/a (pure timing over a synthetic block).

#### Pseudo-code / Signatures
```pseudocode
#[pg_test] fn ah_simd_per_candidate_speedup()
  build a synthetic Lut16 + a large code block
  t_avx = timed(force avx, N iters of ah_score black_box)
  t_scalar = timed(force scalar, …)
  log + write "/build/target/m59-ah-speedup.txt"
  assert t_avx <= t_scalar * 1.2
```

#### Tasks
1. Add the micro-bench `#[pg_test]` with black_box + both forced branches.
2. Log the ratio + best-effort write to the mounted build dir.

#### TDD
```
RED:     ah_simd_per_candidate_speedup() — asserts SIMD not slower than scalar (loose non-flaky guard). MUST fail before ah_score exists (compile).
GREEN:   (kernel already exists from T2.2) — the bench compiles + passes.
REFACTOR: "None expected".
VERIFY:  cargo pgrx test --package theodb_rs (ah_simd_per_candidate_speedup)
```

#### Concurrency tests
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `cargo pgrx test --package theodb_rs ah_simd_per_candidate_speedup` exits 0 and the server log contains an `M59 AH micro-bench … speedup=…x` line (asserting `t_avx <= t_scalar * 1.2`), with the ratio written to `/build/target/m59-ah-speedup.txt` (`test -s /build/target/m59-ah-speedup.txt`).
- [ ] Lint — `cargo clippy --package theodb_rs -- -D warnings` exits 0 on the changed file.

#### DoD
- [ ] `cargo pgrx test --package theodb_rs ah_simd_per_candidate_speedup` exits 0 and `/build/target/m59-ah-speedup.txt` exists with the ratio recorded for the Phase-5 doc (`test -s /build/target/m59-ah-speedup.txt`).

---

## Phase 3: Meta layout v3 persistence (`hnsw_page.rs` + `options.rs`)

**Objective:** persist the AQ codebook in a versioned meta trailer + the `m/2`-byte codes inline in the element tuple, round-tripping through `pack_at`/`decode_meta`, backward-compatible with v1/v2, with a new reloption to enable it.

### T3.1 — Meta v3 codec + AQ pack path

#### Objective
Add `HNSW_STRUCT_VERSION_AQ = 3`, extend `encode_meta`/`decode_meta` to carry the AQ codebook + params, and add an AQ branch to `pack_at` that writes each node's 4-bit code inline.

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — extends the meta codec (`hnsw_page.rs:112-160`) with a v3 trailer `[kind=AQ][m:u8][cb_len:u32][codebook]` and makes `pack_at` (`:363`) encode+write the AQ codes when the code kind is AQ (reusing the trailing-code slot `elem_size` already parameterizes, `:60`).
2. **Why it is necessary now** — the scan (Phase 4) reads v3 off disk; persistence must land first (Dependency Graph 3→4). It mirrors exactly how v2 (SBQ) was added, so v1/v2 stay byte-identical. Motivated by Baseline rows `hnsw_page.rs` + ADR D1.

#### Evidence
`hnsw_page.rs:113-141` (`HNSW_STRUCT_VERSION_SBQ` + the versioned trailer `encode_meta`); `:145-159` (`decode_meta` version switch, the v1/v2 read); `:60-64` (`elem_size(dim, code_len)` trailing-code parameterization); `:354-363` (`pack_sbq`/`pack_at` — the pack signature to extend); ADR D1.

#### Files to edit
```
theodb_rs/src/am/hnsw_page.rs — add HNSW_STRUCT_VERSION_AQ=3; extend encode_meta/decode_meta; add AQ branch to pack_at (encode codes inline)
theodb_rs/src/am/hnsw_page.rs — extend HnswMeta with the AQ params (or reuse a generic code-kind field)
theodb_rs/src/am/hnsw_page.rs — RED pg_tests: aq_meta_v3_roundtrips, v1_v2_meta_still_decodes, pack_aq_writes_codebook_and_matching_codes
```

#### Deep file dependency analysis
- `hnsw_page.rs` — `encode_meta`/`decode_meta` gain a v3 arm; `HnswMeta` gains the AQ params (analogous to `sbq_bits`/`codebook` at `:101-102`); `pack_at` gains an AQ encode branch. Callers `pack`/`pack_sbq` (`:348,355`) unchanged for v1/v2. Downstream: `build.rs:118,319` (Phase 3 T3.3) and `traverse` (Phase 4).
- `decode_element` (`:234`) needs NO change — `code_bytes` already exposes the trailing code for any kind.

#### Deep Dives
- Data structures: extend `HnswMeta` with e.g. `code_kind: u8` (0 none / 1 SBQ / 2 AQ) + `aq_m`/`aq_threshold`, keeping `codebook: Vec<u8>` generic. OR add a parallel `aq_*` set mirroring `sbq_bits`. Choose the minimal-diff option at implement time (KISS).
- Invariants (Baseline `hnsw_page.rs` cell): v1 (magic+v1) and v2 (v2 trailer) decode byte-identically; `elem_size`/analytic addresses unchanged (the code just occupies the trailing slot `bytes_per_vector(dim, m)`).
- Edge cases (edge vs negative): edge — empty graph v3 (entry_level=-1) meta-only; negative — unknown version ⇒ the existing typed `Err` at `:155-158` extended to reject vN>3.

#### Pseudo-code / Signatures
```pseudocode
const HNSW_STRUCT_VERSION_AQ: u32 = 3;
fn encode_meta(m):
  version = match code_kind { none→v1, SBQ→v2, AQ→v3 }
  core 45 bytes …; if AQ: push [m_sub:u8][cb_len:u32][codebook]
fn decode_meta(b): match version { 1|2 → existing; 3 → parse AQ trailer; else Err }
fn pack_at(idx, base, code_kind, params):  # AQ branch: train AqQuantizer, encode each node → inline code
```

#### Tasks
1. Add `HNSW_STRUCT_VERSION_AQ` + the v3 trailer in `encode_meta`.
2. Extend `decode_meta` with the v3 arm (reject vN>3 with a typed Err).
3. Add the AQ branch to `pack_at` (train + `encode` per node, write inline).

#### TDD
```
RED:     aq_meta_v3_roundtrips() — encode_meta(v3) → decode_meta yields identical AQ params + codebook.
RED:     v1_v2_meta_still_decodes() — pre-existing v1 and v2 meta bytes still decode unchanged (backward-compat).
RED:     pack_aq_writes_codebook_and_matching_codes() — a packed v3 graph has the codebook in meta AND each element's code_bytes == AqQuantizer::encode(node_vec) (mirror pack_sbq_writes_codebook_and_matching_codes, hnsw_page.rs:1226).
RED:     decode_meta_rejects_unknown_version() — a v4 magic ⇒ typed Err, not panic (negative).
GREEN:   Implement the v3 codec + pack branch.
REFACTOR: "None expected".
VERIFY:  cargo pgrx test --package theodb_rs (aq_meta_* / pack_aq_*)
```

#### Concurrency tests
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `cargo pgrx test --package theodb_rs aq_meta_v3_roundtrips v1_v2_meta_still_decodes pack_aq_writes_codebook_and_matching_codes decode_meta_rejects_unknown_version` exits 0.
- [ ] v1/v2 decode is byte-identical — `v1_v2_meta_still_decodes` asserts pre-existing v1 and v2 meta bytes decode to the same `HnswMeta` fields as before; `elem_size`/`pack_at` analytic addresses are unchanged for v1/v2 (asserted by the existing SBQ pack tests staying green in the same run).
- [ ] Lint — `cargo clippy --package theodb_rs -- -D warnings` exits 0 on `hnsw_page.rs`.
- [ ] Complexity — no new `hnsw_page.rs` function exceeds cyclomatic 10, verified by `cargo clippy -p theodb_rs -- -W clippy::cognitive_complexity` reporting zero warnings on the changed functions.

#### DoD
- [ ] `cargo pgrx test --package theodb_rs` exits 0; `cargo clippy --package theodb_rs -- -D warnings` exits 0; every changed file's `wc -l` is ≤ 500.

### T3.2 — Reloption `WITH (pq_subspaces, pq_bits, aq_threshold)` + resolvers

#### Objective
Add the three AQ options to the shared reloption struct + `init`/`amoptions` parse table + `*_from_relation` resolvers, defaulting to OFF (v1/v2 unchanged).

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — extends `TheodbIvfflatOptions` (`options.rs:30`) with `pq_subspaces`/`pq_bits`/`aq_threshold`, registers them in `init` (`:42`), adds them to the `amoptions` parse table (`:72`), and adds `pq_*_from_relation` resolvers (mirroring `sbq_bits_from_relation`, `:120`).
2. **Why it is necessary now** — the build (T3.3) reads these to decide v3; the reloption is the interface surface that turns AQ on. Defaults OFF keep every existing index byte-identical. Motivated by Baseline row `options.rs` + ADR D1.

#### Evidence
`options.rs:19-33` (the `sbq_bits` reloption precedent + the shared struct), `:42-62` (`init` add_int_reloption), `:68-92` (`amoptions` parse table), `:114-131` (`sbq_bits_from_relation`); ADR D1 ("reloption/opclass, not a replacement").

#### Files to edit
```
theodb_rs/src/am/options.rs — add pq_subspaces/pq_bits/aq_threshold to the struct + init + amoptions; add pq_*_from_relation resolvers
theodb_rs/src/am/options.rs — RED pg_tests (or integration DDL test): pq_reloption_defaults_off, pq_reloption_parses
```

#### Deep file dependency analysis
- `options.rs` — the struct grows by three fields (still `#[repr(C)]`, `build_reloptions` sets the varlena header); the parse table grows to 5 entries; new resolvers added. Existing `lists`/`sbq_bits` + their resolvers untouched. Downstream: `build.rs` (T3.3).
- Note: `aq_threshold` is a float knob — either an `add_real_reloption` or store T×1000 as an int (KISS: an int-scaled threshold avoids a second reloption type; decide at implement time).

#### Deep Dives
- Invariants (Baseline `options.rs` cell): defaults keep `rd_options` null → OFF → v1/v2 byte-identical; each AM reads only its option (the shared-struct contract, `:26-34`).
- Edge cases: out-of-range values rejected by `build_reloptions` min/max at DDL (typed error, `:64-66`), not scan-time.

#### Pseudo-code / Signatures
```pseudocode
struct TheodbIvfflatOptions { vl_len_, lists, sbq_bits, pq_subspaces, pq_bits, aq_threshold_milli }
fn pq_subspaces_from_relation(rel) -> usize   // 0 = AQ off
fn pq_bits_from_relation(rel) -> u8           // default 4
fn aq_threshold_from_relation(rel) -> f32     // milli → f32; default 1.0 (η≈1)
```

#### Tasks
1. Extend the struct + `init` + `amoptions` parse table.
2. Add the three `*_from_relation` resolvers (default OFF/4/1.0).

#### TDD
```
RED:     pq_reloption_defaults_off() — no WITH() ⇒ pq_subspaces_from_relation == 0 (AQ off) (mirror the sbq default-off behavior).
RED:     pq_reloption_parses() — WITH (pq_subspaces=8, pq_bits=4, aq_threshold=…) resolves to the given values; out-of-range ⇒ DDL error (negative).
GREEN:   Implement the options + resolvers.
REFACTOR: "None expected".
VERIFY:  cargo pgrx test --package theodb_rs (pq_reloption_*)
```

#### Concurrency tests
```
(none — single-threaded; reloption init runs once at _PG_init, single-threaded postmaster, options.rs:42)
```

#### Acceptance Criteria
- [ ] `cargo pgrx test --package theodb_rs pq_reloption_defaults_off pq_reloption_parses` exits 0 — `pq_reloption_defaults_off` asserts `pq_subspaces_from_relation == 0` when no `WITH()` is given (existing indexes byte-identical).
- [ ] Out-of-range values raise a typed DDL error — `pq_reloption_parses` asserts `CREATE INDEX … WITH (pq_bits=99)` errors at DDL (via `build_reloptions` min/max), not at scan time.
- [ ] Lint — `cargo clippy --package theodb_rs -- -D warnings` exits 0 on `options.rs`.
- [ ] Size — `wc -l theodb_rs/src/am/options.rs` reports ≤ 500.

#### DoD
- [ ] `cargo pgrx test --package theodb_rs` exits 0; `cargo clippy --package theodb_rs -- -D warnings` exits 0; every changed file's `wc -l` is ≤ 500.

### T3.3 — Build + fold wiring (`build.rs`) preserves v3 across VACUUM

#### Objective
`ambuild_hnsw` reads the AQ reloption and packs v3; `vacuum_rebuild_hnsw_structured` re-trains the codebook + re-packs v3 across a compaction fold (mirroring the SBQ fold).

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — in `build.rs`, after `sbq_bits_from_relation` (`:117`), read the AQ options and call the AQ pack path when AQ is on; in `vacuum_rebuild_hnsw_structured` (`:319,329`) pass the AQ params so `pack_at` re-packs v3 (like SBQ preserves `meta.sbq_bits`).
2. **Why it is necessary now** — without the fold path, a VACUUM compaction of a v3 index would silently drop it to v1 or corrupt codes (the Risk row); the build path is what materializes v3 at CREATE INDEX. Motivated by Baseline row `build.rs` + Risk "v3 fold".

#### Evidence
`build.rs:111-121` (`ambuild_hnsw` reads `sbq_bits_from_relation` + `pack_sbq`); `build.rs:317-333` (`vacuum_rebuild_hnsw_structured` re-packs preserving `meta.sbq_bits` via `pack_at`); Risk "A v3 fold must re-train the codebook".

#### Files to edit
```
theodb_rs/src/am/build.rs — ambuild_hnsw: read AQ options, pack v3 when on
theodb_rs/src/am/build.rs — vacuum_rebuild_hnsw_structured: pass AQ params to pack_at (re-train codebook)
theodb_rs/src/am/build.rs — RED integration pg_test: aq_index_survives_vacuum_fold
```

#### Deep file dependency analysis
- `build.rs` — `ambuild_hnsw` gains an AQ branch parallel to the SBQ one (`:117-121`); the fold `vacuum_rebuild_hnsw_structured` passes the AQ params to `pack_at` (`:319,329`), reusing the crash-safe fold contract (position-independent pack). The IVF/blob paths are untouched (AQ is HNSW-only, D4).
- Depends on T3.1 (pack_at AQ branch) + T3.2 (resolvers).

#### Deep Dives
- Invariants (Baseline `build.rs` cell): the crash-safe fold's `pack_at` position-independence holds for v3 (the code is inline, addresses analytic); the fold re-trains the codebook from the live vectors (the "códigos gerados no fold" SBQ pattern, `build.rs:317`).
- Edge cases: an AQ index with < k* distinct vectors in a subspace folds without panic (Phase-1 empty-cluster guard); a fold of an empty v3 graph stays meta-only.

#### Pseudo-code / Signatures
```pseudocode
// ambuild_hnsw
let (m, bits, thr) = read_aq_options(indexrel);
let packed = if m > 0 { pack_aq(&idx, m, bits, thr) } else { pack_sbq(&idx, sbq_bits) }?;
// vacuum_rebuild_hnsw_structured
let packed = pack_at(&idx, base, code_kind_from(meta), aq_params_from(meta))?;  // re-train on live
```

#### Tasks
1. `ambuild_hnsw`: read AQ options + pack v3 branch.
2. Fold: pass AQ params to `pack_at` so v3 survives compaction.

#### TDD
```
RED:     aq_index_survives_vacuum_fold() — CREATE INDEX … WITH (pq_subspaces=…); INSERT to trigger pending; DELETE past the compaction ratio; VACUUM; then a scan still returns correct top-k (the v3 codebook was re-trained, not dropped). MUST fail before the fold wiring exists.
GREEN:   Implement the build + fold AQ branches.
REFACTOR: "None expected".
VERIFY:  cargo pgrx test --package theodb_rs (aq_index_survives_vacuum_fold)
```

#### Concurrency tests (applicable — the HNSW build is parallel and the VACUUM fold takes an advisory EXCLUSIVE while scans/inserts hold share)
```
RACE-1 (parallel-build codebook determinism):  aq_codebook_deterministic_under_parallel_build() — build the SAME v3 index twice, once forcing the sequential path and once the parallel path (via THEODB_HNSW_PARALLEL_THRESHOLD, build.rs:16-18), and assert the persisted AQ codebook bytes are IDENTICAL. The codebook is trained on the drained corpus, so a data race in the parallel drain would surface as a codebook divergence. Sequential-vs-parallel parity is the race-aware signal (mirrors the M46 sequential≈parallel bisection, build.rs:18).
RACE-2 (fold vs concurrent insert):  aq_fold_survives_concurrent_insert() — reuse the existing M48/M56 crash-safe-fold lock harness: while a v3 VACUUM fold holds the advisory EXCLUSIVE (build.rs:243 index_exclusive), an aminsert that took index_shared (build.rs:168) must serialize (not corrupt). AQ adds NO new shared mutable state — the codebook is packed into fresh pages before the block-0 pivot (build.rs:334) — so this asserts the AQ fold rides the same GenericXLog pivot without a new lock. Assert: post-fold scan returns the correct top-k and the concurrently-inserted row is present after the next fold.
```

#### Acceptance Criteria
- [ ] `cargo pgrx test --package theodb_rs aq_index_survives_vacuum_fold aq_codebook_deterministic_under_parallel_build aq_fold_survives_concurrent_insert` exits 0.
- [ ] The AQ fold introduces NO new lock — `grep -n 'index_exclusive\|index_shared' theodb_rs/src/am/build.rs` shows the AQ fold reuses the existing M48/M56 lock calls, not a new one (asserted by review of the diff).
- [ ] Lint — `cargo clippy --package theodb_rs -- -D warnings` exits 0 on `build.rs`.
- [ ] Complexity — no changed `build.rs` function exceeds cyclomatic 10, verified by `cargo clippy -p theodb_rs -- -W clippy::cognitive_complexity` reporting zero warnings on them.

#### DoD
- [ ] `cargo pgrx test --package theodb_rs` exits 0; `cargo clippy --package theodb_rs -- -D warnings` exits 0; `wc -l theodb_rs/src/am/build.rs` change keeps every function ≤ 500 lines; `grep -q 'pq_subspaces\|anisotropic' CHANGELOG.md` confirms the `[Unreleased] § Added` entry.

---

## Phase 4: Scan wiring (`hnsw_page.rs::traverse` + `guc.rs`)

**Objective:** `traverse` precomputes the query LUT once for a v3 index, scores the walk by AH, reranks survivors by exact f32; a scan GUC controls the rerank pool.

### T4.1 — AH branch in `load`/`traverse` (LUT once per query + f32 rerank)

#### Objective
When the meta is v3, build the query `Lut16` once (where `qcode_owned` is built today, `:927`), score each candidate in `load` by `ah_score` instead of `hamming_bytes` (`:851`), then rerank survivors by exact f32 (reuse the `:981-1002` path).

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — in `traverse`, add a v3 arm that reconstructs `AqQuantizer` from the meta codebook + builds the per-query `Lut16` once; thread the LUT into `load` so its scoring branch (`:842`) computes `ah_score(lut, ev.code_bytes)` for a v3 index; keep the exact-f32 rerank of survivors (`:981`). The `NeighborSource` seam is unchanged (D1).
2. **Why it is necessary now** — this is where the near-free `score` phase is realized end-to-end; it is the last code phase before the benchmark can measure recall×QPS. It reuses the SBQ walk/rerank scaffold verbatim, swapping only the per-candidate scorer. Motivated by Baseline rows `hnsw_page.rs` + `scan_core.rs` + ADR D1/D4.

#### Evidence
`hnsw_page.rs:924-942` (the SBQ `qcode_owned` build-once site — the analog for the LUT); `:842-853` (`load`'s scoring branch: `hamming_bytes` vs f32 — the swap point `:851`); `:981-1002` (the SBQ over_fetch walk + exact-f32 rerank of survivors — reused verbatim); `:1020-1067` (`PageNeighborSource` — carries `qcode`, will carry the LUT); `scan_core.rs:24-48` (the seam that does NOT change); ADR D1/D4.

#### Files to edit
```
theodb_rs/src/am/hnsw_page.rs — traverse: v3 arm builds the query Lut16 once; load: AH scoring branch; reuse the over_fetch rerank
theodb_rs/src/am/hnsw_page.rs — PageNeighborSource: carry the Lut16 (Some ⇒ AH walk) alongside/instead of qcode
theodb_rs/src/am/hnsw_page.rs — RED integration pg_test: aq_scan_matches_brute_knn_high_ef, aq_scan_reads_flat_in_n
```

#### Deep file dependency analysis
- `hnsw_page.rs` — `traverse` gains a v3 arm (LUT build); `load` gains an AH scoring branch (a third arm beside `qcode`/f32); `PageNeighborSource` carries the LUT. The rerank path (`:981-1002`) and the `over_fetch` widening (`:986`) are reused unchanged. The `NeighborSource` seam (`scan_core.rs`) is untouched (D1). Depends on Phases 1–3.
- The runtime metric (`THEODB_SCAN_PROFILE=1` `pages_read` log, `:1009`) is reused as the wiring-triad metric; it already logs `reads`/`ef`/`results`.

#### Deep Dives
- Algorithm: v3 ⇒ `AqQuantizer::from_meta_bytes(meta.codebook)` (dim guard like `:931`), `build_lut16(q, &quant)` once; `load` computes `ah_score(lut, ev.code_bytes)` (with the length guard `:844`); ground search over `walk_ef = ef·over_fetch`; rerank survivors by exact f32 (`:993`) → top-ef.
- Invariants (Baseline `hnsw_page.rs` cell): `code_bytes.len()` must equal the expected `bytes_per_vector(dim, m)` (the truncation guard `:844` generalizes to AQ); the walk is recall-neutral in reads (dedup-before-load, `scan_core.rs:144`); f32 rerank keeps recall (ADR-0018).
- Edge cases (edge vs negative): edge — over_fetch=1 reranks the ef pool as-is; negative — a truncated on-disk AQ code ⇒ typed `Err` ("REINDEX (v3 corruption)"), mirroring `:845`.

#### Pseudo-code / Signatures
```pseudocode
// traverse (v3 arm)
let lut = if meta.code_kind == AQ {
    let quant = AqQuantizer::from_meta_bytes(&meta.codebook)?;   // dim guard
    Some(build_lut16(q, &quant))
} else { None };
// load: match (lut, qcode) { AQ(lut) => ah_score(lut, ev.code_bytes), SBQ(qc) => hamming, None => f32 }
// after walk: same over_fetch widen + exact-f32 rerank of survivors (:981-1002), truncate ef
```

#### Tasks
1. Build the query `Lut16` once in `traverse` for v3; thread it into `PageNeighborSource`.
2. Add the AH arm to `load`'s scoring branch (with the length guard).
3. Reuse the over_fetch walk + exact-f32 rerank for v3.

#### TDD
```
RED:     aq_scan_matches_brute_knn_high_ef() — a real-graph v3 scan at high ef + sufficient over_fetch returns the exact-kNN set (recall-neutral survivors + f32 rerank) — extends ground_search_matches_brute_exact_knn (scan_core.rs:272) to the AQ path. MUST fail before the scan arm exists.
RED:     aq_scan_truncated_code_is_typed_err() — a hand-corrupted short AQ code ⇒ typed Err, not a wrong score (negative, mirrors hnsw_page.rs:845).
RED:     aq_scan_reads_flat_in_n() — pages_read stays O(ef·M) (flat in N) for a v3 index (the wiring-triad metric via THEODB_SCAN_PROFILE, hnsw_page.rs:1009).
GREEN:   Implement the traverse/load AH branch + rerank reuse.
REFACTOR: "None expected".
VERIFY:  cargo pgrx test --package theodb_rs (aq_scan_*)
```

#### Concurrency tests
```
(none new — the scan is read-only over pinned buffers; reuses the existing with_page_item pin scope, hnsw_page.rs:835)
```

#### Acceptance Criteria
- [ ] `cargo pgrx test --package theodb_rs aq_scan_matches_brute_knn_high_ef aq_scan_truncated_code_is_typed_err aq_scan_reads_flat_in_n` exits 0 — the first asserts the v3 scan returns the exact-kNN set at high ef (recall preserved on the real graph), the second asserts a corrupt AQ code yields a typed `Err`, the third asserts `pages_read` is O(ef·M).
- [ ] The `NeighborSource` seam is UNCHANGED — `git diff --stat theodb_rs/src/ann/scan_core.rs` shows 0 changed lines.
- [ ] Lint — `cargo clippy --package theodb_rs -- -D warnings` exits 0 on `hnsw_page.rs`.
- [ ] Complexity — no changed `traverse`/`load` function exceeds cyclomatic 10, verified by `cargo clippy -p theodb_rs -- -W clippy::cognitive_complexity` reporting zero warnings on them.

#### DoD
- [ ] `cargo pgrx test --package theodb_rs` exits 0; `cargo clippy --package theodb_rs -- -D warnings` exits 0; `wc -l theodb_rs/src/am/hnsw_page.rs` change does not push any function over the budget.

### T4.2 — Scan GUC for the AH rerank pool

#### Objective
Expose the AH rerank-pool knob (reuse `theodb_hnsw.over_fetch` or add `theodb_hnsw.aq_rerank`) so recall vs speed is tunable per session.

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — reuses the existing `over_fetch` GUC (`guc.rs:33,235`) for the AH rerank widening (the SBQ path already uses it at `hnsw_page.rs:986`); adds a distinct `aq_rerank` GUC only if AQ needs a different range (decide at implement time — default: reuse over_fetch, KISS).
2. **Why it is necessary now** — the benchmark (Phase 5) sweeps the rerank pool to find the recall/speed operating point at 0.99; the knob must exist to run that sweep. Motivated by Baseline row `guc.rs` + the reuse-before-add ladder (`rules/parsimony-ladder.md` rung 4).
3. Reuse-first: `over_fetch` already means exactly "widen the candidate pool before the exact f32 rerank" (`guc.rs:27-33`) — reusing it is the parsimony rung-4 win; a new GUC is added only if measurement shows the ranges must differ.

#### Evidence
`guc.rs:27-33` (`OVER_FETCH` GUC def + `:235` `over_fetch()`); `hnsw_page.rs:986` (SBQ already reads `over_fetch()` for the rerank pool); `rules/parsimony-ladder.md` rung 4 (reuse installed knob before adding).

#### Files to edit
```
theodb_rs/src/am/guc.rs — (reuse over_fetch; OR add aq_rerank GUC + init registration only if needed)
theodb_rs/src/am/guc.rs — RED pg_test (only if a new GUC is added): aq_rerank_guc_default
```

#### Deep file dependency analysis
- `guc.rs` — likely NO change (reuse `over_fetch`); the doc string on `over_fetch` may be broadened to mention AQ. If a new GUC is added it mirrors `OVER_FETCH` exactly (`:30-33` + `init` `:202-211`). Consumed by `traverse` (T4.1).

#### Deep Dives
- Invariants: default 1 (rerank the ef pool as-is) keeps a v1/v2 index unaffected (`guc.rs:30`); no effect on non-AQ indexes.
- Edge cases: over_fetch huge + tiny corpus ⇒ min(k, available), no panic (the SBQ `sbq_overfetch_exceeds_pool_no_panic` invariant, `sbq.rs:350` — generalizes).

#### Tasks
1. Reuse `over_fetch()` in the AH rerank (already wired via the reused `:981` path) — confirm the doc string covers AQ.
2. (Only if needed) add `aq_rerank` GUC mirroring `OVER_FETCH`.

#### TDD
```
RED:     (if a new GUC) aq_rerank_guc_default() — unset ⇒ default; SET changes the rerank pool. If reusing over_fetch, this is COVERED by T4.1's aq_scan_matches_brute_knn_high_ef (which sets over_fetch) — state "(covered by T4.1; no new GUC)".
GREEN:   Broaden the over_fetch doc / add the GUC.
REFACTOR: "None expected".
VERIFY:  cargo pgrx test --package theodb_rs
```

#### Concurrency tests
```
(none — GUC read is per-backend; single-threaded config, guc.rs pattern)
```

#### Acceptance Criteria
- [ ] The AH rerank pool is tunable per session — `SET theodb_hnsw.over_fetch = 4` then a v3 scan reranks a 4× pool (covered by `aq_scan_matches_brute_knn_high_ef` which sets `over_fetch`); the default (1) leaves v1/v2 indexes byte-identical (asserted by the existing SBQ/f32 scan tests staying green).
- [ ] Lint — `cargo clippy --package theodb_rs -- -D warnings` exits 0 on `guc.rs`.

#### DoD
- [ ] `cargo pgrx test --package theodb_rs` exits 0; `cargo clippy --package theodb_rs -- -D warnings` exits 0.

---

## Coverage Matrix

| # | Gap / Requirement (M59 DoD) | Task(s) | Resolution |
|---|---|---|---|
| 1 | Anisotropic k-means training (score-aware loss, η-weighted, m subquantizers) | T1.1 | `AqQuantizer::train` with the parallel/orthogonal residual reweighting; `η=1` isotropic hook. |
| 2 | 4-bit / 16-centroid subquantizers | T1.2, T3.2 | `encode` packs two 4-bit codes/byte; `pq_bits=4` reloption default. |
| 3 | Reconstruction/ADC parity test (quantizer validity) | T1.2 | `aq_adc_correlates_with_f32_distance` (analog of `sbq.rs:280`). |
| 4 | Anisotropic loss test (η parallel vs perpendicular) | T1.1 | `aq_eta_one_reduces_to_isotropic` (the reduction hook). |
| 5 | LUT16 per subspace (query→centroids precomputed) | T2.1 | `build_lut16` (per-subspace int8 requant LUT). |
| 6 | AH scan kernel `_mm256_shuffle_epi8` (pshufb 4-bit) + scalar fallback | T2.2 | `simd_x86::ah_score` + `is_x86_feature_detected!` dispatch. |
| 7 | SIMD↔scalar parity within eps; dim tail | T2.2 | `ah_simd_matches_scalar_lut` (both branches + tail); `ah_simd_no_overflow`. |
| 8 | Kernel micro-bench (per-candidate cost) | T2.3 | `ah_simd_per_candidate_speedup` (mirrors `vec.rs:487`). |
| 9 | Codes trailing in the element tuple (layout v3) | T3.1 | `pack_at` AQ branch writes inline codes; `decode_element.code_bytes` reused. |
| 10 | Codebook versioned in the meta | T3.1 | `HNSW_STRUCT_VERSION_AQ=3` + meta trailer. |
| 11 | New reloption/opclass (not replacing SBQ) | T3.2 | `WITH (pq_subspaces,pq_bits,aq_threshold)`; defaults OFF; SBQ untouched. |
| 12 | REINDEX / round-trip serialize+deserialize | T1.2, T3.1 | `aq_codebook_roundtrips_through_meta_bytes`; `aq_meta_v3_roundtrips`. |
| 13 | Backward-compat (v1/v2 still readable) | T3.1 | `v1_v2_meta_still_decodes`; `decode_meta_rejects_unknown_version`. |
| 14 | v3 survives VACUUM compaction fold | T3.3 | `aq_index_survives_vacuum_fold` (re-train codebook in the fold). |
| 15 | Scan branch: LUT once/query + AH score + f32 rerank top-k | T4.1 | `traverse` v3 arm + `load` AH branch + reused over_fetch rerank. |
| 16 | GUC/reloption for the scan | T4.2 | reuse `over_fetch` (or `aq_rerank`). |
| 17 | Recall preserved end-to-end (≥0.99) | T4.1, T5.1 | `aq_scan_matches_brute_knn_high_ef` + the SIFT1M recall gate (`run_recall.py`). |
| 18 | Recall×QPS vs SBQ (M57) and vs ScaNN gap (M33) | T5.2 | `run_recall_qps.py` + `run_m33_scann.py` → `docs/benchmarks/m59-anisotropic-ah.{md,json}`. |
| 19 | Verdict (closes/reduces 25%? else honest-negative + ADR) | T5.3 | `docs/adr/0019-m59-anisotropic-ah-outcome.md` citing the measured numbers. |
| 20 | Wiring-triad runtime metric (pages_read/score flat in N) | T4.1, T5.2 | `aq_scan_reads_flat_in_n` (unit) + the `THEODB_SCAN_PROFILE=1` phase split (bench). |

**Coverage: 20/20 gaps covered (100%)** — every DoD item maps to at least one task ID (T1.1–T5.3).

## Global Definition of Done

- [ ] All phases (1–4) + Integration Validation completed.
- [ ] All tests passing — `cargo pgrx test --package theodb_rs` green.
- [ ] Zero type errors — `cargo check --package theodb_rs`.
- [ ] Zero lint warnings — `cargo clippy --package theodb_rs -- -D warnings` on changed files.
- [ ] File-size budget respected (per `rules/architecture.md`) — `aq.rs` ≤ 500; if `vec.rs` (currently 515) grows, extract the AH kernel into a submodule to stay ≤ 500.
- [ ] `CHANGELOG.md` updated under `[Unreleased] § Added` (Unbreakable Rule 6) — the AQ+AH reloption.
- [ ] Backward compatibility preserved — v1 (f32) and v2 (SBQ) indexes decode + scan byte-identically; SBQ format untouched.
- [ ] Plan-specific: `aq_adc_correlates_with_f32_distance`, `ah_simd_matches_scalar_lut`, `aq_scan_matches_brute_knn_high_ef`, `aq_index_survives_vacuum_fold` all green.
- [ ] **Runtime-metric proof** — `THEODB_SCAN_PROFILE=1` shows the v3 scan `pages_read` O(ef·M) (flat in N) AND (per §Failure/benchmark) the `score` phase collapse in the profile, observed in an integration workload, not just compiled.
- [ ] **Benchmark artifact exists** — `docs/benchmarks/m59-anisotropic-ah.{md,json}` with ≥3-run mean±std on real SIFT1M + the ScaNN re-run; verdict recorded in `docs/adr/0019-*`.
- [ ] **Plan archived** — after `/review` returns `READY_TO_MERGE` AND the PR is merged, move this file to `knowledge-base/plans/completed/m59-anisotropic-ah-plan.md`. NEVER move before merge.

## Failure scenarios (when I/O external)

The AQ+AH code path touches no network/DB/queue external I/O — it is pure in-process compute (train/encode/score) + PostgreSQL **page** reads (the same pinned-buffer reads the existing scan already does, `hnsw_page.rs:835`), which are not the non-deterministic external I/O this section targets. The one external system is the **benchmark harness** (Phase 5) which provisions a DigitalOcean droplet and reads SIFT1M — validation infrastructure, not shipped code.

```
(none — no external I/O touched)
```

## Phase 5: Integration Validation + D3-gated benchmark (MANDATORY)

**Objective:** validate AQ+AH works in a real workload — end-to-end recall on the real graph + the recall×QPS benchmark on real SIFT1M vs SBQ (M57) and ScaNN (M33), with the honest verdict recorded in an ADR. This phase carries explicit task IDs (T5.1–T5.3) so the Coverage Matrix maps every DoD item to a task.

### T5.1 — End-to-end recall gate on real SIFT1M (v3 index)

#### Objective
Build a v3 (`WITH (pq_subspaces=…)`) index on real SIFT1M and measure recall@10, asserting it is ≥ 0.99 or at parity with the same-graph f32 baseline.

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — runs the `benchmarks/theodb_bench/` recall harness against a v3 index on real SIFT1M, sweeping `aq_threshold` + `over_fetch` to the recall operating point.
2. **Why it is necessary now** — the end-to-end recall is the Goal's named metric; unit tests prove per-component correctness but only the real-corpus harness proves recall survives the full walk+rerank. Motivated by the Goal metric + ADR D1 gate.

#### Evidence
Goal (recall@10 ≥ 0.99); `build.rs:15-21` (the known ~0.96–0.974 graph ceiling — the parity fallback); blueprint Recommendation 4 (real SIFT1M, never synthetic).

#### Files to edit
```
benchmarks/theodb_bench/ — invoke the existing recall harness against a v3 index (no code change; a run config)
```

#### Deep file dependency analysis
- `benchmarks/theodb_bench/` — validation harness only; exercises the built v3 index via SQL, never links into the crate.

#### Deep Dives
- Invariants: real SIFT1M corpus (ADR-0012 data-degeneracy trap, blueprint); ≥3 runs mean±std.
- Edge cases: if the graph ceiling caps absolute recall < 0.99, record recall AT PARITY with the same-graph f32 baseline (Q3).

#### Tasks
1. Build a v3 index on real SIFT1M; sweep `aq_threshold`/`over_fetch`.
2. Record recall@10 mean±std over ≥3 runs.

#### TDD
```
RED:     (validation task — the executable oracle is the harness, not a unit test) the harness run FAILS the gate if measured recall@10 < min(0.99, same-graph f32 baseline recall).
GREEN:   Tune aq_threshold/over_fetch until the gate passes (or record the parity outcome per Q3).
REFACTOR: "None expected".
VERIFY:  python benchmarks/theodb_bench/run_recall.py --index v3 --dataset sift1m --runs 3
```

#### Concurrency tests
```
(none — single-threaded; the recall harness issues serial single-query scans over a read-only index)
```

#### Acceptance Criteria
- [ ] `python benchmarks/theodb_bench/run_recall.py --index v3 --dataset sift1m --runs 3` reports recall@10 mean±std with mean ≥ 0.99, OR mean ≥ the same-graph f32 baseline recall (parity, recorded per Q3) — the run's printed `recall@10` field is the oracle.
- [ ] The corpus is real SIFT1M (the harness `--dataset sift1m` flag; NOT synthetic — asserted by the harness's dataset checksum).
- [ ] ≥3 runs; std reported; effect > variance (`analysis-golden-rule § A1`).

#### DoD
- [ ] Recall gate result recorded in `docs/benchmarks/m59-anisotropic-ah.md`.

### T5.2 — recall×QPS head-to-head vs SBQ (M57) and ScaNN (M33) + profile proof

#### Objective
Measure QPS at recall ≥ 0.99 for AQ vs the f32 baseline vs SBQ (M57) vs ScaNN (M33) on the same seeded subsample, and confirm via the profiler that AH collapses the `score` phase.

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — runs the recall×QPS harness for AQ + f32 + SBQ, re-runs ScaNN on the same seeded 1000-query subsample (`run_m33_scann.py`), and runs the scan under `THEODB_SCAN_PROFILE=1` to confirm the `score`/`reads` phase shift.
2. **Why it is necessary now** — the QPS delta vs SBQ/ScaNN is the whole point of M59 (the 25× lever); the profile proves the mechanism actually fired (not just that recall held). Motivated by Goal + ADR D4 (measured under-feeding check).

#### Evidence
`docs/benchmarks/m33-scann-headtohead.md` (the 1920 vs 78 QPS gap + the comparator); `hnsw_page.rs:1009` (`THEODB_SCAN_PROFILE=1` log); ADR D4; blueprint Recommendation 4.

#### Files to edit
```
benchmarks/theodb_bench/ — recall×QPS harness run (AQ + f32 + SBQ)
benchmarks/run_m33_scann.py — re-run ScaNN on the same seeded subsample
```

#### Deep file dependency analysis
- Validation harness only; runs on a re-provisioned droplet (`m57-bench-droplet` memory — destroyed after).

#### Deep Dives
- Invariants: same seeded 1000-query subsample across all four systems (matched-recall comparison, no cherry-picked point); ≥3 runs mean±std.
- Edge cases: if HNSW under-feeds `pshufb` (D4/Q1), the profile shows `score` NOT collapsing → trigger the IVF batch-scan fallback decision.

#### Tasks
1. Run the recall×QPS harness for AQ, f32, SBQ at matched recall.
2. Re-run ScaNN on the same subsample.
3. Run `THEODB_SCAN_PROFILE=1` and record the `score`/`reads` phase split.

#### TDD
```
RED:     (validation) the harness FAILS the D3 gate if AQ QPS-at-recall-0.99 does not exceed the f32 baseline with effect > variance.
GREEN:   record the measured numbers (a non-superior result is a VALID recorded outcome, not a failure — see T5.3).
REFACTOR: "None expected".
VERIFY:  python benchmarks/theodb_bench/run_recall_qps.py --systems aq,f32,sbq --runs 3 && python benchmarks/run_m33_scann.py && THEODB_SCAN_PROFILE=1 psql -c '<v3 scan>'
```

#### Concurrency tests
```
(none — single-threaded; QPS is measured as serial single-query throughput, matching the M33/M57 harness protocol)
```

#### Acceptance Criteria
- [ ] `python benchmarks/theodb_bench/run_recall_qps.py --systems aq,f32,sbq --runs 3` prints QPS-at-recall-0.99 mean±std for each system; the printed `qps` fields are the oracle.
- [ ] `python benchmarks/run_m33_scann.py` re-runs ScaNN on the same seeded 1000-query subsample; its `qps`/`recall` output is recorded alongside.
- [ ] `THEODB_SCAN_PROFILE=1` on a v3 scan prints a `pages_read`/`score`-phase line (`hnsw_page.rs:1009`) showing the `score` phase reduced vs the f32 baseline run — the logged phase split is the oracle.
- [ ] All numbers are ≥3-run mean±std at matched recall (no cherry-picked recall point).

#### DoD
- [ ] The four-system recall×QPS table + the profile split are written to `docs/benchmarks/m59-anisotropic-ah.{md,json}`.

### T5.3 — Verdict ADR (close/reduce 25% OR honest-negative + next seed)

#### Objective
Write `docs/adr/0019-m59-anisotropic-ah-outcome.md` recording the D3-gate verdict: superiority (beats f32-at-0.99), claim-bounded partial (beats SBQ not f32), or honest-negative + the next seed.

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — classifies the T5.2 numbers against the D3 gate and records the decision as an ADR (with the next seed if not superior).
2. **Why it is necessary now** — the verdict is the milestone's DoD item #19; measurement-first + anti-sunk-cost (CLAUDE.md) require the outcome be recorded whether positive or negative, so the next milestone has a documented starting point. Motivated by ADR D1 gate + `public-copy.md § 4`.

#### Evidence
ADR D1 gate (PRD D3, anti-sunk-cost); `public-copy.md § 4` (no superiority claim without a reproducible benchmark); blueprint Recommendation 6 (the DiskANN/SOAR next seed).

#### Files to edit
```
docs/adr/0019-m59-anisotropic-ah-outcome.md — NEW: the verdict ADR
```

#### Deep file dependency analysis
- `docs/adr/0019-*` — a new ADR; no code dependency. References the `docs/benchmarks/m59-*` artifact.

#### Deep Dives
- Invariants: the verdict cites the exact measured numbers from T5.2 (no prose-only claim, `public-copy.md § 4`).
- Edge cases: a non-superior result is recorded as honest-negative with the next seed — NOT a plan failure.

#### Tasks
1. Classify the T5.2 numbers against the D3 gate.
2. Write the ADR with the verdict + (if not superior) the next seed.

#### TDD
```
RED:     (documentation task) the ADR is INCOMPLETE if it states a verdict without citing the T5.2 measured numbers.
GREEN:   author the ADR citing the benchmark artifact.
REFACTOR: "None expected".
VERIFY:  test -f docs/adr/0019-m59-anisotropic-ah-outcome.md && grep -q 'qps' docs/adr/0019-m59-anisotropic-ah-outcome.md
```

#### Concurrency tests
```
(none — single-threaded; documentation)
```

#### Acceptance Criteria
- [ ] `docs/adr/0019-m59-anisotropic-ah-outcome.md` exists and its verdict cites the measured QPS/recall numbers from `docs/benchmarks/m59-anisotropic-ah.json` (verified by `grep -q 'qps' docs/adr/0019-m59-anisotropic-ah-outcome.md`).
- [ ] The verdict is exactly one of: superiority / claim-bounded-partial / honest-negative — with the next seed named when not superior (blueprint Recommendation 6).
- [ ] No superiority language appears unless the benchmark artifact backs it (`public-copy.md § 4`).

#### DoD
- [ ] `docs/adr/0019-m59-anisotropic-ah-outcome.md` committed with the verdict + benchmark citation.

### If Validation Fails

1. Separate plan-caused failures from pre-existing (the `build.rs:15-21` graph recall ceiling + the 37 pre-existing pg_test failures noted in the recent commit log are pre-existing).
2. Fix all plan-caused failures before declaring complete.
3. Re-run the chain (`cargo pgrx test --package theodb_rs`).
4. If the D3 benchmark does NOT beat the f32 baseline, that is a VALID plan outcome (measurement-first, anti-sunk-cost) — record the honest-negative in `docs/adr/0019-*` (T5.3) with the next seed (DiskANN disk-resident + SOAR partitioning, blueprint Recommendation 6); do NOT claim superiority. Pre-existing issues are logged, not blockers.
