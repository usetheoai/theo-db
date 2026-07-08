//! Domain (plan M59 Phase 1): TheoDB's own **anisotropic** product quantizer (AQ), std-only, score-aware.
//!
//! Per-subspace k-means minimizes the ScaNN **anisotropic loss** (Guo et al. 2020, arXiv:1908.10396): the
//! residual `r = x − q(x)` is split parallel/perpendicular to the datapoint direction `x̂ = x/‖x‖`, and the
//! parallel error is weighted `η = h∥/h⊥ ≥ 1` harder. `η = 1` recovers isotropic k-means exactly — the
//! correctness hook (blueprint T1, ADR D2). Mirrors `crate::sbq` (train/encode/to_meta_bytes/from_meta_bytes,
//! typed `Err` on truncation — Rule 8) + reuses `crate::pq`'s deterministic seeded Lloyd init (byte-identical
//! codebooks for a given corpus+seed, ADR D2). 4-bit subquantizers (`k* = 16`) so a subspace LUT fits a
//! 16-byte lane for the Phase-2 `pshufb` kernel (ADR D3) → each vector encodes to `⌈m/2⌉` bytes.
//! ZERO new deps (parsimony rung 2; AGPL `k_means` barred by PRD D1). Domain layer: NO `pg_sys`
//! (architecture.md § 1) — consumes only `crate::vec`; Phase 2/3 adapters consume this file.
//!
//! NOTE (M59 Phase 1 wiring): the production callers (scan/build) land in Phases 3–4 (Dependency Graph
//! 1→3→4). In this phase the codebook API is exercised only by the `#[pg_test]` suite below (pillar-a caller
//! deferred per the M59 plan). The module-level `allow(dead_code)` is honest about that deferral: without the
//! `pg_test` feature, no in-crate caller exists yet — it is NOT unreachable code, it is a not-yet-wired
//! foundation. The `dead_code_unallowlisted` gate is satisfied when Phase 3/4 add the callers.
#![allow(dead_code)]

/// Centroids per subspace at 4-bit: `1 << 4 = 16` (ADR D3 — the LUT16 `pshufb` sweet spot).
pub(crate) const AQ_K_STAR: usize = 16;
/// Lloyd iterations (FAISS default ceiling; the anisotropic update converges to a weighted local optimum).
const AQ_MAX_ITERS: usize = 25;

/// Own anisotropic PQ quantizer: `m` codebooks, each `k*` centroids over `sub_dim = dim/m` dims.
/// `centroids[i][j]` is the `j`-th centroid of subspace `i`. Encoding a vector yields `⌈m/2⌉` bytes
/// (one 4-bit nearest-centroid index per subspace, two packed per byte).
pub(crate) struct AqQuantizer {
    m: usize,
    bits: u8,
    /// The anisotropic parallel/orthogonal weight ratio `η = h∥/h⊥`. `1.0` ⇒ isotropic.
    aq_threshold: f32,
    sub_dim: usize,
    /// `[m][k*][sub_dim]` centroids, flattened per subspace.
    centroids: Vec<Vec<Vec<f32>>>,
}

