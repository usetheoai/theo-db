---
slug: m39-pq-product-quantization
milestone_id: M39
created_at: 2026-07-03
goal: Add a std-only Product Quantization (PQ+ADC) knn path benchmark-gated against SBQ on real SIFT.
---

# Plan: Product Quantization (PQ + ADC LUT) knn — benchmark-gated vs SBQ

> **Version 1.0** — Build a std-only `PqQuantizer` (Lloyd k-means codebooks per subspace + asymmetric distance via per-query lookup table) and expose it as a standalone `theodb.pq_knn` SQL function mirroring the existing `theodb.sbq_knn`, so we can measure PQ vs SBQ recall×QPS on real SIFT with the same harness. PQ is the last remaining algorithmic lever for the P0 vector-superiority claim (M38 falsified SBQ). This ships the quantizer + the SQL surface + the reproducible benchmark; it does NOT wire PQ into the index-AM page format (deferred until the benchmark proves PQ beats SBQ — anti-sunk-cost, PRD D3).

## Goal

> "Enable TheoDB to run a std-only Product-Quantization knn (`theodb.pq_knn`) so that PQ vs SBQ recall×QPS is measurable on real SIFT, measured by `benchmarks/run_m39_pq.py` emitting recall@10 + QPS (mean±std over ≥3 runs) for PQ and SBQ side-by-side and the Rust test `pq_adc_correlates_with_f32_distance` passing."

## Context

TheoDB's P0 GOTO is vector-superiority vs AlloyDB/ScaNN; today only recall-parity is proven (`docs/adr/0002-north-star-equal-or-superior-to-alloydb.md`). The M38 measurement (`docs/benchmarks/m38-copy-free-scan.md:12-19`) falsified our SBQ (recall 0.774/0.854/0.947 < 1.0 on real SIFT) and named **PQ+ADC** — the ScaNN/FAISS technique — as the real remaining lever. The M39 discovery blueprint (`.claude/knowledge-base/discoveries/blueprints/m39-pq-product-quantization-blueprint.md`) confirmed: no permissive peer ships PQ (pgvectorscale removed it; vectorchord's RaBitQ + k_means crate are AGPL, barred by D1), so PQ is a build-from-primary-literature bet, std-only, gated by a recall×QPS benchmark. This plan builds the minimal comparable form — a standalone `theodb.pq_knn` mirroring `theodb.sbq_knn` — measured with the same SIFT harness, BEFORE the expensive AM page-format integration.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `theodb_rs/src/sbq.rs` | 292 | `b167dc5` (2026-07-01) | Own SBQ quantizer + `theodb.sbq_knn` knn pipeline (train→quantize→IVFFlat→rank→f32 rerank) | The pattern `pq.rs` mirrors; MUST stay unchanged (PQ coexists, does not modify SBQ) |
| `theodb_rs/src/pq.rs` (NEW) | 0 | — | (file to be created — the PqQuantizer + pq_knn) | — |
| `theodb_rs/src/vec.rs` | 302 | `61e64db` (2026-07-01) | `l2_dist_from_bytes` (SIMD L2) + `l2_distance` (Vec<f32>) — the f32 kernels | `l2_distance(&[f32],&[f32])->f64` must stay callable (PQ reuses it for k-means + rerank) |
| `theodb_rs/src/api.rs` | ~640 | `b167dc5` (2026-07-01) | `#[pg_extern]` wrappers + `theodb.*` SQL surface incl. `_sbq_knn`/`theodb.sbq_knn` | Existing SQL functions unchanged; new `theodb.pq_knn` REVOKEd FROM PUBLIC (mirror sbq) |
| `theodb_rs/src/lib.rs` | 104 | `3152ae0` (2026-07-02) | Crate root; `mod` declarations | Add `mod pq;` — no other change |
| `benchmarks/run_m39_pq.py` (NEW) | 0 | — | (file to be created — the D3 benchmark harness) | — |
| `CHANGELOG.md` | — | — | Public contract (Rule 6) | `[Unreleased]` gets one entry |

### Current callers / dependents

- **Symbol:** `SbqQuantizer` / `knn()` in `theodb_rs/src/sbq.rs:23,114` — **not modified** by this plan (PQ is a parallel module). Callers: `theodb_rs/src/api.rs:227` (`_sbq_knn`). No change.
- **Symbol:** `l2_dist_from_bytes()` in `theodb_rs/src/vec.rs:167` — callers `theodb_rs/src/am/scan.rs:198`, `theodb_rs/src/am/hnsw_page.rs:420`. **Not modified** (PQ reuses `l2_distance`, does not touch this).
- **Symbol:** `l2_distance()` in `theodb_rs/src/vec.rs` — reused read-only by `pq.rs` for k-means assignment + f32 rerank. No signature change.
- **Symbol (NEW):** `theodb_rs._pq_knn` / `theodb.pq_knn` — no callers yet; the benchmark is the first caller.
- **External (public API consumed by other repos):** no — `theodb.pq_knn` is a new internal-benchmark surface, REVOKEd FROM PUBLIC.

