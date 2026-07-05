//! FU-1: the pure ground-layer HNSW search, extracted behind a `NeighborSource` seam.
//!
//! The M46 change (pre-size the 3 per-query structures + reuse one neighbor scratch) lives in the ground loop
//! of the on-disk `am/hnsw_page.rs::traverse`. To measure its allocation cost cleanly — same-graph,
//! box-noise-immune, without page I/O confounds — the loop is extracted here as [`ground_search`], generic over
//! a [`NeighborSource`]. Production implements the seam over PostgreSQL pages (`PageNeighborSource`); the
//! criterion bench + these tests implement it over an in-memory `HnswIndex` ([`MemNeighborSource`]). This module
//! MUST NOT reference `pg_sys` (the bench link invariant — blueprint Q5): it is pure domain logic
//! (`architecture.md § 1` — the domain does not depend on infrastructure).
//!
//! The seam preserves production's page-read pattern: [`NeighborSource::neighbors_into`] lists neighbor *refs*
//! (one read), the loop dedups on the ref key, and ONLY non-visited refs are [`NeighborSource::load`]ed (one
//! read each). So `pages_read` is unchanged — the extraction is recall-neutral (proven by the M46 oracle on the
//! production side + the brute-force oracle on the in-memory side).

use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashSet};

/// The DIP seam between the pure ground search and its storage (PostgreSQL pages OR an in-memory graph).
///
/// `Node` is a *loaded* candidate (its distance is known, it can be expanded, it carries the result tid).
/// `Ref` is an *unloaded* neighbor reference (an address / index) — cheap, produced by `neighbors_into` and
/// deduped by the loop BEFORE `load` is called, mirroring the on-disk scan (dedup-before-read).
pub(crate) trait NeighborSource {
    /// A loaded node — distance known, expandable. Must be `Copy` (pushed into both heaps).
    type Node: Copy;
    /// An unloaded neighbor reference — cheap, not yet scored. Must be `Copy` (held in the scratch buffer).
    type Ref: Copy;

    /// Distance of a loaded node to the query (cached in the node — never a re-score).
    fn dist(&self, node: &Self::Node) -> f64;
    /// The heap tid of a loaded node (the result payload).
    fn tid(&self, node: &Self::Node) -> i64;
    /// Dedup key of a loaded node (its own address / index).
    fn node_key(&self, node: &Self::Node) -> u64;
    /// Dedup key of an unloaded ref (production: packed `(blk,off)`; in-memory: the node index).
    fn ref_key(&self, r: &Self::Ref) -> u64;
    /// Ground-layer (`layer 0`) neighbor refs of a loaded node, appended into `out` (cleared first). One read.
    fn neighbors_into(&self, node: &Self::Node, out: &mut Vec<Self::Ref>) -> Result<(), String>;
    /// Load an unloaded ref into a node (distance + tid + expansion handle). One read. Only for fresh refs.
    fn load(&self, r: &Self::Ref) -> Result<Self::Node, String>;
}

