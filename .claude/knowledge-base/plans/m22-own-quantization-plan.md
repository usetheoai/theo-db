---
slug: m22-own-quantization
milestone_id: M22
created_at: 2026-06-30
goal: Ship an own Rust SBQ scalar quantizer + quantized ANN search at recall@k parity with pgvectorscale and a measured memory profile, proven by a reproducible benchmark gate.
---

# Plan: M22 — Own scalar quantization (SBQ) in Rust (SQL-callable, recall + memory gated)

> **Version 1.1** (edge-cases absorbed: EC-1 memory parity-by-construction honesty; EC-2 over_fetch>pool; EC-3 2-bit; EC-4 f32-carrier=M22b; EC-5 strict-`>`; EC-6 diskann precondition) — Implement TheoDB's own **SBQ-style scalar quantizer** in Rust (`theodb_rs`): per-dimension
> mean-threshold training (Welford), configurable `bits_per_dim`, bit-packed into `u64`, with Hamming-distance
> candidate search over the M21 IVFFlat carrier and a full-precision **f32 rerank** (M20 kernel) to recover
> recall. Exposed as SQL functions (`theodb.sbq_knn`, `theodb.sbq_bytes_per_vector`). A reproducible benchmark
> proves **recall@k parity AND a memory profile** (bytes/vector) vs pgvectorscale's SBQ (`diskann` AM). ZERO new
> dependencies (pure `std` bit ops); RaBitQ is study-only (AGPL). Coexistence: pgvectorscale/pgvector/embed/
> hybrid/import untouched. SQL-callable measurement-first scope (planner-integrated AM = M22b).

## Goal

> Enable TheoDB to answer top-k vector search over its own SBQ-quantized codes so that recall@k reaches parity
> with pgvectorscale at a measured memory profile, measured by `benchmarks/tests/test_sbq_index.py` passing the
> recall+memory parity gate (own recall@10 ≥ pgvectorscale − tol with rerank AND own bytes/vector ≤ pgvectorscale)
> against the container.

## Context

M22 (`ROADMAP-v2.md:128`) requires an own scale + quantization index in Rust, substituting pgvectorscale **only**
with recall parity **and** measured memory (`ROADMAP-v2.md:135`); else an honest ADR keeps pgvectorscale
(anti-sunk-cost). Risk MÁXIMO. The discovery blueprint
(`.claude/knowledge-base/discoveries/blueprints/m22-own-quantization-blueprint.md`, SHIPPABLE_WITH_CAVEATS 89)
locked: own **SBQ-style** quantizer (NOT RaBitQ — AGPL), Hamming search + f32 rerank reusing M20 (`vec.rs`) + M21
(`ann/`), coexistence, ZERO deps. Memory is measured as **bytes/vector** (a computed formula, not
`pg_relation_size`).

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `theodb_rs/src/vec.rs` | 133 | `da8239d` (2026-06-30) | M20 f32-parity distance kernel (`l2_distance`/`inner_product`/`cosine_distance`, `pub(crate)`) | reused for the rerank; signatures unchanged |
| `theodb_rs/src/ann/ivf.rs` | 139 | `83bae02` (2026-06-30) | M21 IVFFlat (`IvfflatIndex::build/search`, k-means++, `pub(crate)`) | reused as the candidate-gen carrier; M21 API unchanged |
| `theodb_rs/src/ann/mod.rs` | 246 | `83bae02` (2026-06-30) | M21 `Metric` enum (`pub(crate)`) + shared primitives | `Metric` reused; unchanged |
| `theodb_rs/src/sbq.rs` (NEW) | 0 | — | (to create) own SBQ quantizer + Hamming + quantized search | — |
| `theodb_rs/src/lib.rs` | 627 | `83bae02` (2026-06-30) | crate root: externs + `extension_sql!` | only ADD `mod sbq;` + new externs + new extension_sql; existing surfaces unchanged |
| `theodb_rs/src/pg.rs` | 56 | `6f5a01a` (2026-06-30) | typed-error helpers (`err_input`) | reuse for boundary 22023; unchanged |
| `theodb_rs/src/ann_query.rs` | 176 | `83bae02` (2026-06-30) | M21 Spi read + validation helpers | pattern reused (may extract a shared `read_corpus`/`valid_ident`); M21 behavior unchanged |
| `benchmarks/theodb_bench/recall.py` | — | (M2/M9 harness) | `recall_at_k`/`brute_force_ground_truth` | REUSED read-only |
| `benchmarks/bench_sbq_index.py` (NEW) | 0 | — | (to create) recall + memory benchmark vs pgvectorscale | — |
| `benchmarks/tests/test_sbq_index.py` (NEW) | 0 | — | (to create) integration + parity gate | — |
| `docs/benchmarks/m22-sbq-parity.md` (NEW) | 0 | — | (to create) reproducible benchmark record | — |
| `CHANGELOG.md` | — | — | public contract | `[Unreleased]` gets one Added entry |

### Current callers / dependents

