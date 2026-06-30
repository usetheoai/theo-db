//! HNSW (Hierarchical Navigable Small World) — own layered proximity graph (plan M21 T1.1).
//! Shared primitives ([`Metric`], [`Rng`], [`Cand`]) live in the parent `ann` module.
use super::{Cand, Metric, Rng};
use std::collections::BinaryHeap;

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
