//! Domain + IO (plan M22): TheoDB's own SBQ scalar quantizer + quantized ANN search.
//!
//! A permissive, std-only **Scalar Bit Quantization** (per-dimension mean threshold, `bits_per_dim` bits packed
//! into `u64`) learned from pgvectorscale's PostgreSQL-licensed SBQ (`access_method/sbq/quantize.rs`). RaBitQ
//! (vectorchord) is **AGPL — study-only, NOT borrowed** (ADR D1). Search reuses the M21 `IvfflatIndex` as the
//! f32 candidate carrier, ranks candidates by **Hamming distance** on the codes, then **reranks** the top
//! `k·over_fetch` by the exact M20 f32 kernel (`crate::vec`) — the recall-recovery pattern of pgvectorscale
//! (`sbq/storage.rs:304-328`). ZERO new dependencies (ADR D4). Coexistence (ADR D3): reads `embed_col::real[]`
//! only; never touches pgvector/pgvectorscale types/ops/indexes.
//!
//! Memory (ADR D4, honesty EC-1): own bytes/vector `ceil(dim·bits/8)` is IDENTICAL to pgvectorscale's formula at
//! the same bits — memory is **parity with pgvectorscale + ~Nx reduction vs f32 (`4·dim`)**, never "less than
//! pgvectorscale". Recall@k with rerank is the substantive differentiator.
use crate::ann::{IvfflatIndex, Metric};
use crate::ann_query::{read_corpus, require, valid_ident};
use crate::pg::err_input;

const BITS_MAX: i32 = 8;
const OVER_FETCH_MAX: i32 = 64;
const LIST_MAX: i32 = 32768;

/// Own SBQ quantizer: per-dimension mean threshold (1-bit) / z-score unary (n-bit), packed into `u64` words.
pub(crate) struct SbqQuantizer {
    bits: u8,
    mean: Vec<f32>,
    /// Per-dim standard deviation (n-bit only; empty for 1-bit).
    std: Vec<f32>,
}

impl SbqQuantizer {
    /// Train on the corpus: per-dimension mean (and std for n-bit). Order-independent → deterministic.
    pub(crate) fn train(corpus: &[Vec<f32>], bits: u8) -> Self {
        let dim = corpus.first().map(|v| v.len()).unwrap_or(0);
        let mut mean = vec![0f32; dim];
        if !corpus.is_empty() {
            for v in corpus {
                for (j, x) in v.iter().enumerate() {
                    mean[j] += x;
                }
            }
            let n = corpus.len() as f32;
            for m in mean.iter_mut() {
                *m /= n;
            }
        }
        let std = if bits > 1 && !corpus.is_empty() {
            let mut var = vec![0f32; dim];
            for v in corpus {
                for (j, x) in v.iter().enumerate() {
                    let d = x - mean[j];
                    var[j] += d * d;
                }
            }
            let n = corpus.len() as f32;
            var.iter().map(|s| (s / n).sqrt().max(1e-12)).collect()
        } else {
            Vec::new()
        };
        SbqQuantizer { bits, mean, std }
    }

    /// Encode a full f32 vector into a packed bit code. 1-bit: `x > mean[d]`. n-bit: unary z-score buckets.
    pub(crate) fn quantize(&self, v: &[f32]) -> Vec<u64> {
        let bits = self.bits as usize;
        let nwords = (v.len() * bits).div_ceil(64);
        let mut code = vec![0u64; nwords];
        if bits == 1 {
            for (i, (&x, &m)) in v.iter().zip(&self.mean).enumerate() {
                if x > m {
                    code[i / 64] |= 1u64 << (i % 64);
                }
            }
        } else {
            // n-bit unary: z-score in [-2, 2] → 0..=bits set bits (monotone in x).
            for (d, &x) in v.iter().enumerate() {
                let z = (x - self.mean[d]) / self.std[d];
                let frac = ((z + 2.0) / 4.0).clamp(0.0, 1.0);
                let ones = (frac * bits as f32).round() as usize;
                for j in 0..ones.min(bits) {
                    let bit = d * bits + j;
                    code[bit / 64] |= 1u64 << (bit % 64);
                }
            }
        }
        code
    }