- **Symbol:** `IvfflatIndex` in `theodb_rs/src/ann/ivf.rs` — callers: `theodb_rs/src/ann_query.rs` (M21 `_ivfflat_knn`). M22 ADDS `theodb_rs/src/sbq.rs` as a new caller (candidate-gen). M21 path unchanged.
- **Symbol:** `crate::vec::{l2_distance,cosine_distance,inner_product}` — callers: lib.rs M20 externs, ann/. M22 adds sbq.rs (rerank). Unchanged.
- **Symbol:** `crate::pg::err_input` — adds sbq.rs caller. Unchanged.
- `lib.rs` `extension_sql!` — M22 appends ONE block; existing blocks unchanged.

### Domain glossary

- **SBQ** — Scalar Bit Quantization: each dimension encoded in `bits_per_dim` bits via a per-dim mean threshold; vectors become compact bit codes.
- **Hamming distance** — `popcount(a XOR b)` over bit codes; the cheap search-time distance on quantized vectors.
- **rerank** — re-fetch the top `k·over_fetch` candidates' full f32 vectors and re-sort by exact distance (recovers recall lost to quantization).
- **bytes/vector** — storage footprint of one quantized vector: `ceil(dim·bits/8)`; f32 baseline is `4·dim`.
- **over_fetch** — multiplier on k for how many Hamming candidates to rerank (recall/latency knob).
- **coexistence** — own functions read `embed_col::real[]`; never redefine pgvector/pgvectorscale types/ops/indexes.

### Architecture boundaries affected

Per `rules/architecture.md`: `sbq.rs` is **domain** (pure quantizer + Hamming + search orchestration over the M21/M20 primitives). The SQL externs in `lib.rs` are the **interface/composition root** (Spi read + SRF + validation). DIP: `sbq.rs` depends on the in-crate `ann::IvfflatIndex` + `vec::*` (not on pgvectorscale). No new cross-layer import direction.

## Prior Art & Related Work

- **Internal blueprint** — `.claude/knowledge-base/discoveries/blueprints/m22-own-quantization-blueprint.md` (SHIPPABLE_WITH_CAVEATS 89). Consumed: ADR D1 (own SBQ, not RaBitQ — AGPL), D2 (Hamming + f32 rerank), D3 (coexistence + recall+memory gate + anti-sunk-cost), D4 (zero deps, SQL-callable); Corner 4 (SBQ algorithm with pgvectorscale `path:line`); Corner 3 (memory formula + search integration).
- **Reference (algorithm)** — pgvectorscale SBQ: train+quantize (`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/sbq/quantize.rs:52-150`), bytes/vector (`quantize.rs:37-50`), Hamming search + rerank (`sbq/storage.rs:174-179,304-328`), default 1 bit/dim (`meta_page.rs:81`).
- **Reference (study-only, AGPL — NOT borrowed)** — vectorchord RaBitQ (`.claude/knowledge-base/references/vectorchord/crates/rabitq/src/bit.rs`).
- **Internal prior milestones** — M20 distance kernel (`theodb_rs/src/vec.rs`) + M21 IVFFlat (`theodb_rs/src/ann/ivf.rs`), both reused (Rule 9).
- **External literature** — pgvectorscale SBQ design (StreamingDiskANN + Statistical Binary Quantization), Timescale.

## Objective

- [ ] Own SBQ quantizer in Rust (`sbq.rs`) — Welford mean training, 1-bit + n-bit quantize, `u64` packing, deterministic.
- [ ] Hamming distance (`popcount` XOR) over codes + `bytes_per_vector(dim,bits)` formula.
- [ ] Quantized ANN search: candidate-gen via the M21 IVFFlat carrier + Hamming ranking + **f32 rerank** (M20 kernel).
- [ ] SQL surface `theodb.sbq_knn(...)` + `theodb.sbq_bytes_per_vector(dim,bits)`, Spi read, boundary 22023, REVOKE.
- [ ] Reproducible **recall@k + bytes/vector** benchmark vs pgvectorscale SBQ (`diskann`) in `docs/benchmarks/`.
- [ ] Migration decision (coexistence) documented; "retain pgvectorscale" valid if parity not reached (anti-sunk-cost).

## ADRs

### D1 — Own SBQ-style quantizer (NOT RaBitQ); permissive + simpler; ZERO new deps

**Decision:** implement an own SBQ-style quantizer (`sbq.rs`): per-dimension mean threshold (Welford), configurable
`bits_per_dim` (default 1), bits packed into `Vec<u64>`. No new crate (pure `std`: f32 math, `u64` packing,
`u64::count_ones`).

**Rationale:** RaBitQ is in vectorchord which is **AGPL-3.0** (blueprint Q6 / D1) — forbidden in the TheoDB
distribution (rule 2). pgvectorscale SBQ is PostgreSQL-licensed (safe to learn) and ~2× simpler. Parsimony rungs
2/5 (a bit-packer is std).

**Alternatives considered:** port RaBitQ (rejected — AGPL release blocker); product quantization (rejected —
heavier, YAGNI); add `simdeez` for SIMD popcount (rejected — `u64::count_ones` is hardware popcount; perf, YAGNI).

**Consequences:** the quantizer is permissive own code; RaBitQ stays study-only.

### D2 — Hamming candidate search + full-precision f32 rerank (reuse M21 IVFFlat + M20 kernel)

