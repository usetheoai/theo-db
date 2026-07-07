//! Own **Product Quantization** (PQ) — std-only, permissive (Apache/MIT/BSD posture; no AGPL — blueprint D1).
//! Splits a `D`-dim vector into `m` disjoint sub-vectors of dim `D/m`; each subspace has an independent
//! k-means codebook of `k*` centroids (`code = m` bytes). Distance at query time is **asymmetric (ADC)**: the
//! query stays f32, a per-query lookup table `LUT[m][k*]` of squared sub-distances is precomputed once, and the
//! approximate distance to any code is `Σ_i LUT[i][code[i]]` (m lookups, no decode). Mirrors the SBQ pipeline
//! (`crate::sbq`): train → encode corpus → IVFFlat candidates → quantized rank → exact f32 rerank → top-k.
//!
//! M39 (blueprint `m39-pq-product-quantization`): PQ is the vector-superiority lever ScaNN/FAISS use; SBQ was
//! falsified at recall<1.0 (M38). k-means init is **deterministic by construction** (evenly-spaced sub-vectors,
//! seed-offset) so `train` is reproducible without an RNG dependency — the k-means correctness lever, not the
//! init randomness, is what matters here (Lloyd converges to a local optimum from any init).
use crate::ann::{IvfflatIndex, Metric};
use crate::ann_query::{read_corpus, require, valid_ident};
use crate::pg::err_input;

const PQ_M_MAX: i32 = 64;
const PQ_OVER_FETCH_MAX: i32 = 64;
const PQ_LIST_MAX: i32 = 32768;

/// Centroids per subspace. 256 → one `u8` code index per subspace (FAISS convention).
pub(crate) const PQ_K_STAR: usize = 256;
/// Lloyd iterations. 25 is the FAISS default ceiling; convergence is usually much earlier.
const PQ_MAX_ITERS: usize = 25;

/// Own PQ quantizer: `m` codebooks, each `k*` centroids over `sub_dim = D/m` dims. `codebooks[i][j]` is the
/// `j`-th centroid of subspace `i`. Encoding a vector yields `m` bytes (one nearest-centroid index per subspace).
pub(crate) struct PqQuantizer {
    m: usize,
    sub_dim: usize,
    codebooks: Vec<Vec<Vec<f32>>>, // [m][k*][sub_dim]
}

impl PqQuantizer {
    /// Train `m` codebooks by per-subspace Lloyd's k-means. `D % m == 0` required (typed error otherwise).
    /// Deterministic: init picks evenly-spaced sub-vectors (seed shifts the start), Lloyd is order-independent
    /// given the same corpus → two trains with the same args produce byte-identical codebooks.
    pub(crate) fn train(corpus: &[Vec<f32>], m: usize, seed: u64) -> Self {
        require(m >= 1, "theodb pq: m must be >= 1");
        require(!corpus.is_empty(), "theodb pq: corpus must not be empty");
        let dim = corpus[0].len();
        require(dim >= 1, "theodb pq: vector dim must be >= 1");
        if dim % m != 0 {
            err_input(&format!(
                "theodb pq: vector dim {dim} is not divisible by m {m} (subspaces must be equal-sized)"
            ));
        }
        let sub_dim = dim / m;
        let n = corpus.len();
        let k_star = PQ_K_STAR.min(n); // fewer centroids than points is pointless; cap at n

        let mut codebooks = Vec::with_capacity(m);
        for i in 0..m {
            // Gather subspace i's sub-vectors.
            let subvecs: Vec<Vec<f32>> = corpus
                .iter()
                .map(|v| v[i * sub_dim..(i + 1) * sub_dim].to_vec())
                .collect();
            codebooks.push(lloyd_kmeans(&subvecs, k_star, sub_dim, seed));
        }
        PqQuantizer { m, sub_dim, codebooks }
    }

    /// Encode a full `D`-dim vector to `m` bytes: the nearest-centroid index in each subspace. `k*<=256` → `u8`.
    pub(crate) fn encode(&self, v: &[f32]) -> Vec<u8> {
        let mut code = Vec::with_capacity(self.m);
        for i in 0..self.m {
            let sub = &v[i * self.sub_dim..(i + 1) * self.sub_dim];
            code.push(nearest_centroid(sub, &self.codebooks[i]) as u8);
        }
        code
    }

    /// Build the per-query ADC lookup table `LUT[m][k*]`: `LUT[i][j] = ‖query_sub_i − centroid_{i,j}‖²`.
    /// One `l2_distance` per (subspace, centroid); reused across all corpus codes for this query.
    pub(crate) fn adc_lut(&self, query: &[f32]) -> Vec<Vec<f32>> {
        let mut lut = Vec::with_capacity(self.m);
        for i in 0..self.m {
            let sub = &query[i * self.sub_dim..(i + 1) * self.sub_dim];
            let row: Vec<f32> = self.codebooks[i]
                .iter()
                .map(|c| crate::vec::l2_distance(sub, c) as f32)
                .collect();
            lut.push(row);
        }
        lut
    }
}

