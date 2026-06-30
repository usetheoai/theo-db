//! Domain layer (plan M21): TheoDB's own HNSW + IVFFlat ANN search algorithms, in pure Rust.
//!
//! Two own index structures — [`HnswIndex`] (layered proximity graph; Malkov & Yashunin, arXiv:1603.09320,
//! in `hnsw.rs`) and [`IvfflatIndex`] (k-means++ inverted lists, in `ivf.rs`) — built in-memory over a corpus
//! of `(id, vector)` and answering top-k nearest-neighbour queries. The algorithm mirrors pgvector's C
//! reference (build params M / ef_construction / ef_search; lists / probes) so recall@k can reach parity,
//! gated by the reproducible benchmark (`benchmarks/bench_ann_index.py`). Per ADR D2 all distances go through
//! the M20 f32-parity kernel `crate::vec` (no new distance code). Per ADR D1 this is a SQL-callable
//! build+search (the SQL surface lives in `lib.rs`); coexistence with pgvector is total (we read its values,
//! never its index). Per ADR D3 the RNG is a std-only seeded SplitMix64 (deterministic, NOT cryptographic).
//!
//! This module holds the shared primitives ([`Metric`], [`Rng`], [`Cand`]); the two algorithms live in the
//! `hnsw` / `ivf` submodules (split to keep each file within the 500-LoC budget — `rules/architecture.md`).
//! Validated standalone before porting (10/10 algorithm tests, recall ≥ 0.95 for both); the `#[pg_test]` mod
//! below locks the contract and runs under `cargo pgrx test` (the always-on CI proof is the Python recall gate).
use std::cmp::Ordering;

mod hnsw;
mod ivf;

pub(crate) use hnsw::HnswIndex;
pub(crate) use ivf::IvfflatIndex;

/// Distance family. `dist` returns the ORDER-BY key (smaller = nearer); for inner product pgvector orders by the
/// NEGATIVE inner product (`<#>`), so the key is `-inner_product` (parity with pgvector's `<#>`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Metric {
    L2,
    Ip,
    Cosine,
}

impl Metric {
    /// Parse the SQL `metric` text. Returns `None` for an unknown value (the caller raises 22023).
    pub(crate) fn parse(s: &str) -> Option<Metric> {
        match s.to_ascii_lowercase().as_str() {
            "l2" | "euclidean" => Some(Metric::L2),
            "ip" | "inner_product" | "dot" => Some(Metric::Ip),
            "cosine" => Some(Metric::Cosine),
            _ => None,
        }
    }

    /// Order-by distance via the M20 f32-parity kernel (ADR D2). All vectors here are equal-dim (validated at
    /// the SQL boundary), so `crate::vec`'s dim check never fires.
    pub(super) fn dist(self, a: &[f32], b: &[f32]) -> f64 {
        match self {
            Metric::L2 => crate::vec::l2_distance(a, b),
            Metric::Cosine => crate::vec::cosine_distance(a, b),
            Metric::Ip => -crate::vec::inner_product(a, b),
        }
    }
}

/// Seeded SplitMix64 PRNG — std-only, deterministic (ADR D3). NOT cryptographic; used only for HNSW layer
/// assignment + IVF k-means++ seeding so results are reproducible across runs.
pub(super) struct Rng(u64);
impl Rng {
    pub(super) fn new(seed: u64) -> Self {
        Rng(seed)
    }
    pub(super) fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Uniform in (0,1] — never 0, so `ln()` stays finite for the HNSW level formula.
    pub(super) fn next_f64(&mut self) -> f64 {
        let v = (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
        if v <= 0.0 {
            f64::MIN_POSITIVE
        } else {
            v
        }
    }
}

/// A `(distance, node-index)` entry. `Ord` puts NaN LAST (worst) so a zero-norm cosine vector falls to the end
/// of the result rather than panicking on `partial_cmp` (edge-case EC-3).
#[derive(Clone, Copy, Debug)]
pub(super) struct Cand {
    pub(super) d: f64,
    pub(super) i: usize,
}
impl PartialEq for Cand {
    fn eq(&self, o: &Self) -> bool {
        self.d == o.d && self.i == o.i
    }
}
impl Eq for Cand {}
impl PartialOrd for Cand {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Cand {
    fn cmp(&self, o: &Self) -> Ordering {
        match self.d.partial_cmp(&o.d) {
            Some(ord) => ord.then(self.i.cmp(&o.i)),
            None => {
                if self.d.is_nan() && o.d.is_nan() {
                    self.i.cmp(&o.i)
                } else if self.d.is_nan() {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            }
        }
    }
}

// Algorithm unit tests (plan T1.1/T1.2). `#[pg_test]` (not plain `#[test]`) because the crate links pg symbols
// via `crate::vec` → `crate::pg::err_input`; runs under `cargo pgrx test`, compiles under `cargo check --tests`.
// The standalone prototype (validated 10/10 before porting) + the Python container recall gate are the
// always-on proofs. Recall thresholds here mirror the prototype.
#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use super::*;
    use pgrx::prelude::*;

    fn rand_corpus(n: usize, dim: usize, seed: u64) -> Vec<(i64, Vec<f32>)> {
        let mut r = Rng::new(seed);
        (0..n)
            .map(|i| {
                (
                    i as i64,
                    (0..dim).map(|_| (r.next_f64() as f32) * 2.0 - 1.0).collect(),
                )
            })
            .collect()
    }
    fn brute(corpus: &[(i64, Vec<f32>)], q: &[f32], k: usize, metric: Metric) -> Vec<i64> {
        let mut v: Vec<Cand> = corpus
            .iter()
            .enumerate()
            .map(|(idx, (_, x))| Cand {
                d: metric.dist(q, x),
                i: idx,
            })
            .collect();
        v.sort();
        v.truncate(k);
        v.into_iter().map(|c| corpus[c.i].0).collect()
    }
    fn recall(truth: &[i64], got: &[(i64, f64)]) -> f64 {
        if truth.is_empty() {
            return 1.0;
        }
        let t: std::collections::HashSet<i64> = truth.iter().copied().collect();
        got.iter().filter(|x| t.contains(&x.0)).count() as f64 / truth.len() as f64
    }

