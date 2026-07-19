//! Domain (vector research E1): TheoDB's own **extended multi-bit RaBitQ** quantizer — the f32-FREE rerank codec.
//!
//! Reimplements the published algorithm (Gao & Long, base RaBitQ SIGMOD'24 arXiv:2405.12497; extended multi-bit
//! arXiv:2409.09913 — **Apache-2.0 algorithm**, reimplemented own-code; the vendored `rabitq-rs` tree was deleted
//! in ADR-0046, so this is from-scratch like `aq.rs`/`ah.rs`. ZERO new deps, std-only — D1/D4). The load-bearing
//! property (paper, verbatim): at ~4.5× compression a 5–7-bit code "stably produces >95%/99% recall **WITHOUT
//! accessing raw vectors for re-ranking**". That deletes the exact measured bind of the M82/v5 scan path (the
//! Stage-2 exact-f32 random-read, `am/scan.rs:562`).
//!
//! ## The estimator (why it is f32-free and cheap)
//! A shared random orthogonal rotation `P` (built once per index) spreads the quantization error uniformly. For a
//! residual `r = x − c` (IVF centroid `c`): `nr = ‖r‖`, unit `o = r/nr`, rotated `o' = P·o`. Quantize `o'` to a
//! per-vector B-bit signed integer code `u` (uniform, step `Δ = max|o'|/L`, `L = 2^{B-1}−1`). Store per vector:
//! the code `u`, the norm `nr`, and the scalar `W = ⟨u, o'⟩` (computed at build). At search, with the rotated
//! query residual `q_r = P·(q−c)`:
//!   `⟨q_r, o⟩ ≈ ⟨q_r, ō⟩ / ⟨ō, o⟩ = ⟨q_r, u⟩ / W`   (the Δ and ‖ō‖ factors cancel — pure integer-weighted dot),
//!   `‖q − x‖² ≈ ‖q − c‖² + nr² − 2·nr·(⟨q_r, u⟩ / W)`.
//! `⟨q_r, u⟩` is the only per-candidate work — an integer-weighted dot over the code, no raw f32 vector touched.
//! Higher B ⇒ `ō` closer to `o` ⇒ tighter estimate ⇒ higher recall (7-bit reaches 0.99 with no rerank).

/// Extended multi-bit RaBitQ quantizer. Holds the shared random orthogonal rotation `P` (row-major `dim×dim`).
pub(crate) struct RabitqQuantizer {
    dim: usize,
    bits: u8,           // bits per dimension (1..=8); 1 = base RaBitQ, 5–7 = f32-free rerank
    rotation: Vec<f32>, // dim×dim orthogonal matrix, row-major (P); shared by all vectors
}

/// One encoded vector: the B-bit signed code `u`, the residual norm `nr = ‖r‖`, and `w = ⟨u, o'⟩`.
pub(crate) struct RabitqCode {
    pub code: Vec<i8>, // len = dim; each in [-L, L], L = 2^{bits-1}-1
    pub nr: f32,
    pub w: f32,
}

impl RabitqQuantizer {
    /// The maximum code magnitude `L = 2^{bits-1} − 1` (fits i8 for bits ≤ 8).
    fn level(bits: u8) -> i32 {
        (1i32 << (bits.max(1) - 1)) - 1
    }

    /// Build the quantizer: a seeded random orthogonal rotation (Gram–Schmidt on a Gaussian matrix — deterministic
    /// under `seed`, so codebooks are reproducible; ADR D2 discipline). `bits ∈ 1..=8`.
    pub(crate) fn train(dim: usize, bits: u8, seed: u64) -> RabitqQuantizer {
        let bits = bits.clamp(1, 8);
        let rotation = random_orthogonal(dim, seed);
        RabitqQuantizer { dim, bits, rotation }
    }

    pub(crate) fn dim(&self) -> usize {
        self.dim
    }
    pub(crate) fn bits(&self) -> u8 {
        self.bits
    }

