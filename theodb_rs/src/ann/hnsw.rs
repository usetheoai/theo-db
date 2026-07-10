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
    /// gets `2*m`), `ef_construction` build candidate-list size. Non-cancellable convenience wrapper — the pure
    /// domain callers (tests, the criterion bench, small sequential builds) use this; the PostgreSQL callers use
    /// [`build_cancellable`] to inject `check_for_interrupts!` so a long `CREATE INDEX` responds to cancel (T4.1).
    pub(crate) fn build(
        corpus: &[(i64, Vec<f32>)],
        m: usize,
        ef_construction: usize,
        metric: Metric,
        seed: u64,
    ) -> Self {
        Self::build_cancellable(corpus, m, ef_construction, metric, seed, &|| {})
    }

    /// Build with a caller-injected `check_interrupt`, called once per parallel batch (M48 T4.1). This is the DIP
    /// seam that keeps the `ann/` layer pure (zero `pg_sys`): the domain declares the contract (`&dyn Fn()`), the
    /// `am/` infrastructure implements it with `pgrx::check_for_interrupts!`. Same pattern as the FU-1
    /// `NeighborSource` seam. The closure runs only between batches, where all workers are joined (longjmp-safe).
    ///
    /// Borrow wrapper over the consuming [`build_owned`]: it clones the corpus first. Domain callers (tests, the
    /// criterion bench, the ad-hoc in-memory ANN in `ann_query`) keep ownership of their corpus and use this — at
    /// their scale the clone is immaterial. The PostgreSQL AM build path (`ambuild_hnsw`, the VACUUM fold) feeds
    /// the owned heap-scan result to [`build_owned`] DIRECTLY, skipping this clone (M56 DoD-4).
    pub(crate) fn build_cancellable(
        corpus: &[(i64, Vec<f32>)],
        m: usize,
        ef_construction: usize,
        metric: Metric,
        seed: u64,
        check_interrupt: &(dyn Fn() + Sync),
    ) -> Self {
        Self::build_owned(corpus.to_vec(), m, ef_construction, metric, seed, check_interrupt)
    }

    /// Consuming build — MOVES each vector out of `corpus` into the graph, never cloning, so the build's peak
    /// working set is O(N) once (the graph's `vectors`), not the corpus-plus-copy ~O(2N) the borrow wrapper holds.
    /// `ambuild_hnsw` and the VACUUM fold pass their owned heap-scan `Vec` here directly — this is the M56 DoD-4
    /// memory ceiling for `CREATE INDEX` / `REINDEX` of a 1M×768d relation (the corpus is freed as it is drained,
    /// so the f32 vectors are never resident twice). The HNSW graph is inherently O(N) in vectors; DoD-4 removes
    /// the AVOIDABLE second copy, not the intrinsic floor.
    pub(crate) fn build_owned(
        corpus: Vec<(i64, Vec<f32>)>,
        m: usize,
        ef_construction: usize,
        metric: Metric,
        seed: u64,
        check_interrupt: &(dyn Fn() + Sync),
    ) -> Self {
        let n = corpus.len();
        let mut rng = Rng::new(seed);
        let ml = 1.0 / (m.max(2) as f64).ln();
        // Pre-assign every node's level DETERMINISTICALLY (sequential RNG, same order as the old build) so BOTH
        // the sequential and parallel paths use identical levels — only the parallel LINKING order races (M44 D2).
        // Cap the layer so a node's neighbor tuple always fits ONE page in the M35 structured layout
        // (`nbr_size(level)` ≤ BLCKSZ). Real levels are ~ln(n)/ln(m) (≤ ~5 at 1M, m=16); the cap only clamps the
        // astronomically-rare tail (pgvector caps the same way — `HnswGetMaxLevel`). NOT an algorithm change.
        let levels: Vec<usize> = (0..n)
            .map(|_| ((-(rng.next_f64().ln()) * ml) as usize).min(HNSW_MAX_LEVEL))
            .collect();

        // Small corpora: the unchanged sequential build (deterministic, no thread overhead — the tiny AM test
        // corpora take this path). Large corpora: the M44 parallel build.
        if n < crate::ann::hnsw_parallel::parallel_threshold() {
            return Self::build_sequential(corpus, m, ef_construction, metric, &levels);
        }

        // DoD-4: drain the corpus into `vectors`/`ids` by MOVE — no `.clone()`. The corpus `Vec` is consumed
        // here, so the graph's `vectors` is the ONLY O(N) copy of the f32 data resident during the parallel link.
        let mut vectors: Vec<Vec<f32>> = Vec::with_capacity(n);
        let mut ids: Vec<i64> = Vec::with_capacity(n);
        for (id, v) in corpus {
            ids.push(id);
            vectors.push(v);
        }
        let m0 = m * 2;
        let efc = ef_construction.max(m);
        let (neighbors, entry, max_level) =
            crate::ann::hnsw_parallel::build_parallel(&vectors, &levels, m, m0, efc, metric, check_interrupt);
        HnswIndex {
            metric,
            m,
            m0,
            ef_construction: efc,
            vectors,
            ids,
            levels,
            neighbors,
            entry: Some(entry),
            max_level,
        }
    }

    /// The sequential graph build (deterministic given `levels`): insert every node in order into a growing graph.
    /// Used directly for corpora below `PARALLEL_BUILD_THRESHOLD` and as the reference the parallel path matches.
    /// Consumes `corpus`, moving each vector into `insert` (no clone) — the same DoD-4 discipline as the parallel path.
    fn build_sequential(
        corpus: Vec<(i64, Vec<f32>)>,
        m: usize,
        ef_construction: usize,
        metric: Metric,
        levels: &[usize],
    ) -> Self {
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
        for (i, (id, v)) in corpus.into_iter().enumerate() {
            idx.insert(id, v, levels[i]);
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
        // M71 (latency): carry the FULL search result W forward as the next layer's entry SET (Malkov-Yashunin
        // INSERT Alg.1 `ep ← W`; pgvector `HnswFindElementNeighbors` `ep = w`, hnswutils.c). The previous
        // `ep = selected.first()` collapsed W to a SINGLE node between layers, yielding a less-connected graph that
        // needs a wider query beam. Carrying W builds a better-connected graph → **+29% query QPS at equal recall**,
        // measured at 500k×768d (docs/benchmarks/m60-raw/m60_theodb_f32_fix2_multientry_500k768d.json). Recall-neutral.
        let mut entries: Vec<usize> = vec![ep];
        while lc >= 0 {
            let layer = lc as usize;
            let mut candidates = self.search_layer(&q, &entries, self.ef_construction, layer);
            let next_entries: Vec<usize> = candidates.iter().map(|c| c.i).collect();
            let m_layer = if layer == 0 { self.m0 } else { self.m };
            // Gap-1 fix (navigability): extendCandidates (Malkov-Yashunin INSERT, `extendCandidates=true` — the
            // paper recommends it for EXTREMELY CLUSTERED data, which is exactly our regime). Extend the candidate
            // pool with the candidates' own layer-`layer` neighbors before the diversity `select_from`. White-box
            // anatomy (scratch) proved this collapses the ef-insensitive routing gap: recall@10 0.975→1.000 at
            // 80k×768d and holds across scale (meanHop entry→trueNN → 0). Without it, the graph lacks long-range
            // "highways" → high hop-distance → the beam needs a wide ef → the iso-recall latency gap vs pgvector.
            if crate::ann::hnsw_parallel::extend_candidates() {
                let mut seen: std::collections::HashSet<usize> = candidates.iter().map(|c| c.i).collect();
                let base: Vec<usize> = candidates.iter().map(|c| c.i).collect();
                for ci in base {
                    for &nb in self.neighbors[ci].get(layer).map(|v| v.as_slice()).unwrap_or(&[]) {
                        if seen.insert(nb) {
                            candidates.push(Cand { d: self.metric.dist_simd(&q, &self.vectors[nb]), i: nb });
                        }
                    }
                }
            }
            let selected = self.select_from(candidates, m_layer);
            self.neighbors[node][layer] = selected.clone();
            for &nb in &selected {
                self.neighbors[nb][layer].push(node);
                if self.neighbors[nb][layer].len() > m_layer {
                    let nbvec = self.vectors[nb].clone();
                    let cand: Vec<Cand> = self.neighbors[nb][layer]
                        .iter()
                        .map(|&x| Cand {
                            d: self.metric.dist_simd(&nbvec, &self.vectors[x]),
                            i: x,
                        })
                        .collect();
                    self.neighbors[nb][layer] = self.select_from(cand, m_layer);
                }
            }
            if !next_entries.is_empty() {
                entries = next_entries;
            }
            lc -= 1;
        }
        if level > self.max_level {
            self.max_level = level;
            self.entry = Some(node);
        }
    }

    fn greedy_descend(&self, q: &[f32], mut ep: usize, layer: usize) -> usize {
        let mut best_d = self.metric.dist_simd(q, &self.vectors[ep]);
        loop {
            let mut improved = false;
            let li = layer.min(self.neighbors[ep].len().saturating_sub(1));
            // Clone the small neighbour slice to avoid borrowing self across the dist calls.
            let nbs = self.neighbors[ep][li].clone();
            for nb in nbs {
                let d = self.metric.dist_simd(q, &self.vectors[nb]);
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
            let d = self.metric.dist_simd(q, &self.vectors[e]);
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
                let d = self.metric.dist_simd(q, &self.vectors[nb]);
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
                .any(|&k| self.metric.dist_simd(cv, &self.vectors[k]) < c.d);
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

    // --- Accessors for the M35 structured page codec (`am/hnsw_page.rs`). Read-only views of the built graph;
    // no algorithm change. The codec maps node index → (blkno,offno) and serializes these into element/neighbor
    // tuples. `dim` is 0 for an empty graph. ---
    pub(crate) fn params(&self) -> (Metric, usize, usize, usize) {
        (self.metric, self.m, self.m0, self.ef_construction)
    }
    pub(crate) fn node_count(&self) -> usize {
        self.vectors.len()
    }
    pub(crate) fn dim(&self) -> usize {
        self.vectors.first().map(|v| v.len()).unwrap_or(0)
    }
    pub(crate) fn entry(&self) -> Option<usize> {
        self.entry
    }
    pub(crate) fn node_id(&self, node: usize) -> i64 {
        self.ids[node]
    }
    pub(crate) fn node_level(&self, node: usize) -> usize {
        self.levels[node]
    }
    pub(crate) fn node_vector(&self, node: usize) -> &[f32] {
        &self.vectors[node]
    }
    /// Neighbors of `node` at `layer` (node indices). Empty if `layer > node.level`.
    pub(crate) fn node_neighbors(&self, node: usize, layer: usize) -> &[usize] {
        self.neighbors[node].get(layer).map(|v| v.as_slice()).unwrap_or(&[])
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
                out.push((*id, self.metric.dist_simd(q, v)));
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
/// Max HNSW layer — bounds a node's neighbor tuple to one page in the M35 structured layout (`am/hnsw_page.rs`).
/// 32 ≫ the real max (~ln(N)/ln(m) ≈ 5 at 1M, m=16), so it is invisible on real corpora; a conservative cap that
/// keeps `nbr_size(32) = 4 + (32·m + m0)·6` well under BLCKSZ for the fixed HNSW_M=16.
const HNSW_MAX_LEVEL: usize = 32;

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
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

    // M44 — a random `n`-node corpus (dim 16), deterministic given `seed`, to exercise the PARALLEL build path.
    fn big_corpus(n: usize, dim: usize, seed: u64) -> Vec<(i64, Vec<f32>)> {
        let mut r = Rng::new(seed);
        (0..n)
            .map(|i| {
                let v: Vec<f32> = (0..dim).map(|_| (r.next_f64() as f32) * 2.0 - 1.0).collect();
                (i as i64, v)
            })
            .collect()
    }

    #[pgrx::pg_test]
    fn hnsw_parallel_build_produces_valid_searchable_graph() {
        // > PARALLEL_BUILD_THRESHOLD (4096) → the M44 parallel path. Every corpus point must find ITSELF as the
        // exact nearest (distance 0) → the concurrently-built graph is valid + connected + searchable (a lost
        // link / deadlock / corruption would fail this or hang).
        let n = 5000;
        let c = big_corpus(n, 16, 2026);
        assert!(n >= crate::ann::hnsw_parallel::PARALLEL_BUILD_THRESHOLD, "corpus must trigger the parallel path");
        let idx = HnswIndex::build(&c, 16, 64, Metric::L2, 2026);
        assert_eq!(idx.node_count(), n, "all nodes present");
        // Sample 20 corpus points; each must be its own top-1 (self-recall).
        let mut self_hits = 0;
        for i in (0..n).step_by(n / 20) {
            let q = &c[i].1;
            let res = idx.search(q, 1, 64);
            if res.first().map(|(id, _)| *id) == Some(i as i64) {
                self_hits += 1;
            }
        }
        assert!(self_hits >= 19, "parallel graph self-recall too low: {self_hits}/20");
    }

    #[pgrx::pg_test]
    fn hnsw_parallel_build_recall_reasonable() {
        // Parallel-built graph recall@10 vs brute-force exact, over a query sample — must be high (the racy build
        // is recall-EQUIVALENT to sequential, ADR D2). Not identity — a statistical parity gate.
        let n = 5000;
        let dim = 16;
        let c = big_corpus(n, dim, 7);
        let idx = HnswIndex::build(&c, 16, 64, Metric::L2, 7);
        let mut r = Rng::new(999);
        let mut total = 0.0f64;
        let nq = 30;
        for _ in 0..nq {
            let q: Vec<f32> = (0..dim).map(|_| (r.next_f64() as f32) * 2.0 - 1.0).collect();
            // exact top-10 by brute force
            let mut exact: Vec<(usize, f64)> =
                c.iter().enumerate().map(|(j, (_, v))| (j, crate::vec::l2_distance(&q, v))).collect();
            exact.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            let gt: std::collections::HashSet<i64> = exact[..10].iter().map(|(j, _)| *j as i64).collect();
            let got: std::collections::HashSet<i64> = idx.search(&q, 10, 100).iter().map(|(id, _)| *id).collect();
            total += (gt.intersection(&got).count() as f64) / 10.0;
        }
        let recall = total / nq as f64;
        assert!(recall >= 0.85, "parallel build recall@10 too low: {recall:.3}");
    }
}
