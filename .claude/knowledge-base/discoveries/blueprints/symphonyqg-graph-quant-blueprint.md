---
slug: symphonyqg-graph-quant
date: 2026-07-17
kind: discovery-blueprint
status: SHIPPABLE_WITH_CAVEATS (design mapped; gain UNMEASURED — spike gates the build)
sources:
  - arXiv:2411.12229 (SymphonyQG, SIGMOD'25) — paper, freely studiable (algorithm/math)
  - knowledge-base/references/SymphonyQG (VectorDB-NTU) — NTUITIVE non-commercial license, STUDY-ONLY (D1)
  - knowledge-base/references/RaBitQ-Library (Apache-2.0) — permissive FastScan+RaBitQ machinery
  - Our own permissive assets: theodb_rs/src/vec/rabitq.rs, vec/ah.rs, ann/hnsw.rs
---

# SymphonyQG — quantization co-located in the graph (E2 discovery blueprint)

## § 0 — License gate (D1, INVIOLABLE — read first)

`VectorDB-NTU/SymphonyQG` is under the **NTUITIVE License — Non-Commercial Use Only** (proprietary, NOT
open-source; stricter than AGPL). Per TheoDB D1 (`CLAUDE.md`) and the [[vectorchord-agpl-study-only]] precedent:
**the C++ is STUDY-ONLY — never copied, never transcribed, never vendored into the distribution.** The cloned
tree lives in the gitignored `references/` study zone. The E2 implementation, if it happens, is a **clean-room
own-code reimplementation from the paper** (algorithms/math are not copyrightable), exactly as `vec/rabitq.rs` was
reimplemented from arXiv:2409.09913 after the vendored RaBitQ tree was deleted (ADR-0046).

## § 1 — Why this is the right lever (ties to the E1 verdict)

E1 (`docs/benchmarks/e1-rabitq-inpg-verdict.md`) MEASURED that warm in-RAM vector QPS is **not** bound by Stage-2
refinement — it is bound by **Stage-1** (scanning the probed lists / traversing the graph: random memory access +
exact-distance costs). SymphonyQG attacks exactly that: it folds the quantized distance estimate INTO the graph
traversal so the search never does exact distances (except the popped center) and never random-accesses raw
vectors for a neighbor. This is the one structural change that could move the warm number E1 could not.

## § 2 — The design (Coverage Corner: Techniques) — clean-room target

### Data layout (confirmed in `symqglib/qg/qg.hpp`, studied not copied)
Each vertex is ONE contiguous row (`row_offset_` bytes), `degree_bound R` a multiple of 32:
```
[ RawData (f32 vector) | QuantizationCodes (R neighbors, FastScan block-32 transposed) |
  Factors (per neighbor: triple_x, factor_dq, factor_vq — 3×f32) | neighborIDs (R × PID) ]
```
- Neighbor codes are **replicated** at every vertex that points to them → space `n·(32D + 32R + DR) bits`;
  at R=32 the code storage ≈ 1× the raw-vector overhead, at R=64 ≈ 2×.
- Uses **1-bit RaBitQ** (base, D-bit strings) + Fast Johnson–Lindenstrauss rotation `O(D log D)` (not our extended
  multi-bit; the FastScan 4-bit LUT accumulates the 1-bit codes over 32 lanes).

### Query (Algorithm 1 — beam search, no explicit rerank)
```
beam S ← {entry}; NN ← {}
while ∃ unvisited p ∈ S:
    p ← unvisited vertex in S with smallest ESTIMATED distance
    mark p visited; compute p's EXACT distance (this is ‖q_r − c‖, reused as the normalizer)
    update NN with p's exact distance
    FastScan-estimate ALL R neighbors of p in ONE batched kernel   ← the hot op
    push unvisited neighbors + estimated distances into S; prune S to beam_size
return NN
```
Estimator (relative to the popped center c):
`est[i] = (1/‖q_r − c‖)·(⟨x̄_i, P⁻¹q_r⟩ − ⟨x̄_i, P⁻¹c⟩)`, with `⟨x̄_i, P⁻¹c⟩` precomputed into the Factors.
The exact distance of a vertex (computed once when popped, as the center) doubles as its own refinement → **no
separate rerank pass**.

### Build (NSG-style, not HNSW)
Random init → iterative refinement (t=3–4) using Algorithm 1 with beam E_F for candidates → NSG angle-based
pruning (out-edges ≥ 60° apart) → **degree alignment**: supplement edges (binary-search the angle threshold)
until every vertex has exactly R edges → pack the FastScan layout + precompute Factors.

### Reported gains (standalone C++, NO PG MVCC/WAL/heap tax)
1.5–4.5× QPS vs NGT-QG, **3.5–17× vs HNSWlib @ 95% recall (K=10)** on SIFT/GIST/Deep-1M; indexing 8–18× faster
than NGT-QG. NGT-QG is unstable on some sets (PQ failure); SymphonyQG is not (RaBitQ has no such failure mode).

## § 3 — What we reuse vs what is new (Coverage Corner: Dependencies)

| Piece | Have it? | Source |
|---|---|---|
| RaBitQ 1-bit code + estimator | **Yes** (multi-bit; 1-bit is `bits=1`) | `vec/rabitq.rs` (E1) |
| FastScan block-32 batched LUT (pshufb) | **Yes** | `vec/ah.rs` `ah_score_block`, `build_lut16` |
| Graph build + beam search | **Partial** (HNSW, not NSG) | `ann/hnsw.rs`, `hnsw_parallel.rs` |
| Per-vertex co-located row layout (codes+factors+IDs) | **New** | — |
| NSG angle-pruning + degree-alignment to R | **New** | — |
| Fast JL rotation `O(D log D)` | **New** (we have `O(D²)` dense rotate) | `vec/rabitq.rs::rotate` |
| RaBitQ-relative-to-center factor precompute | **New** | — |

## § 4 — Honest gap & the in-PG risk (why "se tivermos ganho" needs a spike)

- The 3.5–17× is a **standalone mmap'd C++** number. In-PG, each graph hop is a **random page read** (the vertex
  row lives on a heap/index page) — the PG tax the C++ avoids. The co-located FastScan over 32 neighbors is WITHIN
  that one page (good), but the per-hop page-fault pattern is the open question.