    /// Rotate a length-`dim` vector by `P` (used for both the encode residual and the query residual at search).
    pub(crate) fn rotate(&self, v: &[f32]) -> Vec<f32> {
        let d = self.dim;
        debug_assert_eq!(v.len(), d);
        let mut out = vec![0.0f32; d];
        for (i, o) in out.iter_mut().enumerate() {
            let row = &self.rotation[i * d..(i + 1) * d];
            let mut acc = 0.0f32;
            for k in 0..d {
                acc += row[k] * v[k];
            }
            *o = acc;
        }
        out
    }

    /// Encode a residual `r = x − c` (pass `x` directly if `c = 0`). Returns the f32-free code.
    pub(crate) fn encode(&self, residual: &[f32]) -> RabitqCode {
        let d = self.dim;
        debug_assert_eq!(residual.len(), d);
        let nr = residual.iter().map(|&x| (x as f64) * (x as f64)).sum::<f64>().sqrt() as f32;
        if nr == 0.0 {
            // zero residual: degenerate direction — empty code, w=0 (the search guards w==0 → returns nr²+qc2).
            return RabitqCode { code: vec![0i8; d], nr: 0.0, w: 0.0 };
        }
        // unit residual, then rotate
        let mut o = vec![0.0f32; d];
        for i in 0..d {
            o[i] = residual[i] / nr;
        }
        let o_rot = self.rotate(&o);
        // per-vector uniform B-bit quantization: Δ = max|o'| / L
        let l = Self::level(self.bits);
        let maxabs = o_rot.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
        if maxabs == 0.0 {
            return RabitqCode { code: vec![0i8; d], nr, w: 0.0 };
        }
        let delta = maxabs / l as f32;
        let mut code = vec![0i8; d];
        let mut w = 0.0f64; // W = <u, o'>
        for i in 0..d {
            let q = (o_rot[i] / delta).round().clamp(-l as f32, l as f32) as i32;
            code[i] = q as i8;
            w += (q as f64) * (o_rot[i] as f64);
        }
        RabitqCode { code, nr, w: w as f32 }
    }

    /// Estimate the squared L2 distance `‖q − x‖²` from the code, the ROTATED query residual `q_r = P·(q−c)`, and
    /// `qc2 = ‖q − c‖²`. f32-FREE: touches only the integer code `u`, `nr`, `w` (no raw vector). `⟨q_r,u⟩/w`
    /// estimates `⟨q_r, o⟩`.
    pub(crate) fn estimate_l2_sq(&self, code: &RabitqCode, q_rot: &[f32], qc2: f64) -> f64 {
        debug_assert_eq!(q_rot.len(), self.dim);
        if code.w == 0.0 || code.nr == 0.0 {
            // degenerate residual (x == c): distance is exactly qc2 (‖q−c‖²) since ‖r‖=0.
            return qc2 + (code.nr as f64) * (code.nr as f64);
        }
        let mut ip_u = 0.0f64; // <q_r, u>
        for i in 0..self.dim {
            ip_u += (q_rot[i] as f64) * (code.code[i] as f64);
        }
        let ip_qo = ip_u / (code.w as f64); // ≈ <q_r, o> = <q−c, o>
        let nr = code.nr as f64;
        qc2 + nr * nr - 2.0 * nr * ip_qo
    }

