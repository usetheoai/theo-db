//! IVFFlat — own k-means++ inverted-list index (plan M21 T1.2).
//! Shared primitives ([`Metric`], [`Rng`], [`Cand`]) live in the parent `ann` module.
use super::{Cand, Metric, Rng};

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
