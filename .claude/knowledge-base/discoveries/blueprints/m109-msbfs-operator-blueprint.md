---
slug: m109-msbfs-operator
milestone_id: M109
date: 2026-07-16
cycle: discover
verdict: SHIPPABLE_WITH_CAVEATS
---

# M109 Blueprint — Vectorized Multi-Source BFS operator over persisted CSR

Deep-research dossier (R0-compliant: every load-bearing claim resolves to a primary source
fetched live from an authoritative domain — VLDB, CIDR, arXiv, verified-license OSS repos).
Two independent council agents (research-adr + performance-simd) converged on the findings.

## Ground truth (our real code, shipped M108)

- `theodb_rs/src/graph.rs` — `struct Csr { nnodes, offsets: Vec<u64>, adj: Vec<u32> }` (undirected,
  dense `u32` ids). Single-source `Csr::expand(seeds, max_hops)` (L91–126): frontier BFS + vertex
  `visited` bitset (L93–95), bounded ≤H hops, returns reachable node set. Set-hash oracle
  `bit_xor(hashint8(node))` (L425–434). Per-backend `CSR_CACHE: Rc<Csr>` keyed by `built_at` (L22–24).
- `theodb_rs/src/vec/ah.rs` — `ah_score_block32` (L204–238) `pshufb` AVX2 kernel: int8-LUT byte-lane
  scoring + int16 arithmetic reduction. Runtime dispatch `is_x86_feature_detected!` (L144–157);
  scalar oracle L119/L306; release-asserts L276–294; measured-scalar-wins lesson L269–274.

## Coverage Corner 1 — Integration Tests

- **Per-source set-hash decomposition oracle (primary gate).** MS-BFS is *exactly* N single-source
  BFSs sharing edge traversal, so for each seed-set `s` in the batch, the MS-BFS reachable set for
  lane `s` MUST byte-for-byte equal `expand([s], hops)` (M108). Reuse `bit_xor(hashint8(node))`
  per-lane. This catches word-boundary, tail-padding, and axis-conflation bugs (a leaked padding bit
  changes the set → hash diverges). RED first, before the kernel.
- **Differential vs recursive-CTE baseline** (ROADMAP GATE): same reachable set per trial vs the SQL
  `WITH RECURSIVE` UNION-dedup baseline (the M108 baseline formulation).
- **Bounded ≤H semantics identical to theo-rag** (`graph-retriever.ts`): H=1,2,3 differential tests.
- **Edge cases:** empty seed-set lane; seed ≥ nnodes (skip, as M108); single-seed batch (must equal
  `expand`); W-boundary batch (exactly 64 seed-sets); >64 seed-sets (tiling correctness).
- **Negative:** malformed seed_sets (non-array element) → typed error at the boundary, not panic.

## Coverage Corner 2 — Dependencies

- **No new crate.** `Vec<u64>` masks + plain Rust bitwise ops. Auto-vectorization is the compiler's
  job (LLVM emits `vpor`), per DuckPGQ VLDB'23. Parsimony rung 4: reuse installed toolchain only.
- Reuses M108 `CSR_CACHE` + `theodb.graph_csr` catalog (no re-deserialize per tile).
- pgrx `#[pg_extern]` + `extension_sql!` wrapper pattern (identical to M108).

## Coverage Corner 3 — Tools

- `cargo pgrx test pg17` on droplet `theo-m108-pgrx19` (64.226.91.7) — same harness as M108.
- Benchmark: criterion-style timing inside a pg_test writing `docs/benchmarks/m109-*.json`
  (N-seeds-parallel MS-BFS vs N sequential `expand`; ≥3 runs mean±std). `council-benchmark` audits.
- Set-hash oracle via SQL `bit_xor(hashint8(...))` (M108 pattern).

## Coverage Corner 4 — Techniques (SOTA)

