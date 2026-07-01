//! Domain layer (blueprint M20): TheoDB's own f32-parity distance operators over pgvector's values.
//!
//! M20 implements the three pgvector distance operations — `<->` (L2), `<#>` (negative inner product),
//! `<=>` (cosine) — as own Rust functions, at **numeric parity** with pgvector's `vector.c`. Per ADR D1
//! (coexistence) the ops read pgvector's EXACT f32 values via the lossless `vector::real[]` cast (pgvector
//! `CREATE CAST (vector AS real[])`, `sql/vector.sql:157`) mapped to pgrx-native `Vec<f32>` — NOT a competing
//! storage type, NOT pgvector's operators redefined. Parsimony refinement of the blueprint's `#[repr(C)]` FFI
//! observation (which pgvectorscale needs only because it is an index AM on raw datums): for a standalone
//! distance FUNCTION the array-cast is binary-equivalent (same f32 bits), needs no `unsafe`, and lets pgrx
//! handle detoast (closes edge-case EC-1 with zero unsafe code). The values are pgvector's; we own the
//! computation in Rust — the M20 intent ("reduzir dependência do pgvector no tipo/ops").
//!
//! Parity (ADR D2 — the bit determinant): pgvector accumulates the distance SUMS in `float` (f32), NOT f64
//! (`vector.c:557` L2, `:604` IP, `:646` cosine). We accumulate in `f32` likewise; `sqrt`/division in f64;
//! cosine clamps to [-1,1] then `1.0 - sim` (`vector.c:683-689`). Bit-exactness vs a SIMD pgvector build is
//! best-effort (ADR D3) — parity is asserted to pgvector's rounded TEXT output.
use crate::pg::err_input;

/// Reject mismatched dimensions, fail-fast at the boundary (Unbreakable Rule 8). Both TheoDB and pgvector's
/// `CheckDims` reject `a->dim != b->dim`; pgvector raises SQLSTATE 22000 (data_exception) while TheoDB uses
/// its house typed error 22023 (invalid_parameter_value, via `err_input`) — same fail-fast semantics, a
/// deliberate code divergence consistent with the rest of theodb_rs (documented; the parity tests assert 22023).
fn check_dims(a: &[f32], b: &[f32]) {
    if a.len() != b.len() {
        err_input(&format!(
            "theodb vector op: different vector dimensions {} and {}",
            a.len(),
            b.len()
        ));
    }
}

/// L2 distance `<->` = `sqrt(Σ (a_i − b_i)²)` — f32 accumulation, f64 sqrt (pgvector `VectorL2SquaredDistance`
/// + `l2_distance`, `vector.c:554-583`).
pub(crate) fn l2_distance(a: &[f32], b: &[f32]) -> f64 {
    check_dims(a, b);
    let squared: f32 = a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum();
    (squared as f64).sqrt()
}

/// Inner product `Σ a_i·b_i` — f32 accumulation, f64 result (pgvector `VectorInnerProduct` + `inner_product`,
/// `vector.c:601-626`).
pub(crate) fn inner_product(a: &[f32], b: &[f32]) -> f64 {
    check_dims(a, b);
    let ip: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    ip as f64
}

// The `<#>` operator distance is the NEGATIVE inner product (pgvector `vector_negative_inner_product`,
// `vector.c:631-641`) — i.e. just `-inner_product(a, b)`. It is a sign flip, not a separate algorithm, so it
// is applied at the call site rather than as its own function (KISS).

/// Cosine distance `<=>` = `1 − sim`, `sim = Σab / sqrt(Σa²·Σb²)` clamped to [-1,1] — f32 sums in one pass,
/// f64 divide/sqrt (pgvector `VectorCosineSimilarity` + `cosine_distance`, `vector.c:643-689`).
pub(crate) fn cosine_distance(a: &[f32], b: &[f32]) -> f64 {
    check_dims(a, b);
    let mut sim: f32 = 0.0;
    let mut norma: f32 = 0.0;
    let mut normb: f32 = 0.0;
    for (x, y) in a.iter().zip(b) {
        sim += x * y;
        norma += x * x;
        normb += y * y;
    }
    // Use sqrt(a * b) over sqrt(a) * sqrt(b) — byte-for-byte with pgvector (vector.c:659). Clamp to [-1,1]
    // (pgvector's `if sim > 1 ... else if sim < -1 ...`, vector.c:683-688); `.clamp` is equivalent for finite,
    // inf (→ bound), and NaN (→ NaN) inputs, so parity holds.
    let similarity = (sim as f64 / ((norma as f64) * (normb as f64)).sqrt()).clamp(-1.0, 1.0);
    1.0 - similarity
}