    /// Serialize the quantizer for the index meta page: `[dim u32][bits u8][rotation f32 × dim²]`. Since the full
    /// rotation is stored, `from_meta_bytes` reconstructs the EXACT quantizer (no re-seeding needed).
    pub(crate) fn to_meta_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(5 + self.rotation.len() * 4);
        out.extend_from_slice(&(self.dim as u32).to_le_bytes());
        out.push(self.bits);
        for &f in &self.rotation {
            out.extend_from_slice(&f.to_le_bytes());
        }
        out
    }

    /// Reconstruct from `to_meta_bytes`. Typed `Err` on truncation (Rule 8).
    pub(crate) fn from_meta_bytes(bytes: &[u8]) -> Result<RabitqQuantizer, String> {
        if bytes.len() < 5 {
            return Err("theodb rabitq: truncated meta header".into());
        }
        let dim = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
        let bits = bytes[4];
        let need = 5 + dim * dim * 4;
        if bytes.len() != need {
            return Err(format!("theodb rabitq: meta length mismatch (got {}, want {need})", bytes.len()));
        }
        let rotation: Vec<f32> = bytes[5..].chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
        Ok(RabitqQuantizer { dim, bits, rotation })
    }

    /// Per-vector page-record byte size: `[i8 × dim][nr f32][w f32]` = `dim + 8`.
    pub(crate) fn record_bytes(dim: usize) -> usize {
        dim + 8
    }

    /// Serialize one encoded vector to its page record `[i8 × dim][nr f32 LE][w f32 LE]` (dim+8 bytes).
    pub(crate) fn code_to_record(code: &RabitqCode) -> Vec<u8> {
        let mut out = Vec::with_capacity(code.code.len() + 8);
        for &c in &code.code {
            out.push(c as u8);
        }
        out.extend_from_slice(&code.nr.to_le_bytes());
        out.extend_from_slice(&code.w.to_le_bytes());
        out
    }

    /// Parse a page record back into a `RabitqCode`. Typed `Err` on truncation.
    pub(crate) fn record_to_code(rec: &[u8], dim: usize) -> Result<RabitqCode, String> {
        let need = dim + 8;
        if rec.len() < need {
            return Err(format!("theodb rabitq: truncated record (got {}, want {need})", rec.len()));
        }
        let code: Vec<i8> = rec[..dim].iter().map(|&b| b as i8).collect();
        let nr = f32::from_le_bytes(rec[dim..dim + 4].try_into().unwrap());
        let w = f32::from_le_bytes(rec[dim + 4..dim + 8].try_into().unwrap());
        Ok(RabitqCode { code, nr, w })
    }
}

/// A seeded random orthogonal `dim×dim` matrix (row-major), via Gram–Schmidt on a Gaussian matrix. Deterministic.
fn random_orthogonal(dim: usize, seed: u64) -> Vec<f32> {
    let mut rng = SplitMix64::new(seed);
    // Gaussian columns (store as rows of the working matrix `a`; we orthonormalize the ROWS then that IS an
    // orthogonal matrix since orthonormal rows ⇒ orthonormal matrix).
    let mut a = vec![0.0f64; dim * dim];
    for x in a.iter_mut() {
        *x = rng.next_gaussian();
    }
    // Gram–Schmidt on rows
    for i in 0..dim {
        // subtract projections onto previous rows
        for j in 0..i {
            let dot = {
                let mut s = 0.0f64;
                for k in 0..dim {
                    s += a[i * dim + k] * a[j * dim + k];
                }
                s
            };
            for k in 0..dim {
                a[i * dim + k] -= dot * a[j * dim + k];
            }
        }
        // normalize row i
        let norm = (0..dim).map(|k| a[i * dim + k] * a[i * dim + k]).sum::<f64>().sqrt();
        let inv = if norm > 1e-12 { 1.0 / norm } else { 0.0 };
        for k in 0..dim {
            a[i * dim + k] *= inv;
        }
    }
    a.iter().map(|&x| x as f32).collect()
}

/// SplitMix64 PRNG + Box–Muller Gaussian — std-only, deterministic (no rand crate; `Math.random` forbidden).
struct SplitMix64 {
    state: u64,
    spare: Option<f64>,
}
impl SplitMix64 {
    fn new(seed: u64) -> SplitMix64 {
        SplitMix64 { state: seed, spare: None }
    }
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }
    fn next_f64(&mut self) -> f64 {
        // 53-bit uniform in [0,1)
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn next_gaussian(&mut self) -> f64 {
        if let Some(g) = self.spare.take() {
            return g;
        }
        // Box–Muller
        let u1 = self.next_f64().max(1e-300);
        let u2 = self.next_f64();
        let mag = (-2.0 * u1.ln()).sqrt();
        let (s, c) = (2.0 * std::f64::consts::PI * u2).sin_cos();
        self.spare = Some(mag * s);
        mag * c
    }
}

#[cfg(any(test, feature = "pg_test"))]
mod tests {
    use super::*;

    fn dot(a: &[f32], b: &[f32]) -> f64 {
        a.iter().zip(b).map(|(&x, &y)| (x as f64) * (y as f64)).sum()
    }