- **MS-BFS core (Then et al., VLDB 2014, PVLDB 8(4), https://www.vldb.org/pvldb/vol8/p449-then.pdf).**
  Per-vertex W-bit fields `seen`/`visit`/`visitNext`; level step `D = visit[v] & ~seen[n];
  visitNext[n] |= D; seen[n] |= D`. Set-union→`|`, set-diff→`&~`. Measured 12.1–88.5× vs textbook/DO
  BFS **for millions-of-sources closeness centrality** (Table 4). **Aggregated Neighbor Processing**
  (§4.1.1): split level into Pass-1 pure-OR push + Pass-2 single `seen` sweep (distributive law);
  ANP alone +60–110%.
- **Batch width = cache line = 512 (Then §4.2.1; DuckPGQ `LANE_LIMIT=512`, MIT).** "bit fields exactly
  sized to a cache line … 512 sources per run." DuckPGQ VLDB'23 (https://www.vldb.org/pvldb/vol16/p4034-wolde.pdf):
  *"a single bit suffices … does not require assembly or intrinsics, well-coded MS-BFS can be
  auto-vectorized by C++ compilers."*
- **Direction-optimizing / bottom-up (Beamer SC'12, GAP arXiv:1508.03619, BSD).** Composes with MS-BFS
  (Then §4.1.2, +30%) but DuckPGQ ships **uni-directional** and still beats Neo4j. Rejected for M109
  (bounded ≤H + few seeds never saturates the frontier → α threshold never crossed). ADR-recorded.
- **OSS templates (permissive):** DuckPGQ `iterativelength.cpp` (MIT — the exact bitset frontier to
  mirror: `next[n] |= visit[i]; next[i] &= ~seen[i]; seen[i] |= next[i]`, double-buffered `iter&1`
  swap), GAP `bfs.cc` (BSD), NetworkX `bfs_layers` (BSD — correctness oracle).

## ADRs

### ADR-1 — "reuse vec/ah.rs kernels" is a misframing → reuse the *discipline*, not the kernel

**Decision.** The MS-BFS bitwise core is **auto-vectorized plain safe Rust** over `Vec<u64>` source
masks (`next[nbr] |= visit[u]`), NOT a call into `ah_score_block`. Reuse from `vec/ah.rs`: (a) the
`is_x86_feature_detected!` runtime-dispatch pattern *if* explicit SIMD is ever needed, (b) the
scalar-oracle-first discipline, (c) release-mode length asserts around any `unsafe`.
**Rationale.** `pshufb` = int8-LUT gather + arithmetic reduction across the *candidate* dimension
(distance scoring). MS-BFS = bitwise-OR mask propagation across the *source* dimension. Orthogonal
mechanisms; `pshufb` cannot express bitwise-OR and bitwise-OR needs no `pshufb`. Two agents + DuckPGQ
VLDB'23 confirm. **Rejected alternative:** bending the int8-LUT kernel into a reachability-OR — sends
the implementer down a wrong path. The ROADMAP DoD line "reusando os kernels SIMD vec/ah.rs" is
honestly satisfied by reusing the *discipline + conceptual layout*, with the kernel rebuilt.

### ADR-2 — Scalar u64-mask sweep for W≤64; auto-vectorized `[u64;K]` for W>64; NO hand-rolled intrinsics

**Decision.** v1 ships **W=64 (single `u64` per vertex), scalar `|=`** — already a 64× batching over
M108's single-source `expand`. Widen to `[u64; K]` (compiler auto-vectorizes to `vpor`) only on
*measured* lane-starvation. **Rationale.** MS-BFS frontier expansion is **memory-bound** (irregular
gather over `adj` + scatter to `next[nbr]`); explicit AVX2/AVX512 buys nothing on the critical path.
Mirrors `ah.rs`'s own measured lesson (scalar shipped over per-candidate SIMD, L269–274) and DuckPGQ
VLDB'23 (auto-vectorization, no intrinsics). **Rejected:** hand-written AVX-512 `vpternlogq` v1 —
premature (YAGNI/KISS), unmeasured, and the gather dominates. Measure edges-traversed-per-hop vs
wall-clock to *prove* memory-bound before any intrinsic (M35 pages-vs-wallclock discipline).

### ADR-3 — M109 is a distinct BATCHED operator, measured honestly; single-source path keeps `expand`

**Decision.** New surface `graph_expand_multi(edge_rel, seed_sets, max_hops)` returning per-seed-set
reachable sets. `expand` (single-source) is unchanged. **Rationale.** MS-BFS's win is *super-linear in
#sources sharing edges* and collapses to ~1× (plus overhead) at a single source. The 12–88× SOTA
numbers are for millions-of-sources workloads and are **NOT portable** to GraphRAG's few-seed regime —
they are `UNBENCHMARKED` for TheoDB and MUST be measured. The M109 measurement gate is precisely this:
benchmark batched-MS-BFS vs looping `expand`, report the *honest measured* N-seeds gain. If a real
caller (theo-rag batched neighborhood-expansion for reranking) batches dozens of seed-sets, the
amortization pays; if not, the honest-negative informs whether theo-rag should batch. **Rejected:**
replacing `expand` with MS-BFS (regresses the single-seed path). **Rejected:** claiming the SOTA
speedup without our own benchmark (Rule 5 / public-copy.md).

### ADR-4 — Top-down ANP only; bottom-up is a recorded rejected alternative

**Decision.** Ship top-down Aggregated-Neighbor-Processing MS-BFS (Pass-1 pure-OR push, Pass-2 `seen`
sweep). No bottom-up/direction-optimizing switch. **Rationale.** Bounded ≤H (H=1–3) + few seeds → the
frontier never saturates → Beamer's α threshold never triggers; bottom-up needs the in-edge CSR (free
for undirected, but unused work now). DuckPGQ ships uni-directional and still wins. **Rejected
alternative (recorded):** direction-optimizing (Beamer SC'12) — revisit only if a benchmark shows a
saturating-frontier regime.

## Honest caveats / UNBENCHMARKED markers

- TheoDB's batched-MS-BFS speedup vs looping `expand` is **UNBENCHMARKED** — the M109 gate measures it.
- The 12–88× literature numbers are millions-of-sources closeness centrality — **not** our regime.
- If no caller batches ≥ dozens of seed-sets, M109 risks failing YAGNI rung-1; the measurement gate is
  the honest arbiter. Building the primitive + benchmark IS the roadmap-authorized way to find out.

## Recommended design (hand-off to /to-plan)

Layout parallel to CSR (dense `u32` ids, no renumber):
```
visit, visit_next, seen : Vec<u64>  // len nnodes; bit i = seed-set i
```
Level step (top-down ANP, mirror DuckPGQ `IterativeLength`):
```
Pass1: for v where visit[v]!=0 { for nbr in adj[off[v]..off[v+1]] { visit_next[nbr] |= visit[v] } }
Pass2: for v where visit_next[v]!=0 { visit_next[v] &= !seen[v]; seen[v] |= visit_next[v] }
       swap(visit, visit_next); zero(visit_next)
Run exactly max_hops levels (or until visit all-zero).
```
Tiling: >64 seed-sets → outer loop over 64-lane tiles, same cached `Rc<Csr>`; `lane_to_set: [id;64]`.
Extract: scan `seen`; per set bit i, emit vertex to seed-set i's result. Per-lane set-hash == `expand`.
