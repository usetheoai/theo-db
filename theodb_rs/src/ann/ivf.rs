//! IVFFlat — own k-means++ inverted-list index (plan M21 T1.2).
//! Shared primitives ([`Metric`], [`Rng`], [`Cand`]) live in the parent `ann` module.
use super::{Cand, Metric, Rng};

/// Bounded Lloyd (k-means) refinement iterations — enough to converge centroids without unbounded work.
const LLOYD_ITERS: usize = 10;

/// M88 — cap on the k-means training subsample. `n ≤ this` trains on the whole corpus (byte-identical to the
/// pre-M88 build — every test + the 1M benchmarks unchanged); a larger corpus (100M+) trains centroids on a
/// deterministic stride subsample of this size, so the O(k·train·d) seeding + O(iters·train·k·d) Lloyd stay
/// bounded by the ~1M-scale cost. The full-N assignment is separate (and parallel). 1.1M keeps the 1M benchmarks
/// on the exact path while giving ≥ ~34 points/centroid even at the 32768-list max.
const KMEANS_TRAIN_SAMPLE: usize = 1_100_000;

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
    pub(crate) fn build(
        corpus: &[(i64, Vec<f32>)],
        lists: usize,
        metric: Metric,
        seed: u64,
    ) -> Self {
        // Borrow-taking entry point (small "carrier" builds: sbq/pq/ivf_aqah). Delegates to `build_owned` by
        // cloning once — negligible for the tiny corpora these callers pass.
        Self::build_owned(corpus.to_vec(), lists, metric, seed)
    }

    /// M89 (Roadmap v7 — ambuild streaming Increment 1) — ownership-taking build that MOVES the corpus vectors into
    /// `self.vectors` instead of cloning them (`ivf.rs` pre-M89 cloned the whole corpus → the 2nd of the ~4× copies
    /// the M88 OOM was made of, `wiki/decisions/0038`). The `ambuild` caller hands its owned `collect_corpus` output here
    /// and no longer holds a second copy; the AQ/SQ8 encode reads the vectors back from `self.vectors()` (no
    /// `corpus_vecs` clone). BYTE-IDENTICAL to the pre-M89 build (same vectors, same order, same kmeans) — the only
    /// change is who owns the bytes.
    pub(crate) fn build_owned(
        corpus: Vec<(i64, Vec<f32>)>,
        lists: usize,
        metric: Metric,
        seed: u64,
    ) -> Self {
        let n = corpus.len();
        let mut ids: Vec<i64> = Vec::with_capacity(n);
        let mut vectors: Vec<Vec<f32>> = Vec::with_capacity(n);
        for (id, v) in corpus {
            ids.push(id);
            vectors.push(v); // MOVE — no clone
        }
        let mut idx =
            IvfflatIndex { metric, centroids: Vec::new(), lists: Vec::new(), vectors, ids };
        if n == 0 {
            return idx;
        }
        let k = lists.clamp(1, n);
        idx.centroids = idx.kmeanspp(k, seed);
        // M88: assign every vector to its nearest centroid IN PARALLEL — O(N·k·d) is the build bottleneck at 100M+
        // (a single thread is ~hours). The per-vector assignment is order-independent (`nearest_in` is
        // deterministic), so the lists are built sequentially by ascending `i` afterward — BYTE-IDENTICAL to the
        // single-threaded assignment, so every existing recall/persist test is unchanged.
        let assignment = idx.assign_all_parallel();
        idx.lists = vec![Vec::new(); idx.centroids.len()];
        for (i, &c) in assignment.iter().enumerate() {
            idx.lists[c].push(i);
        }
        idx
    }

    /// M88 — assign every vector to its nearest centroid across all CPU cores (`std::thread::scope`, mirroring
    /// `hnsw_parallel`). `assignment[i]` is deterministic (`nearest_in` — min distance, first-index tie-break), so
    /// the result is independent of thread count → byte-identical to the sequential assignment. Read-only over
    /// `self.centroids`/`self.vectors`; disjoint mutable output slices, no shared mutable state.
    fn assign_all_parallel(&self) -> Vec<usize> {
        let n = self.vectors.len();
        let mut assignment = vec![0usize; n];
        if n == 0 || self.centroids.is_empty() {
            return assignment;
        }
        let nthreads = std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1).min(n);
        let chunk = n.div_ceil(nthreads);
        std::thread::scope(|s| {
            for (t, out) in assignment.chunks_mut(chunk).enumerate() {
                let base = t * chunk;
                let this = &*self;
                s.spawn(move || {
                    for (j, slot) in out.iter_mut().enumerate() {
                        *slot = this.nearest_in(&this.centroids, &this.vectors[base + j]);
                    }
                });
            }
        });
        assignment
    }

    /// M86 (Roadmap v7) — SOAR spill (Sun et al., NeurIPS 2023, arXiv:2404.00774): assign each vector to a SECOND
    /// list chosen to minimize the orthogonality-amplified residual loss `‖v−c′‖² + λ·⟨v−c′, r⟩²/‖r‖²`
    /// (`r = v − c₁`, the primary residual). The secondary is the "backup route" that is good precisely when the
    /// primary partition mis-estimates the query — so a query probing FEWER lists still finds the vector (fewer
    /// probes for the same recall, attacking the centroid-probe bind). Only the vector's index (→ its code) is
    /// duplicated; the f32 stays single-copy. `λ ≤ 0` is a no-op (byte-identical to the primary-only build). A
    /// vector found via both its lists is de-duplicated by tid at scan time. The exact `argmin` over all centroids
    /// matches the paper (§3.5); cost is ≈ one extra assignment pass.
    pub(crate) fn with_soar_spill(mut self, lambda: f64) -> Self {
        if lambda <= 0.0 || self.centroids.is_empty() || self.vectors.is_empty() {
            return self;
        }
        for i in 0..self.vectors.len() {
            let v = self.vectors[i].clone();
            let c1 = self.nearest_in(&self.centroids, &v);
            // r = v − c₁ (primary residual); r_norm2 guarded against a degenerate zero residual.
            let r: Vec<f32> = v.iter().zip(&self.centroids[c1]).map(|(x, c)| x - c).collect();
            let r_norm2 = r.iter().map(|x| (*x as f64) * (*x as f64)).sum::<f64>().max(1e-12);
            let mut best_loss = f64::INFINITY;
            let mut best_c = usize::MAX;
            for (ci, cen) in self.centroids.iter().enumerate() {
                if ci == c1 {
                    continue;
                }
                let mut d2 = 0.0f64;
                let mut dot = 0.0f64;
                for ((x, cc), rr) in v.iter().zip(cen).zip(&r) {
                    let rp = (*x - *cc) as f64; // r′ component
                    d2 += rp * rp;
                    dot += rp * (*rr as f64); // ⟨r′, r⟩
                }
                let loss = d2 + lambda * dot * dot / r_norm2;
                if loss < best_loss {
                    best_loss = loss;
                    best_c = ci;
                }
            }
            if best_c != usize::MAX {
                self.lists[best_c].push(i);
            }
        }
        self
    }

    fn kmeanspp(&self, k: usize, seed: u64) -> Vec<Vec<f32>> {
        let mut rng = Rng::new(seed);
        // M88: train the centroids on a bounded deterministic subsample so the O(k·train·d) seeding + the
        // O(iters·train·k·d) Lloyd stay tractable at 100M+ (the full-N assignment is separate + parallel). When
        // `n ≤ KMEANS_TRAIN_SAMPLE` the sample IS the whole corpus, so the seeding/Lloyd — and every recall/persist
        // test + the 1M benchmarks — are byte-for-byte unchanged (same RNG order, same centroids).
        let n_all = self.vectors.len();
        let train: Vec<&Vec<f32>> = if n_all > KMEANS_TRAIN_SAMPLE {
            let step = n_all / KMEANS_TRAIN_SAMPLE; // deterministic stride (seed-free, reproducible)
            (0..KMEANS_TRAIN_SAMPLE).map(|i| &self.vectors[i * step]).collect()
        } else {
            self.vectors.iter().collect()
        };
        let n = train.len();
        let dim = train[0].len();
        let mut centers: Vec<Vec<f32>> = Vec::with_capacity(k);
        let first = train[(rng.next_u64() as usize) % n].clone();
        // `d2[i]` = squared distance from point i to the NEAREST chosen center, maintained INCREMENTALLY (fold each
        // new center into the running min). This is the standard k-means++ at O(k·n·d), not the O(k²·n·d) of
        // recomputing the min over ALL centers every step. The RNG is consumed in the exact same order (only on
        // `sum > 0`), so the produced centroids are byte-for-byte unchanged when the sample is the whole corpus.
        let mut d2: Vec<f64> = train
            .iter()
            .map(|v| {
                let d = crate::vec::l2_distance(v, &first);
                d * d
            })
            .collect();
        centers.push(first);
        while centers.len() < k {
            let sum: f64 = d2.iter().sum();
            let chosen = if sum <= 0.0 {
                centers.len() % n // degenerate (all points on a center) — no rng draw, matching the original
            } else {
                let mut target = rng.next_f64() * sum;
                let mut c = 0usize;
                for (i, w) in d2.iter().enumerate() {
                    target -= *w;
                    if target <= 0.0 {
                        c = i;
                        break;
                    }
                }
                c
            };
            let center = train[chosen].clone();
            for (i, v) in train.iter().enumerate() {
                let d = crate::vec::l2_distance(v, &center);
                let dd = d * d;
                if dd < d2[i] {
                    d2[i] = dd;
                }
            }
            centers.push(center);
        }
        // Bounded Lloyd refinement (over the training sample).
        for _ in 0..LLOYD_ITERS {
            let mut sums = vec![vec![0f64; dim]; centers.len()];
            let mut counts = vec![0usize; centers.len()];
            for v in &train {
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
            .map(|(i, c)| Cand { d: self.metric.dist(q, c), i })
            .collect();
        cdist.sort();
        let mut results: Vec<Cand> = Vec::new();
        for c in cdist.iter().take(p) {
            for &node in &self.lists[c.i] {
                results.push(Cand { d: self.metric.dist(q, &self.vectors[node]), i: node });
            }
        }
        results.sort();
        results.truncate(k);
        results.into_iter().map(|c| (self.ids[c.i], c.d)).collect()
    }

    /// Rebuild over `live` reusing this index's parameters (M26 VACUUM fold). `lists` = the current centroid count.
    pub(crate) fn rebuilt_with(&self, live: &[(i64, Vec<f32>)], seed: u64) -> IvfflatIndex {
        IvfflatIndex::build(live, self.centroids.len().max(1), self.metric, seed)
    }

    /// The centroids (M31 — the AM persists them in the structured meta page so a scan can pick probed lists).
    pub(crate) fn centroids(&self) -> &[Vec<f32>] {
        &self.centroids
    }

    /// M89 — the stored vectors, by reference (no clone). Used by `ambuild` to train the SQ8 quantizer (a cheap
    /// one-pass min/max) directly from the index instead of cloning the corpus (`build.rs` pre-M89 `corpus_vecs`).
    pub(crate) fn vectors(&self) -> &[Vec<f32>] {
        &self.vectors
    }

    /// M89 (ambuild streaming Increment 2) — each inverted list as POSITIONS into `vectors()`/`ids()`, by reference
    /// (no clone). The streaming page writers read `vectors()[pos]`/`ids()[pos]` per list and flush one list at a
    /// time, so the full corpus is never re-materialized as `Vec<(id, vector)>` (the `list_entries()` clone that,
    /// with the writers' `enc_vec`/`items` buffering, made the M88 build peak ~4× base — `wiki/decisions/0038`).
    pub(crate) fn list_positions(&self) -> &[Vec<usize>] {
        &self.lists
    }

    /// M89 — the stored heap TIDs, by reference (parallel to `vectors()`, indexed by list position).
    pub(crate) fn ids(&self) -> &[i64] {
        &self.ids
    }

    /// M89 — number of stored vectors (the corpus size). Used for `build_result` after the owned corpus is moved in.
    pub(crate) fn len(&self) -> usize {
        self.vectors.len()
    }

    /// M89 — companion to `len()` (satisfies clippy `len_without_is_empty`; a zero-vector index is the empty build).
    pub(crate) fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }

    /// M89 — a deterministic stride subsample of the stored vectors (for the AQ codebook train, which pre-M89
    /// sampled the corpus directly). `k >= n` returns clones of all; the stride is seed-free/reproducible so the
    /// trained codebook is byte-identical to the pre-M89 sample-of-corpus.
    pub(crate) fn train_sample(&self, k: usize) -> Vec<Vec<f32>> {
        let n = self.vectors.len();
        if n == 0 {
            return Vec::new();
        }
        if n > k {
            let step = n / k;
            (0..k).map(|i| self.vectors[i * step].clone()).collect()
        } else {
            self.vectors.clone()
        }
    }

    /// Each centroid's inverted-list entries as `(id, vector)` (M31 — the AM persists these as list pages).
    pub(crate) fn list_entries(&self) -> Vec<Vec<(i64, Vec<f32>)>> {
        self.lists
            .iter()
            .map(|list| {
                list.iter().map(|&pos| (self.ids[pos], self.vectors[pos].clone())).collect()
            })
            .collect()
    }

    /// Every `(id, vector)` stored in the index (M26 — the AM enumerates these during VACUUM to rebuild the
    /// index over only the live heap TIDs).
    pub(crate) fn entries(&self) -> Vec<(i64, Vec<f32>)> {
        self.ids.iter().copied().zip(self.vectors.iter().cloned()).collect()
    }

    /// Like [`search`] but also folds in `pending` `(id, vector)` tuples inserted after the build (M26 Phase 5).
    /// Pending tuples are scored with the SAME metric and merged into the ranking, so newly-inserted rows surface
    /// without a rebuild. Returns the top-`k` `(id, distance)` overall.
    pub(crate) fn search_merged(
        &self,
        q: &[f32],
        k: usize,
        probes: usize,
        pending: &[(i64, Vec<f32>)],
    ) -> Vec<(i64, f64)> {
        let mut out = self.search(q, k, probes);
        if !pending.is_empty() {
            for (id, v) in pending {
                out.push((*id, self.metric.dist(q, v)));
            }
            out.sort_by(|a, b| {
                a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0))
            });
            out.truncate(k);
        }
        out
    }

    /// Candidate generation for a quantized re-ranker (M22): return the corpus POSITIONS (0-based, aligned with
    /// the build corpus order) of every member of the `probes` nearest lists, UNranked. The caller re-ranks by
    /// its own (e.g. Hamming) distance + a full-precision rerank. Additive to M21 — `search` is unchanged.
    pub(crate) fn candidate_positions(&self, q: &[f32], probes: usize) -> Vec<usize> {
        if self.vectors.is_empty() {
            return Vec::new();
        }
        let p = probes.clamp(1, self.centroids.len().max(1));
        let mut cdist: Vec<Cand> = self
            .centroids
            .iter()
            .enumerate()
            .map(|(i, c)| Cand { d: self.metric.dist(q, c), i })
            .collect();
        cdist.sort();
        let mut out: Vec<usize> = Vec::new();
        for c in cdist.iter().take(p) {
            out.extend_from_slice(&self.lists[c.i]);
        }
        out
    }

    /// Serialize the built index into a self-describing little-endian byte blob for page persistence (M26 index
    /// AM). Hand-rolled (no serde dep — the fields are flat) and bit-faithful (f32 stored verbatim), so a
    /// `from_bytes` round-trip reproduces identical `search` results. Layout: magic+version, metric tag, then
    /// length-prefixed centroids / lists / vectors / ids.
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        use crate::ann::wire::*;
        let mut b = Vec::new();
        put_u32(&mut b, IVF_MAGIC);
        put_u32(&mut b, IVF_VERSION);
        b.push(self.metric.tag());
        put_vecs_f32(&mut b, &self.centroids);
        put_vecs_usize(&mut b, &self.lists);
        put_vecs_f32(&mut b, &self.vectors);
        put_u32(&mut b, self.ids.len() as u32);
        for id in &self.ids {
            put_i64(&mut b, *id);
        }
        b
    }

    /// Inverse of [`to_bytes`]. Fail-fast typed `Err` (never panic) on truncation / bad magic / unknown metric —
    /// a corrupt index page surfaces as a clean AM error, never UB.
    pub(crate) fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let mut c = crate::ann::wire::Cur::new(bytes);
        if c.u32()? != IVF_MAGIC {
            return Err("theodb ivfflat: bad index page magic".into());
        }
        if c.u32()? != IVF_VERSION {
            return Err("theodb ivfflat: unsupported index page version".into());
        }
        let metric = Metric::from_tag(c.u8()?).ok_or("theodb ivfflat: unknown metric tag")?;
        let centroids = c.vecs_f32()?;
        let lists = c.vecs_usize()?;
        let vectors = c.vecs_f32()?;
        let ids = c.i64_vec()?;
        // Referential-integrity validation (M26): a structurally-complete but semantically-corrupt blob must NOT
        // reach `search` (which indexes `self.lists[..]`, `self.vectors[node]`, `self.ids[c.i]`) — an OOB index
        // there would panic across the C FFI boundary. Fail-fast with a typed Err instead.
        let n = vectors.len();
        if ids.len() != n || lists.len() != centroids.len() {
            return Err("theodb ivfflat: inconsistent counts in index page".into());
        }
        if lists.iter().flatten().any(|&node| node >= n) {
            return Err("theodb ivfflat: list references an out-of-bounds vector".into());
        }
        Ok(IvfflatIndex { metric, centroids, lists, vectors, ids })
    }
}