**Decision:** search = (1) candidate generation via the M21 `IvfflatIndex` (f32 k-means clustering, proven) to pick
the `probes` nearest lists; (2) within the candidates, rank by **Hamming distance** on the SBQ codes; (3) **rerank**
the top `k·over_fetch` by exact f32 distance (`crate::vec`) and return top-k.

**Rationale:** mirrors pgvectorscale's Hamming-traversal + `get_full_distance_for_resort` rerank
(`sbq/storage.rs:304-328`) — the only way 1-bit recall reaches parity. Reuses M21 (clustering) + M20 (distance) —
Rule 9.

**Alternatives considered:** Hamming-only, no rerank (rejected — recall too low at 1 bit/dim); a new Hamming-HNSW
(rejected — duplicates M21's graph; IVFFlat carrier is enough to prove recall+memory; HNSW-over-codes is a future
extension); generalize M21 HnswIndex over a generic distance (rejected — refactors released M21 code, regression
risk).

**Consequences:** keep the full f32 vectors for rerank (read once from the table); `over_fetch` is a tunable knob.

### D3 — Coexistence, measurement-first, recall + memory gate, anti-sunk-cost

**Decision:** ship the quantizer + quantized search as SQL-callable functions (coexistence — pgvectorscale/pgvector
untouched), gated by (a) recall@k parity (tolerance band, with rerank) and (b) bytes/vector memory profile, both vs
pgvectorscale SBQ, measured + reproducible in `docs/benchmarks/`. If parity at a comparable memory profile is NOT
reached → honest ADR **retain pgvectorscale** (anti-sunk-cost); the milestone delivers the measurement.

**Rationale:** the M22 DoD (`ROADMAP-v2.md:135`) is measurement-first; mirrors the M21 user-approved scope.

**Alternatives considered:** full StreamingDiskANN AM (rejected — M22b, multi-month); force substitution (rejected
— violates measurement-first).

**Consequences:** acceptance metric is the recall+memory benchmark; "retain pgvectorscale" is first-class.

### D4 — bytes/vector memory metric is a computed formula (not pg_relation_size); SQL-callable scope