    /// Storage bytes per quantized vector: `ceil(dim·bits / 64) · 8`. f32 baseline is `4·dim`.
    pub(crate) fn bytes_per_vector(dim: usize, bits: u8) -> usize {
        (dim * bits as usize).div_ceil(64) * 8
    }

    /// Serialize the codebook (bits + per-dim means + per-dim std) to LE bytes for the layout-v3 meta page
    /// (T1.1, M51). Format: `[bits:u8][dim:u32 LE][mean: dim×f32 LE][std: dim×f32 LE, n-bit only]`.
    pub(crate) fn to_meta_bytes(&self) -> Vec<u8> {
        let dim = self.mean.len();
        let mut b = Vec::with_capacity(5 + dim * 8);
        b.push(self.bits);
        b.extend_from_slice(&(dim as u32).to_le_bytes());
        for m in &self.mean {
            b.extend_from_slice(&m.to_le_bytes());
        }
        for s in &self.std {
            b.extend_from_slice(&s.to_le_bytes());
        }
        b
    }

    /// Decode a codebook from meta bytes. Validates the exact length first (Rule 8: typed `Err` on
    /// truncation/corruption, never a panic crossing the C boundary).
    pub(crate) fn from_meta_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 5 {
            return Err(format!("sbq codebook: truncated header ({} bytes, need ≥ 5)", bytes.len()));
        }
        let bits = bytes[0];
        let dim = u32::from_le_bytes(bytes[1..5].try_into().unwrap()) as usize;
        let has_std = bits > 1;
        let expected = 5 + dim * 4 + if has_std { dim * 4 } else { 0 };
        if bytes.len() != expected {
            return Err(format!("sbq codebook: expected {expected} bytes for dim {dim} bits {bits}, got {}", bytes.len()));
        }
        let mut off = 5;
        let read_f32 = |o: usize| f32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let mut mean = Vec::with_capacity(dim);
        for _ in 0..dim {
            mean.push(read_f32(off));
            off += 4;
        }
        let std = if has_std {
            let mut s = Vec::with_capacity(dim);
            for _ in 0..dim {
                s.push(read_f32(off));
                off += 4;
            }
            s
        } else {
            Vec::new()
        };
        Ok(SbqQuantizer { bits, mean, std })
    }
}

/// Hamming distance between two equal-length packed codes: `popcount(a XOR b)` (hardware `count_ones`).
pub(crate) fn hamming(a: &[u64], b: &[u64]) -> u32 {
    a.iter().zip(b).map(|(x, y)| (x ^ y).count_ones()).sum()
}

/// Hamming distance over the raw LE code bytes (as stored inline in the element tuple, M51). Byte-wise XOR
/// popcount equals the word-wise result. Compares up to the shorter length (defensive; equal-length in practice).
pub(crate) fn hamming_bytes(a: &[u8], b: &[u8]) -> u32 {
    a.iter().zip(b).map(|(x, y)| (x ^ y).count_ones()).sum()
}

/// Per-call SBQ knn knobs (numeric; already the SQL defaults when the caller omits them). Extracted (M25) so
/// `knn` is not a 12-parameter function — mirrors `ann_query::Params` (clippy `too_many_arguments`).
pub(crate) struct SbqParams {
    pub qdim: i32,
    pub k: i32,
    pub bits: i32,
    pub lists: i32,
    pub probes: i32,
    pub over_fetch: i32,
    pub seed: i64,
}