### Domain glossary

- **PQ (product quantization)** — split a vector into `m` disjoint sub-vectors; each subspace has a k-means codebook of `k*=256` centroids; the code is `m` bytes (one centroid index/subspace).
- **ADC (asymmetric distance computation)** — keep the query in f32; precompute a per-query LUT `[m][k*]` of squared sub-distances; approximate distance = `Σ LUT[i][code[i]]` (m lookups, no decode).
- **codebook** — the `k*` centroids of one subspace, learned by Lloyd's k-means.
- **rerank** — after ADC ranks candidates, recompute exact f32 distance on the top `k·over_fetch` to produce the final top-k (mirrors SBQ).
- **over_fetch** — multiplier: ADC keeps `k·over_fetch` candidates before the exact f32 rerank.

### Architecture boundaries affected

Per `.claude/rules/architecture.md`: `pq.rs` is a new domain module (pure quantization logic, std-only), sibling to `sbq.rs`. It depends inward on `crate::vec` (kernels) and `crate::pg` (typed errors), never on the AM. The `#[pg_extern]` boundary lives only in `api.rs` (PG coupling at one boundary — same as `_sbq_knn`). No inner→outer import. File-size budget 500 LoC per file (`pq.rs` target ≤ ~280, mirroring `sbq.rs`).

## Prior Art & Related Work

- **Internal blueprint:** `.claude/knowledge-base/discoveries/blueprints/m39-pq-product-quantization-blueprint.md` — Corner 4 "Product Quantization" (algorithm + ADC), ADR D1 (std-only, benchmark-gated), Recommendations §2 (minimal shape).
- **Reference project (pattern, PostgreSQL License):** `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/sbq/quantize.rs:115-148` — quantizer trained in std, no clustering crate (precedent for zero-new-dep).
- **Reference project (SOTA cross-check, AGPL — study-only):** `.claude/knowledge-base/references/vectorchord/crates/index/src/accessor.rs:412-432` — RaBitQ (not borrowed; informs why PQ over RaBitQ).
- **External literature:** Jégou, Douze, Schmid 2011 "Product Quantization for Nearest Neighbor Search", IEEE TPAMI (`https://dl.acm.org/doi/10.1109/TPAMI.2010.57`) — the PQ algorithm + ADC. Guo et al. 2020 "Accelerating Large-Scale Inference with Anisotropic Vector Quantization" (`https://arxiv.org/abs/1908.10396`) — the ScaNN improvement (the fallback lever if PQ underperforms).
- **Internal pattern (in-repo):** `theodb_rs/src/sbq.rs:114-174` — the SBQ knn pipeline `pq_knn` mirrors exactly (train→encode→candidate→rank→truncate→f32 rerank→top-k).

## Objective

- [ ] `PqQuantizer` (std-only) trains `m` codebooks by Lloyd k-means with a fixed seed (deterministic) and encodes a vector to `m` bytes.
- [ ] `pq_adc_lut(query)` builds the per-query `[m][k*]` LUT; `pq_adc_distance(lut, code)` returns `Σ LUT[i][code[i]]`.
- [ ] `pq_adc_correlates_with_f32_distance` proves ADC orders neighbors like exact f32 (the quantizer-validity gate).
- [ ] `theodb.pq_knn` SQL function (REVOKEd FROM PUBLIC) mirrors `theodb.sbq_knn`'s signature/pipeline, returning `(query_idx, id, distance)`.
- [ ] `benchmarks/run_m39_pq.py` emits recall@10 + QPS (mean±std, ≥3 runs) for PQ and SBQ side-by-side on real SIFT.
- [ ] Typed-error negatives (bad m, bad qdim, non-multiple queries) return SQLSTATE 22023 (mirror SBQ).

## ADRs

### D1 — Standalone `theodb.pq_knn` first; defer AM page-format integration until the benchmark proves the win

**Decision:** Ship PQ as a standalone `theodb.pq_knn` (in-memory train+encode per call, mirroring `theodb.sbq_knn`), NOT wired into the `theodb_ivfflat` page format yet.