**Decision:** the memory gate compares **computed bytes/vector** — own SBQ `ceil(dim·bits/8)` (via the Rust
quantizer's own `sbq_bytes_per_vector`) vs f32 `4·dim` vs pgvectorscale's `quantized_size_bytes` (same formula).
NOT `pg_relation_size` (which only measures a real on-disk index — M22b). The deliverable is SQL-callable; the
planner-integrated AM is M22b.

**Honesty (EC-1):** at the same `bits_per_dim`, own bytes/vector `ceil(dim·bits/8)` is IDENTICAL to pgvectorscale's
`quantized_size_bytes` (same formula) — so the memory result is **parity by construction with pgvectorscale AND a
~Nx reduction vs f32 (`4·dim`)**, NEVER framed as "less memory than pgvectorscale". The **recall@k with rerank**
is the substantive differentiator the gate tests. The benchmark doc + CHANGELOG state this exactly
(`rules/public-copy.md`). Runtime (not storage) memory win — search touching only codes — requires the on-disk AM
storing codes only → M22b (EC-4).

**Rationale:** the SQL-callable form has no on-disk index to `pg_relation_size`; the honest memory metric is the
code's own byte formula (blueprint EC-3). KISS.

**Alternatives considered:** `pg_relation_size` of a diskann index for our side (rejected — we have no on-disk
index in this scope; apples-to-oranges).

**Consequences:** `theodb.sbq_bytes_per_vector(dim,bits)` exposes the real formula for the gate; pgvectorscale's
on-disk index size is reported as context, not the head-to-head metric.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| 1-bit SBQ recall may not reach pgvectorscale parity even with rerank | High | tolerance band + `over_fetch`/`bits_per_dim` sweep (D2/D3); anti-sunk-cost fallback ships the *measurement* of the gap (DoD) | impl |
| IVFFlat carrier candidate-gen weaker than pgvectorscale's DiskANN graph → lower recall at low probes | Medium | sweep probes; rerank over-fetch; the gate is recall@k WITH rerank at matched memory; document the carrier choice | impl |
| Reading f32 corpus + keeping it for rerank negates runtime memory savings in this scope | Medium | the memory metric is STORAGE bytes/vector (codes), which is the real win; rerank f32 is the heap (like pgvectorscale); documented (D4) | impl |
| SQL-callable form is not a planner `USING` index (literal AM deferred) | Medium | scope decision (M21 precedent); M22b tracked; coexistence applies to both | owner |
| AGPL contamination if RaBitQ code is copied | High | RaBitQ is study-only (D1); own SBQ is independent permissive code; deps-audit confirms zero AGPL | impl |

## Unresolved Questions

- Q1 — What `bits_per_dim` + `over_fetch` make the recall+memory gate both fair and meaningful? (resolved at plan time: default 1 bit/dim, over_fetch ∈ {1,4,8}, probes sweep; tol=0.05; documented + tunable in the benchmark.)
- Q2 — Synthetic vs real corpus for the gate? (resolved: deterministic synthetic — recall is relative own-vs-pgvectorscale on the SAME corpus; real dataset is a nice-to-have.)
- Q3 — Compare memory vs pgvectorscale's full on-disk index size or just the quantized bytes/vector? (resolved: bytes/vector code formula is the head-to-head, D4; pgvectorscale index_size is context only.)

## Dependencies

### Existing — use as-is

| Package | Version | Ecosystem | Why |
|---|---|---|---|
| `pgrx` | `0.16.1` | rust | extension framework (`#[pg_extern]`, `Spi`, `TableIterator`) — ADR D4 |
| `psycopg2`/`numpy` | (harness) | python | recall + memory benchmark + integration test |
| pgvectorscale (`diskann`) | (in image) | pg ext | the SBQ baseline the benchmark compares against (already in the theo-db image) |

### New — to be introduced

| Package | Version | Ecosystem | Rule 9 rationale (libs evaluated) | Why this one |
|---|---|---|---|---|
| (none) | — | — | Evaluated: vectorchord/rabitq (rejected — **AGPL**, D1 release blocker); `simdeez` SIMD popcount (rejected — `u64::count_ones` is hardware popcount, parsimony); `rkyv`/`bincode` (rejected — batch SRF needs no serialization). | M22 needs ZERO new deps: `std` bit ops + reuse of M20 `vec.rs` + M21 `ann/`. |

### Removed

| Package | Last version | Why removed |
|---|---|---|
| (none) | | |

## Dependency Graph

```
Phase 1 (sbq.rs quantizer: train/quantize/hamming/bytes + unit tests)
   │
   ▼
Phase 2 (SBQ search: IVFFlat carrier + Hamming + f32 rerank; SQL surface + Spi + REVOKE + integration test)
   │
   ▼
Phase 3 (recall@k + bytes/vector benchmark vs pgvectorscale SBQ + docs)
   │
   ▼
Final Phase (integration validation)
```

All phases sequential.

---

## Phase 1: SBQ quantizer core in Rust

**Objective:** implement the own scalar quantizer (train + quantize + Hamming + bytes/vector) as pure in-crate Rust, verified by unit tests on known inputs.

### T1.1 — SBQ quantizer: train, quantize, hamming, bytes_per_vector

#### Objective
Implement `SbqQuantizer` in `theodb_rs/src/sbq.rs`: `train(corpus) -> thresholds`, `quantize(&[f32]) -> Vec<u64>`, `hamming(&[u64],&[u64]) -> u32`, `bytes_per_vector(dim,bits) -> usize`.

#### Why this step (action + reasoning)
1. **What this step does** — adds the per-dimension mean-threshold quantizer (Welford), the n-bit encode + `u64` packing, the Hamming distance (`count_ones` of XOR), and the byte formula.
2. **Why now** — it is the core deliverable (the memory win) that Phase 2's search and Phase 3's gate depend on (ADR D1; blueprint Corner 4 cites pgvectorscale `quantize.rs:52-150`).

#### Evidence
pgvectorscale SBQ: 1-bit `x > mean[d]` (`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/sbq/quantize.rs:57-62`); Welford train (`:115-148`); bytes formula `ceil(dim·bits/64)·8` (`:37-50`); default 1 bit (`meta_page.rs:81`).

#### Files to edit
```
theodb_rs/src/sbq.rs (NEW) — SbqQuantizer + hamming + bytes_per_vector + #[pg_test]
theodb_rs/src/lib.rs — add `mod sbq;`
```

#### Deep file dependency analysis
- `sbq.rs` (new): depends on `crate::vec` (rerank later) + `crate::pg::err_input` (validation later). Phase 1 is pure (no pgrx in the quantizer math).
- `lib.rs`: add one `mod sbq;` line.

#### Deep Dives
- `SbqQuantizer { bits: u8, mean: Vec<f32> }` (1-bit: threshold = mean per dim; n-bit: also `std` from M2).
- `quantize`: for each dim, `bit = x > mean[d]`; pack `code[i/64] |= (b as u64) << (i%64)`.
- `hamming(a,b) = a.iter().zip(b).map(|(x,y)| (x^y).count_ones()).sum()`.
- `bytes_per_vector(dim,bits) = ((dim*bits + 63)/64)*8`.
- Invariants: deterministic (mean is order-independent); equal-dim corpus (validated upstream).
- Edge cases: empty corpus → empty mean; dim=1; all-equal values (mean = value, all bits 0 — fine).

#### Pseudo-code / Signatures
```rust
pub(crate) struct SbqQuantizer { bits: u8, mean: Vec<f32> }
impl SbqQuantizer {
  pub(crate) fn train(corpus: &[Vec<f32>], bits: u8) -> Self; // per-dim mean
  pub(crate) fn quantize(&self, v: &[f32]) -> Vec<u64>;        // packed bit code
  pub(crate) fn bytes_per_vector(dim: usize, bits: u8) -> usize; // ceil(dim*bits/64)*8
}
pub(crate) fn hamming(a: &[u64], b: &[u64]) -> u32;
// Example: train([[0,0],[2,2]],1).quantize([3,3]) -> bits set where x>mean(1,1) -> [0b11]
```

#### Tasks
1. Add `mod sbq;` to `lib.rs`.
2. Implement `SbqQuantizer::train` (per-dim mean; n-bit also variance).
3. Implement `quantize` (1-bit threshold + n-bit unary) + `u64` packing.
4. Implement `hamming` (XOR + `count_ones`).
5. Implement `bytes_per_vector`.

#### TDD
```
RED: sbq_quantize_1bit_threshold() — train [[0,0],[2,2]] (mean [1,1]); quantize [3,0] → bit0 set, bit1 unset
RED: sbq_hamming_counts_bit_diffs() — hamming([0b1010],[0b0011]) == 2
RED: sbq_bytes_per_vector_formula() — (dim=1024,bits=1)→128; (768,1)→96; (1536,2)→384; f32(1024)=4096 → ~32x
RED: sbq_deterministic_train() — train twice on same corpus → identical mean/codes
RED: sbq_quantize_recall_preserved_with_rerank() — 200 random 8-d: top-10 by hamming over-fetch×4 then f32 rerank ⊇ ≥0.9 of brute-force top-10
RED: sbq_quantize_2bit_monotonic() — bits=2: along a dim, a larger value sets ≥ as many bits as a smaller; bytes_per_vector(dim,2)==ceil(dim*2/64)*8 (EC-3)
RED: sbq_value_equal_mean_encodes_zero() — x == mean[d] → bit 0 (strict `>`, parity with pgvector EC-5)
GREEN: implement quantizer
REFACTOR: none expected
VERIFY: cargo pgrx test --features pg17 (sbq tests)
```

#### Concurrency tests

(none — single-threaded) — the quantizer trains + encodes in-memory within a single call; no shared mutable state, no threads.

#### Acceptance Criteria
- [ ] All RED tests green — `cargo pgrx test --features pg17` exits 0 on the `sbq` tests.
- [ ] `sbq_quantize_recall_preserved_with_rerank` ≥ 0.90.
- [ ] Pass: lint — `cargo clippy --release --features pg17 -- -D warnings` exits 0 on `sbq.rs`.
- [ ] Pass: size — `sbq.rs` ≤ 500 lines (split into `sbq/` if exceeded).

#### DoD
- [ ] Tests passing, clippy clean, `mod sbq;` wired.

---

## Phase 2: SBQ quantized search + SQL surface

**Objective:** build the quantized search (IVFFlat carrier + Hamming + f32 rerank) and expose `theodb.sbq_knn` + `theodb.sbq_bytes_per_vector`, with Spi read + boundary validation + REVOKE — proven by a Python integration test.

### T2.1 — SBQ search + SQL externs

#### Objective
Implement `sbq::knn(...)` (quantize corpus, M21 IVFFlat candidate-gen, Hamming rank, f32 rerank top-`k·over_fetch`) and the `#[pg_extern]` SRF `theodb_rs._sbq_knn` + scalar `theodb_rs._sbq_bytes_per_vector`; SQL wrappers `theodb.sbq_knn`/`theodb.sbq_bytes_per_vector` + REVOKE.

#### Why this step (action + reasoning)
1. **What this step does** — wires the quantizer to a real top-k search reusing M21 IVFFlat + M20 rerank, and to SQL; validates args at the boundary (ADR D2/D4).
2. **Why now** — the recall+memory gate (Phase 3) drives the quantizer through SQL; this is the caller (wiring triad pillar a).

#### Evidence
M21 `IvfflatIndex` (`theodb_rs/src/ann/ivf.rs:16,111`); M20 `crate::vec::*`; M21 extern + extension_sql + REVOKE pattern (`theodb_rs/src/lib.rs` M21 block); pgvectorscale Hamming+rerank (`sbq/storage.rs:174-179,304-328`).

#### Files to edit
```
theodb_rs/src/sbq.rs — add knn(): quantize + IVFFlat carrier + Hamming + f32 rerank; validation
theodb_rs/src/lib.rs — add _sbq_knn (TableIterator SRF) + _sbq_bytes_per_vector externs + extension_sql (theodb.sbq_knn / theodb.sbq_bytes_per_vector + REVOKE)
```

#### Deep file dependency analysis
- `sbq.rs`: adds `knn()` reusing `crate::ann::IvfflatIndex` (candidate-gen on f32) + `crate::vec` (rerank) + `crate::pg::err_input`.
- `lib.rs`: ADD two externs + one extension_sql block; existing surfaces unchanged.

#### Deep Dives
- SQL: `theodb.sbq_knn(src_table regclass, embed_col text, queries vector[], k int default 10, bits int default 1, lists int default 100, probes int default 1, over_fetch int default 4, metric text default 'l2', id_col text default 'id', seed bigint default 42) RETURNS TABLE(query_idx int, id bigint, distance float8)`.
- `theodb.sbq_bytes_per_vector(dim int, bits int default 1) RETURNS bigint` → the real `SbqQuantizer::bytes_per_vector`.
- Search: read (id, f32) corpus; train quantizer; build M21 IVFFlat on f32 for list assignment; per query: quantize query, gather probed-list members, rank by Hamming, take top `k·over_fetch`, rerank by f32 (`crate::vec`), return top-k with the f32 distance.
- Validation (22023): `bits ∈ [1,8]`, `over_fetch ∈ [1,64]`, plus the M21 caps (k≥1, lists/probes∈[1,32768], metric, identifier allowlist, integer id_col), NULL-vector skip, dim consistency, empty queries → 0 rows.
- REVOKE both public + private.

#### Pseudo-code / Signatures
```rust
pub(crate) fn knn(src_table, embed_col, id_col, metric, queries:&[f32], qdim, k, bits, lists, probes, over_fetch, seed) -> Vec<(i32,i64,f64)> {
   validate(...);
   let corpus = read_corpus(...);                 // Vec<(i64, Vec<f32>)>
   let q = SbqQuantizer::train(&vecs, bits);
   let codes: Vec<Vec<u64>> = vecs.map(|v| q.quantize(v));
   let ivf = IvfflatIndex::build(&corpus, lists, metric, seed);  // f32 carrier
   for each query chunk:
     let cand = ivf.candidates(query, probes);     // candidate ids
     rank cand by hamming(q.quantize(query), codes[id]); take k*over_fetch
     rerank by crate::vec::dist(full[id], query); take k
}
```

#### Tasks
1. Implement `sbq::knn` (reuse IvfflatIndex for candidate-gen; add a `candidates()` accessor if needed, or reuse `search` then re-rank).
2. Implement the two externs + extension_sql + REVOKE.
3. Boundary validation (bits/over_fetch caps + M21 caps).

#### TDD
```
RED (#[pg_test]): sbq_knn_smoke() — temp table, 3 rows; theodb.sbq_knn returns top-k in distance order
RED (#[pg_test]): sbq_bytes_per_vector_sql() — theodb.sbq_bytes_per_vector(1024,1) == 128
RED (#[pg_test], error): sbq_knn_bad_bits_raises_22023() — bits=0 / bits=9 → 22023
RED (#[pg_test], error): sbq_knn_bad_metric_raises_22023()
RED (#[pg_test]): sbq_overfetch_exceeds_pool() — over_fetch huge / tiny corpus → returns min(k, available), no panic (EC-2)
GREEN: implement externs + extension_sql
REFACTOR: extract shared read_corpus/valid_ident with ann_query if clean; else none
VERIFY: cargo pgrx test --features pg17
```

#### Concurrency tests

(none — single-threaded) — build + search run in-memory within a single backend call; no shared mutable state.

#### Acceptance Criteria
- [ ] All RED `#[pg_test]`s green — `cargo pgrx test --features pg17` exits 0.
- [ ] `has_function_privilege('public','theodb.sbq_knn(...)','execute')` is false — asserted in Phase 3.
- [ ] Pass: lint — `cargo clippy --release --features pg17 -- -D warnings` exits 0.

#### DoD
- [ ] `cargo pgrx test` green; functions installed; CHANGELOG `[Unreleased]` Added entry.

### T2.2 — Python integration test: top-k correctness + memory + negatives (container)

#### Objective
`benchmarks/tests/test_sbq_index.py` builds a table on the container, calls `theodb.sbq_knn` + `theodb.sbq_bytes_per_vector`, asserts top-k recall vs brute force + the memory formula + 22023 negatives + REVOKE.

#### Why this step (action + reasoning)
1. **What this step does** — proves the SQL surface end-to-end on the real container (the wiring-triad integration test).
2. **Why now** — `#[pg_test]`s don't run in CI; the container test is the always-on proof.

#### Evidence
M21 `benchmarks/tests/test_ann_index.py` pattern; harness `recall_at_k`/`brute_force_ground_truth`.

#### Files to edit
```
benchmarks/tests/test_sbq_index.py (NEW) — integration: recall (with rerank) + bytes/vector + 22023 negatives + REVOKE
```

#### Deep file dependency analysis
- New test; psycopg2 + PG* env (M21 pattern); reuses `theodb_bench.recall`.

#### Deep Dives
- Seeded corpus; `theodb.sbq_knn` recall@10 (with over_fetch) ≥ 0.85 vs brute force; `theodb.sbq_bytes_per_vector(dim,1)` == ceil(dim/8 rounded to 8) and << 4·dim; negatives (bits, metric, dim) → 22023; REVOKE false.

#### Tasks
1. Fixture: connect, temp table, seeded corpus.
2. recall test (sbq_knn with rerank).
3. bytes/vector test (formula + compression vs f32).
4. 22023 negatives + REVOKE.

#### TDD
```
RED: test_sbq_knn_recall_high_with_rerank — recall@10 ≥ 0.85 at over_fetch=8, probes high
RED: test_sbq_bytes_per_vector_compression — sbq(dim,1) ≤ f32/16
RED: test_sbq_knn_bad_bits_raises_22023 / test_sbq_knn_dim_mismatch_raises_22023
RED: test_sbq_knn_revoked_from_public
GREEN: (functions from T2.1) — container green
VERIFY: pytest benchmarks/tests/test_sbq_index.py -v
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] All integration tests green — `pytest benchmarks/tests/test_sbq_index.py` exits 0 against the container.
- [ ] Negatives assert exact `22023` (psycopg2 `pgcode`).

#### DoD
- [ ] `pytest benchmarks/tests/test_sbq_index.py` green on the built image.

## Failure scenarios (external I/O — Spi table reads)

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| `src_table` (Spi read) | table/column does not exist | bogus `embed_col` | typed error / `err_input` 22023, no panic |
| `src_table` rows | inconsistent vector dimension | two rows of different dim | `err_input` 22023, fail-fast before quantize |
| `src_table` rows | NULL vectors | a NULL embedding row | NULL rows SKIPPED (pgvector index semantics), no panic |
| args | bits / over_fetch out of range | bits=9, over_fetch=0 | `err_input` 22023 |
| identifier args | injection via `embed_col`/`id_col` | `e; DROP TABLE` | rejected by identifier allowlist (22023); table survives |

---

## Phase 3: recall@k + memory benchmark vs pgvectorscale SBQ

**Objective:** reproducible benchmark (mean±std ≥3 runs) comparing own SBQ recall@k + bytes/vector against pgvectorscale's SBQ (`diskann`, memory_optimized), gate the parity, record in `docs/benchmarks/`.

### T3.1 — `bench_sbq_index.py` + recall+memory gate + doc

#### Objective
`benchmarks/bench_sbq_index.py` builds a corpus, computes brute-force ground truth, measures recall@k for (a) pgvectorscale `diskann` SBQ and (b) `theodb.sbq_knn`, plus bytes/vector for both, over ≥3 runs, writes `docs/benchmarks/m22-sbq-parity.md`; a gate test asserts the recall tolerance band AND own bytes/vector ≤ pgvectorscale.

#### Why this step (action + reasoning)
1. **What this step does** — the measurement-first deliverable: the recall@k + memory parity proof (DoD).
2. **Why now** — it is the milestone's acceptance metric (Goal); reuses `theodb_bench` (Rule 9; blueprint Corner 1).

#### Evidence
`benchmarks/theodb_bench/recall.py:41,61`; M21 `bench_ann_index.py` pattern; pgvectorscale diskann DDL (`sbq/tests.rs:8-40`).

#### Files to edit
```
benchmarks/bench_sbq_index.py (NEW) — recall sweep (bits/over_fetch/probes) + memory (bytes/vector) mean±std ≥3 runs, --tolerance, --write-doc
benchmarks/tests/test_sbq_index.py — add test_recall_memory_parity_gate
docs/benchmarks/m22-sbq-parity.md (NEW) — results + methodology + migration decision
```

#### Deep file dependency analysis
- `bench_sbq_index.py`: imports `theodb_bench.recall` + db; calls own `theodb.sbq_knn`/`sbq_bytes_per_vector` and pgvectorscale `CREATE INDEX … USING diskann … (storage_layout=memory_optimized)`.
- gate test asserts own recall ≥ pgvectorscale − tol AND own bytes/vector ≤ pgvectorscale bytes/vector.

#### Deep Dives
- Corpus seeded (e.g., 5k × 128). Ground truth via `brute_force_ground_truth`.
- pgvectorscale arm: `CREATE INDEX … USING diskann (embedding vector_cosine_ops) WITH (num_neighbors=…, storage_layout=memory_optimized)`; query; recall; report quantized bytes/vector (= ceil(dim·1/8)).
- own arm: `theodb.sbq_knn(... bits=1, over_fetch=…)`; recall; `theodb.sbq_bytes_per_vector(dim,1)`.
- Sweep: bits∈{1,2}, over_fetch∈{1,4,8}, probes∈{1,8,32}; ≥3 runs mean±std.
- Gate: tol=0.05; recall own ≥ pgvectorscale − tol at a strong point AND own bytes/vector ≤ pgvectorscale; honest RETAIN_PGVECTORSCALE verdict on miss.

#### Pseudo-code / Signatures
```python
def parity(con, corpus, queries, k, sweep, runs, tol):
    gt_idx, gt_d = brute_force_ground_truth(corpus, queries, k, 'l2')
    for cfg in sweep:
        own = mean(recall_at_k(gt_d, own_sbq_knn(con, cfg)) for _ in runs)
        pg  = mean(recall_at_k(gt_d, pgvectorscale_diskann(con, cfg)) for _ in runs)
        own_bytes = sbq_bytes_per_vector(dim, cfg.bits); pg_bytes = ceil(dim*cfg.bits/8)
        assert own >= pg - tol and own_bytes <= pg_bytes   # or record RETAIN honestly
```

#### Tasks
1. Corpus/query generation + ground truth.
2. pgvectorscale diskann arm (build, recall, bytes/vector).
3. own arm (sbq_knn, recall, bytes/vector).
4. Aggregate mean±std ≥3 runs; `--write-doc`.
5. `test_recall_memory_parity_gate`.

#### TDD
```
RED: test_recall_memory_parity_gate — own recall@10 ≥ pgvectorscale − 0.05 at a strong (bits,over_fetch,probes) AND own bytes/vector ≤ pgvectorscale bytes/vector; FAIL records honest RETAIN_PGVECTORSCALE
GREEN: implement bench arms + aggregation
REFACTOR: dedupe arms behind a runner; else none
VERIFY: python benchmarks/bench_sbq_index.py --write-doc && pytest benchmarks/tests/test_sbq_index.py::test_recall_memory_parity_gate -v
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] `docs/benchmarks/m22-sbq-parity.md` exists with recall sweep + bytes/vector (mean±std ≥3 runs), methodology, repro commands, migration decision.
- [ ] `test_recall_memory_parity_gate` green (parity) OR honest RETAIN_PGVECTORSCALE verdict + ADR (anti-sunk-cost).
- [ ] Numbers carry units + methodology (`rules/analysis-golden-rule.md`; `rules/public-copy.md`).

