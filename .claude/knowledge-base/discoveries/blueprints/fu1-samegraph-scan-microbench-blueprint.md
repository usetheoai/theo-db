# Blueprint: FU-1 — same-graph, box-noise-immune micro-benchmark for the HNSW scan allocation change

## Context

M46 shipped a recall-neutral allocation change to the on-disk HNSW scan (`theodb_rs/src/am/hnsw_page.rs::traverse`
— pre-size the three per-query structures + reuse one neighbor scratch). Its QPS benefit was **not measurable**
(`docs/benchmarks/m46-highrecall-qps.md`): the two-container A/B was confounded by dev-box contention (the
pgvector control drifted +122%) AND by M44 parallel-build graph differences (`ann/hnsw.rs:34`). FU-1 needs a
**same-graph, box-noise-immune** harness. This blueprint distills the design from the Rust ANN prior art —
pgvectorscale (single pgrx crate, `criterion` benches a re-implemented copy) and vectorchord (modular pure
crates, benches the real code) — and specifies exactly what theodb builds.

## Objective

Specify a criterion micro-benchmark that isolates the M46 allocation change (pre-size vs `::new()`) over a
**byte-identical fixed graph**, exercising **production** ground-loop code via a DIP seam (`NeighborSource`), so
the latency delta is attributable solely to the allocation strategy — immune to box noise (same-process
interleaved measurement) and build nondeterminism (one seeded graph).

## Coverage Corner 1 — Integration Tests

**Q7 — how the references guard the bench against measuring a DIFFERENT path than production.**
pgvectorscale does **NOT** guard: `benches/lsr.rs:38-64` re-implements `ListSearchResult` (candidate storage) as
a standalone copy, while the production one lives in `references/pgvectorscale/pgvectorscale/src/access_method/graph/mod.rs`.
There is no test asserting the two are equivalent — a **known divergence hazard** (the bench can pass while the
production search regresses). This is the anti-pattern theodb must NOT repeat.

**theodb's guard (improves on the SOTA peer):** the benched code path IS the production ground-loop (the DIP
extraction — Corner 4), so there is no copy to drift. The equivalence is additionally locked by **reusing M46's
recall-neutral oracle**: an integration test (`traverse_presize_is_recall_neutral_end_to_end`,
`theodb_rs/src/am/hnsw_page.rs`) already asserts the PG-backed path returns the exact-NN order. A NEW pure-side
test asserts the in-memory `NeighborSource` path (`ground_search` over the seeded `HnswIndex`) returns the SAME
visit order + result as the in-memory oracle (`ann/hnsw.rs::search` / brute-force `brute()` in `ann/mod.rs`
tests). Both paths call the SAME `ground_search` → equivalence is structural, not asserted-by-copy.