    // The rotation is orthogonal: P·Pᵀ ≈ I (rows orthonormal).
    #[test]
    fn rabitq_rotation_is_orthogonal() {
        let d = 64;
        let q = RabitqQuantizer::train(d, 5, 42);
        for i in 0..d {
            for j in 0..d {
                let ri = &q.rotation[i * d..(i + 1) * d];
                let rj = &q.rotation[j * d..(j + 1) * d];
                let dp = dot(ri, rj);
                let expect = if i == j { 1.0 } else { 0.0 };
                assert!((dp - expect).abs() < 1e-4, "P·Pᵀ[{i},{j}]={dp} (want {expect})");
            }
        }
    }

    // THE correctness gate: the f32-free estimator recovers ‖q−x‖² with error that (a) is small and (b)
    // MONOTONICALLY tightens with more bits, and (c) has low bias (mean signed error ≈ 0) — the RaBitQ property.
    #[test]
    fn rabitq_estimator_unbiased_and_tightens_with_bits() {
        let d = 128;
        let n = 200usize; // data vectors
        let nq = 40usize; // queries
        let mut rng = SplitMix64::new(7);
        let sample_vec = |rng: &mut SplitMix64| -> Vec<f32> { (0..d).map(|_| rng.next_gaussian() as f32).collect() };
        let data: Vec<Vec<f32>> = (0..n).map(|_| sample_vec(&mut rng)).collect();
        let queries: Vec<Vec<f32>> = (0..nq).map(|_| sample_vec(&mut rng)).collect();

        let mut prev_err = f64::INFINITY;
        for &bits in &[1u8, 3, 5, 7] {
            let quant = RabitqQuantizer::train(d, bits, 1234);
            let codes: Vec<RabitqCode> = data.iter().map(|x| quant.encode(x)).collect();
            let mut sum_abs_rel = 0.0f64;
            let mut sum_signed = 0.0f64;
            let mut cnt = 0.0f64;
            for q in &queries {
                let q_rot = quant.rotate(q); // c = 0 → q_r = P·q
                let qc2 = dot(q, q); // ‖q−0‖²
                for (x, code) in data.iter().zip(&codes) {
                    // true ‖q−x‖²
                    let mut t = 0.0f64;
                    for k in 0..d {
                        let diff = (q[k] - x[k]) as f64;
                        t += diff * diff;
                    }
                    let est = quant.estimate_l2_sq(code, &q_rot, qc2);
                    sum_abs_rel += ((est - t) / t.max(1e-9)).abs();
                    sum_signed += (est - t) / t.max(1e-9);
                    cnt += 1.0;
                }
            }
            let mean_abs_rel = sum_abs_rel / cnt;
            let mean_signed = sum_signed / cnt;
            // (c) low bias — the ratio estimator over a random rotation is near-unbiased
            assert!(mean_signed.abs() < 0.05, "bits={bits}: bias {mean_signed:.4} too high (estimator should be ~unbiased)");
            // (b) monotone tightening with bits
            assert!(mean_abs_rel < prev_err + 1e-9, "bits={bits}: error {mean_abs_rel:.4} did not tighten vs prev {prev_err:.4}");
            prev_err = mean_abs_rel;
            // (a) 7-bit is accurate enough to be the FINAL ranking (f32-free) — the load-bearing claim
            if bits == 7 {
                assert!(mean_abs_rel < 0.02, "bits=7: mean rel err {mean_abs_rel:.4} must be small enough for f32-free rerank");
            }
        }
    }