#### DoD
- [ ] Benchmark reproducible from one command; doc + JSON written; gate green or honest fallback documented.

---

## Coverage Matrix

| # | Gap / Requirement | Task(s) | Resolution |
|---|---|---|---|
| 1 | Own SBQ quantizer (train/quantize/pack) | T1.1 | `SbqQuantizer` in `sbq.rs` |
| 2 | Hamming distance + bytes/vector formula | T1.1 | `hamming` + `bytes_per_vector` |
| 3 | Quantized ANN search (carrier + Hamming + f32 rerank) | T2.1 | `sbq::knn` reusing M21 IVFFlat + M20 kernel |
| 4 | SQL surface + Spi + validation + REVOKE | T2.1 | `theodb.sbq_knn` / `theodb.sbq_bytes_per_vector` |
| 5 | recall@k parity measured vs pgvectorscale, reproducible | T3.1 | `bench_sbq_index.py` + `docs/benchmarks/m22-sbq-parity.md` |
| 6 | MEMORY profile measured (bytes/vector) | T1.1, T3.1 | `bytes_per_vector` formula + benchmark column |
| 7 | Anti-sunk-cost: honest ADR keeps pgvectorscale on miss | T3.1, ADR D3 | gate records RETAIN_PGVECTORSCALE on FAIL |
| 8 | Coexistence: pgvectorscale/pgvector/embed/hybrid/import untouched | T2.1 (ADR D3) | `theodb` schema funcs read `::real[]`; REVOKE |
| 9 | Reuse M20 + M21 (Rule 9); ZERO new deps; no AGPL | T1.1/T2.1 (ADR D1/D4) | `crate::vec` + `crate::ann` reused; std-only quantizer |
| 10 | Boundary validation + typed 22023 | T2.1, Failure scenarios | `err_input` on bad args/dim/identifier |

