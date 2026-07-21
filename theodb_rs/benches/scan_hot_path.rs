//! FU-1 — same-graph, box-noise-immune micro-benchmark of the HNSW ground-loop allocation change (M46).
//!
//! Measures the SOLE axis M46 flipped: pre-sizing the three per-query structures + reusing one neighbor scratch
//! (`presize = true`) vs `::new()` (`presize = false`), over a SINGLE fixed graph built once and shared by both
//! bench functions — so the delta is purely the allocation strategy (same-graph within-run; blueprint D2, EC-1).
//!
//! It `#[path]`-includes the PURE `ann/scan_core.rs` (trait + `ground_search`, zero `crate::`/`pg_sys` refs) so
//! the bench links WITHOUT a PostgreSQL runtime — the resolution of blueprint Q5 (a pgrx crate's `#[no_mangle]`
//! exports would otherwise force unresolved pg symbols at bench link time; pgvectorscale sidesteps this with a
//! divergent copy, we sidestep it by benching the REAL pure `ground_search`).
//!
//! Honesty (EC-2): a no-page-I/O micro-bench magnifies the allocation share vs production (where page reads
//! amortize it), so the delta is an UPPER BOUND on the production QPS benefit. The graph is synthetic (random
//! regular graph + random vectors) — representative for the ALLOCATION workload (the cost scales with ef, node
//! degree, and visit count, all reproduced), NOT for recall (correctness is proven separately on a REAL
//! `HnswIndex` by `ground_search_matches_brute_exact_knn` in `ann/scan_core.rs`).

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};

#[path = "../src/ann/scan_core.rs"]
mod scan_core;

use scan_core::{NeighborSource, ground_search};

/// A fixed synthetic navigable graph: `n` nodes, `m0` neighbors each, `dim`-d vectors. Built once (seeded), then
/// shared by the presized + unsized bench functions → same graph, the only variable is the allocation strategy.
struct BenchGraph {
    neighbors: Vec<Vec<u32>>,
    vectors: Vec<Vec<f32>>,
    query: Vec<f32>,
    m0: usize,
}

#[derive(Clone, Copy)]
struct BNode {
    idx: u32,
    d: f64,
}

fn l2(a: &[f32], b: &[f32]) -> f64 {
    a.iter().zip(b).map(|(x, y)| ((x - y) as f64) * ((x - y) as f64)).sum::<f64>().sqrt()
}

impl NeighborSource for BenchGraph {
    type Node = BNode;
    type Ref = u32;
    fn dist(&self, node: &BNode) -> f64 {
        node.d
    }
    fn tid(&self, node: &BNode) -> i64 {
        node.idx as i64
    }
    fn node_key(&self, node: &BNode) -> u64 {
        node.idx as u64
    }
    fn ref_key(&self, r: &u32) -> u64 {
        *r as u64
    }
    fn neighbors_into(&self, node: &BNode, out: &mut Vec<u32>) -> Result<(), String> {
        out.clear();
        out.extend_from_slice(&self.neighbors[node.idx as usize]);
        Ok(())
    }
    fn load(&self, r: &u32) -> Result<BNode, String> {
        Ok(BNode { idx: *r, d: l2(&self.query, &self.vectors[*r as usize]) })
    }
}

/// Deterministic SplitMix64 — the fixture is seeded so the graph is byte-identical within a run (D2).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn f32(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
    fn below(&mut self, n: u32) -> u32 {
        (self.next() % n as u64) as u32
    }
}

fn build_graph(n: usize, dim: usize, m0: usize, seed: u64) -> BenchGraph {
    let mut r = Rng(seed);
    let vectors: Vec<Vec<f32>> =
        (0..n).map(|_| (0..dim).map(|_| r.f32() * 10.0).collect()).collect();
    // Random regular graph: each node gets m0 distinct-ish random neighbors (long-range connectivity → the
    // ef-search visits a realistic spread of nodes, exercising the visited/heap/scratch allocation).
    let neighbors: Vec<Vec<u32>> =
        (0..n).map(|_| (0..m0).map(|_| r.below(n as u32)).collect()).collect();
    let query: Vec<f32> = (0..dim).map(|_| r.f32() * 10.0).collect();
    BenchGraph { neighbors, vectors, query, m0 }
}

fn entry_node(g: &BenchGraph) -> BNode {
    BNode { idx: 0, d: l2(&g.query, &g.vectors[0]) }
}

fn bench_scan(c: &mut Criterion) {
    // Fixture: built ONCE, shared by every measurement in this run (same-graph within-run — D2/EC-1).
    // N=50k, dim=128, m0=32 — the regime where `ef*m0` makes pre-sizing matter (EC-3).
    let g = build_graph(50_000, 128, 32, 42);
    let mut group = c.benchmark_group("scan_ground_loop");
    for ef in [100usize, 200, 400] {
        group.bench_with_input(BenchmarkId::new("presized", ef), &ef, |b, &ef| {
            b.iter(|| {
                let out = ground_search(black_box(&g), entry_node(&g), ef, g.m0, true).unwrap();
                black_box(out.len())
            })
        });
        group.bench_with_input(BenchmarkId::new("unsized", ef), &ef, |b, &ef| {
            b.iter(|| {
                let out = ground_search(black_box(&g), entry_node(&g), ef, g.m0, false).unwrap();
                black_box(out.len())
            })
        });
    }
    group.finish();
}

criterion_group!(benches, bench_scan);
criterion_main!(benches);