    #[pg_test]
    fn hnsw_exact_tiny() {
        let c = vec![(0, vec![0.0, 0.0]), (1, vec![1.0, 0.0]), (2, vec![5.0, 5.0])];
        let r = HnswIndex::build(&c, 16, 64, Metric::L2, 42).search(&[0.0, 0.0], 2, 40);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].0, 0);
        assert_eq!(r[1].0, 1);
    }
    #[pg_test]
    fn hnsw_recall_high_on_random_corpus() {
        let c = rand_corpus(300, 8, 7);
        let idx = HnswIndex::build(&c, 16, 100, Metric::L2, 42);
        let qs = rand_corpus(20, 8, 99);
        let avg: f64 = qs
            .iter()
            .map(|(_, q)| recall(&brute(&c, q, 10, Metric::L2), &idx.search(q, 10, 100)))
            .sum::<f64>()
            / qs.len() as f64;
        assert!(avg >= 0.95, "hnsw recall {avg} < 0.95");
    }
    #[pg_test]
    fn hnsw_deterministic_same_seed() {
        let c = rand_corpus(100, 6, 3);
        let a = HnswIndex::build(&c, 16, 64, Metric::L2, 42).search(&[0.1; 6], 5, 40);
        let b = HnswIndex::build(&c, 16, 64, Metric::L2, 42).search(&[0.1; 6], 5, 40);
        assert_eq!(a, b);
    }
    #[pg_test]
    fn hnsw_k_greater_than_n_returns_n() {
        let c = vec![(0, vec![0.0]), (1, vec![1.0]), (2, vec![2.0])];
        assert_eq!(HnswIndex::build(&c, 16, 64, Metric::L2, 42).search(&[0.0], 10, 40).len(), 3);
    }
    #[pg_test]
    fn hnsw_single_element() {
        let c = vec![(7, vec![1.0, 2.0, 3.0])];
        let r = HnswIndex::build(&c, 16, 64, Metric::L2, 42).search(&[0.0, 0.0, 0.0], 10, 40);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].0, 7);
    }
    #[pg_test]
    fn cosine_zero_norm_does_not_panic() {
        let c = vec![(0, vec![0.0, 0.0]), (1, vec![1.0, 1.0]), (2, vec![2.0, 1.0])];
        let r = HnswIndex::build(&c, 16, 64, Metric::Cosine, 42).search(&[1.0, 1.0], 3, 40);
        assert_eq!(r.len(), 3);
        assert_eq!(r.last().unwrap().0, 0); // zero-norm (NaN dist) sorts last
    }
    #[pg_test]
    fn ivfflat_exact_tiny() {
        let c = vec![(0, vec![0.0, 0.0]), (1, vec![1.0, 0.0]), (2, vec![5.0, 5.0])];
        let r = IvfflatIndex::build(&c, 2, Metric::L2, 42).search(&[0.0, 0.0], 2, 2);
        assert_eq!(r[0].0, 0);
    }
    #[pg_test]
    fn ivfflat_recall_with_enough_probes() {
        let c = rand_corpus(300, 8, 7);
        let idx = IvfflatIndex::build(&c, 16, Metric::L2, 42);
        let qs = rand_corpus(20, 8, 99);
        let avg: f64 = qs
            .iter()
            .map(|(_, q)| recall(&brute(&c, q, 10, Metric::L2), &idx.search(q, 10, 16)))
            .sum::<f64>()
            / qs.len() as f64;
        assert!(avg >= 0.95, "ivf recall {avg} < 0.95");
    }
    #[pg_test]
    fn ivfflat_clamps_lists_gt_n() {
        let c = vec![(0, vec![0.0]), (1, vec![1.0])];
        assert_eq!(IvfflatIndex::build(&c, 1000, Metric::L2, 42).search(&[0.0], 5, 1000).len(), 2);
    }
    #[pg_test]
    fn ivfflat_all_identical_corpus_no_panic() {
        // EC: every vector identical → k-means++ zero-sum fallback path; must not panic / NaN-cluster.
        let c = vec![(0, vec![1.0, 1.0, 1.0]), (1, vec![1.0, 1.0, 1.0]), (2, vec![1.0, 1.0, 1.0])];
        let r = IvfflatIndex::build(&c, 4, Metric::L2, 42).search(&[1.0, 1.0, 1.0], 2, 4);
        assert_eq!(r.len(), 2);
    }
    #[pg_test]
    fn empty_corpus_returns_empty() {
        let c: Vec<(i64, Vec<f32>)> = vec![];
        assert!(HnswIndex::build(&c, 16, 64, Metric::L2, 42).search(&[0.0], 5, 40).is_empty());
        assert!(IvfflatIndex::build(&c, 4, Metric::L2, 42).search(&[0.0], 5, 4).is_empty());
    }
    #[pg_test]
    fn metric_parse_rejects_unknown() {
        assert!(Metric::parse("nope").is_none());
        assert_eq!(Metric::parse("L2"), Some(Metric::L2));
        assert_eq!(Metric::parse("cosine"), Some(Metric::Cosine));
        assert_eq!(Metric::parse("ip"), Some(Metric::Ip));
    }
}