**Rationale:** SBQ was proven the same way (standalone `sbq_knn`, measured by M38). The AM page-format integration (persisting codebooks in index pages, branching `scan.rs:198`) is the "high effort" part the blueprint flagged; investing it before PQ is measured would be sunk-cost if PQ loses (PRD D3, `CLAUDE.md § Esforço ≠ Complexidade`). Standalone-first gives a directly-comparable recall×QPS number with the existing harness.

**Alternatives considered:** (a) wire PQ into the AM page format immediately — rejected (expensive, sunk-cost risk before measurement); (b) only unit-test the quantizer without an end-to-end SQL surface — rejected (can't measure recall×QPS, the D3 gate needs it).

**Consequences:** enables a fast go/no-go; constrains PQ to the same per-call train cost as SBQ (fine for the benchmark; a future milestone persists codebooks if PQ wins).

### D2 — std-only Lloyd k-means, zero new dependency

**Decision:** Hand-roll Lloyd's k-means in std, reusing `crate::vec::l2_distance` for assignment and `crate::ann::Rng` for a seeded init.

**Rationale:** pgvectorscale trains its quantizer with no clustering crate (`.../sbq/quantize.rs:115-148`); the AGPL `k_means` crate (`vectorchord/Cargo.toml:30`) is barred by D1 (license). An MIT crate (`kmeans`/`linfa`) would pull `ndarray`/`rand` transitives for ~50 lines — parsimony ladder rung 4/5 (`.claude/rules/parsimony-ladder.md`).

**Alternatives considered:** (a) MIT `kmeans` crate — rejected (transitive deps, KISS/YAGNI); (b) AGPL `k_means` — rejected (D1 license gate).

**Consequences:** ~50 essential lines; deterministic with a fixed seed (required for `pq_train_deterministic`).

### D3 — Merge gate: PQ must beat SBQ at fixed recall (anti-sunk-cost)

**Decision:** PQ merges to a release ONLY if `run_m39_pq.py` shows PQ beats SBQ at equal recall/bytes (or QPS at equal recall), effect > measurement variance (≥3 runs mean±std). If PQ underperforms, do NOT merge — record as next-seed (next lever: ScaNN anisotropic loss).

**Rationale:** PRD D3 fork/build discipline; `public-copy.md` (no performance claim without a benchmark). M38 taught that variance dominates on a throttled CPU (~±50%), so the effect must exceed variance.

**Alternatives considered:** (a) merge PQ unconditionally as "more SOTA" — rejected (M38-style dishonesty; SBQ also looked promising and lost); (b) no benchmark, ship on theory — rejected (violates measurement-first).

**Consequences:** the plan's Goal metric is the benchmark itself; a losing benchmark is a valid honest outcome (implementation kept on develop as measurement, not released — the M38 precedent).

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| PQ may also lose to f32+IVFFlat at recall 1.0 on our corpus (as SBQ did) | High | D3 benchmark gate before merge; honest no-merge outcome documented; fallback lever (ScaNN anisotropic) named | dev |
| Lloyd k-means non-determinism could make tests flaky | Medium | Fixed seed via `crate::ann::Rng` + `pq_train_deterministic` test asserting identical codebooks across two trains | dev |
| Per-call k-means train cost inflates QPS vs a pre-built index | Medium | Benchmark measures PQ and SBQ under the SAME per-call model (apples-to-apples); AM persistence deferred to a future milestone | dev |
| ADC LUT for high `m` could regress cache behavior | Low | `k*=256` keeps LUT at `m·256·8` bytes (m=16 → 32KB, L2-resident); benchmark measures actual QPS | dev |

## Unresolved Questions

- Q1 — What `(m, k*)` config best trades recall vs bytes on SIFT-128? (resolved empirically by the benchmark sweep in Phase 3; default `m=16, k*=256` from FAISS convention.)
- Q2 — Does the fixed-seed k-means converge deterministically across platforms (float summation order)? (mitigated by seeded init + order-independent centroid mean; asserted by `pq_train_deterministic` — if it flakes, pin summation order.)

## Dependency Graph

```
Phase 1 (PqQuantizer + ADC, pq.rs) ──▶ Phase 2 (theodb.pq_knn SQL surface) ──▶ Phase 3 (benchmark D3 gate)
                                                                                      │
                                                                                      ▼
                                                                                Phase 4 (Integration Validation)
```

Phase 1 blocks Phase 2 (the SQL surface calls the quantizer). Phase 2 blocks Phase 3 (the benchmark calls `theodb.pq_knn`). Sequential — no parallelism (single module, single author).

---

## Phase 1: PqQuantizer + ADC (std-only)

**Objective:** a std-only quantizer that trains `m` k-means codebooks, encodes to `m` bytes, and computes ADC distance via a per-query LUT — proven to order neighbors like exact f32.

### T1.1 — `PqQuantizer::train` + `encode` (Lloyd k-means per subspace)

#### Objective
Train `m` codebooks (each `k*` centroids over dim `D/m`) by seeded Lloyd's k-means; encode a vector to `m` bytes (nearest centroid index per subspace).

#### Why this step (action + reasoning)
1. **What this step does** — introduce `PqQuantizer { m, k_star, sub_dim, codebooks: Vec<Vec<Vec<f32>>> }` with `train(corpus, m, seed)` and `encode(v) -> Vec<u8>`.
2. **Why it is necessary now** — the encoder is the foundation; ADC (T1.2) and knn (Phase 2) depend on it. Std-only per D2; mirrors `SbqQuantizer::train`/`quantize` (`theodb_rs/src/sbq.rs:32,63`). Deterministic per Risk-2.

#### Evidence
`theodb_rs/src/sbq.rs:32-92` (the mirror pattern); blueprint Corner 4 "Product Quantization"; `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/sbq/quantize.rs:115-148` (std-only train precedent).

#### Files to edit
```
theodb_rs/src/pq.rs (NEW) — PqQuantizer struct + train() + encode() + unit tests
theodb_rs/src/lib.rs — add `mod pq;`
```

#### Deep file dependency analysis
- `pq.rs` (NEW) reuses `crate::vec::l2_distance` (assignment step) and `crate::ann::Rng` (seeded init). No change to `vec.rs`/`ann`.
- `lib.rs` (`theodb_rs/src/lib.rs:104`) gains one `mod pq;` line; no other module affected.

#### Deep Dives
- Data structure: `codebooks: Vec<Vec<Vec<f32>>>` = `[m][k*][sub_dim]`. `code: Vec<u8>` length `m` (k*≤256 → u8 index).
- Algorithm (Lloyd per subspace `i`): init `k*` centroids by seeded sampling of sub-vectors; repeat ≤ `MAX_ITERS` (e.g. 25): assign each sub-vector to `argmin_j l2_distance(sub, centroid_j)`, update centroid = mean of assigned. Empty cluster → re-seed from a random point (deterministic via Rng).
- Invariants: `train` is order-independent given the seed → `pq_train_deterministic` holds. `sub_dim = D/m` requires `D % m == 0` (validate; typed error otherwise).
- Edge cases: corpus smaller than `k*` (fewer unique centroids — dedup, allow < k* used); `m` not dividing `D` → typed error 22023.

#### Pseudo-code / Signatures
```pseudocode
struct PqQuantizer { m: usize, k_star: usize, sub_dim: usize, codebooks: Vec<Vec<Vec<f32>>> }
fn train(corpus: &[Vec<f32>], m: usize, seed: u64) -> PqQuantizer
  -- precondition: all vectors dim D, D % m == 0
  sub_dim = D / m
  for i in 0..m:
    subvecs = corpus.map(v => v[i*sub_dim .. (i+1)*sub_dim])
    codebooks[i] = lloyd_kmeans(subvecs, k_star=256, seed, max_iters=25)
fn encode(v) -> Vec<u8>
  for i in 0..m: code[i] = argmin_j l2_distance(v_sub_i, codebooks[i][j])   # u8
# Example: D=8, m=2, sub_dim=4 → encode([...]) -> [j0, j1] (2 bytes)
```

#### Tasks
1. Add `mod pq;` to `lib.rs`.
2. Define `PqQuantizer` struct + `Pq_K_STAR=256`, `PQ_MAX_ITERS=25` consts.
3. Implement `lloyd_kmeans(subvecs, k_star, seed, max_iters)` (std, reuse `l2_distance`, `Rng`).
4. Implement `train()` (per-subspace loop) + `encode()`.
5. Validate `D % m == 0` at train (typed error via `crate::pg::err_input`).

#### TDD
```
RED:  pq_encode_2subspace_matches_manual_argmin() — D=8,m=2: encode picks the nearest centroid index per subspace against a hand-built codebook
RED:  pq_train_deterministic() — train(corpus,m,seed) twice → byte-identical codebooks
RED:  pq_train_rejects_indivisible_dim() — D=8,m=3 → err_input (D % m != 0)
GREEN: implement train/encode/lloyd_kmeans
REFACTOR: extract lloyd_kmeans if train exceeds ~40 lines
VERIFY: cargo pgrx test --package theodb_rs pq (pg_test schema)
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `pq_encode_2subspace_matches_manual_argmin`, `pq_train_deterministic`, `pq_train_rejects_indivisible_dim` pass.
- [ ] `encode` returns exactly `m` bytes; `code[i] < k*`.
- [ ] `pq.rs` ≤ 500 LoC; `cargo clippy` clean.

#### DoD
- [ ] `cargo pgrx test` green for the 3 tests; `git commit` referencing T1.1.

### T1.2 — ADC LUT + distance + quantizer-validity gate

#### Objective
Build the per-query LUT `[m][k*]` of squared sub-distances and compute `pq_adc_distance(lut, code) = Σ lut[i][code[i]]`; prove ADC orders neighbors like exact f32.

#### Why this step (action + reasoning)
1. **What this step does** — add `pq_adc_lut(&self, query) -> Vec<Vec<f32>>` and `pq_adc_distance(lut, code) -> f64`.
2. **Why it is necessary now** — ADC is the scan-time ranking primitive Phase 2's knn uses instead of SBQ's Hamming. The correlation gate is the load-bearing proof (analog of `sbq.rs:219`) that PQ is a valid quantizer before we spend a benchmark on it.

#### Evidence
Blueprint Corner 4 "Product Quantization" (ADC formula `d² ≈ Σ LUT[i][code[i]]`); `theodb_rs/src/sbq.rs:219` (the SBQ correlation gate to mirror).

#### Files to edit
```
theodb_rs/src/pq.rs — add pq_adc_lut() + pq_adc_distance() + tests
```

#### Deep file dependency analysis
- `pq.rs` gains two methods reusing `crate::vec::l2_distance` (sub-distance to each centroid). No external file changes.

#### Deep Dives
- `pq_adc_lut(query)`: for each subspace `i`, for each centroid `j`: `lut[i][j] = l2_distance(query_sub_i, codebooks[i][j])` (squared L2). Shape `[m][k*]`.
- `pq_adc_distance(lut, code)`: `Σ_{i} lut[i][code[i] as usize]`. `m` lookups + adds, no decode.
- Invariant: for any query, `pq_adc_distance(lut(q), encode(v))` approximates `l2_distance(q, v)²` monotonically enough to preserve top-k ranking (asserted statistically).
- Edge cases: `code[i]` index bounds (guaranteed < k* by encode); empty subspace unused.

#### Pseudo-code / Signatures
```pseudocode
fn pq_adc_lut(query) -> Vec<Vec<f32>>
  for i in 0..m: for j in 0..k_star: lut[i][j] = l2_distance(query_sub_i, codebooks[i][j])
fn pq_adc_distance(lut, code) -> f64
  sum(lut[i][code[i]] for i in 0..m)
# correlation gate: closer-half by exact f32 has lower mean ADC than farther-half
```

#### Tasks
1. Implement `pq_adc_lut`.
2. Implement `pq_adc_distance`.
3. Add the correlation gate test (mirror `sbq_hamming_correlates_with_f32_distance`).

#### TDD
```
RED:  pq_adc_distance_matches_lut_sum() — hand-built lut+code → exact Σ
RED:  pq_adc_correlates_with_f32_distance() — random corpus+query: the f32-closer half has strictly lower mean ADC than the farther half (quantizer-validity gate)
GREEN: implement pq_adc_lut + pq_adc_distance
REFACTOR: None expected
VERIFY: cargo pgrx test --package theodb_rs pq
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `pq_adc_distance_matches_lut_sum` and `pq_adc_correlates_with_f32_distance` pass.
- [ ] LUT shape is `[m][k*]`; distance is `Σ` of `m` lookups.

#### DoD
- [ ] `cargo pgrx test` green; commit referencing T1.2.

---

## Phase 2: `theodb.pq_knn` SQL surface

**Objective:** expose PQ end-to-end as `theodb.pq_knn`, mirroring `theodb.sbq_knn` (train→encode corpus→IVFFlat candidates→ADC rank→f32 rerank→top-k), REVOKEd FROM PUBLIC.

### T2.1 — `pq_knn` pipeline + `_pq_knn` #[pg_extern] + `theodb.pq_knn` SQL

#### Objective
Implement `pq::knn(...)` mirroring `sbq::knn`, wire it as `theodb_rs._pq_knn` #[pg_extern], and create the `theodb.pq_knn` SQL wrapper (REVOKE FROM PUBLIC).

#### Why this step (action + reasoning)
1. **What this step does** — add `pq::knn(src_table, embed_col, id_col, metric, queries, PqParams)` returning `(query_idx, id, distance)`; add `_pq_knn` extern + `theodb.pq_knn` SQL in `api.rs`.
2. **Why it is necessary now** — the benchmark (Phase 3) needs a callable SQL surface to measure recall×QPS. Mirrors `sbq.rs:114-174` + `api.rs:224-634` exactly (only the ranking primitive changes: ADC instead of Hamming).

#### Evidence
`theodb_rs/src/sbq.rs:101-174` (`SbqParams` + `knn` pipeline); `theodb_rs/src/api.rs:224-634` (`_sbq_knn` extern + `theodb.sbq_knn` SQL + REVOKE).

#### Files to edit
```
theodb_rs/src/pq.rs — add PqParams + knn() (mirror sbq::knn, ADC ranking)
theodb_rs/src/api.rs — add _pq_knn #[pg_extern] + theodb.pq_knn SQL (REVOKE FROM PUBLIC)
```

#### Deep file dependency analysis
- `pq::knn` reuses `read_corpus`/`IvfflatIndex`/`Metric` (same helpers `sbq::knn` uses — confirm visibility; if `read_corpus` is `sbq`-private, promote to a shared `crate::` helper or duplicate the 5-line Spi read per DRY-vs-KISS judgment).
- `api.rs` gains `_pq_knn` + the SQL block; existing `_sbq_knn`/`theodb.sbq_knn` unchanged.

#### Deep Dives
- `PqParams { qdim, k, m, lists, probes, over_fetch, seed }` (mirror `SbqParams`, `bits`→`m`).
- `knn` loop: `train` PQ on corpus → `encode` corpus → per query: `IvfflatIndex.candidate_positions` → `lut = pq_adc_lut(query)` → `sort_by ADC(lut, code[i])` → truncate `k·over_fetch` → exact f32 rerank (`metric.dist`) → top-k.
- Invariant: identical output shape to `sbq_knn` (`(i32,i64,f64)` rows); empty queries → 0 rows (EC mirror).
- Boundary validation (Rule 8, typed 22023): `m ≥ 1`, `qdim % m == 0`, `qdim ≥ 1`, `k ≥ 1`, `over_fetch ∈ [1,64]`, queries length multiple of qdim, valid identifiers.

#### Pseudo-code / Signatures
```pseudocode
fn knn(src, embed, id, metric_s, queries, p: PqParams) -> Vec<(i32,i64,f64)>
  validate(p) ; if queries empty: return []
  corpus = read_corpus(src, embed, id, p.qdim)
  quant = PqQuantizer::train(vecs, p.m, p.seed) ; codes = vecs.map(encode)
  carrier = IvfflatIndex::build(corpus, p.lists, metric, p.seed)
  for (qi, q) in queries.chunks(qdim):
    lut = quant.pq_adc_lut(q)
    cand = carrier.candidate_positions(q, p.probes)
    cand.sort_by_key(i => pq_adc_distance(lut, codes[i]))
    cand.truncate(k*over_fetch) ; cand.sort_by(f32 metric.dist) ; cand.truncate(k)
    emit (qi, id, dist)
```

#### Tasks
1. Add `PqParams`.
2. Implement `pq::knn` (mirror `sbq::knn`, swap Hamming→ADC).
3. Add `_pq_knn` #[pg_extern] in `api.rs`.
4. Add `theodb.pq_knn` SQL wrapper + `REVOKE ALL ... FROM PUBLIC` (both the wrapper and `_pq_knn`).
5. Boundary validation with typed errors.

#### TDD
```
RED:  pq_knn_smoke() — small corpus via Spi: pq_knn returns k rows/query, ids within corpus, ascending distance
RED:  pq_knn_recall_reasonable() — pq_knn top-1 matches exact f32 nearest on a separable corpus (sanity, not the full benchmark)
RED:  pq_knn_bad_m_rejected() — m=0 → err_input (22023)
RED:  pq_knn_qdim_not_multiple_of_m_rejected() — qdim=7,m=2 → err_input (22023)
RED:  pq_knn_empty_queries_no_read() — empty queries → 0 rows, no Spi read
GREEN: implement knn + extern + SQL
REFACTOR: dedup shared Spi/IVFFlat helpers if promoted
VERIFY: cargo pgrx test --package theodb_rs pq
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```
`pq_knn` is per-call with no shared mutable state — mirrors `sbq_knn`.

#### Acceptance Criteria
- [ ] `pq_knn_smoke`, `pq_knn_recall_reasonable`, and the 3 negative tests pass.
- [ ] `theodb.pq_knn` and `theodb_rs._pq_knn` are REVOKEd FROM PUBLIC (mirror `api.rs:633-634`).
- [ ] Output shape identical to `theodb.sbq_knn`.

#### DoD
- [ ] `cargo pgrx test` green; extension installs; `git commit` referencing T2.1; CHANGELOG `[Unreleased]` updated.

---

## Phase 3: Benchmark (D3 gate)

**Objective:** measure PQ vs SBQ recall@10 + QPS on real SIFT, ≥3 runs mean±std — the go/no-go evidence.

### T3.1 — `benchmarks/run_m39_pq.py` (PQ vs SBQ side-by-side)

#### Objective
A reproducible harness that runs `theodb.pq_knn` and `theodb.sbq_knn` over the same SIFT subset, computes recall@10 vs exact seqscan, and reports QPS mean±std over ≥3 runs.

#### Why this step (action + reasoning)
1. **What this step does** — mirror `benchmarks/run_m36_scan.py`, add a PQ leg; sweep `m ∈ {8,16,32}` vs SBQ `bits ∈ {1,2,4}`.
2. **Why it is necessary now** — this IS the Goal metric + the D3 merge gate. Without it, no PQ claim (public-copy.md).

#### Evidence
`benchmarks/run_m36_scan.py` (QPS driver skeleton); `docs/benchmarks/m38-copy-free-scan.md:36-40,63-66` (SIFT recall harness + variance discipline).

#### Files to edit
```
benchmarks/run_m39_pq.py (NEW) — PQ vs SBQ recall×QPS harness
docs/benchmarks/m39-pq.md (NEW) — results (written after the run)
```

#### Deep file dependency analysis
- New standalone Python; connects to the container, loads SIFT, calls `theodb.pq_knn`/`theodb.sbq_knn`, compares to exact. No Rust dependency beyond the installed extension.

#### Deep Dives
- Metric: recall@10 = |PQ_top10 ∩ exact_top10| / 10, averaged over queries; QPS = queries / wall-clock, ≥3 runs, report mean±std.
- Dataset: reuse the SIFT subset from M38 (`docs/benchmarks/m38-copy-free-scan.md`) for direct comparability.
- Gate (D3): PQ beats SBQ at equal recall (higher QPS) OR equal QPS (higher recall), effect > std. Emit a PASS/FAIL verdict line.
- Edge cases: warm-cache (discard first run); fixed seed for reproducibility.

#### Tasks
1. Copy the `run_m36_scan.py` connection + SIFT-load scaffold.
2. Add exact seqscan ground-truth top-10.
3. Add PQ leg (`theodb.pq_knn`) + SBQ leg (`theodb.sbq_knn`).
4. Compute recall@10 + QPS mean±std (≥3 runs) per config.
5. Emit a machine-readable JSON + a PASS/FAIL D3 verdict.

#### TDD
```
RED:  test_run_m39_pq_emits_recall_and_qps() — on a tiny synthetic corpus, the harness returns a dict with recall@10 and qps_mean/qps_std for both pq and sbq (structure + non-degeneracy)
GREEN: implement the harness
REFACTOR: extract the shared recall() helper if duplicated from m36
VERIFY: cd benchmarks && python3 -m pytest tests/test_run_m39_pq.py
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `test_run_m39_pq_emits_recall_and_qps` passes (structure + non-degeneracy — no all-zeros).
- [ ] Running against the container emits recall@10 + QPS mean±std for PQ and SBQ + a D3 PASS/FAIL verdict.
- [ ] Results written to `docs/benchmarks/m39-pq.md` with methodology + hardware (honest, per `public-copy.md`).

#### DoD
- [ ] Harness test green; a real run recorded in `docs/benchmarks/m39-pq.md`; commit referencing T3.1.

---

## Phase 4: Integration Validation

**Objective:** the full chain is green and the D3 verdict is recorded honestly.

### T4.1 — Full validation + honest D3 verdict

#### Objective
Run the complete gate: `cargo pgrx test` (all pq tests) + extension install + benchmark harness test + a real SIFT run, and record the D3 PASS/FAIL verdict.

#### Why this step (action + reasoning)
1. **What this step does** — the "eat your own cooking" gate: prove PQ works end-to-end and record whether it beats SBQ.
2. **Why it is necessary now** — a plan is not done until the integration chain passes; the D3 verdict decides release vs no-merge (per M38 precedent, a losing benchmark is honestly recorded, not hidden).

#### Evidence
`.claude/rules/cycle-implement.md` (Integration Validation mandatory); `docs/benchmarks/m38-copy-free-scan.md` (the honest-negative-result precedent).

#### Files to edit
```
docs/benchmarks/m39-pq.md — final verdict (PASS → proceed to review/release; FAIL → no-merge, next-seed recorded)
CHANGELOG.md — final [Unreleased] entry reflecting the honest outcome
```

#### Deep file dependency analysis
- Documentation + CHANGELOG only; no code change in this task.

#### Deep Dives
- If PASS: PQ proceeds to `/code-quality` → `/review` → release; claim carries the benchmark link.
- If FAIL: record no-merge honestly; the quantizer stays on develop as measurement (M38 precedent); next lever = ScaNN anisotropic loss (blueprint §Recommendations 5).

#### Tasks
1. `cargo pgrx test` full pq suite green.
2. Extension installs; `theodb.pq_knn` callable in the container.
3. `python3 -m pytest tests/test_run_m39_pq.py` green.
4. Real SIFT run; record recall×QPS + D3 verdict in `docs/benchmarks/m39-pq.md`.
5. CHANGELOG entry reflecting the actual outcome.

#### TDD
```
RED:  (integration — no new unit test) the D3 verdict line must be present in the benchmark JSON
GREEN: run the chain; record the verdict
REFACTOR: None expected
VERIFY: cargo pgrx test && cd benchmarks && python3 -m pytest tests/test_run_m39_pq.py && python3 run_m39_pq.py --sift <path>
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] All pq `cargo pgrx test` green; extension installs; `theodb.pq_knn` callable.
- [ ] `run_m39_pq.py` produces recall×QPS mean±std + a D3 PASS/FAIL verdict on real SIFT.
- [ ] `docs/benchmarks/m39-pq.md` records the honest verdict; CHANGELOG reflects it.