**Coverage: 10/10 gaps covered (100%)**

## Global Definition of Done

- [ ] All phases completed
- [ ] All tests passing — `cargo pgrx test --features pg17` + `pytest benchmarks/tests/test_sbq_index.py` green
- [ ] Zero lint warnings — `cargo clippy --release --features pg17 -- -D warnings`
- [ ] File-size budget respected (`sbq.rs` ≤ 500 lines or split into `sbq/`)
- [ ] CHANGELOG.md updated under `[Unreleased] § Added`
- [ ] Backward compatibility — pgvectorscale/pgvector + `theodb.embed/hybrid/import` + M21 ann unchanged (coexistence)
- [ ] Plan-specific: reproducible **recall@k + bytes/vector** benchmark vs pgvectorscale in `docs/benchmarks/m22-sbq-parity.md` (mean±std ≥3 runs) + parity-gate test green OR honest RETAIN_PGVECTORSCALE ADR (anti-sunk-cost)
- [ ] Runtime-metric proof — the recall+memory gate runs against the real container (not just compiles)
- [ ] Plan archived after `/review` READY_TO_MERGE + PR merged

## Final Phase: Integration Validation (MANDATORY)

**Objective:** validate the own SBQ quantizer + search work in a real container workload and the recall+memory gate holds.