// ---------------------------------------------------------------------------------------------------------------
// M31b — fused decode+distance for the index-AM SCAN hot loop. Reads the candidate vector's f32 DIRECTLY from the
// page bytes (little-endian, native on x86) and computes L2 in ONE pass — eliminating the separate byte-decode
// (Phase 0 profile: decode 45% + scalar distance 55% of the ~38 ms scan). AVX2+FMA (8-wide) with a runtime
// dispatch + scalar fallback (portability). Used ONLY by the scan (`Metric::dist_fast`); the M20 SQL-callable ops
// above are untouched (byte-parity contract). Numeric: SIMD sums in a different f32 order → ~1 ULP·√dim off the
// scalar sum — recall-preserving (ranking unchanged), NOT bit-identical. pgvector has the same property (FMA).
// ---------------------------------------------------------------------------------------------------------------

/// Scalar squared-L2 between `query` (`&[f32]`) and a candidate vector stored as little-endian f32 bytes `raw`
/// (`raw.len() == query.len()*4`). The portable fallback + the correctness oracle for the AVX2 path.
fn l2_sq_from_bytes_scalar(query: &[f32], raw: &[u8]) -> f32 {
    let mut s = 0f32;
    for (i, &q) in query.iter().enumerate() {
        let o = i * 4;
        let r = f32::from_le_bytes([raw[o], raw[o + 1], raw[o + 2], raw[o + 3]]);
        let d = q - r;
        s += d * d;
    }
    s
}

#[cfg(target_arch = "x86_64")]
mod simd_x86 {
    use std::arch::x86_64::*;
    use std::sync::atomic::{AtomicU8, Ordering};

    static AVX2_FMA: AtomicU8 = AtomicU8::new(2); // 2 = unknown, 1 = yes, 0 = no

    /// Detect AVX2+FMA once, cache in an atomic (idempotent — any thread writes the same value).
    pub(super) fn available() -> bool {
        match AVX2_FMA.load(Ordering::Relaxed) {
            1 => true,
            0 => false,
            _ => {
                let ok = is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma");
                AVX2_FMA.store(u8::from(ok), Ordering::Relaxed);
                ok
            }
        }
    }

    /// Squared-L2 with AVX2+FMA, reading `raw` as unaligned little-endian f32. SAFETY: caller MUST ensure AVX2+FMA
    /// is available (via `available()`) and `raw.len() >= query.len()*4`.
    #[target_feature(enable = "avx2,fma")]
    pub(super) unsafe fn l2_sq(query: &[f32], raw: &[u8]) -> f32 {
        let dim = query.len();
        let qp = query.as_ptr();
        let rp = raw.as_ptr();
        let mut acc = _mm256_setzero_ps();
        let mut i = 0usize;
        while i + 8 <= dim {
            let q = _mm256_loadu_ps(qp.add(i));
            let r = _mm256_loadu_ps(rp.add(i * 4) as *const f32); // unaligned f32 straight from page bytes
            let d = _mm256_sub_ps(q, r);
            acc = _mm256_fmadd_ps(d, d, acc);
            i += 8;
        }
        // Horizontal sum of the 8 lanes.
        let mut lanes = [0f32; 8];
        _mm256_storeu_ps(lanes.as_mut_ptr(), acc);
        let mut s: f32 = lanes.iter().sum();
        // Scalar tail for dim % 8.
        while i < dim {
            let o = i * 4;
            let r = f32::from_le_bytes([*rp.add(o), *rp.add(o + 1), *rp.add(o + 2), *rp.add(o + 3)]);
            let d = *qp.add(i) - r;
            s += d * d;
            i += 1;
        }
        s
    }
}

/// L2 distance between `query` and a candidate stored as little-endian f32 bytes `raw` — the SCAN hot-path entry
/// (M31b). Dispatches to AVX2+FMA when available (cached), else the scalar fallback. `raw.len() == query.len()*4`.
pub(crate) fn l2_dist_from_bytes(query: &[f32], raw: &[u8]) -> f64 {
    debug_assert_eq!(raw.len(), query.len() * 4);
    #[cfg(target_arch = "x86_64")]
    let sq = if simd_x86::available() {
        // SAFETY: `available()` confirmed AVX2+FMA; `raw.len() == query.len()*4` (debug-asserted; the caller
        // slices exactly dim*4 bytes per entry).
        unsafe { simd_x86::l2_sq(query, raw) }
    } else {
        l2_sq_from_bytes_scalar(query, raw)
    };
    #[cfg(not(target_arch = "x86_64"))]
    let sq = l2_sq_from_bytes_scalar(query, raw);
    (sq as f64).sqrt()
}