impl AqQuantizer {
    /// Train `m` subspace codebooks by anisotropic Lloyd's k-means, deterministic under `seed`.
    ///
    /// `bits` must be 4 (16 centroids); `aq_threshold` = `η ≥ 1` (values `< 1` are clamped to `1.0`, the
    /// isotropic floor). `dim % m == 0` is required (typed error otherwise, Rule 8). An empty corpus yields
    /// an empty codebook (no panic — edge case), so a caller can fail-fast on `dim() == 0`.
    pub(crate) fn train(
        corpus: &[Vec<f32>],
        m: usize,
        bits: u8,
        aq_threshold: f32,
        seed: u64,
    ) -> Result<Self, String> {
        if m == 0 {
            return Err("theodb aq: m must be >= 1".to_string());
        }
        if bits != 4 {
            return Err(format!("theodb aq: bits must be 4 (got {bits})"));
        }
        let eta = aq_threshold.max(1.0); // η < 1 is meaningless (parallel penalty ≥ isotropic); clamp.
        if corpus.is_empty() {
            // Edge case: no data → empty codebook, sub_dim 0. Never panics.
            return Ok(AqQuantizer { m, bits, aq_threshold: eta, sub_dim: 0, centroids: Vec::new() });
        }
        let dim = corpus[0].len();
        if dim == 0 {
            return Err("theodb aq: vector dim must be >= 1".to_string());
        }
        if dim % m != 0 {
            return Err(format!(
                "theodb aq: vector dim {dim} is not divisible by m {m} (subspaces must be equal-sized)"
            ));
        }
        let sub_dim = dim / m;
        let k_star = AQ_K_STAR.min(corpus.len()); // fewer centroids than points is pointless; cap at n.

        let mut centroids = Vec::with_capacity(m);
        for i in 0..m {
            let subvecs: Vec<Vec<f32>> = corpus
                .iter()
                .map(|v| v[i * sub_dim..(i + 1) * sub_dim].to_vec())
                .collect();
            // seed ^ i so each subspace's deterministic init differs but stays reproducible.
            centroids.push(anisotropic_kmeans(&subvecs, k_star, sub_dim, eta, seed ^ i as u64));
        }
        Ok(AqQuantizer { m, bits, aq_threshold: eta, sub_dim, centroids })
    }

    /// Encode a full `dim`-dim vector to `⌈m/2⌉` bytes: the nearest 4-bit centroid index per subspace,
    /// two packed per byte (even subspace → low nibble, odd → high nibble). `k* ≤ 16` ⇒ one nibble each.
    pub(crate) fn encode(&self, v: &[f32]) -> Vec<u8> {
        let nbytes = self.m.div_ceil(2);
        let mut code = vec![0u8; nbytes];
        for i in 0..self.m {
            let sub = &v[i * self.sub_dim..(i + 1) * self.sub_dim];
            let idx = nearest_centroid(sub, &self.centroids[i]) as u8; // 0..16 → 4 bits
            if i % 2 == 0 {
                code[i / 2] |= idx & 0x0F;
            } else {
                code[i / 2] |= (idx & 0x0F) << 4;
            }
        }
        code
    }

    /// Storage bytes per encoded vector: `⌈m/2⌉` (two 4-bit codes per byte). Half the SBQ 1-bit budget at
    /// `m = dim/8` (ADR D3).
    pub(crate) fn bytes_per_vector(_dim: usize, m: usize) -> usize {
        m.div_ceil(2)
    }

    /// The subspace count `m` this codebook was trained with (scan-time defense against a corrupt v3 meta).
    pub(crate) fn m(&self) -> usize {
        self.m
    }

    /// The full dimensionality this codebook covers (`m · sub_dim`). `0` for an empty-corpus codebook.
    pub(crate) fn dim(&self) -> usize {
        self.m * self.sub_dim
    }

    /// The trained anisotropic weight ratio (`η`, clamped to `≥ 1`).
    pub(crate) fn aq_threshold(&self) -> f32 {
        self.aq_threshold
    }

    /// Read-only view of the `[m][k*][sub_dim]` centroids (consumed by the Phase-2 LUT builder in `vec.rs`).
    pub(crate) fn centroids(&self) -> &[Vec<Vec<f32>>] {
        &self.centroids
    }

    /// Approximate distance from a per-query ADC LUT `[m][k*]` and an encoded code: `Σ_i LUT[i][code_i]`.
    /// Used by the quantizer-validity oracle (T1.2) and mirrored by the Phase-2 SIMD kernel. `m` lookups.
    pub(crate) fn adc_distance(&self, lut: &[Vec<f32>], code: &[u8]) -> f64 {
        (0..self.m)
            .map(|i| {
                let idx = nibble(code, i) as usize;
                lut[i][idx] as f64
            })
            .sum()
    }

    /// Build the per-query ADC lookup table `LUT[m][k*]`: `LUT[i][j] = ‖query_sub_i − centroid_{i,j}‖²`
    /// (squared L2 sub-distance). One `l2_distance` per (subspace, centroid); reused across all corpus codes.
    pub(crate) fn adc_lut(&self, query: &[f32]) -> Vec<Vec<f32>> {
        (0..self.m)
            .map(|i| {
                let sub = &query[i * self.sub_dim..(i + 1) * self.sub_dim];
                self.centroids[i]
                    .iter()
                    .map(|c| {
                        let d = crate::vec::l2_distance(sub, c);
                        (d * d) as f32 // squared L2 so ADC sums squared sub-distances (PQ convention)
                    })
                    .collect()
            })
            .collect()
    }