### Execution
```
cargo pgrx test --features pg17                       # Rust quantizer + SQL #[pg_test]
cargo clippy --release --features pg17 -- -D warnings # zero warnings
docker build / run the theo-db image                  # ship the new functions
pytest benchmarks/tests/test_sbq_index.py -v          # integration + parity gate vs container
python benchmarks/bench_sbq_index.py --write-doc      # reproducible recall+memory benchmark + doc
```

### Acceptance Criteria
- [ ] Rust + container suites green — `cargo pgrx test --features pg17` exits 0 AND `pytest benchmarks/tests/test_sbq_index.py` exits 0
- [ ] Zero clippy warnings — `cargo clippy --release --features pg17 -- -D warnings` exits 0
- [ ] Recall+memory parity gate green — `pytest benchmarks/tests/test_sbq_index.py::test_recall_memory_parity_gate` exits 0 (own recall@10 ≥ pgvectorscale − tol AND own bytes/vector ≤ pgvectorscale) OR `docs/benchmarks/m22-sbq-parity.md` records a RETAIN_PGVECTORSCALE verdict + an anti-sunk-cost ADR
- [ ] Failure scenarios exercised — `pytest benchmarks/tests/test_sbq_index.py` covers the 22023 negatives, NULL skip, and injection-rejection rows
- [ ] Benchmark doc written — `docs/benchmarks/m22-sbq-parity.md` exists with methodology + exact repro commands

### If Validation Fails
1. Separate plan-caused failures from pre-existing.
2. Fix all plan-caused failures; re-run the chain.
3. If recall/memory parity genuinely cannot be reached, record the honest anti-sunk-cost verdict (keep pgvectorscale) — the milestone ships the *measurement* (DoD), a PASS for M22, not a failure.
