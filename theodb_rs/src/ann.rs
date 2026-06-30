//! Domain layer (plan M21): TheoDB's own HNSW + IVFFlat ANN search algorithms, in pure Rust.
//!
//! Two own index structures — `HnswIndex` (layered proximity graph; Malkov & Yashunin, arXiv:1603.09320) and
//! `IvfflatIndex` (k-means++ inverted lists) — built in-memory over a corpus of `(id, vector)` and answering
//! top-k nearest-neighbour queries. The algorithm mirrors pgvector's C reference (build params M /
//! ef_construction / ef_search; lists / probes) so recall@k can reach parity, gated by the reproducible
//! benchmark (`benchmarks/bench_ann_index.py`). Per ADR D2 all distances go through the M20 f32-parity kernel
//! `crate::vec` (no new distance code). Per ADR D1 this is a SQL-callable build+search (the SQL surface lives in
//! `lib.rs`); coexistence with pgvector is total (we read its values, never its index). Per ADR D3 the RNG is a
//! std-only seeded SplitMix64 (deterministic, NOT cryptographic).
//!
//! Validated standalone before porting (10/10 algorithm tests, recall ≥ 0.95 for both); the `#[pg_test]` mod
//! below locks the contract and runs under `cargo pgrx test` (the always-on CI proof is the Python recall gate).
use std::cmp::Ordering;
use std::collections::BinaryHeap;

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
    fn dist(self, a: &[f32], b: &[f32]) -> f64 {
        match self {
            Metric::L2 => crate::vec::l2_distance(a, b),
            Metric::Cosine => crate::vec::cosine_distance(a, b),
            Metric::Ip => -crate::vec::inner_product(a, b),
        }
    }
}

