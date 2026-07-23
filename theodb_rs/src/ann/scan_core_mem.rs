//! In-memory [`NeighborSource`] over a built [`HnswIndex`] + the `ground_search`/resume equivalence tests.
//!
//! M144 (pre-existing test-infra fix): this code references `crate::ann` (HnswIndex / Metric / Rng), so it must
//! NOT live in `ann/scan_core.rs` — that file is `#[path]`-included **standalone** by the criterion bench
//! (`benches/scan_hot_path.rs`), which has no `crate::ann`. The sections used to be `#[cfg(feature = "pg_test")]`
//! inside scan_core.rs on the assumption the bench compiles WITHOUT pg_test; but `cargo pgrx test` turns the
//! `pg_test` feature ON for **all** targets, including the bench, so the bench pulled `crate::ann` and failed to
//! compile (E0432/E0433) — which blocked EVERY `#[pg_test]` in the crate. Living here as a normal `mod` (only
//! included by the library, never `#[path]`-included) keeps scan_core.rs pure and lets `cargo pgrx test` run.
#![cfg(feature = "pg_test")]

use super::scan_core::NeighborSource;

/// An in-memory [`NeighborSource`] over a built [`HnswIndex`] — used by the equivalence tests (the D3 guard:
/// `ground_search` over the REAL graph == brute exact kNN). Node ids are the graph's node indices; distances are
/// the metric score against the query.
pub(crate) struct MemNeighborSource<'a> {
    idx: &'a crate::ann::HnswIndex,
    query: &'a [f32],
    metric: crate::ann::Metric,
    n: u32,
}

/// A loaded in-memory node: its index + its cached distance to the query.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MemNode {
    idx: u32,
    d: f64,
}

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

#[pgrx::pg_schema]
mod tests {
    use super::MemNeighborSource;
    use crate::ann::scan_core::{NeighborSource, ResumableGround, ground_search};
    use crate::ann::{HnswIndex, Metric};
    use std::cmp::Ordering;
    use std::collections::HashSet;

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
        assert_eq!(
            a, b,
            "ground_search top-10 must equal brute exact-kNN set at ef=200 (100% recall)"
        );
    }

    /// M118 recall-neutral invariant: the union of resumed `next_batch` results (batch ef=50, accumulated) is a
    /// SUPERSET of the single-shot ef=200 top-10. Resume-from-discarded finds at least as good as the re-search
    /// it replaces (the frontier is retained, never discarded).
    #[pgrx::pg_test]
    fn resumed_batches_union_superset_of_single_ef() {
        let corpus = seeded_corpus(2000, 16, 42);
        let idx = HnswIndex::build(&corpus, 16, 64, Metric::L2, 42);
        let (_metric, _m, m0, _ef) = idx.params();
        let q = corpus[7].1.clone();
        let src = MemNeighborSource::new(&idx, &q);
        let single = ground_search(&src, src.entry_node().unwrap(), 200, m0, true).unwrap();
        let single_ids: HashSet<i64> = single.iter().take(10).map(|(t, _)| *t).collect();

        let mut rg = ResumableGround::init(&src, src.entry_node().unwrap(), 50, m0, true);
        let mut union: HashSet<i64> = HashSet::new();
        let mut passes = 0;
        while !rg.exhausted() && passes < 40 {
            for (node, _d) in rg.next_batch(&src).unwrap() {
                union.insert(src.tid(&node));
            }
            passes += 1;
            if union.len() >= 200 {
                break;
            }
        }
        assert!(
            single_ids.is_subset(&union),
            "resumed union (n={}) must contain the single-ef top-10 (missing: {:?})",
            union.len(),
            single_ids.difference(&union).collect::<Vec<_>>()
        );
    }

    /// EC-1: a small graph exhausts the frontier in finite passes; once exhausted, `next_batch` yields `[]`
    /// (no spin, no re-search fallback — the highly-selective case M118 targets).
    #[pgrx::pg_test]
    fn resume_exhausts_when_frontier_empty() {
        let corpus = seeded_corpus(200, 16, 3);
        let idx = HnswIndex::build(&corpus, 16, 64, Metric::L2, 3);
        let (_metric, _m, m0, _ef) = idx.params();
        let q = corpus[0].1.clone();
        let src = MemNeighborSource::new(&idx, &q);
        let mut rg = ResumableGround::init(&src, src.entry_node().unwrap(), 20, m0, true);
        let mut passes = 0;
        while !rg.exhausted() {
            let _ = rg.next_batch(&src).unwrap();
            passes += 1;
            assert!(passes < 200, "resume must terminate on a finite graph");
        }
        assert!(rg.exhausted(), "frontier must empty on a small reachable graph");
        assert!(
            rg.next_batch(&src).unwrap().is_empty(),
            "exhausted frontier yields an empty batch"
        );
    }

    /// EC-3: a single-node index with ef=1 returns the node once, then exhausts (smallest-graph boundary).
    #[pgrx::pg_test]
    fn resume_single_node_index_ef1() {
        let corpus = seeded_corpus(1, 16, 5);
        let idx = HnswIndex::build(&corpus, 16, 64, Metric::L2, 5);
        let (_metric, _m, m0, _ef) = idx.params();
        let q = corpus[0].1.clone();
        let src = MemNeighborSource::new(&idx, &q);
        let mut rg = ResumableGround::init(&src, src.entry_node().unwrap(), 1, m0, true);
        let first = rg.next_batch(&src).unwrap();
        assert_eq!(first.len(), 1, "single-node index returns the node once");
        assert!(rg.exhausted(), "single node exhausts after one batch");
        assert!(rg.next_batch(&src).unwrap().is_empty(), "second batch is empty");
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
