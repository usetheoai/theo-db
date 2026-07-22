//! M118 standalone check of `ResumableGround` (resume-from-discarded) — links WITHOUT a PostgreSQL runtime by
//! `#[path]`-including the PURE `ann/scan_core.rs` (same trick as `benches/scan_hot_path.rs`; `cargo pgrx test`
//! does not link on the build droplet — validation is done here + A/B in-PG, per the project convention).
//!
//! Run: `cargo run --example resumable_check` (or `--release`). Panics on any failed invariant; prints OK.

#[path = "../src/ann/scan_core.rs"]
mod scan_core;

use scan_core::{NeighborSource, ResumableGround, ground_search};
use std::collections::HashSet;

#[derive(Clone, Copy)]
struct BNode {
    idx: u32,
    d: f64,
}

struct Graph {
    neighbors: Vec<Vec<u32>>,
    vectors: Vec<Vec<f32>>,
    query: Vec<f32>,
}

fn l2(a: &[f32], b: &[f32]) -> f64 {
    a.iter().zip(b).map(|(x, y)| ((x - y) as f64) * ((x - y) as f64)).sum::<f64>().sqrt()
}

impl NeighborSource for Graph {
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

fn build_graph(n: usize, dim: usize, m0: usize, seed: u64) -> Graph {
    let mut r = Rng(seed);
    let vectors: Vec<Vec<f32>> =
        (0..n).map(|_| (0..dim).map(|_| r.f32() * 10.0).collect()).collect();
    let neighbors: Vec<Vec<u32>> =
        (0..n).map(|_| (0..m0).map(|_| r.below(n as u32)).collect()).collect();
    let query: Vec<f32> = (0..dim).map(|_| r.f32() * 10.0).collect();
    Graph { neighbors, vectors, query }
}

fn entry(g: &Graph) -> BNode {
    BNode { idx: 0, d: l2(&g.query, &g.vectors[0]) }
}

fn main() {
    // --- Invariant 1: recall-neutral — resumed union ⊇ single-ef top-10 ---
    {
        let g = build_graph(2000, 16, 32, 42);
        let m0 = 32;
        let single = ground_search(&g, entry(&g), 200, m0, true).unwrap();
        let single_ids: HashSet<i64> = single.iter().take(10).map(|(t, _)| *t).collect();

        let mut rg = ResumableGround::init(&g, entry(&g), 50, m0, true);
        let mut union: HashSet<i64> = HashSet::new();
        let mut passes = 0;
        while !rg.exhausted() && passes < 60 {
            for (node, _d) in rg.next_batch(&g).unwrap() {
                union.insert(g.tid(&node));
            }
            passes += 1;
            if union.len() >= 300 {
                break;
            }
        }
        assert!(
            single_ids.is_subset(&union),
            "INV1 FAIL: resumed union (n={}, passes={}) missing single-ef top-10: {:?}",
            union.len(),
            passes,
            single_ids.difference(&union).collect::<Vec<_>>()
        );
        println!(
            "INV1 ok — resumed union (n={union_len}) ⊇ single-ef top-10",
            union_len = union.len()
        );
    }

    // --- Invariant 2 (EC-1): frontier exhausts in finite passes, then next_batch is empty ---
    {
        let g = build_graph(200, 16, 32, 3);
        let m0 = 32;
        let mut rg = ResumableGround::init(&g, entry(&g), 20, m0, true);
        let mut passes = 0;
        while !rg.exhausted() {
            let _ = rg.next_batch(&g).unwrap();
            passes += 1;
            assert!(passes < 500, "INV2 FAIL: resume did not terminate on a finite graph");
        }
        assert!(rg.exhausted(), "INV2 FAIL: frontier must be empty");
        assert!(rg.next_batch(&g).unwrap().is_empty(), "INV2 FAIL: exhausted → empty batch");
        println!("INV2 ok — frontier exhausts in {passes} passes, then empty");
    }

    // --- Invariant 3 (EC-3): single-node graph, ef=1 ---
    {
        let g = build_graph(1, 16, 32, 5);
        let m0 = 32;
        let mut rg = ResumableGround::init(&g, entry(&g), 1, m0, true);
        let first = rg.next_batch(&g).unwrap();
        assert_eq!(
            first.len(),
            1,
            "INV3 FAIL: single-node returns the node once, got {}",
            first.len()
        );
        assert!(rg.exhausted(), "INV3 FAIL: single node exhausts after one batch");
        assert!(rg.next_batch(&g).unwrap().is_empty(), "INV3 FAIL: second batch empty");
        println!("INV3 ok — single-node ef=1 returns once then exhausts");
    }

    println!("ALL RESUMABLE_GROUND INVARIANTS PASS");
}