#### DoD
- [ ] Integration chain green; D3 verdict recorded; ready for `/code-quality` → `/review` (if PASS) or no-merge (if FAIL).

## Coverage Matrix

| Goal/Objective claim | Task(s) |
|---|---|
| PqQuantizer trains m codebooks by seeded Lloyd, encodes to m bytes | T1.1 |
| pq_adc_lut + pq_adc_distance (Σ LUT[i][code[i]]) | T1.2 |
| pq_adc_correlates_with_f32_distance (quantizer-validity gate) | T1.2 |
| theodb.pq_knn SQL (REVOKE FROM PUBLIC), mirrors sbq_knn | T2.1 |
| Typed-error negatives (bad m, bad qdim, non-multiple) → 22023 | T2.1 |
| benchmarks/run_m39_pq.py recall@10 + QPS mean±std (≥3 runs), PQ vs SBQ | T3.1 |
| D3 merge gate verdict recorded honestly | T3.1, T4.1 |
| Deterministic train (fixed seed) | T1.1 |
| Integration chain green | T4.1 |

**Coverage: 9/9 claims mapped (100%).**

## Failure scenarios

External I/O touched: the corpus is read from a table via `Spi` (PG-internal, same as `sbq_knn`) — not an external HTTP/queue/object-store client. Scenarios:

- **Missing table / column** — `read_corpus` on a non-existent `src_table`/`embed_col` → PG raises; `pq_knn` surfaces a typed error (mirror `sbq_knn` validation). Test: `pq_knn_bad_identifier_rejected` (covered by identifier validation in T2.1).
- **Corpus vector dim ≠ qdim** — a row whose vector length ≠ qdim → typed error 22023 (boundary validation, T2.1).
- **Empty corpus** — 0 rows read → knn returns 0 result rows (no panic); covered by `pq_knn_empty_queries_no_read` + an empty-corpus assertion.

(No external network I/O — the AI/HTTP surface is untouched by this plan.)

## Global Definition of Done

- [ ] All tasks' DoD checked; Coverage Matrix 100%.
- [ ] `cargo pgrx test` green; `cargo clippy` clean; every new file ≤ 500 LoC (`pq.rs` target ≤ 280).
- [ ] `theodb.pq_knn` + `theodb_rs._pq_knn` REVOKEd FROM PUBLIC (least-privilege, mirror SBQ).
- [ ] No new dependency (D2); `Cargo.toml` unchanged.
- [ ] `benchmarks/run_m39_pq.py` produces recall×QPS mean±std + D3 verdict on real SIFT.
- [ ] `docs/benchmarks/m39-pq.md` records the honest outcome; no performance claim without the benchmark link (`public-copy.md`).
- [ ] CHANGELOG `[Unreleased]` updated (Rule 6).
- [ ] `/code-quality` verdict ∉ {FAIL_HARD, INVALID}.
