//! IVFFlat — own k-means++ inverted-list index (plan M21 T1.2).
//! Shared primitives ([`Metric`], [`Rng`], [`Cand`]) live in the parent `ann` module.
use super::{Cand, Metric, Rng};

/// Bounded Lloyd (k-means) refinement iterations — enough to converge centroids without unbounded work.
const LLOYD_ITERS: usize = 10;

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
        for _ in 0..LLOYD_ITERS {
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

    /// The index's metric (M26 — the AM reads it back after `from_bytes` to score pending tuples consistently).
    pub(crate) fn metric(&self) -> Metric {
        self.metric
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
            out.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
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
            .map(|(i, c)| Cand {
                d: self.metric.dist(q, c),
                i,
            })
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
        let mut b = Vec::new();
        b.extend_from_slice(&IVF_MAGIC.to_le_bytes());
        b.extend_from_slice(&IVF_VERSION.to_le_bytes());
        b.push(self.metric.tag());
        put_vecs_f32(&mut b, &self.centroids);
        put_vecs_usize(&mut b, &self.lists);
        put_vecs_f32(&mut b, &self.vectors);
        put_u32(&mut b, self.ids.len() as u32);
        for id in &self.ids {
            b.extend_from_slice(&id.to_le_bytes());
        }
        b
    }

    /// Inverse of [`to_bytes`]. Fail-fast typed `Err` (never panic) on truncation / bad magic / unknown metric —
    /// a corrupt index page surfaces as a clean AM error, never UB.
    pub(crate) fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let mut c = Cur { b: bytes, p: 0 };
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
        let n_ids = c.u32()? as usize;
        let mut ids = Vec::with_capacity(n_ids);
        for _ in 0..n_ids {
            ids.push(c.i64()?);
        }
        Ok(IvfflatIndex { metric, centroids, lists, vectors, ids })
    }
}

const IVF_MAGIC: u32 = 0x5449_5646; // "TIVF"
const IVF_VERSION: u32 = 1;

fn put_u32(b: &mut Vec<u8>, n: u32) {
    b.extend_from_slice(&n.to_le_bytes());
}
fn put_vecs_f32(b: &mut Vec<u8>, vs: &[Vec<f32>]) {
    put_u32(b, vs.len() as u32);
    for v in vs {
        put_u32(b, v.len() as u32);
        for x in v {
            b.extend_from_slice(&x.to_le_bytes());
        }
    }
}
fn put_vecs_usize(b: &mut Vec<u8>, vs: &[Vec<usize>]) {
    put_u32(b, vs.len() as u32);
    for v in vs {
        put_u32(b, v.len() as u32);
        for x in v {
            b.extend_from_slice(&(*x as u64).to_le_bytes());
        }
    }
}

/// A bounds-checked little-endian read cursor (fail-fast `Err` on short read — never panics on a corrupt page).
struct Cur<'a> {
    b: &'a [u8],
    p: usize,
}
impl Cur<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let end = self.p.checked_add(n).ok_or("theodb ivfflat: length overflow")?;
        if end > self.b.len() {
            return Err("theodb ivfflat: truncated index page".into());
        }
        let s = &self.b[self.p..end];
        self.p = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i64(&mut self) -> Result<i64, String> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn vecs_f32(&mut self) -> Result<Vec<Vec<f32>>, String> {
        let outer = self.u32()? as usize;
        let mut out = Vec::with_capacity(outer);
        for _ in 0..outer {
            let inner = self.u32()? as usize;
            let mut v = Vec::with_capacity(inner);
            for _ in 0..inner {
                v.push(self.f32()?);
            }
            out.push(v);
        }
        Ok(out)
    }
    fn vecs_usize(&mut self) -> Result<Vec<Vec<usize>>, String> {
        let outer = self.u32()? as usize;
        let mut out = Vec::with_capacity(outer);
        for _ in 0..outer {
            let inner = self.u32()? as usize;
            let mut v = Vec::with_capacity(inner);
            for _ in 0..inner {
                v.push(self.u64()? as usize);
            }
            out.push(v);
        }
        Ok(out)
    }
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod ivf_persist_tests {
    use super::*;

    fn corpus() -> Vec<(i64, Vec<f32>)> {
        vec![
            (10, vec![1.0, 0.0, 0.0]),
            (20, vec![0.0, 1.0, 0.0]),
            (30, vec![0.0, 0.0, 1.0]),
            (40, vec![0.9, 0.1, 0.0]),
        ]
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