/// Validate args, read the corpus once, quantize it, build the M21 IVFFlat carrier, and for each query in the
/// flattened `queries` (`qdim`-sized chunks): generate candidates (IVFFlat probes) → Hamming rank → keep top
/// `k·over_fetch` → full-precision f32 rerank → top-k. Returns `(query_idx, id, distance)` rows (f32 distance).
pub(crate) fn knn(
    src_table: &str,
    embed_col: &str,
    id_col: &str,
    metric_s: &str,
    queries: &[f32],
    p: SbqParams,
) -> Vec<(i32, i64, f64)> {
    // --- boundary validation (Rule 8; typed 22023) ---
    let metric = Metric::parse(metric_s)
        .unwrap_or_else(|| err_input(&format!("theodb sbq: unknown metric '{metric_s}' (use l2|cosine|ip)")));
    require(valid_ident(embed_col), "theodb sbq: embed_col is not a valid identifier");
    require(valid_ident(id_col), "theodb sbq: id_col is not a valid identifier");
    require(p.qdim >= 1, "theodb sbq: qdim must be >= 1");
    require(p.k >= 1, "theodb sbq: k must be >= 1");
    require((1..=BITS_MAX).contains(&p.bits), "theodb sbq: bits must be in [1, 8]");
    require((1..=OVER_FETCH_MAX).contains(&p.over_fetch), "theodb sbq: over_fetch must be in [1, 64]");
    require((1..=LIST_MAX).contains(&p.lists), "theodb sbq: lists must be in [1, 32768]");
    require((1..=LIST_MAX).contains(&p.probes), "theodb sbq: probes must be in [1, 32768]");

    // Empty queries → 0 rows, no Spi read / build (EC-5).
    if queries.is_empty() {
        return Vec::new();
    }
    let qd = p.qdim as usize;
    if !queries.len().is_multiple_of(qd) {
        err_input(&format!(
            "theodb sbq: queries length {} is not a multiple of qdim {qd}",
            queries.len()
        ));
    }

    let corpus = read_corpus(src_table, embed_col, id_col, qd);
    let vecs: Vec<Vec<f32>> = corpus.iter().map(|(_, v)| v.clone()).collect();
    let quant = SbqQuantizer::train(&vecs, p.bits as u8);
    let codes: Vec<Vec<u64>> = vecs.iter().map(|v| quant.quantize(v)).collect();
    let carrier = IvfflatIndex::build(&corpus, p.lists as usize, metric, p.seed as u64);

    let k = p.k as usize;
    let of = p.over_fetch as usize;
    let mut out: Vec<(i32, i64, f64)> = Vec::new();
    for (qi, chunk) in queries.chunks(qd).enumerate() {
        let qcode = quant.quantize(chunk);
        // Candidate generation (f32 carrier) → Hamming rank → keep top k*over_fetch.
        let mut cand = carrier.candidate_positions(chunk, p.probes as usize);
        cand.sort_by_key(|&i| hamming(&qcode, &codes[i]));
        cand.truncate(k * of);
        // Full-precision rerank → top-k. `metric.dist` is the single-source metric→kernel mapping (M25 DRY).
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

// Unit tests (plan T1.1/T2.1). `#[pg_test]` because the crate links pg symbols via `crate::vec`/`crate::pg`.
// The standalone prototype (validated 6/6: recall@10 = 0.86 at 1-bit + over_fetch=8) + the Python container gate
// are the always-on proofs.
#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use super::*;
    use crate::ann::Rng; // M25: pre-existing missing import (test only compiled under pg_test before)
    use pgrx::prelude::*;

    #[pg_test]
    fn sbq_quantize_1bit_threshold() {
        let s = SbqQuantizer::train(&[vec![0.0, 0.0], vec![2.0, 2.0]], 1); // mean [1,1]
        assert_eq!(s.quantize(&[3.0, 0.0]), vec![0b01]); // dim0: 3>1 set; dim1: 0>1 no
    }
    #[pg_test]
    fn sbq_hamming_counts_bit_diffs() {
        assert_eq!(hamming(&[0b1010], &[0b0011]), 2);
    }
    #[pg_test]
    fn sbq_bytes_per_vector_formula() {
        assert_eq!(SbqQuantizer::bytes_per_vector(1024, 1), 128);
        assert_eq!(SbqQuantizer::bytes_per_vector(768, 1), 96);
        assert_eq!(SbqQuantizer::bytes_per_vector(1536, 2), 384);
        assert_eq!(SbqQuantizer::bytes_per_vector(1024, 1) * 32, 1024 * 4); // 32x vs f32
    }
    #[pg_test]
    fn sbq_value_equal_mean_encodes_zero() {
        let s = SbqQuantizer::train(&[vec![0.0], vec![2.0]], 1); // mean [1]
        assert_eq!(s.quantize(&[1.0]), vec![0]); // 1 > 1 is false (strict, parity with pgvector EC-5)
    }
    #[pg_test]
    fn sbq_quantize_2bit_monotonic() {
        // n-bit: a larger value along a dim sets STRICTLY more bits at the z-score extremes (EC-3). Use a
        // wider corpus + well-separated probe values so the bit counts actually differ (not a trivial pass).
        let s = SbqQuantizer::train(&[vec![-4.0], vec![-2.0], vec![0.0], vec![2.0], vec![4.0]], 2); // mean 0
        let lo = s.quantize(&[-4.0]).iter().map(|w| w.count_ones()).sum::<u32>(); // z≈-1.4 → 0 bits
        let hi = s.quantize(&[4.0]).iter().map(|w| w.count_ones()).sum::<u32>(); // z≈+1.4 → 2 bits
        assert!(hi > lo, "2-bit not strictly monotone at extremes: lo={lo} hi={hi}");
        assert_eq!(SbqQuantizer::bytes_per_vector(1, 2), 8);
    }

    #[pg_test]
    fn sbq_hamming_correlates_with_f32_distance() {
        // QUANTIZER-VALIDITY (review TST): isolate the quantizer's signal — prove Hamming on the codes ORDERS
        // neighbours like the true f32 distance (independent of the IVFFlat carrier + f32 rerank that the recall
        // gate also exercises). For a query, the f32-nearest half of the corpus must have a LOWER mean Hamming
        // distance than the f32-farthest half. A broken quantizer (no signal) would show ~equal means.
        let mut r = Rng::new(7);
        let dim = 24;
        let corpus: Vec<Vec<f32>> = (0..400)
            .map(|_| (0..dim).map(|_| (r.next_f64() as f32) * 2.0 - 1.0).collect())
            .collect();
        let q = SbqQuantizer::train(&corpus, 1);
        let codes: Vec<Vec<u64>> = corpus.iter().map(|v| q.quantize(v)).collect();
        let query: Vec<f32> = (0..dim).map(|_| (r.next_f64() as f32) * 2.0 - 1.0).collect();
        let qcode = q.quantize(&query);
        // rank corpus by true f32 L2
        let mut by_f32: Vec<usize> = (0..corpus.len()).collect();
        by_f32.sort_by(|&a, &b| {
            crate::vec::l2_distance(&corpus[a], &query)
                .partial_cmp(&crate::vec::l2_distance(&corpus[b], &query))
                .unwrap()
        });
        let half = corpus.len() / 2;
        let near_ham: f64 = by_f32[..half].iter().map(|&i| hamming(&qcode, &codes[i]) as f64).sum::<f64>() / half as f64;
        let far_ham: f64 = by_f32[half..].iter().map(|&i| hamming(&qcode, &codes[i]) as f64).sum::<f64>() / (corpus.len() - half) as f64;
        assert!(
            near_ham < far_ham - 0.5,
            "Hamming carries no NN signal: near_mean={near_ham:.2} not < far_mean={far_ham:.2}"
        );
    }
    #[pg_test]
    fn sbq_codebook_roundtrips_through_meta_bytes() {
        // T1.1 (M51): o codebook (means/std/bits) serializa para bytes e volta byte-exato — é o que a meta
        // page do layout v3 persiste para o scan reproduzir a quantização do build (means fixados no build).
        let q = SbqQuantizer::train(&[vec![1.0, -2.0, 3.0], vec![3.0, 0.0, 1.0], vec![-1.0, 2.0, 0.0]], 2);
        let bytes = q.to_meta_bytes();
        let q2 = SbqQuantizer::from_meta_bytes(&bytes).expect("codebook decodes");
        assert_eq!(q.bits, q2.bits, "bits mismatch");
        assert_eq!(q.mean, q2.mean, "mean mismatch");
        assert_eq!(q.std, q2.std, "std mismatch");
        // e o comportamento observável (quantize) é idêntico
        assert_eq!(q.quantize(&[2.0, -1.0, 2.0]), q2.quantize(&[2.0, -1.0, 2.0]));
    }
    #[pg_test]
    fn sbq_codebook_from_bytes_rejects_truncated() {
        // negative case (Rule 8): bytes truncados → Err tipado, nunca panic.
        let q = SbqQuantizer::train(&[vec![1.0, 2.0]], 1);
        let mut bytes = q.to_meta_bytes();
        bytes.truncate(bytes.len() - 1);
        assert!(SbqQuantizer::from_meta_bytes(&bytes).is_err(), "truncated codebook must be rejected");
    }
    #[pg_test]
    fn sbq_deterministic_train() {
        let c = vec![vec![1.0, 2.0], vec![3.0, 4.0], vec![5.0, 6.0]];
        let a = SbqQuantizer::train(&c, 1);
        let b = SbqQuantizer::train(&c, 1);
        assert_eq!(a.quantize(&[2.0, 2.0]), b.quantize(&[2.0, 2.0]));
    }

    #[pg_test]
    fn sbq_knn_smoke() {
        Spi::run("CREATE TEMP TABLE sbq_t (id int PRIMARY KEY, e vector(2))").unwrap();
        Spi::run("INSERT INTO sbq_t VALUES (0,'[0,0]'),(1,'[1,0]'),(2,'[5,5]')").unwrap();
        let r = knn(
            "sbq_t", "e", "id", "l2", &[0.0, 0.0],
            SbqParams { qdim: 2, k: 2, bits: 1, lists: 4, probes: 4, over_fetch: 8, seed: 42 },
        );
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].1, 0); // nearest is id 0
    }
    #[pg_test]
    fn sbq_overfetch_exceeds_pool_no_panic() {
        Spi::run("CREATE TEMP TABLE sbq_of (id int PRIMARY KEY, e vector(1))").unwrap();
        Spi::run("INSERT INTO sbq_of VALUES (0,'[0]'),(1,'[1]'),(2,'[2]')").unwrap();
        // over_fetch huge + tiny corpus → returns min(k, available), no panic (EC-2).
        let r = knn(
            "sbq_of", "e", "id", "l2", &[0.0],
            SbqParams { qdim: 1, k: 10, bits: 1, lists: 2, probes: 2, over_fetch: 64, seed: 42 },
        );
        assert_eq!(r.len(), 3);
    }
    #[pg_test(error = "bits must be in")]
    fn sbq_knn_bad_bits_rejected() {
        let _ = knn(
            "t", "e", "id", "l2", &[0.0],
            SbqParams { qdim: 1, k: 5, bits: 9, lists: 4, probes: 4, over_fetch: 8, seed: 42 },
        );
    }
    #[pg_test(error = "unknown metric")]
    fn sbq_knn_bad_metric_rejected() {
        let _ = knn(
            "t", "e", "id", "nope", &[0.0],
            SbqParams { qdim: 1, k: 5, bits: 1, lists: 4, probes: 4, over_fetch: 8, seed: 42 },
        );
    }
}