**Q8 — criterion statistical rigor.** pgvectorscale uses criterion's DEFAULT config: `c.bench_function("lsr OG",
|b| b.iter(|| run_lsr(black_box(&mut lsr))))` (`benches/lsr.rs:169`) with two named functions
(`benchmark_lsr` vs `benchmark_lsr_min_heap`, `:156,:172`) grouped via `criterion_group!` (`:190`). Default
criterion = 100 samples, 3s warm-up + 5s measurement per function, bootstrapped 95% CI, outlier classification.
**theodb adopts:** two `bench_function`s in one group — `"ground_search/presized"` vs `"ground_search/unsized"`
— over the IDENTICAL seeded graph + query set, at an ef sweep (100/200/400). criterion reports each with its CI;
the delta is trustworthy when the CIs do not overlap (the same-process interleaving makes steady-state box noise
common-mode). This is strictly more rigorous than the two-container A/B (no cross-process, no build race).

## Coverage Corner 2 — Dependencies

**Q6 — criterion version + footprint.** pgvectorscale pins `criterion = "0.5.1"` under `[dev-dependencies]`
(`references/pgvectorscale/pgvectorscale/Cargo.toml`, alongside `pgrx-tests`, `tempfile`); vectorchord's pure
`crates/simd` pins `criterion = "0.8.2"` (`references/vectorchord/crates/simd/Cargo.toml`). **theodb adds
`criterion` to `theodb_rs/Cargo.toml` `[dev-dependencies]` only** (the section already exists, holding
`pgrx-tests`) — **zero cdylib impact** (dev-deps never link into the released extension; parsimony rung 4 — a
dev-only test/bench dep is the minimal footprint, no runtime dependency added). Pin `criterion = "0.5.1"` to
match theodb's pgrx-0.16.1 / Rust-1.91 toolchain (same as pgvectorscale, the same-stack peer — D1); 0.8.2 is
vectorchord's newer pin but 0.5.1 is the proven-compatible choice for this exact pgrx version.

## Coverage Corner 3 — Tools

**Q4 — criterion wiring in a pgrx crate WITHOUT a running Postgres.** Both peers use
`[[bench]]` with `harness = false`: pgvectorscale `Cargo.toml` declares `[[bench]] name="lsr" harness=false` (and
`name="distance"`); vectorchord `crates/simd/Cargo.toml` declares `[[bench]] name="bench" harness=false
required-features=["internal"]`. **theodb copies:**
```toml
[[bench]]
name = "scan_hot_path"
harness = false
```
placed in `theodb_rs/Cargo.toml`, with the bench file at `theodb_rs/benches/scan_hot_path.rs`. `harness=false`
hands `main()` to criterion (via `criterion_main!`) instead of libtest.

**Q5 — avoiding pg_sys / cdylib symbols at bench link time (the load-bearing constraint).** pgvectorscale's
`benches/lsr.rs:1-7` imports ONLY `std` + `criterion` + `rand` — NO `use pgvectorscale`, NO `pg_sys` — which is
exactly WHY it re-implements the struct (a single pgrx crate cannot easily bench its pg-coupled modules; the
extern-"C" pg symbols are unresolved at bench link time). vectorchord sidesteps this by putting the benched logic
in a **pure crate** (`crates/simd` has NO pgrx dependency). **theodb's resolution (the key architectural
decision — Corner 4 / D1):** extract the ground-loop into the ALREADY-PURE `ann/` domain module (which compiles
with no `pg_sys` — `ann/hnsw.rs` is pure Rust). The bench then imports `theodb_rs::ann::scan_core::ground_search`
+ an in-memory `NeighborSource` — pulling only pg-free code → links cleanly, no pg runtime. This is the
vectorchord pattern applied WITHIN the single crate via the layer boundary, and it satisfies `architecture.md §1`
(domain has no infrastructure dependency).

## Coverage Corner 4 — Techniques

**Q1 — how pgvectorscale isolates the search for benching.** By RE-IMPLEMENTATION: `benches/lsr.rs:38-118`
defines a standalone `ListSearchResult` (`candidate_storage: Vec`, `best_candidate: Vec<usize>`) + `insert_neighbor`/
`visit_closest` that MIRROR the production ones in `src/access_method/graph/mod.rs`, driven by synthetic
`rand::thread_rng()` data (`:124,:142`). Benches two candidate-management strategies (`benchmark_lsr` OG-Vec vs
`benchmark_lsr_min_heap`, `:156/:172`). **Divergence risk + non-determinism** (`thread_rng` is unseeded → each
run different data; acceptable for their coarse strategy comparison, NOT for a same-graph attribution). theodb
improves on both axes: real code (no copy) + a SEEDED graph (deterministic same-graph).

**Q2 — the storage-access trait seam + OUR extraction shape (EC-1).** pgvectorscale's production search depends
on the `Storage` trait (`src/access_method/storage.rs:41`, with associated `NodeDistanceMeasure`
(`:21`), `ArchivedData` (`:30`), `LSNPrivateData` (`:51`), `get_node_distance_measure` (`:72`)) — a rich seam
abstracting SBQ/plain storage + distance. That is the SOTA pattern: **the search is generic over a storage
trait; a bench can implement it in memory.** For theodb the seam is MINIMAL (KISS — we only bench the ground
loop, not full storage). Reading OUR `traverse` (`theodb_rs/src/am/hnsw_page.rs`): the ground loop needs exactly
two operations per expanded node — (a) read its neighbor addrs (`neighbors_into` → page), and (b) load+score a
neighbor (`load` → page + SIMD distance). So the extracted trait is:

```rust
// ann/scan_core.rs (pure domain — no pg_sys)
pub(crate) trait NeighborSource {
    /// Append the neighbor node-ids of `node` on the ground layer into `out` (cleared first). L1-B scratch reuse.
    fn neighbors_into(&self, node: u32, out: &mut Vec<u32>) -> Result<(), String>;
    /// Distance from the query to `node`'s vector (the SIMD score). L2/cosine per the built metric.
    fn distance(&self, node: u32) -> f64;
}
pub(crate) fn ground_search<S: NeighborSource>(
    src: &S, entry: u32, entry_dist: f64, ef: usize, m0: usize, presize: bool,
) -> Result<Vec<(u32, f64)>, String> { /* the M46 ground loop, presize toggles with_capacity vs ::new() */ }
```
- **Production adapter** (`am/hnsw_page.rs`): a `PageNeighborSource { rel, nblocks, … }` implementing the trait
  via `with_page_item` (wrapping the existing `neighbors_into`/`load`). `traverse` calls `ground_search(&pg_src,
  …, presize=true)`. Node-ids map to `(blk,off)` via the existing analytic addressing — the adapter holds that.
- **Bench + test in-memory adapter**: a `MemNeighborSource { idx: &HnswIndex, query }` implementing the trait via
  `idx.node_neighbors(node, 0)` + `idx.metric().dist(query, idx.node_vector(node))` — all pure (`ann/hnsw.rs`
  already exposes these: `node_neighbors`, `node_vector`, `node_level`, `entry`).
- `presize: bool` is the ONLY axis the bench flips (pre-size the 3 structures + scratch, vs `::new()`) → the
  criterion delta is purely the M46 allocation change. `Cand`/`Addr` are pure types (no pg_sys) → the extraction
  is pg-free (Q5).

**Q3 — the fixed-graph fixture (EC-3 scale).** pgvectorscale uses unseeded `thread_rng()` (Q1) — NOT
reproducible. theodb uses a **seeded deterministic fixture**: `HnswIndex::build(&corpus, m=16, ef_c=64,
Metric::L2, seed=42)` where `corpus` is a seeded pseudo-random set (the existing `ann/mod.rs` test RNG /
`rand_corpus(seed)`), producing a byte-identical graph every run (the in-memory build is deterministic given the
seed — `ann/mod.rs` has `hnsw_deterministic_same_seed`). **Scale (EC-3):** N must be large enough that
`ef*m0` makes pre-sizing matter. With `m0 = 2*m = 32` and ef=200, the `visited` HashSet target is `ef*m0*2 =
12,800` slots → the default `::new()` HashSet rehashes ~log2(12800/ (7/8·capacity steps)) ≈ 10-12× over the
search. So **N ≥ 50,000** (SIFT-representative; the graph must have enough nodes that an ef=200 search visits
thousands of distinct nodes). The fixture builds ONCE (outside the criterion timing loop) and is shared by both
bench functions → same graph, zero build cost in the measurement.

## Cross-cutting Comparison

| Dimension | pgvectorscale (same stack) | vectorchord (modular) | theodb FU-1 (chosen) |
|---|---|---|---|
| Bench isolation | re-implemented copy | pure crate, real code | real code via `ann/` domain extraction (no copy) |
| Divergence guard | NONE (hazard) | N/A (real code) | M46 recall-neutral oracle + shared `ground_search` |
| Fixture determinism | `thread_rng` (unseeded) | per-bench | seeded `HnswIndex::build(seed=42)`, same graph both fns |
| criterion | 0.5.1, `harness=false`, default cfg | 0.8.2, `required-features` | 0.5.1 dev-dep, `harness=false`, 2 fns/group + CI |
| pg_sys at link | avoided by copy | avoided by pure crate | avoided by `ann/` domain layer (no pg_sys) |
| What it measures | strategy A vs B (coarse) | SIMD kernels | pre-size vs `::new()` (the exact M46 axis) |

**Honesty caveat (EC-2):** the micro-bench has NO page I/O — so the allocation cost is a LARGER share of the
measured time than in production (where page reads via `with_page_item` amortize it). The criterion delta is
therefore an **UPPER bound** on the production QPS benefit — it proves + quantifies "the allocation cost the M46
change removes", NOT the end-to-end production speedup. The blueprint's recommendation pairs it with the
(already-shipped) SQL recall-neutral proof; a production QPS number still requires a quiet-box SQL run, but the
micro-bench is the box-noise-immune, same-graph attribution FU-1 was created to provide.

## ADRs

### D1 — Extract `ground_search` into the pure `ann/` domain layer (not a bench copy)
**Decision:** put the ground-loop + `NeighborSource` trait in `ann/scan_core.rs` (pg-free); production
`am/hnsw_page.rs::traverse` and the bench both call it. **Rationale:** vectorchord proves pure-crate/pure-module
benching avoids the pg_sys-link problem AND the divergence hazard that pgvectorscale's copy carries (Q1/Q5/Q7);
`architecture.md §1` mandates the domain not depend on infrastructure — this extraction IS that boundary.
**Alternatives rejected:** (a) pgvectorscale-style bench copy — divergence risk, needs a separate equivalence
test to be safe (D3 fallback only); (b) bench the pg-coupled `traverse` directly — fails to link without a pg
runtime (Q5).

### D2 — Seeded deterministic fixture, one graph shared by both bench functions
**Decision:** `HnswIndex::build(seed=42)` at N≥50k, built once outside timing, shared by presized/unsized fns.
**Rationale:** the entire point of FU-1 is same-graph attribution; pgvectorscale's `thread_rng` (Q3) cannot
provide it. Determinism is proven by `ann/mod.rs::hnsw_deterministic_same_seed`. **Alternative rejected:**
snapshot/restore a PG index (heavier, reintroduces the storage-format coupling the `ann/` extraction avoids).

### D3 — Equivalence guard is mandatory (structural + oracle)
**Decision:** the bench path (`ground_search` over `MemNeighborSource`) MUST be covered by a test asserting it
returns the same visit order/result as the in-memory oracle (`brute()` exact kNN at 100% recall on the fixture),
AND the production path keeps M46's `traverse_presize_is_recall_neutral_end_to_end`. **Rationale:** even with a
shared function, a wrong `MemNeighborSource` mapping (node-id↔vector) could make the bench measure a bogus graph;
the oracle catches it (`testing.md §4.1` — the equivalence is the correctness contract). **Alternative
rejected:** trusting the shared function without an oracle (pgvectorscale's unguarded gap — Q7).

## Recommendations

1. **Implement D1**: `ann/scan_core.rs` with `NeighborSource` + `ground_search<S>(…, presize: bool)`; refactor
   `am/hnsw_page.rs::traverse` to call it via a `PageNeighborSource` adapter (recall-neutral — reuse M46 oracle).
2. **Add** `criterion = "0.5.1"` to `theodb_rs/Cargo.toml` `[dev-dependencies]` + `[[bench]] name="scan_hot_path"
   harness=false`; bench at `theodb_rs/benches/scan_hot_path.rs`.
3. **Fixture**: seeded `HnswIndex::build(seed=42)`, N≥50k, dim 128; build once; bench `presized` vs `unsized` at
   ef ∈ {100,200,400}; report criterion CIs.
4. **Guard**: a pure test asserting `ground_search`/`MemNeighborSource` == `brute()` exact kNN on the fixture;
   keep the M46 PG-side oracle.
5. **Report** the delta with the EC-2 caveat (upper bound on production benefit) into `docs/benchmarks/`.
6. **Validate** the bench binary links without a pg runtime (the Q5 risk) — first implement step; if it fails,
   fall back to D3's guarded-copy, but D1's `ann/` extraction should make it a non-issue.