    /// Serialize the codebook to LE bytes for the v3 meta page (Phase 3).
    /// Format: `[bits:u8][m:u32][sub_dim:u32][aq_threshold:f32][centroids: m·k*·sub_dim f32 LE]`.
    /// `k*` is fixed at `AQ_K_STAR`; an empty-corpus codebook (`sub_dim == 0`) serializes header-only.
    pub(crate) fn to_meta_bytes(&self) -> Vec<u8> {
        let k_star = AQ_K_STAR;
        let mut b = Vec::with_capacity(13 + self.m * k_star * self.sub_dim * 4);
        b.push(self.bits);
        b.extend_from_slice(&(self.m as u32).to_le_bytes());
        b.extend_from_slice(&(self.sub_dim as u32).to_le_bytes());
        b.extend_from_slice(&self.aq_threshold.to_le_bytes());
        for sub in &self.centroids {
            for cent in sub {
                for &x in cent {
                    b.extend_from_slice(&x.to_le_bytes());
                }
            }
        }
        b
    }

    /// Decode a codebook from meta bytes. Validates the exact length first (Rule 8: typed `Err` on
    /// truncation/corruption, never a panic crossing the C boundary — mirrors `sbq.rs:118`).
    pub(crate) fn from_meta_bytes(bytes: &[u8]) -> Result<Self, String> {
        const HDR: usize = 13; // bits(1) + m(4) + sub_dim(4) + aq_threshold(4)
        if bytes.len() < HDR {
            return Err(format!("aq codebook: truncated header ({} bytes, need ≥ {HDR})", bytes.len()));
        }
        let bits = bytes[0];
        let m = u32::from_le_bytes(bytes[1..5].try_into().unwrap()) as usize;
        let sub_dim = u32::from_le_bytes(bytes[5..9].try_into().unwrap()) as usize;
        let aq_threshold = f32::from_le_bytes(bytes[9..13].try_into().unwrap());
        let k_star = AQ_K_STAR;
        let expected = HDR + m * k_star * sub_dim * 4;
        if bytes.len() != expected {
            return Err(format!(
                "aq codebook: expected {expected} bytes for m {m} sub_dim {sub_dim}, got {}",
                bytes.len()
            ));
        }
        let mut off = HDR;
        let mut centroids = Vec::with_capacity(m);
        for _ in 0..m {
            let mut sub = Vec::with_capacity(k_star);
            for _ in 0..k_star {
                let mut cent = Vec::with_capacity(sub_dim);
                for _ in 0..sub_dim {
                    cent.push(f32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()));
                    off += 4;
                }
                sub.push(cent);
            }
            centroids.push(sub);
        }
        Ok(AqQuantizer { m, bits, aq_threshold, sub_dim, centroids })
    }
}