- Our own M73/M82 verdict: the ScaNN/AlloyDB warm-QPS gap (25–44×) is **paradigm** (MVCC/WAL). SymphonyQG will NOT
  make us beat ScaNN. The realistic prize is **our best in-PG vector engine** (beat our own HNSW/IVF-AQ at matched
  recall), not SOTA-warm-superiority. Honesty (Rule 3): this is an internal-improvement bet, not a North-Star flip.
- Index size grows (replicated codes, 1–2× raw-vector overhead) — the OPPOSITE of E1's memory win. Trade-off must
  be measured, not assumed.

## § 5 — The GATE: measured spike BEFORE any in-PG build (anti-sunk-cost)

**Do NOT build the in-PG AM first.** Build a hermetic own-code spike (off-PG Rust, like the RaBitQ Monte-Carlo
validation) and MEASURE on real SIFT1M:

1. Own-code `symqg` spike: NSG-lite graph (or reuse HNSW adjacency as a first cut) + per-vertex co-located 1-bit
   RaBitQ FastScan blocks (reuse `ah_score_block` + `rabitq.rs bits=1`) + beam search with FastScan neighbor
   estimation + no rerank.
2. A/B on SIFT1M (in-memory, single-thread, same machine): **QPS @ recall 0.95 and 0.99** vs (a) our HNSW own-code,
   (b) our IVF-AQ v8. Report recall@10 vs official GT + build time + index bytes.
3. **GO gate:** spike QPS @0.95 ≥ **1.5×** our best own-code engine at matched recall, off-PG. Only then design the
   in-PG AM (where the page tax is measured next). **NO-GO:** if the co-located graph does not beat our HNSW
   off-PG, the in-PG version certainly won't — stop and record the honest negative (like M74's 1-bit spike).

## § 6 — ADRs

- **ADR-E2-1 — Clean-room from paper, never transcribe the NTUITIVE code (D1).** Alternative rejected: "translate
  the C++ to Rust" — violates D1 (non-commercial proprietary → cannot ship a derivative). The paper + our
  permissive assets are sufficient.
- **ADR-E2-2 — Spike off-PG first, gate on 1.5× before the in-PG AM.** Alternative rejected: "build the in-PG AM
  directly" — sunk-cost risk; E1/M74 show the honest-negative is cheap only if measured early.
- **ADR-E2-3 — 1-bit RaBitQ + FastScan (per the paper), not our extended multi-bit.** Alternative (multi-bit in
  the graph) deferred — the paper's whole point is 1-bit is enough WHEN the graph refines the ranking; test the
  paper's design first, only enrich bits if recall falls short.

## § 7 — Coverage corners (rigor profile)
- **Techniques:** § 2 (layout + query + build) — from paper + code study. ✓
- **Dependencies:** § 3 (reuse map). ✓
- **Tools:** the spike uses our existing `cargo test`/criterion harness + the SIFT1M droplet recipe from E1
  (`benchmarks/e1_*.py` pattern). ✓
- **Integration tests:** deferred to the in-PG phase (ADR-E2-2) — the spike is hermetic; in-PG integration tests
  come only if the GO gate passes. ✓ (explicit deferral)