// Rust unit tests for the pure distance math (plan T2.1). `#[pg_test]` (not plain `#[test]`) because the
// pgrx cdylib links pg symbols — runs under `cargo pgrx test`, compiles under `cargo check --tests`. The
// OBSERVABLE parity gate that runs in CI is the Python suite (benchmarks/tests/test_vector_ops.py) replaying
// pgvector's live functions; these document + lock the f32-parity contract. Oracle values from pgvector's
// regression suite (test/sql/vector_type.sql).
#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use super::*;
    use pgrx::prelude::*;

    #[pg_test]
    fn l2_matches_pgvector_oracle() {
        assert_eq!(l2_distance(&[0.0, 0.0], &[3.0, 4.0]), 5.0);
        assert_eq!(l2_distance(&[0.0, 0.0], &[0.0, 1.0]), 1.0);
    }

    #[pg_test]
    fn inner_product_matches_pgvector_oracle() {
        assert_eq!(inner_product(&[1.0, 2.0], &[3.0, 4.0]), 11.0); // 3 + 8
        assert_eq!(-inner_product(&[1.0, 2.0], &[3.0, 4.0]), -11.0); // <#> = negative inner product
    }

    #[pg_test]
    fn cosine_identical_is_zero_and_orthogonal_is_one() {
        assert_eq!(cosine_distance(&[1.0, 0.0], &[1.0, 0.0]), 0.0);
        assert_eq!(cosine_distance(&[1.0, 0.0], &[0.0, 1.0]), 1.0);
    }

    #[pg_test]
    fn cosine_opposite_is_two_clamped() {
        // opposite vectors: sim = -1 → distance 2.0 (clamp keeps sim ≥ -1).
        assert_eq!(cosine_distance(&[1.0, 0.0], &[-1.0, 0.0]), 2.0);
    }

    #[pg_test]
    fn dim1_boundary_holds() {
        // EC-2: smallest valid dim still computes.
        assert_eq!(l2_distance(&[3.0], &[0.0]), 3.0);
        assert_eq!(inner_product(&[3.0], &[2.0]), 6.0);
    }

    #[pg_test]
    fn f32_accumulation_not_f64() {
        // ADR-D2: prove we accumulate in f32 (like pgvector), not f64. With a large base, many tiny additions
        // fall below the f32 ULP and are lost — an f64 accumulator would keep them. Σa² in f32 = big² exactly
        // (the +64 of unit terms is below ULP(1e14)≈8e6); l2 = sqrt of that.
        let big = 1.0e7_f32;
        let a: Vec<f32> = std::iter::once(big).chain(std::iter::repeat_n(1.0, 64)).collect();
        let b: Vec<f32> = std::iter::repeat_n(0.0, 65).collect();
        let got = l2_distance(&a, &b);
        assert_eq!(got, (big as f64) * 1.0, "f32 sum drops sub-ULP terms → exactly big"); // sqrt(big²)=big
        let f64_acc: f64 = a.iter().map(|x| (*x as f64) * (*x as f64)).sum::<f64>().sqrt();
        assert_ne!(got, f64_acc, "an f64 accumulator would NOT lose the +64");
    }

    #[pg_test(error = "different vector dimensions")]
    fn dim_mismatch_rejected_22023() {
        // EC-1 negative: mismatched dims fail-fast with the typed 22023 (err_input).
        let _ = l2_distance(&[1.0, 2.0], &[3.0]);
    }

    // --- M31b: fused decode+distance from page bytes ---------------------------------------------------------

    fn to_le_bytes(v: &[f32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    #[pg_test]
    fn l2_from_bytes_scalar_matches_m20_l2() {
        // The scalar fallback reads f32 from LE bytes and MUST equal the M20 scalar l2 exactly (same summation
        // order — same f32 accumulator). Bit-identical, no eps.
        let q = [3.0f32, 4.0, 0.0, 12.0];
        let c = [0.0f32, 0.0, 0.0, 0.0];
        let raw = to_le_bytes(&c);
        assert_eq!(l2_sq_from_bytes_scalar(&q, &raw), l2_distance(&q, &c).powi(2) as f32);
    }

    #[pg_test]
    fn l2_from_bytes_matches_scalar_within_eps_across_dims() {
        // The dispatched path (AVX2 when available, else scalar) MUST match the scalar oracle within a tiny eps.
        // AVX2 sums in 8 lanes then reduces — a different f32 order → ~1 ULP·√dim off, recall-preserving (NOT
        // bit-identical). Sweep dims across the 8-lane boundary (tail handling) and non-trivial magnitudes.
        for dim in [1usize, 7, 8, 9, 16, 17, 128, 129] {
            let q: Vec<f32> = (0..dim).map(|i| (i as f32) * 0.5 - 3.0).collect();
            let c: Vec<f32> = (0..dim).map(|i| 2.0 - (i as f32) * 0.3).collect();
            let raw = to_le_bytes(&c);
            let got = l2_dist_from_bytes(&q, &raw);
            let oracle = l2_distance(&q, &c);
            let eps = 1e-4 * (dim as f64).sqrt() * oracle.max(1.0);
            assert!(
                (got - oracle).abs() <= eps,
                "dim={dim}: fused={got} vs scalar-oracle={oracle} (eps={eps})"
            );
        }
    }
}