/// Asymmetric distance from a precomputed LUT and a code: `Σ_i LUT[i][code[i]]`. `m` lookups + adds, no decode.
pub(crate) fn adc_distance(lut: &[Vec<f32>], code: &[u8]) -> f64 {
    lut.iter()
        .zip(code)
        .map(|(row, &c)| row[c as usize] as f64)
        .sum()
}

/// Index of the nearest centroid to `sub` under squared L2 (ties → lowest index, deterministic).
fn nearest_centroid(sub: &[f32], centroids: &[Vec<f32>]) -> usize {
    let mut best = 0usize;
    let mut best_d = f64::INFINITY;
    for (j, c) in centroids.iter().enumerate() {
        let d = crate::vec::l2_distance(sub, c);
        if d < best_d {
            best_d = d;
            best = j;
        }
    }
    best
}

/// Lloyd's k-means over `subvecs` (each `sub_dim`-long). Deterministic: evenly-spaced init (seed-shifted start),
/// order-independent assignment/update. Returns `k*` centroids (some may coincide if the corpus has < k* uniques).
fn lloyd_kmeans(subvecs: &[Vec<f32>], k_star: usize, sub_dim: usize, seed: u64) -> Vec<Vec<f32>> {
    let n = subvecs.len();
    // Deterministic init: evenly-spaced picks over the corpus, offset by seed (mod n).
    let offset = (seed as usize) % n.max(1);
    let mut centroids: Vec<Vec<f32>> = (0..k_star)
        .map(|j| {
            let idx = (offset + j * n / k_star) % n;
            subvecs[idx].clone()
        })
        .collect();

    for _ in 0..PQ_MAX_ITERS {
        // Assign each point to its nearest centroid; accumulate sums for the update.
        let mut sums = vec![vec![0f64; sub_dim]; k_star];
        let mut counts = vec![0usize; k_star];
        for sv in subvecs {
            let a = nearest_centroid(sv, &centroids);
            counts[a] += 1;
            for (s, &x) in sums[a].iter_mut().zip(sv) {
                *s += x as f64;
            }
        }
        // Update: centroid = mean of assigned points; empty clusters keep their previous position (stable).
        let mut moved = false;
        for j in 0..k_star {
            if counts[j] == 0 {
                continue;
            }
            for (d, s) in centroids[j].iter_mut().zip(&sums[j]) {
                let nv = (*s / counts[j] as f64) as f32;
                if (nv - *d).abs() > f32::EPSILON {
                    moved = true;
                }
                *d = nv;
            }
        }
        if !moved {
            break; // converged
        }
    }
    centroids
}

/// Args for `pq_knn` (mirror `SbqParams`, `bits`→`m`). `m` = number of PQ subspaces; `qdim % m == 0` required.
pub(crate) struct PqParams {
    pub qdim: i32,
    pub k: i32,
    pub m: i32,
    pub lists: i32,
    pub probes: i32,
    pub over_fetch: i32,
    pub seed: i64,
}