/// A `(distance, node)` pair ordered by distance, NaN LAST (worst) — mirrors the on-disk `Cand` ordering
/// (`am/hnsw_page.rs`) so a zero-norm cosine vector (NaN distance) falls to the end instead of corrupting the
/// heaps (edge-case EC-5).
struct Ranked<N> {
    d: f64,
    node: N,
}
impl<N> PartialEq for Ranked<N> {
    fn eq(&self, o: &Self) -> bool {
        self.d == o.d
    }
}
impl<N> Eq for Ranked<N> {}
impl<N> PartialOrd for Ranked<N> {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl<N> Ord for Ranked<N> {
    fn cmp(&self, o: &Self) -> Ordering {
        self.d.partial_cmp(&o.d).unwrap_or(Ordering::Equal)
    }
}

/// On-demand top-`ef` ground-layer search (the M46 hot loop): a min-heap of candidates to expand, a max-heap of
/// the `ef` best found, a `visited` dedup set, and one reused neighbor `scratch`. Returns `(tid, dist)` ascending.
///
/// `entry` is the loaded starting node (production passes the upper-layer-descended entry point; the in-memory
/// bench passes the graph's entry via [`MemNeighborSource::entry_node`]). `presize` is the SOLE axis the FU-1
/// bench flips: `true` pre-sizes the three per-query structures + scratch with
/// `with_capacity(ef·m0·2 / ef·m0 / ef+1 / m0)` (the M46 change — anchors pgvector `tidhash_create(ef*m*2)` +
/// pgvectorscale `with_capacity(search_list_size*neigbors)`); `false` uses `::new()` (the pre-M46 baseline).
/// The RESULT is byte-identical for both — pre-sizing is a capacity hint that cannot change visit order.
pub(crate) fn ground_search<S: NeighborSource>(
    src: &S,
    entry: S::Node,
    ef: usize,
    m0: usize,
    presize: bool,
) -> Result<Vec<(i64, f64)>, String> {
    let ef = ef.max(1); // ef=0 clamp (edge-case) — never an empty search
    let (mut visited, mut cands, mut result, mut scratch): (
        HashSet<u64>,
        BinaryHeap<Reverse<Ranked<S::Node>>>,
        BinaryHeap<Ranked<S::Node>>,
        Vec<S::Ref>,
    ) = if presize {
        let cap = ef.saturating_mul(m0.max(1)).max(1);
        (
            HashSet::with_capacity(cap.saturating_mul(2)),
            BinaryHeap::with_capacity(cap),
            BinaryHeap::with_capacity(ef + 1),
            Vec::with_capacity(m0.max(1)),
        )
    } else {
        (HashSet::new(), BinaryHeap::new(), BinaryHeap::new(), Vec::new())
    };

    let entry_d = src.dist(&entry);
    visited.insert(src.node_key(&entry));
    cands.push(Reverse(Ranked { d: entry_d, node: entry }));
    result.push(Ranked { d: entry_d, node: entry });

    while let Some(Reverse(Ranked { d: cd, node: c })) = cands.pop() {
        let worst = result.peek().map(|w| w.d).unwrap_or(f64::INFINITY);
        if cd > worst && result.len() >= ef {
            break;
        }
        src.neighbors_into(&c, &mut scratch)?;
        for i in 0..scratch.len() {
            let r = scratch[i];
            if !visited.insert(src.ref_key(&r)) {
                continue; // dedup BEFORE load — keeps pages_read identical (recall-neutral)
            }
            let cand = src.load(&r)?;
            let nd = src.dist(&cand);
            let worst = result.peek().map(|w| w.d).unwrap_or(f64::INFINITY);
            if nd < worst || result.len() < ef {
                cands.push(Reverse(Ranked { d: nd, node: cand }));
                result.push(Ranked { d: nd, node: cand });
                if result.len() > ef {
                    result.pop();
                }
            }
        }
    }

    let mut out: Vec<(i64, f64)> =
        result.into_iter().map(|r| (src.tid(&r.node), r.d)).collect();
    out.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal).then(a.0.cmp(&b.0)));
    Ok(out)
}

/// An in-memory [`NeighborSource`] over a built [`HnswIndex`] — used by the equivalence tests (the D3 guard:
/// `ground_search` over the REAL graph == brute exact kNN). Node ids are the graph's node indices; distances are
/// the metric score against the query. Gated to test/pg_test so the pure `ground_search` above can be
/// `#[path]`-included by the criterion bench WITHOUT pulling `crate::ann` / `pg_sys` (blueprint Q5 — the bench
/// links standalone; the module above this line has ZERO `crate::` references).
#[cfg(feature = "pg_test")]
pub(crate) struct MemNeighborSource<'a> {
    idx: &'a crate::ann::HnswIndex,
    query: &'a [f32],
    metric: crate::ann::Metric,
    n: u32,
}

