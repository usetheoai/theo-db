//! M85 (Roadmap v7) — SQ8 scalar quantizer for the v6 storage-separated IVF-AQ refine tier.
//!
//! Per-dimension 8-bit scalar quantization (FAISS `QT_8bit`, non-uniform): each dimension `i` is mapped from
//! `[vmin[i], vmax[i]]` to a byte, so a vector shrinks from `dim*4` bytes (f32) to `dim` bytes (¼ at any dim).
//! The v5 storage-separated scan reranks survivors on the raw f32 (512 B/vec at dim=128); the v6 scan reranks on
//! these SQ8 codes (128 B/vec) — 4× less Stage-2 random-read I/O, at a small recall cost (SQ8 is approximate).
//! Rerank is **asymmetric**: the query stays f32, only the stored side is quantized (decode-then-metric), which
//! preserves recall better than a symmetric int8 path and keeps the byte-identical-recall test discipline
//! (`sq8_l2_correlates_with_f32_distance`). Domain layer — no `pg_sys` (mirrors `am::aq`); consumes only `crate::vec`.
//!
//! Encode/decode follow FAISS `Codec8bit` exactly: `code = (255 * clamp((x-vmin)/vdiff, 0, 1)) as u8`;
//! reconstruct `x̂ = vmin + ((code+0.5)/255) * vdiff` (bin-center). `train` is order-independent → deterministic
//! codebook (mirrors `sbq::SbqQuantizer::train`), so a parallel-drain build yields a byte-identical codebook.

/// A trained per-dimension SQ8 codebook: `vmin[i]` / `vdiff[i]` (= `vmax[i]-vmin[i]`) per dimension.
pub(crate) struct Sq8Quantizer {
    vmin: Vec<f32>,
    vdiff: Vec<f32>,
}

impl Sq8Quantizer {
    /// Train per-dimension min/max over the corpus (FAISS `RS_minmax`). Order-independent → deterministic.
    /// An empty corpus (or dim 0) yields an empty codebook; a degenerate dimension (`vmax==vmin`) gets `vdiff=0`
    /// (every value in that dim encodes to 0, decodes to `vmin` — no divide-by-zero).
    pub(crate) fn train(corpus: &[Vec<f32>]) -> Self {
        let dim = corpus.first().map(|v| v.len()).unwrap_or(0);
        let mut vmin = vec![f32::INFINITY; dim];
        let mut vmax = vec![f32::NEG_INFINITY; dim];
        for v in corpus {
            for (i, &x) in v.iter().enumerate().take(dim) {
                if x < vmin[i] {
                    vmin[i] = x;
                }
                if x > vmax[i] {
                    vmax[i] = x;
                }
            }
        }
        let mut vdiff = vec![0f32; dim];
        for i in 0..dim {
            // Guard an untouched dim (empty corpus already handled by dim==0; belt-and-suspenders for NaN/Inf).
            if !vmin[i].is_finite() || !vmax[i].is_finite() {
                vmin[i] = 0.0;
                vdiff[i] = 0.0;
            } else {
                vdiff[i] = (vmax[i] - vmin[i]).max(0.0);
            }
        }
        Sq8Quantizer { vmin, vdiff }
    }

    /// Encode a vector to `dim` bytes (FAISS `Codec8bit::encode_component`, truncating).
    pub(crate) fn encode(&self, v: &[f32]) -> Vec<u8> {
        let dim = self.vmin.len();
        let mut out = Vec::with_capacity(dim);
        for i in 0..dim {
            let x = *v.get(i).unwrap_or(&0.0);
            let xi = if self.vdiff[i] > 0.0 {
                ((x - self.vmin[i]) / self.vdiff[i]).clamp(0.0, 1.0)
            } else {
                0.0
            };
            out.push((255.0 * xi) as u8);
        }
        out
    }

    /// Reconstruct the approximate f32 vector from its SQ8 code (bin-center decode). Used by the scan rerank,
    /// which then applies the exact `Metric::dist` against the f32 query (asymmetric).
    pub(crate) fn decode(&self, code: &[u8]) -> Vec<f32> {
        let dim = self.vmin.len();
        let mut out = Vec::with_capacity(dim);
        for i in 0..dim {
            let c = *code.get(i).unwrap_or(&0) as f32;
            let xi = (c + 0.5) / 255.0;
            out.push(self.vmin[i] + xi * self.vdiff[i]);
        }
        out
    }

    /// Bytes an SQ8 code occupies — exactly `dim` (1 byte/dim).
    pub(crate) fn bytes_per_vector(dim: usize) -> usize {
        dim
    }

    /// Serialize the codebook: `[dim u32][vmin f32×dim][vdiff f32×dim]`. Round-trips through `from_meta_bytes`.
    pub(crate) fn to_meta_bytes(&self) -> Vec<u8> {
        let dim = self.vmin.len();
        let mut out = Vec::with_capacity(4 + dim * 8);
        out.extend_from_slice(&(dim as u32).to_le_bytes());
        for x in &self.vmin {
            out.extend_from_slice(&x.to_le_bytes());
        }
        for x in &self.vdiff {
            out.extend_from_slice(&x.to_le_bytes());
        }
        out
    }

