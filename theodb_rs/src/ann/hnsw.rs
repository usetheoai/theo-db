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

    /// Rebuild over `live` reusing this graph's parameters (M26 VACUUM fold).
    pub(crate) fn rebuilt_with(&self, live: &[(i64, Vec<f32>)], seed: u64) -> HnswIndex {
        HnswIndex::build(live, self.m, self.ef_construction, self.metric, seed)
    }

    /// Every `(id, vector)` stored (M26 — enumerated during VACUUM to rebuild over only the live heap TIDs).
    pub(crate) fn entries(&self) -> Vec<(i64, Vec<f32>)> {
        self.ids.iter().copied().zip(self.vectors.iter().cloned()).collect()
    }

    /// Like [`search`] but folds in `pending` `(id, vector)` tuples inserted after the build (M26 Phase 5/6).
    pub(crate) fn search_merged(
        &self,
        q: &[f32],
        k: usize,
        ef_search: usize,
        pending: &[(i64, Vec<f32>)],
    ) -> Vec<(i64, f64)> {
        let mut out = self.search(q, k, ef_search);
        if !pending.is_empty() {
            for (id, v) in pending {
                out.push((*id, self.metric.dist(q, v)));
            }
            out.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
            out.truncate(k);
        }
        out
    }

    /// Serialize the graph for page persistence (M26 Phase 6). Bit-faithful; layout mirrors the struct fields.
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        use crate::ann::wire::*;
        let mut b = Vec::new();
        put_u32(&mut b, HNSW_MAGIC);
        put_u32(&mut b, HNSW_VERSION);
        b.push(self.metric.tag());
        put_u32(&mut b, self.m as u32);
        put_u32(&mut b, self.m0 as u32);
        put_u32(&mut b, self.ef_construction as u32);
        put_vecs_f32(&mut b, &self.vectors);
        put_u32(&mut b, self.ids.len() as u32);
        for id in &self.ids {
            put_i64(&mut b, *id);
        }
        put_vec_usize(&mut b, &self.levels);
        put_vecs_vecs_usize(&mut b, &self.neighbors);
        match self.entry {
            Some(e) => {
                b.push(1);
                put_u64(&mut b, e as u64);
            }
            None => b.push(0),
        }
        put_u32(&mut b, self.max_level as u32);
        b
    }

    /// Inverse of [`to_bytes`]. Fail-fast typed `Err` on any truncation / bad magic / unknown metric.
    pub(crate) fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let mut c = crate::ann::wire::Cur::new(bytes);
        if c.u32()? != HNSW_MAGIC {
            return Err("theodb hnsw: bad index page magic".into());
        }
        if c.u32()? != HNSW_VERSION {
            return Err("theodb hnsw: unsupported index page version".into());
        }
        let metric = Metric::from_tag(c.u8()?).ok_or("theodb hnsw: unknown metric tag")?;
        let m = c.u32()? as usize;
        let m0 = c.u32()? as usize;
        let ef_construction = c.u32()? as usize;
        let vectors = c.vecs_f32()?;
        let ids = c.i64_vec()?;
        let levels = c.vec_usize()?;
        let neighbors = c.vecs_vecs_usize()?;
        let entry = if c.u8()? == 1 { Some(c.usize()?) } else { None };
        let max_level = c.u32()? as usize;
        // Referential-integrity validation (M26): a structurally-complete but semantically-corrupt blob must NOT
        // reach `search` (which does `self.entry.unwrap()` then `self.vectors[ep]`) — that would panic across the
        // C FFI boundary. Fail-fast with a typed Err instead.
        let n = vectors.len();
        if ids.len() != n || levels.len() != n || neighbors.len() != n {
            return Err("theodb hnsw: inconsistent node counts in index page".into());
        }
        match entry {
            Some(e) if e >= n => return Err("theodb hnsw: entry index out of bounds".into()),
            None if n > 0 => return Err("theodb hnsw: non-empty index without an entry point".into()),
            _ => {}
        }
        Ok(HnswIndex { metric, m, m0, ef_construction, vectors, ids, levels, neighbors, entry, max_level })
    }
}

pub(crate) const HNSW_MAGIC: u32 = 0x5448_4E53; // "THNS"
const HNSW_VERSION: u32 = 1;

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod hnsw_persist_tests {
    use super::*;

    fn corpus() -> Vec<(i64, Vec<f32>)> {
        vec![
            (10, vec![1.0, 0.0, 0.0]),
            (20, vec![0.0, 1.0, 0.0]),
            (30, vec![0.0, 0.0, 1.0]),
            (40, vec![0.9, 0.1, 0.0]),
            (50, vec![0.1, 0.9, 0.0]),
        ]
    }

    #[pgrx::pg_test]
    fn hnsw_roundtrip_bytes_reproduces_search() {
        let idx = HnswIndex::build(&corpus(), 16, 64, Metric::L2, 42);
        let back = HnswIndex::from_bytes(&idx.to_bytes()).expect("round-trip");
        let q = vec![1.0, 0.0, 0.0];
        assert_eq!(idx.search(&q, 3, 40), back.search(&q, 3, 40));
    }

    #[pgrx::pg_test]
    fn hnsw_empty_roundtrips() {
        let idx = HnswIndex::build(&[], 16, 64, Metric::Cosine, 1);
        let back = HnswIndex::from_bytes(&idx.to_bytes()).expect("empty round-trip");
        assert!(back.search(&[1.0, 0.0], 3, 40).is_empty());
    }

    #[pgrx::pg_test]
    fn hnsw_from_bytes_rejects_truncated_and_bad_magic() {
        let good = HnswIndex::build(&corpus(), 16, 64, Metric::L2, 7).to_bytes();
        assert!(HnswIndex::from_bytes(&good[..good.len() - 4]).is_err(), "truncated must Err");
        let mut bad = good.clone();
        bad[0] ^= 0xFF;
        assert!(HnswIndex::from_bytes(&bad).is_err(), "bad magic must Err");
    }
}
