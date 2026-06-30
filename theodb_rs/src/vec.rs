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

/// Reject mismatched dimensions with a typed 22023 (parity with pgvector's `CheckDims`, which errors on
/// `a->dim != b->dim`). Fail-fast at the boundary (Unbreakable Rule 8).
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
        let a: Vec<f32> = std::iter::once(big).chain(std::iter::repeat(1.0).take(64)).collect();
        let b: Vec<f32> = std::iter::repeat(0.0).take(65).collect();
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
}