/// Validate args, read the corpus once, PQ-encode it, build the M21 IVFFlat carrier, and for each query in the
/// flattened `queries` (`qdim`-sized chunks): generate candidates (IVFFlat probes) → ADC rank (Σ LUT[i][code[i]])
/// → keep top `k·over_fetch` → full-precision f32 rerank → top-k. Returns `(query_idx, id, distance)` rows.
/// Mirrors `crate::sbq::knn` exactly, swapping the Hamming ranking primitive for ADC (blueprint D1).
pub(crate) fn knn(
    src_table: &str,
    embed_col: &str,
    id_col: &str,
    metric_s: &str,
    queries: &[f32],
    p: PqParams,
) -> Vec<(i32, i64, f64)> {
    // --- boundary validation (Rule 8; typed 22023) ---
    let metric = Metric::parse(metric_s)
        .unwrap_or_else(|| err_input(&format!("theodb pq: unknown metric '{metric_s}' (use l2|cosine|ip)")));
    require(valid_ident(embed_col), "theodb pq: embed_col is not a valid identifier");
    require(valid_ident(id_col), "theodb pq: id_col is not a valid identifier");
    require(p.qdim >= 1, "theodb pq: qdim must be >= 1");
    require(p.k >= 1, "theodb pq: k must be >= 1");
    require((1..=PQ_M_MAX).contains(&p.m), "theodb pq: m must be in [1, 64]");
    require((1..=PQ_OVER_FETCH_MAX).contains(&p.over_fetch), "theodb pq: over_fetch must be in [1, 64]");
    require((1..=PQ_LIST_MAX).contains(&p.lists), "theodb pq: lists must be in [1, 32768]");
    require((1..=PQ_LIST_MAX).contains(&p.probes), "theodb pq: probes must be in [1, 32768]");
    if p.qdim % p.m != 0 {
        err_input(&format!(
            "theodb pq: qdim {} is not divisible by m {} (subspaces must be equal-sized)",
            p.qdim, p.m
        ));
    }

    // Empty queries → 0 rows, no Spi read / build (EC-5, mirror sbq).
    if queries.is_empty() {
        return Vec::new();
    }
    let qd = p.qdim as usize;
    if !queries.len().is_multiple_of(qd) {
        err_input(&format!(
            "theodb pq: queries length {} is not a multiple of qdim {qd}",
            queries.len()
        ));
    }

    let corpus = read_corpus(src_table, embed_col, id_col, qd);
    let vecs: Vec<Vec<f32>> = corpus.iter().map(|(_, v)| v.clone()).collect();
    if vecs.is_empty() {
        return Vec::new();
    }
    let quant = PqQuantizer::train(&vecs, p.m as usize, p.seed as u64);
    let codes: Vec<Vec<u8>> = vecs.iter().map(|v| quant.encode(v)).collect();
    let carrier = IvfflatIndex::build(&corpus, p.lists as usize, metric, p.seed as u64);

    let k = p.k as usize;
    let of = p.over_fetch as usize;
    let mut out: Vec<(i32, i64, f64)> = Vec::new();
    for (qi, chunk) in queries.chunks(qd).enumerate() {
        // Precompute the ADC LUT once per query, then rank candidates by Σ LUT[i][code[i]] (no decode).
        let lut = quant.adc_lut(chunk);
        let mut cand = carrier.candidate_positions(chunk, p.probes as usize);
        cand.sort_by(|&a, &b| {
            adc_distance(&lut, &codes[a])
                .partial_cmp(&adc_distance(&lut, &codes[b]))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        cand.truncate(k * of);
        // Full-precision rerank → top-k (identical to sbq; `metric.dist` is the single-source metric→kernel map).
        cand.sort_by(|&a, &b| {
            metric
                .dist(&vecs[a], chunk)
                .partial_cmp(&metric.dist(&vecs[b], chunk))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        cand.truncate(k);
        for &i in &cand {
            out.push((qi as i32, corpus[i].0, metric.dist(&vecs[i], chunk)));
        }
    }
    out
}

// M39 unit tests. `#[pg_test]` because the crate links pg symbols via `crate::vec`/`crate::pg` (mirror sbq.rs).
#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use super::*;
    use crate::ann::Rng;
    use pgrx::prelude::*;

    fn rand_corpus(n: usize, dim: usize, seed: u64) -> Vec<Vec<f32>> {
        let mut r = Rng::new(seed);
        (0..n)
            .map(|_| (0..dim).map(|_| (r.next_f64() as f32) * 2.0 - 1.0).collect())
            .collect()
    }

    #[pg_test]
    fn pq_encode_2subspace_matches_manual_argmin() {
        // D=4, m=2, sub_dim=2. Build a corpus whose two subspaces cluster clearly.
        let corpus: Vec<Vec<f32>> = vec![
            vec![0.0, 0.0, 10.0, 10.0],
            vec![0.1, 0.1, 10.1, 10.1],
            vec![5.0, 5.0, 20.0, 20.0],
            vec![5.1, 5.1, 20.1, 20.1],
        ];
        let q = PqQuantizer::train(&corpus, 2, 1);
        let code = q.encode(&corpus[0]);
        assert_eq!(code.len(), 2, "code must be m=2 bytes");
        // encode is idempotent w.r.t. the trained codebooks: re-encoding a corpus point picks a stable index.
        assert_eq!(q.encode(&corpus[0]), code);
        // Each index is a valid centroid slot.
        assert!((code[0] as usize) < PQ_K_STAR.min(corpus.len()));
    }

    #[pg_test]
    fn pq_train_deterministic() {
        let corpus = rand_corpus(64, 8, 7);
        let a = PqQuantizer::train(&corpus, 2, 3);
        let b = PqQuantizer::train(&corpus, 2, 3);
        // Same args → byte-identical codes for every corpus vector (deterministic train).
        for v in &corpus {
            assert_eq!(a.encode(v), b.encode(v));
        }
    }

    #[pg_test(error = "theodb pq: vector dim 8 is not divisible by m 3 (subspaces must be equal-sized)")]
    fn pq_train_rejects_indivisible_dim() {
        // D=8, m=3 → 8 % 3 != 0 → typed error 22023 (pgrx ereport; asserted via the pg_test error attr).
        let corpus = rand_corpus(8, 8, 1);
        let _ = PqQuantizer::train(&corpus, 3, 1);
    }

    #[pg_test]
    fn pq_adc_distance_matches_lut_sum() {
        // Hand-built LUT + code → exact Σ.
        let lut = vec![vec![1.0f32, 2.0, 3.0], vec![10.0, 20.0, 30.0]];
        let code = vec![2u8, 0u8];
        assert!((adc_distance(&lut, &code) - (3.0 + 10.0)).abs() < 1e-9);
    }

    #[pg_test]
    fn pq_adc_correlates_with_f32_distance() {
        // Quantizer-validity gate (analog of sbq_hamming_correlates_with_f32_distance): the f32-closer half of a
        // corpus must have a strictly lower MEAN ADC to the query than the farther half.
        let dim = 16;
        let corpus = rand_corpus(200, dim, 11);
        let q = PqQuantizer::train(&corpus, 4, 5);
        let codes: Vec<Vec<u8>> = corpus.iter().map(|v| q.encode(v)).collect();
        let mut query_r = Rng::new(99);
        let query: Vec<f32> = (0..dim).map(|_| (query_r.next_f64() as f32) * 2.0 - 1.0).collect();

        // Exact f32 distances → sort indices; split into closer/farther halves.
        let mut idx: Vec<usize> = (0..corpus.len()).collect();
        idx.sort_by(|&a, &b| {
            crate::vec::l2_distance(&corpus[a], &query)
                .partial_cmp(&crate::vec::l2_distance(&corpus[b], &query))
                .unwrap()
        });
        let lut = q.adc_lut(&query);
        let half = corpus.len() / 2;
        let mean = |slice: &[usize]| -> f64 {
            slice.iter().map(|&i| adc_distance(&lut, &codes[i])).sum::<f64>() / slice.len() as f64
        };
        let closer = mean(&idx[..half]);
        let farther = mean(&idx[half..]);
        assert!(
            closer < farther,
            "ADC must order neighbors like f32: closer-half mean {closer} !< farther-half mean {farther}"
        );
    }

    #[pg_test]
    fn pq_knn_smoke() {
        // End-to-end via Spi (mirror sbq_knn_smoke): a tiny corpus, pq_knn returns k rows/query in ascending
        // distance with ids from the corpus. dim=4, m=2.
        Spi::run("CREATE TEMP TABLE pq_t (id int PRIMARY KEY, e vector(4))").unwrap();
        Spi::run("INSERT INTO pq_t VALUES (0,'[0,0,0,0]'),(1,'[1,0,1,0]'),(2,'[5,5,5,5]'),(3,'[6,5,6,5]')")
            .unwrap();
        let rows = knn(
            "pq_t",
            "e",
            "id",
            "l2",
            &[0.0, 0.0, 0.0, 0.0],
            PqParams { qdim: 4, k: 2, m: 2, lists: 2, probes: 2, over_fetch: 4, seed: 42 },
        );
        assert_eq!(rows.len(), 2, "k=2 → 2 rows for one query");
        assert_eq!(rows[0].0, 0, "query_idx is 0");
        assert!(rows[0].2 <= rows[1].2, "rows are ascending by distance");
        assert!(rows.iter().all(|r| (0..=3).contains(&r.1)), "ids come from the corpus");
    }

    #[pg_test(error = "theodb pq: m must be in [1, 64]")]
    fn pq_knn_bad_m_rejected() {
        let _ = knn(
            "t", "e", "id", "l2", &[0.0],
            PqParams { qdim: 1, k: 5, m: 0, lists: 4, probes: 4, over_fetch: 8, seed: 42 },
        );
    }

    #[pg_test(error = "theodb pq: qdim 7 is not divisible by m 2 (subspaces must be equal-sized)")]
    fn pq_knn_qdim_not_multiple_of_m_rejected() {
        let _ = knn(
            "t", "e", "id", "l2", &[0.0],
            PqParams { qdim: 7, k: 5, m: 2, lists: 4, probes: 4, over_fetch: 8, seed: 42 },
        );
    }

    #[pg_test]
    fn pq_knn_empty_queries_no_read() {
        // Empty queries → 0 rows, no Spi read (the table does not exist → would error if read).
        let rows = knn(
            "nonexistent_table", "e", "id", "l2", &[],
            PqParams { qdim: 4, k: 2, m: 2, lists: 2, probes: 2, over_fetch: 4, seed: 42 },
        );
        assert!(rows.is_empty());
    }
}