pub(crate) const IVF_MAGIC: u32 = 0x5449_5646; // "TIVF"
const IVF_VERSION: u32 = 1;

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
        ]
    }

    /// M89 (Roadmap v7 — ambuild streaming Increment 1) — `build_owned` (MOVE, no clone) is BYTE-IDENTICAL to the
    /// borrow-taking `build` (clone). The whole point: eliminate the redundant corpus copy without changing a single
    /// bit of the resulting index (same vectors, same order, same kmeans seed → same centroids/lists/bytes).
    #[pgrx::pg_test]
    fn ivfflat_build_owned_byte_identical() {
        let borrowed = IvfflatIndex::build(&corpus(), 2, Metric::L2, 42).to_bytes();
        let owned = IvfflatIndex::build_owned(corpus(), 2, Metric::L2, 42).to_bytes();
        assert_eq!(borrowed, owned, "build_owned MUST be byte-identical to build");
        // And the M89 accessors are consistent with the corpus.
        let idx = IvfflatIndex::build_owned(corpus(), 2, Metric::L2, 42);
        assert_eq!(idx.len(), 4);
        assert_eq!(idx.vectors().len(), 4);
        assert_eq!(idx.train_sample(2).len(), 2);
        assert_eq!(idx.train_sample(100).len(), 4, "k>=n returns all");
    }

    #[pgrx::pg_test]
    fn ivf_roundtrip_bytes_reproduces_search() {
        let idx = IvfflatIndex::build(&corpus(), 2, Metric::L2, 42);
        let bytes = idx.to_bytes();
        let back = IvfflatIndex::from_bytes(&bytes).expect("round-trip");
        // Bit-faithful: the deserialized index returns identical neighbors for the same query.
        let q = vec![1.0, 0.0, 0.0];
        assert_eq!(idx.search(&q, 3, 2), back.search(&q, 3, 2));
    }

    #[pgrx::pg_test]
    fn ivf_empty_roundtrips() {
        let idx = IvfflatIndex::build(&[], 2, Metric::Cosine, 1);
        let back = IvfflatIndex::from_bytes(&idx.to_bytes()).expect("empty round-trip");
        assert!(back.search(&[1.0, 0.0], 3, 1).is_empty());
    }

    #[pgrx::pg_test]
    fn ivf_from_bytes_rejects_truncated_and_bad_magic() {
        let good = IvfflatIndex::build(&corpus(), 2, Metric::L2, 7).to_bytes();
        assert!(IvfflatIndex::from_bytes(&good[..good.len() - 4]).is_err(), "truncated must Err");
        let mut bad = good.clone();
        bad[0] ^= 0xFF; // corrupt the magic
        assert!(IvfflatIndex::from_bytes(&bad).is_err(), "bad magic must Err");
    }
}