/// A loaded in-memory node: its index + its cached distance to the query.
#[cfg(feature = "pg_test")]
#[derive(Clone, Copy)]
pub(crate) struct MemNode {
    idx: u32,
    d: f64,
}

#[cfg(feature = "pg_test")]
impl<'a> MemNeighborSource<'a> {
    pub(crate) fn new(idx: &'a crate::ann::HnswIndex, query: &'a [f32]) -> Self {
        let (metric, _m, _m0, _ef) = idx.params();
        Self { idx, query, metric, n: idx.node_count() as u32 }
    }
    fn score(&self, node: u32) -> f64 {
        self.metric.dist(self.query, self.idx.node_vector(node as usize))
    }
    /// The loaded entry node for the ground search (the graph's global entry point). The bench measures the
    /// ground-loop allocation from here — it does NOT replicate the upper-layer descent (not what M46 changed);
    /// starting from the global entry visits ef nodes, a fair/representative ground-loop workload.
    pub(crate) fn entry_node(&self) -> Result<MemNode, String> {
        let e = self.idx.entry().ok_or("scan_core: empty index has no entry point")? as u32;
        Ok(MemNode { idx: e, d: self.score(e) })
    }
}

#[cfg(feature = "pg_test")]
impl<'a> NeighborSource for MemNeighborSource<'a> {
    type Node = MemNode;
    type Ref = u32;

    fn dist(&self, node: &MemNode) -> f64 {
        node.d
    }
    fn tid(&self, node: &MemNode) -> i64 {
        self.idx.node_id(node.idx as usize)
    }
    fn node_key(&self, node: &MemNode) -> u64 {
        node.idx as u64
    }
    fn ref_key(&self, r: &u32) -> u64 {
        *r as u64
    }
    fn neighbors_into(&self, node: &MemNode, out: &mut Vec<u32>) -> Result<(), String> {
        out.clear();
        for &nb in self.idx.node_neighbors(node.idx as usize, 0) {
            out.push(nb as u32);
        }
        Ok(())
    }
    fn load(&self, r: &u32) -> Result<MemNode, String> {
        if *r >= self.n {
            // negative case (EC-3): a ref outside the graph is a typed error, never an OOB panic
            return Err(format!("scan_core: node id out of range ({r} >= {})", self.n));
        }
        Ok(MemNode { idx: *r, d: self.score(*r) })
    }
}

#[cfg(feature = "pg_test")]
#[pgrx::pg_schema]
mod tests {
    use super::*;
    use crate::ann::{HnswIndex, Metric};

    /// Seeded deterministic corpus (mirror of the `ann/mod.rs` test helper) — reused via the crate-internal RNG.
    fn seeded_corpus(n: usize, dim: usize, seed: u64) -> Vec<(i64, Vec<f32>)> {
        let mut r = crate::ann::Rng::new(seed);
        (0..n)
            .map(|i| {
                let v: Vec<f32> = (0..dim).map(|_| (r.next_f64() as f32) * 10.0).collect();
                (i as i64 + 1, v)
            })
            .collect()
    }