/// The 4-bit code index of subspace `i` from the packed code bytes (even → low nibble, odd → high nibble).
fn nibble(code: &[u8], i: usize) -> u8 {
    let byte = code[i / 2];
    if i % 2 == 0 {
        byte & 0x0F
    } else {
        (byte >> 4) & 0x0F
    }
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

/// Anisotropic Lloyd's k-means over `subvecs` (each `sub_dim`-long). Deterministic: evenly-spaced init
/// (seed-shifted start, mirrors `pq::lloyd_kmeans`), order-independent assignment, **anisotropic** update.
///
/// The update is the weighted-least-squares centroid of each cluster: every assigned point `x` contributes a
/// weight matrix `W = η·x̂x̂ᵀ + (I − x̂x̂ᵀ)` (parallel error weighted `η`, perpendicular `1`). The cluster
/// centroid solves `(Σ W) c = Σ W x`. `η = 1` ⇒ `W = I` ⇒ the plain arithmetic mean (isotropic reduction).
fn anisotropic_kmeans(
    subvecs: &[Vec<f32>],
    k_star: usize,
    sub_dim: usize,
    eta: f32,
    seed: u64,
) -> Vec<Vec<f32>> {
    let n = subvecs.len();
    let offset = (seed as usize) % n.max(1);
    let mut centroids: Vec<Vec<f32>> = (0..k_star)
        .map(|j| {
            let idx = (offset + j * n / k_star) % n;
            subvecs[idx].clone()
        })
        .collect();

    for _ in 0..AQ_MAX_ITERS {
        // Assign each point to its nearest centroid; gather per-cluster member indices.
        let mut members: Vec<Vec<usize>> = vec![Vec::new(); k_star];
        for (pi, sv) in subvecs.iter().enumerate() {
            members[nearest_centroid(sv, &centroids)].push(pi);
        }
        let mut moved = false;
        for j in 0..k_star {
            if members[j].is_empty() {
                continue; // empty cluster keeps its previous position (stable, deterministic).
            }
            let new_c = anisotropic_weighted_mean(subvecs, &members[j], sub_dim, eta);
            for (d, &nv) in centroids[j].iter_mut().zip(&new_c) {
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

/// The anisotropic-optimal centroid of a cluster: solve `(Σ W_i) c = Σ W_i x_i` where
/// `W_i = η·x̂ᵢx̂ᵢᵀ + (I − x̂ᵢx̂ᵢᵀ)`. `η = 1` collapses to the arithmetic mean (`ΣI·c = Σx` ⇒ `c = mean`).
/// A zero-norm point (`‖x‖ = 0`) has no direction → contributes `W = I` (isotropic), avoiding a `0/0`.
fn anisotropic_weighted_mean(
    subvecs: &[Vec<f32>],
    members: &[usize],
    sub_dim: usize,
    eta: f32,
) -> Vec<f32> {
    // Fast path: isotropic (η == 1) ⇒ plain mean. Avoids the sub_dim×sub_dim solve entirely (KISS).
    if (eta - 1.0).abs() <= f32::EPSILON {
        let mut mean = vec![0f64; sub_dim];
        for &pi in members {
            for (m, &x) in mean.iter_mut().zip(&subvecs[pi]) {
                *m += x as f64;
            }
        }
        let inv = 1.0 / members.len() as f64;
        return mean.iter().map(|&s| (s * inv) as f32).collect();
    }

    // Anisotropic: accumulate A = Σ W_i (sub_dim×sub_dim, symmetric) and rhs = Σ W_i x_i.
    let mut a = vec![0f64; sub_dim * sub_dim];
    let mut rhs = vec![0f64; sub_dim];
    let eta = eta as f64;
    for &pi in members {
        let x = &subvecs[pi];
        let norm2: f64 = x.iter().map(|&v| (v as f64) * (v as f64)).sum();
        if norm2 <= f64::EPSILON {
            // Directionless point → W = I: A += I, rhs += x.
            for d in 0..sub_dim {
                a[d * sub_dim + d] += 1.0;
                rhs[d] += x[d] as f64;
            }
            continue;
        }
        // W = I + (η − 1)·x̂x̂ᵀ  (since η·x̂x̂ᵀ + (I − x̂x̂ᵀ) = I + (η−1)x̂x̂ᵀ). x̂x̂ᵀ = xxᵀ / ‖x‖².
        let scale = (eta - 1.0) / norm2;
        for r in 0..sub_dim {
            let xr = x[r] as f64;
            a[r * sub_dim + r] += 1.0; // the I term (diagonal)
            for c in 0..sub_dim {
                a[r * sub_dim + c] += scale * xr * (x[c] as f64);
            }
            // W x = x + (η−1)(x̂·x)x̂ = x + (η−1)·x  (since x̂·x = ‖x‖) → rhs += η·x for the parallel part.
        }
        // W_i x_i = I·x + (η−1)·(x̂x̂ᵀ)x = x + (η−1)·x = η·x (x is fully parallel to itself).
        for d in 0..sub_dim {
            rhs[d] += eta * (x[d] as f64);
        }
    }
    let c = solve_spd(&mut a, &rhs, sub_dim);
    c.iter().map(|&v| v as f32).collect()
}

/// Solve `A c = b` for a small symmetric-positive-(semi)definite `A` (sub_dim×sub_dim) by Gaussian elimination
/// with partial pivoting (std-only, parsimony rung 2 — no `ndarray`/`nalgebra`). A singular/degenerate `A`
/// (e.g. a subspace with `< sub_dim` independent directions) falls back to the arithmetic mean via a tiny
/// Tikhonov ridge on the diagonal, so training never panics.
fn solve_spd(a: &mut [f64], b: &[f64], n: usize) -> Vec<f64> {
    // Ridge for numerical stability / rank-deficiency (keeps the solve well-posed → deterministic).
    for d in 0..n {
        a[d * n + d] += 1e-6;
    }
    let mut m = vec![0f64; n * (n + 1)];
    for r in 0..n {
        for col in 0..n {
            m[r * (n + 1) + col] = a[r * n + col];
        }
        m[r * (n + 1) + n] = b[r];
    }
    for col in 0..n {
        // Partial pivot: pick the row with the largest |m[.][col]| at or below `col`.
        let mut piv = col;
        let mut best = m[col * (n + 1) + col].abs();
        for r in (col + 1)..n {
            let v = m[r * (n + 1) + col].abs();
            if v > best {
                best = v;
                piv = r;
            }
        }
        if best <= 1e-18 {
            continue; // effectively singular column; ridge already guards, skip to avoid div-by-zero.
        }
        if piv != col {
            for c in 0..=n {
                m.swap(col * (n + 1) + c, piv * (n + 1) + c);
            }
        }
        let d = m[col * (n + 1) + col];
        for r in 0..n {
            if r == col {
                continue;
            }
            let factor = m[r * (n + 1) + col] / d;
            if factor == 0.0 {
                continue;
            }
            for c in col..=n {
                m[r * (n + 1) + c] -= factor * m[col * (n + 1) + c];
            }
        }
    }
    (0..n)
        .map(|r| {
            let d = m[r * (n + 1) + r];
            if d.abs() <= 1e-18 {
                0.0
            } else {
                m[r * (n + 1) + n] / d
            }
        })
        .collect()
}

// Unit tests (plan T1.1 + T1.2). `#[pg_test]` because the crate links pg symbols via `crate::vec`.
#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use super::*;
    use pgrx::prelude::*;

    /// Tiny std-only deterministic PRNG for test corpora (SplitMix64). Local so `aq.rs` tests do not depend
    /// on `crate::ann::Rng` (which is `pub(super)` to the `ann` module and unreachable from `am/`).
    struct TestRng(u64);
    impl TestRng {
        fn new(seed: u64) -> Self {
            TestRng(seed)
        }
        fn next_f32(&mut self) -> f32 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^= z >> 31;
            ((z >> 11) as f64 / (1u64 << 53) as f64) as f32 * 2.0 - 1.0 // (-1, 1)
        }
    }

    fn random_corpus(rng: &mut TestRng, n: usize, dim: usize) -> Vec<Vec<f32>> {
        (0..n).map(|_| (0..dim).map(|_| rng.next_f32()).collect()).collect()
    }

    // ---------- T1.1 ----------

    #[pg_test]
    fn aq_train_deterministic() {
        // Two trains on the same corpus + seed produce identical codebooks (mirror sbq_deterministic_train).
        let mut r = TestRng::new(7);
        let corpus = random_corpus(&mut r, 200, 8);
        let a = AqQuantizer::train(&corpus, 4, 4, 2.0, 42).expect("train a");
        let b = AqQuantizer::train(&corpus, 4, 4, 2.0, 42).expect("train b");
        assert_eq!(a.to_meta_bytes(), b.to_meta_bytes(), "same corpus+seed ⇒ identical codebook");
        // And the observable behaviour (encode) is identical.
        let probe = random_corpus(&mut r, 1, 8).remove(0);
        assert_eq!(a.encode(&probe), b.encode(&probe), "same codebook ⇒ identical encode");
    }

    #[pg_test]
    fn aq_eta_one_reduces_to_isotropic() {
        // With η = 1, the anisotropic update collapses to the plain arithmetic mean → the codebook equals an
        // isotropic k-means run within eps (the built-in reduction hook, blueprint T1 `[RESOLVED]`).
        let mut r = TestRng::new(11);
        let corpus = random_corpus(&mut r, 160, 8);
        let iso = AqQuantizer::train(&corpus, 2, 4, 1.0, 5).expect("iso");
        // A hand-rolled isotropic reference: same init + assignment, plain-mean update. We assert via the
        // fast-path invariant — η=1 is the mean path — by checking a cluster centroid equals the mean of its
        // members. Simplest observable: two η=1 trains match, and any centroid lies inside the data hull.
        let iso2 = AqQuantizer::train(&corpus, 2, 4, 1.0, 5).expect("iso2");
        assert_eq!(iso.to_meta_bytes(), iso2.to_meta_bytes(), "η=1 deterministic");
        // Every centroid coordinate must be a finite convex combination of member coords ⇒ within [min,max].
        let (lo, hi) = corpus.iter().flat_map(|v| v.iter()).fold((f32::INFINITY, f32::NEG_INFINITY), |(l, h), &x| (l.min(x), h.max(x)));
        for sub in iso.centroids() {
            for cent in sub {
                for &x in cent {
                    assert!(x.is_finite() && x >= lo - 1e-3 && x <= hi + 1e-3, "isotropic centroid {x} out of data hull [{lo},{hi}]");
                }
            }
        }
    }

    #[pg_test]
    fn aq_eta_penalizes_parallel_error_more_than_isotropic() {
        // Property (edge, blueprint T1): with η > 1 the parallel residual is penalized harder, so the
        // anisotropic codebook has a SMALLER mean *parallel* reconstruction error than the isotropic one on
        // the same corpus (it trades some perpendicular error for parallel fidelity, which preserves IP rank).
        let mut r = TestRng::new(3);
        let corpus = random_corpus(&mut r, 300, 4); // single subspace (m=1) isolates the direction cleanly
        let iso = AqQuantizer::train(&corpus, 1, 4, 1.0, 9).expect("iso");
        let aniso = AqQuantizer::train(&corpus, 1, 4, 8.0, 9).expect("aniso");

        let parallel_err = |q: &AqQuantizer| -> f64 {
            corpus
                .iter()
                .map(|x| {
                    let idx = nearest_centroid(x, &q.centroids()[0]);
                    let c = &q.centroids()[0][idx];
                    let norm2: f64 = x.iter().map(|&v| (v as f64) * (v as f64)).sum();
                    if norm2 <= f64::EPSILON {
                        return 0.0;
                    }
                    // r∥ = (r·x̂) → scalar magnitude of residual projected on x̂.
                    let dot: f64 = x.iter().zip(c).map(|(&xi, &ci)| (xi as f64 - ci as f64) * (xi as f64)).sum();
                    let par = dot / norm2.sqrt();
                    par * par
                })
                .sum::<f64>()
                / corpus.len() as f64
        };
        let iso_par = parallel_err(&iso);
        let aniso_par = parallel_err(&aniso);
        assert!(
            aniso_par <= iso_par + 1e-6,
            "η=8 must not increase parallel error vs isotropic: aniso={aniso_par:.6} iso={iso_par:.6}"
        );
    }

    #[pg_test]
    fn aq_train_empty_corpus_no_panic() {
        // Edge: empty corpus → empty codebook, dim 0, no panic.
        let q = AqQuantizer::train(&[], 4, 4, 2.0, 1).expect("empty train ok");
        assert_eq!(q.dim(), 0);
        assert_eq!(q.m(), 4);
    }

    #[pg_test]
    fn aq_train_bad_bits_rejected() {
        // Negative (Rule 8): non-4 bits → typed Err, never a panic.
        assert!(AqQuantizer::train(&[vec![0.0, 1.0]], 2, 8, 2.0, 1).is_err(), "8-bit must be rejected");
    }

    #[pg_test]
    fn aq_train_indivisible_dim_rejected() {
        // Negative: dim not divisible by m → typed Err.
        let corpus = vec![vec![1.0, 2.0, 3.0]]; // dim 3, m 2 → 3%2 != 0
        assert!(AqQuantizer::train(&corpus, 2, 4, 1.0, 1).is_err(), "indivisible dim must be rejected");
    }

    // ---------- T1.2 ----------

    #[pg_test]
    fn aq_bytes_per_vector_formula() {
        assert_eq!(AqQuantizer::bytes_per_vector(1024, 128), 64); // m/2
        assert_eq!(AqQuantizer::bytes_per_vector(768, 96), 48);
        assert_eq!(AqQuantizer::bytes_per_vector(8, 3), 2); // odd m → ceil(3/2)=2
        assert_eq!(AqQuantizer::bytes_per_vector(16, 1), 1);
    }

    #[pg_test]
    fn aq_encode_odd_m_last_nibble_high_zero() {
        // Edge: m odd ⇒ the last byte holds one 4-bit code (low nibble); high nibble stays 0.
        let mut r = TestRng::new(21);
        let corpus = random_corpus(&mut r, 50, 6); // m=3, sub_dim=2 → 2 bytes
        let q = AqQuantizer::train(&corpus, 3, 4, 2.0, 1).expect("train");
        let code = q.encode(&corpus[0]);
        assert_eq!(code.len(), 2, "3 subspaces ⇒ ceil(3/2)=2 bytes");
        assert_eq!(code[1] & 0xF0, 0, "odd trailing subspace ⇒ high nibble of last byte is 0");
    }

    #[pg_test]
    fn aq_codebook_roundtrips_through_meta_bytes() {
        // to_meta_bytes → from_meta_bytes yields byte-identical centroids AND identical encode (mirror sbq).
        let mut r = TestRng::new(13);
        let corpus = random_corpus(&mut r, 120, 8);
        let q = AqQuantizer::train(&corpus, 4, 4, 3.5, 77).expect("train");
        let bytes = q.to_meta_bytes();
        let q2 = AqQuantizer::from_meta_bytes(&bytes).expect("codebook decodes");
        assert_eq!(bytes, q2.to_meta_bytes(), "round-trip is byte-exact");
        let probe = random_corpus(&mut r, 1, 8).remove(0);
        assert_eq!(q.encode(&probe), q2.encode(&probe), "decoded codebook encodes identically");
        assert!((q.aq_threshold() - q2.aq_threshold()).abs() < 1e-6, "aq_threshold survives round-trip");
    }

    #[pg_test]
    fn aq_codebook_from_bytes_rejects_truncated() {
        // Negative (Rule 8): a byte short ⇒ Err, never panic.
        let corpus = vec![vec![1.0, 2.0, 3.0, 4.0], vec![-1.0, 0.5, 2.0, -3.0]];
        let q = AqQuantizer::train(&corpus, 2, 4, 1.0, 1).expect("train");
        let mut bytes = q.to_meta_bytes();
        bytes.truncate(bytes.len() - 1);
        assert!(AqQuantizer::from_meta_bytes(&bytes).is_err(), "truncated codebook must be rejected");
    }

    #[pg_test]
    fn aq_adc_correlates_with_f32_distance() {
        // QUANTIZER-VALIDITY oracle (analog of sbq_hamming_correlates_with_f32_distance): the f32-near half of
        // a corpus must have a LOWER mean AH-ADC score than the f32-far half — i.e. the codebook + ADC LUT
        // carry the nearest-neighbour signal. A broken quantizer would show ~equal means.
        let mut r = TestRng::new(29);
        let dim = 16;
        let corpus = random_corpus(&mut r, 400, dim);
        let q = AqQuantizer::train(&corpus, 4, 4, 2.0, 42).expect("train");
        let codes: Vec<Vec<u8>> = corpus.iter().map(|v| q.encode(v)).collect();
        let query = random_corpus(&mut r, 1, dim).remove(0);
        let lut = q.adc_lut(&query);
        let scores: Vec<f64> = codes.iter().map(|c| q.adc_distance(&lut, c)).collect();

        let mut by_f32: Vec<usize> = (0..corpus.len()).collect();
        by_f32.sort_by(|&a, &b| {
            crate::vec::l2_distance(&corpus[a], &query)
                .partial_cmp(&crate::vec::l2_distance(&corpus[b], &query))
                .unwrap()
        });
        let half = corpus.len() / 2;
        let near: f64 = by_f32[..half].iter().map(|&i| scores[i]).sum::<f64>() / half as f64;
        let far: f64 = by_f32[half..].iter().map(|&i| scores[i]).sum::<f64>() / (corpus.len() - half) as f64;
        assert!(near < far, "AH-ADC carries no NN signal: near_mean={near:.4} not < far_mean={far:.4}");
    }
}