    // Meta + per-vector record serialization round-trips: a reconstructed quantizer + parsed code give the
    // byte-identical distance estimate (the page-storage contract for the v8 AM path).
    #[test]
    fn rabitq_serialization_round_trips() {
        let d = 48;
        let q = RabitqQuantizer::train(d, 7, 99);
        let q2 = RabitqQuantizer::from_meta_bytes(&q.to_meta_bytes()).expect("meta round-trip");
        assert_eq!(q2.dim, d);
        assert_eq!(q2.bits, 7);
        let mut rng = SplitMix64::new(3);
        let x: Vec<f32> = (0..d).map(|_| rng.next_gaussian() as f32).collect();
        let qy: Vec<f32> = (0..d).map(|_| rng.next_gaussian() as f32).collect();
        let code = q.encode(&x);
        let rec = RabitqQuantizer::code_to_record(&code);
        assert_eq!(rec.len(), RabitqQuantizer::record_bytes(d));
        let code2 = RabitqQuantizer::record_to_code(&rec, d).expect("record round-trip");
        let q_rot = q2.rotate(&qy);
        let qc2 = dot(&qy, &qy);
        let e1 = q.estimate_l2_sq(&code, &q_rot, qc2);
        let e2 = q2.estimate_l2_sq(&code2, &q_rot, qc2);
        assert!((e1 - e2).abs() < 1e-3, "estimate must survive serialization: {e1} vs {e2}");
    }

    // Zero residual (x == c) is handled without panic and estimates exactly qc2.
    #[test]
    fn rabitq_zero_residual_is_exact() {
        let d = 32;
        let quant = RabitqQuantizer::train(d, 7, 9);
        let zero = vec![0.0f32; d];
        let code = quant.encode(&zero);
        assert_eq!(code.nr, 0.0);
        let q: Vec<f32> = (0..d).map(|i| (i as f32) * 0.1).collect();
        let q_rot = quant.rotate(&q);
        let qc2 = dot(&q, &q);
        let est = quant.estimate_l2_sq(&code, &q_rot, qc2);
        assert!((est - qc2).abs() < 1e-6, "zero-residual distance must equal ‖q−c‖²={qc2}, got {est}");
    }

    // Regression (E1 in-PG scan bug): `qc2` MUST be the SQUARED residual norm ‖q−c‖², not the sqrt L2 distance.
    // The AM scan originally passed `metric.dist(q,c)` (= sqrt(‖·‖²) for L2), which mixed a linear-scale term with
    // the squared nr² inside the estimator and corrupted cross-list ranking (SIFT1M recall fell to 0.13–0.23,
    // DROPPING with more probes). This test pins the contract: squared qc2 is accurate; sqrt(qc2) is grossly wrong.
    #[test]
    fn rabitq_qc2_must_be_squared_not_sqrt() {
        let d = 64;
        let quant = RabitqQuantizer::train(d, 7, 21);
        let mut rng = SplitMix64::new(5);
        // A residual with a deliberately large norm so ‖q−c‖ and ‖q−c‖² differ by a wide factor.
        let c: Vec<f32> = (0..d).map(|_| rng.next_gaussian() as f32).collect();
        let x: Vec<f32> = c.iter().map(|&cc| cc + rng.next_gaussian() as f32 * 3.0).collect();
        let q: Vec<f32> = c.iter().map(|&cc| cc + rng.next_gaussian() as f32 * 3.0).collect();
        let residual: Vec<f32> = x.iter().zip(&c).map(|(&xi, &ci)| xi - ci).collect();
        let code = quant.encode(&residual);
        let qmc: Vec<f32> = q.iter().zip(&c).map(|(&qi, &ci)| qi - ci).collect();
        let q_rot = quant.rotate(&qmc);
        let true_d2: f64 = q.iter().zip(&x).map(|(&qi, &xi)| { let e = (qi - xi) as f64; e * e }).sum();
        let qc2_sq: f64 = qmc.iter().map(|&e| (e as f64) * (e as f64)).sum(); // ‖q−c‖² (CORRECT)
        let qc2_sqrt = qc2_sq.sqrt(); // the wrong value the scan bug passed
        let est_ok = quant.estimate_l2_sq(&code, &q_rot, qc2_sq);
        let est_bad = quant.estimate_l2_sq(&code, &q_rot, qc2_sqrt);
        let err_ok = (est_ok - true_d2).abs() / true_d2;
        let err_bad = (est_bad - true_d2).abs() / true_d2;
        assert!(err_ok < 0.05, "squared qc2 must estimate the true ‖q−x‖²={true_d2:.1} (got {est_ok:.1}, err {err_ok:.3})");
        assert!(err_bad > 0.3, "sqrt(qc2) must be grossly wrong — the guard against reintroducing the scan bug (err {err_bad:.3})");
    }
}