    /// Brute-force exact kNN (the independent oracle) — the same math as `ann/mod.rs::brute`.
    fn brute(corpus: &[(i64, Vec<f32>)], q: &[f32], k: usize, metric: Metric) -> Vec<i64> {
        let mut d: Vec<(f64, i64)> =
            corpus.iter().map(|(id, v)| (metric.dist(q, v), *id)).collect();
        d.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal).then(a.1.cmp(&b.1)));
        d.into_iter().take(k).map(|(_, id)| id).collect()
    }

    /// D3 guard: `ground_search` over the in-memory seam returns EXACTLY the brute-force exact-kNN order on a
    /// small distinct corpus at high ef (100% recall) — proves the pure traversal is correct (not a bogus graph).
    #[pgrx::pg_test]
    fn ground_search_matches_brute_exact_knn() {
        let corpus = seeded_corpus(2000, 16, 42);
        let idx = HnswIndex::build(&corpus, 16, 64, Metric::L2, 42);
        let (_metric, _m, m0, _ef) = idx.params();
        let q = corpus[7].1.clone();
        let src = MemNeighborSource::new(&idx, &q);
        let got = ground_search(&src, src.entry_node().unwrap(), 200, m0, true).unwrap();
        let got_ids: Vec<i64> = got.iter().take(10).map(|(tid, _)| *tid).collect();
        let want = brute(&corpus, &q, 10, Metric::L2);
        let (mut a, mut b) = (got_ids.clone(), want.clone());
        a.sort_unstable();
        b.sort_unstable();
        assert_eq!(a, b, "ground_search top-10 must equal brute exact-kNN set at ef=200 (100% recall)");
    }

    /// The bench's SOLE axis is result-neutral: presize=true and presize=false return byte-identical output.
    #[pgrx::pg_test]
    fn ground_search_presize_is_result_neutral() {
        let corpus = seeded_corpus(3000, 16, 7);
        let idx = HnswIndex::build(&corpus, 16, 64, Metric::L2, 7);
        let (_metric, _m, m0, _ef) = idx.params();
        let q = corpus[13].1.clone();
        let src = MemNeighborSource::new(&idx, &q);
        for ef in [50usize, 200, 400] {
            let presized = ground_search(&src, src.entry_node().unwrap(), ef, m0, true).unwrap();
            let plain = ground_search(&src, src.entry_node().unwrap(), ef, m0, false).unwrap();
            assert_eq!(presized, plain, "presize must not change the result at ef={ef}");
        }
    }

    /// Edge (EC-2): ef larger than the graph returns ≤ node_count results, no panic, no ef padding.
    #[pgrx::pg_test]
    fn ground_search_ef_exceeds_node_count_returns_all() {
        let corpus = seeded_corpus(5, 4, 3);
        let idx = HnswIndex::build(&corpus, 16, 64, Metric::L2, 3);
        let (_metric, _m, m0, _ef) = idx.params();
        let q = corpus[0].1.clone();
        let src = MemNeighborSource::new(&idx, &q);
        let got = ground_search(&src, src.entry_node().unwrap(), 200, m0, true).unwrap();
        assert!(got.len() <= 5, "ef>node_count must return <= node_count, got {}", got.len());
        assert!(!got.is_empty(), "a non-empty graph must return at least the entry");
    }

    /// Edge: ef=0 is clamped to 1 (no empty/degenerate search, no panic).
    #[pgrx::pg_test]
    fn ground_search_ef_zero_clamped() {
        let corpus = seeded_corpus(100, 8, 1);
        let idx = HnswIndex::build(&corpus, 16, 64, Metric::L2, 1);
        let (_metric, _m, m0, _ef) = idx.params();
        let q = corpus[0].1.clone();
        let src = MemNeighborSource::new(&idx, &q);
        let got = ground_search(&src, src.entry_node().unwrap(), 0, m0, true).unwrap();
        assert!(got.len() <= 1, "ef=0 clamps to 1 → <= 1 result, got {}", got.len());
    }

    /// Negative (EC-3): a ref outside the graph is a typed error, not an index-out-of-bounds panic.
    #[pgrx::pg_test]
    fn mem_neighbor_source_out_of_range_node_is_typed_err() {
        let corpus = seeded_corpus(10, 4, 5);
        let idx = HnswIndex::build(&corpus, 16, 64, Metric::L2, 5);
        let q = corpus[0].1.clone();
        let src = MemNeighborSource::new(&idx, &q);
        let err = src.load(&u32::MAX).unwrap_err();
        assert!(err.contains("out of range"), "out-of-range ref must be a typed error, got: {err}");
    }
}