/// Seeded SplitMix64 PRNG — std-only, deterministic (ADR D3). NOT cryptographic; used only for HNSW layer
/// assignment + IVF k-means++ seeding so results are reproducible across runs.
struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed)
    }
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Uniform in (0,1] — never 0, so `ln()` stays finite for the HNSW level formula.
    fn next_f64(&mut self) -> f64 {
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
struct Cand {
    d: f64,
    i: usize,
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

// ====================== HNSW ======================

/// Own HNSW index: a layered proximity graph over the corpus, built in-memory.
pub(crate) struct HnswIndex {
    metric: Metric,
    m: usize,
    m0: usize,
    ef_construction: usize,
    vectors: Vec<Vec<f32>>,
    ids: Vec<i64>,
    levels: Vec<usize>,
    neighbors: Vec<Vec<Vec<usize>>>,
    entry: Option<usize>,
    max_level: usize,
}

impl HnswIndex {
    /// Build the graph from `corpus` (`(id, vector)` pairs, all equal-dim). `m` neighbours per node (layer 0
    /// gets `2*m`), `ef_construction` build candidate-list size.
    pub(crate) fn build(
        corpus: &[(i64, Vec<f32>)],
        m: usize,
        ef_construction: usize,
        metric: Metric,
        seed: u64,
    ) -> Self {
        let mut rng = Rng::new(seed);
        let ml = 1.0 / (m.max(2) as f64).ln();
        let mut idx = HnswIndex {
            metric,
            m,
            m0: m * 2,
            ef_construction: ef_construction.max(m),
            vectors: Vec::new(),
            ids: Vec::new(),
            levels: Vec::new(),
            neighbors: Vec::new(),
            entry: None,
            max_level: 0,
        };
        for (id, v) in corpus {
            let level = (-(rng.next_f64().ln()) * ml) as usize;
            idx.insert(*id, v.clone(), level);
        }
        idx
    }

    fn insert(&mut self, id: i64, vec: Vec<f32>, level: usize) {
        let node = self.vectors.len();
        self.vectors.push(vec);
        self.ids.push(id);
        self.levels.push(level);
        self.neighbors.push(vec![Vec::new(); level + 1]);

        if self.entry.is_none() {
            self.entry = Some(node);
            self.max_level = level;
            return;
        }
        let q = self.vectors[node].clone();
        let mut ep = self.entry.unwrap();
        let mut lc = self.max_level;
        while lc > level {
            ep = self.greedy_descend(&q, ep, lc);
            lc -= 1;
        }
        let mut lc = level.min(self.max_level) as isize;
        while lc >= 0 {
            let layer = lc as usize;
            let candidates = self.search_layer(&q, &[ep], self.ef_construction, layer);
            let m_layer = if layer == 0 { self.m0 } else { self.m };
            let selected = self.select_from(candidates, m_layer);
            self.neighbors[node][layer] = selected.clone();
            for &nb in &selected {
                self.neighbors[nb][layer].push(node);
                if self.neighbors[nb][layer].len() > m_layer {
                    let nbvec = self.vectors[nb].clone();
                    let cand: Vec<Cand> = self.neighbors[nb][layer]
                        .iter()
                        .map(|&x| Cand {
                            d: self.metric.dist(&nbvec, &self.vectors[x]),
                            i: x,
                        })
                        .collect();
                    self.neighbors[nb][layer] = self.select_from(cand, m_layer);
                }
            }
            if let Some(&best) = selected.first() {
                ep = best;
            }
            lc -= 1;
        }
        if level > self.max_level {
            self.max_level = level;
            self.entry = Some(node);
        }
    }

    fn greedy_descend(&self, q: &[f32], mut ep: usize, layer: usize) -> usize {
        let mut best_d = self.metric.dist(q, &self.vectors[ep]);
        loop {
            let mut improved = false;
            let li = layer.min(self.neighbors[ep].len().saturating_sub(1));
            // Clone the small neighbour slice to avoid borrowing self across the dist calls.
            let nbs = self.neighbors[ep][li].clone();
            for nb in nbs {
                let d = self.metric.dist(q, &self.vectors[nb]);
                if d < best_d {
                    best_d = d;
                    ep = nb;
                    improved = true;
                }
            }
            if !improved {
                return ep;
            }
        }
    }

    /// Search `layer` from `entries` keeping the `ef` nearest; returns them sorted (nearest first).
    fn search_layer(&self, q: &[f32], entries: &[usize], ef: usize, layer: usize) -> Vec<Cand> {
        let mut visited = vec![false; self.vectors.len()];
        let mut cand: BinaryHeap<std::cmp::Reverse<Cand>> = BinaryHeap::new();
        let mut result: BinaryHeap<Cand> = BinaryHeap::new();
        for &e in entries {
            if e >= self.vectors.len() {
                continue;
            }
            let d = self.metric.dist(q, &self.vectors[e]);
            visited[e] = true;
            cand.push(std::cmp::Reverse(Cand { d, i: e }));
            result.push(Cand { d, i: e });
        }
        while let Some(std::cmp::Reverse(c)) = cand.pop() {
            let worst = result.peek().map(|w| w.d).unwrap_or(f64::INFINITY);
            if c.d > worst && result.len() >= ef {
                break;
            }
            if layer >= self.neighbors[c.i].len() {
                continue;
            }
            let nbs = self.neighbors[c.i][layer].clone();
            for nb in nbs {
                if visited[nb] {
                    continue;
                }
                visited[nb] = true;
                let d = self.metric.dist(q, &self.vectors[nb]);
                let worst = result.peek().map(|w| w.d).unwrap_or(f64::INFINITY);
                if d < worst || result.len() < ef {
                    cand.push(std::cmp::Reverse(Cand { d, i: nb }));
                    result.push(Cand { d, i: nb });
                    if result.len() > ef {
                        result.pop();
                    }
                }
            }
        }
        result.into_sorted_vec()
    }

    /// pgvector "closer" neighbour heuristic: keep candidate `e` only if it is closer to the query than to any
    /// already-kept neighbour; top up with the nearest remaining if the heuristic left us short.
    fn select_from(&self, mut candidates: Vec<Cand>, m: usize) -> Vec<usize> {
        candidates.sort();
        let mut kept: Vec<usize> = Vec::new();
        for c in &candidates {
            if kept.len() >= m {
                break;
            }
            let cv = &self.vectors[c.i];
            let closer_to_kept = kept
                .iter()
                .any(|&k| self.metric.dist(cv, &self.vectors[k]) < c.d);
            if !closer_to_kept {
                kept.push(c.i);
            }
        }
        if kept.len() < m {
            for c in &candidates {
                if kept.len() >= m {
                    break;
                }
                if !kept.contains(&c.i) {
                    kept.push(c.i);
                }
            }
        }
        kept
    }

    /// Top-k search: greedy-descend the upper layers, then `ef_search` at layer 0.
    pub(crate) fn search(&self, q: &[f32], k: usize, ef_search: usize) -> Vec<(i64, f64)> {
        if self.vectors.is_empty() || k == 0 {
            return Vec::new();
        }
        let ef = ef_search.max(k);
        let mut ep = self.entry.unwrap();
        let mut lc = self.max_level;
        while lc >= 1 {
            ep = self.greedy_descend(q, ep, lc);
            lc -= 1;
        }
        let mut found = self.search_layer(q, &[ep], ef, 0);
        found.sort();
        found.truncate(k);
        found.into_iter().map(|c| (self.ids[c.i], c.d)).collect()
    }
}

// ====================== IVFFlat ======================

/// Own IVFFlat index: k-means++ centroids partition the corpus into inverted lists; search scans the `probes`
/// nearest lists.
pub(crate) struct IvfflatIndex {
    metric: Metric,
    centroids: Vec<Vec<f32>>,
    lists: Vec<Vec<usize>>,
    vectors: Vec<Vec<f32>>,
    ids: Vec<i64>,
}

impl IvfflatIndex {
    pub(crate) fn build(corpus: &[(i64, Vec<f32>)], lists: usize, metric: Metric, seed: u64) -> Self {
        let n = corpus.len();
        let vectors: Vec<Vec<f32>> = corpus.iter().map(|(_, v)| v.clone()).collect();
        let ids: Vec<i64> = corpus.iter().map(|(id, _)| *id).collect();
        let mut idx = IvfflatIndex {
            metric,
            centroids: Vec::new(),
            lists: Vec::new(),
            vectors,
            ids,
        };
        if n == 0 {
            return idx;
        }
        let k = lists.clamp(1, n);
        idx.centroids = idx.kmeanspp(k, seed);
        idx.lists = vec![Vec::new(); idx.centroids.len()];
        for i in 0..idx.vectors.len() {
            let c = idx.nearest_in(&idx.centroids, &idx.vectors[i]);
            idx.lists[c].push(i);
        }
        idx
    }

    fn kmeanspp(&self, k: usize, seed: u64) -> Vec<Vec<f32>> {
        let mut rng = Rng::new(seed);
        let n = self.vectors.len();
        let dim = self.vectors[0].len();
        let mut centers: Vec<Vec<f32>> = Vec::with_capacity(k);
        centers.push(self.vectors[(rng.next_u64() as usize) % n].clone());
        while centers.len() < k {
            let d2: Vec<f64> = self
                .vectors
                .iter()
                .map(|v| {
                    centers
                        .iter()
                        .map(|c| {
                            let d = crate::vec::l2_distance(v, c);
                            d * d
                        })
                        .fold(f64::INFINITY, f64::min)
                })
                .collect();
            let sum: f64 = d2.iter().sum();
            if sum <= 0.0 {
                centers.push(self.vectors[centers.len() % n].clone());
                continue;
            }
            let mut target = rng.next_f64() * sum;
            let mut chosen = 0usize;
            for (i, w) in d2.iter().enumerate() {
                target -= *w;
                if target <= 0.0 {
                    chosen = i;
                    break;
                }
            }
            centers.push(self.vectors[chosen].clone());
        }
        // Bounded Lloyd refinement.
        for _ in 0..10 {
            let mut sums = vec![vec![0f64; dim]; centers.len()];
            let mut counts = vec![0usize; centers.len()];
            for v in &self.vectors {
                let c = self.nearest_in(&centers, v);
                counts[c] += 1;
                for (j, x) in v.iter().enumerate() {
                    sums[c][j] += *x as f64;
                }
            }
            for (c, cnt) in counts.iter().enumerate() {
                if *cnt > 0 {
                    for j in 0..dim {
                        centers[c][j] = (sums[c][j] / *cnt as f64) as f32;
                    }
                }
            }
        }
        centers
    }

    fn nearest_in(&self, centers: &[Vec<f32>], v: &[f32]) -> usize {
        let mut best = 0usize;
        let mut bd = f64::INFINITY;
        for (i, c) in centers.iter().enumerate() {
            let d = self.metric.dist(v, c);
            if d < bd {
                bd = d;
                best = i;
            }
        }
        best
    }

    pub(crate) fn search(&self, q: &[f32], k: usize, probes: usize) -> Vec<(i64, f64)> {
        if self.vectors.is_empty() || k == 0 {
            return Vec::new();
        }
        let p = probes.clamp(1, self.centroids.len().max(1));
        let mut cdist: Vec<Cand> = self
            .centroids
            .iter()
            .enumerate()
            .map(|(i, c)| Cand {
                d: self.metric.dist(q, c),
                i,
            })
            .collect();
        cdist.sort();
        let mut results: Vec<Cand> = Vec::new();
        for c in cdist.iter().take(p) {
            for &node in &self.lists[c.i] {
                results.push(Cand {
                    d: self.metric.dist(q, &self.vectors[node]),
                    i: node,
                });
            }
        }
        results.sort();
        results.truncate(k);
        results.into_iter().map(|c| (self.ids[c.i], c.d)).collect()
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