    /// Deserialize a codebook; typed `Err` on truncation (Rule 8 — never a silent partial parse).
    pub(crate) fn from_meta_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 4 {
            return Err("theodb sq8: truncated codebook header".into());
        }
        let dim = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
        let need = 4 + dim * 8;
        if bytes.len() < need {
            return Err(format!(
                "theodb sq8: truncated codebook (need {need}, got {})",
                bytes.len()
            ));
        }
        let mut vmin = Vec::with_capacity(dim);
        let mut vdiff = Vec::with_capacity(dim);
        for i in 0..dim {
            let o = 4 + i * 4;
            vmin.push(f32::from_le_bytes(bytes[o..o + 4].try_into().unwrap()));
        }
        for i in 0..dim {
            let o = 4 + dim * 4 + i * 4;
            vdiff.push(f32::from_le_bytes(bytes[o..o + 4].try_into().unwrap()));
        }
        Ok(Sq8Quantizer { vmin, vdiff })
    }
}

// `#[pg_test]` (not plain `#[test]`) because the crate links pg symbols via `crate::vec`; a standalone unit-test
// binary would fail to resolve `errstart`/`errmsg` at link time (mirrors `sbq.rs`).
#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use super::*;
    use pgrx::prelude::*;

    fn corpus() -> Vec<Vec<f32>> {
        (0..64).map(|i| (0..8).map(|j| ((i * 7 + j * 13) % 97) as f32 * 0.1).collect()).collect()
    }

    #[pgrx::pg_test]
    fn sq8_train_is_deterministic_and_order_independent() {
        let c = corpus();
        let a = Sq8Quantizer::train(&c);
        let mut rev = c.clone();
        rev.reverse();
        let b = Sq8Quantizer::train(&rev);
        assert_eq!(a.vmin, b.vmin, "min/max are order-independent");
        assert_eq!(a.vdiff, b.vdiff);
    }

    #[pgrx::pg_test]
    fn sq8_roundtrips_through_meta_bytes() {
        let q = Sq8Quantizer::train(&corpus());
        let q2 = Sq8Quantizer::from_meta_bytes(&q.to_meta_bytes()).expect("codebook decodes");
        assert_eq!(q.vmin, q2.vmin);
        assert_eq!(q.vdiff, q2.vdiff);
    }

    #[pgrx::pg_test]
    fn sq8_from_bytes_rejects_truncated() {
        let mut bytes = Sq8Quantizer::train(&corpus()).to_meta_bytes();
        bytes.truncate(bytes.len() - 4);
        assert!(
            Sq8Quantizer::from_meta_bytes(&bytes).is_err(),
            "truncated codebook must be rejected"
        );
    }

    #[pgrx::pg_test]
    fn sq8_encode_produces_dim_bytes() {
        let q = Sq8Quantizer::train(&corpus());
        assert_eq!(q.encode(&corpus()[0]).len(), 8);
        assert_eq!(Sq8Quantizer::bytes_per_vector(8), 8);
    }

    #[pgrx::pg_test]
    fn sq8_decode_approximates_the_original() {
        // The decoded vector must be close to the original — SQ8 max error per dim is vdiff/255. Assert the
        // reconstruction error is bounded by the theoretical quantization step, proving encode/decode are inverse.
        let q = Sq8Quantizer::train(&corpus());
        let v = &corpus()[3];
        let d = q.decode(&q.encode(v));
        for i in 0..v.len() {
            let step = q.vdiff[i] / 255.0;
            assert!(
                (d[i] - v[i]).abs() <= step + 1e-4,
                "dim {i}: |{}-{}| > step {step}",
                d[i],
                v[i]
            );
        }
    }

    #[pgrx::pg_test]
    fn sq8_l2_correlates_with_f32_distance() {
        // Quantizer-validity oracle (mirrors sbq_hamming_correlates_with_f32_distance): a query's near neighbor
        // by exact f32 L2 must also be near by SQ8-decoded L2 — i.e. SQ8 preserves the ranking well enough to
        // rerank. Build a query, split the corpus into "near" and "far" halves by exact f32, assert the SQ8
        // mean distance of the near half is below the far half.
        let c = corpus();
        let q = Sq8Quantizer::train(&c);
        let query = &c[0];
        let mut exact: Vec<(f64, usize)> =
            c.iter().enumerate().map(|(i, v)| (crate::vec::l2_distance(query, v), i)).collect();
        exact.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let near: Vec<usize> = exact[..16].iter().map(|&(_, i)| i).collect();
        let far: Vec<usize> = exact[48..].iter().map(|&(_, i)| i).collect();
        let sq8_l2 = |i: usize| crate::vec::l2_distance(query, &q.decode(&q.encode(&c[i])));
        let near_mean: f64 = near.iter().map(|&i| sq8_l2(i)).sum::<f64>() / near.len() as f64;
        let far_mean: f64 = far.iter().map(|&i| sq8_l2(i)).sum::<f64>() / far.len() as f64;
        assert!(near_mean < far_mean, "SQ8 near-half mean {near_mean} !< far-half mean {far_mean}");
    }
}
