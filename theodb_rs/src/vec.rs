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

// M59 Phase 2 — the Asymmetric-Hashing LUT16 `pshufb` scoring kernel lives in a sibling file so neither this
// module nor `ah.rs` exceeds the 500-LoC budget (`rules/architecture.md`). `#[path]` keeps `vec.rs` a plain
// file (not a `vec/mod.rs` dir) — minimal diff, no reshuffle of the M20/M58 code above.
#[path = "vec/ah.rs"]
pub(crate) mod ah;
#[path = "vec/aq.rs"]
pub(crate) mod aq; // M104 — relocated from am/ (pure domain quantizer; fixes the vec->am layering inversion)
pub(crate) mod rabitq; // vector E1 — extended multi-bit RaBitQ (f32-free rerank codec; own-code, arXiv:2409.09913)

// B-023/B-053 — o núcleo numérico saiu para um módulo PURO (zero `crate::`), para que o micro-bench de SIMD
// possa ser `#[path]`-incluído num binário de `criterion` e sair da suíte funcional, onde `rules/testing.md § 6`
// não admite teste dependente de tempo. Mesmo movimento do `ann/scan_core.rs` (FU-1).
//
// O `use` abaixo re-exporta tudo o que já era `pub(crate)`, então os 17 call sites em `am/`, `ann/` e `hybrid.rs`
// seguem escrevendo `vec::cosine_dist_from_bytes` — a extração é invisível para eles por construção, e é isso
// que a torna segura.
#[path = "vec/kernels.rs"]
pub(crate) mod kernels;
pub(crate) use kernels::{cosine_dist_from_bytes, ip_dist_from_bytes, l2_dist_from_bytes};

// Os testes de CORREÇÃO do despacho (que ficaram aqui, ao contrário do micro-bench) forçam o branch e comparam
// contra o oráculo escalar, então precisam de dois nomes que a produção não usa. Re-exportados sob `cfg` para
// não deixarem re-export morto no binário shipado.
//
// Foi esta linha que a extração esqueceu, e o modo como isso apareceu vale registro: `cargo check --features
// pg18` passou LIMPO, porque sem `pg_test` o módulo de testes nem é compilado. O erro só surgiu em
// `cargo pgrx test`, 25 minutos depois. Um gate mais barato que o gate que importa dá uma confiança que ele
// não sustenta — que é, literalmente, o assunto deste ciclo.
#[cfg(any(test, feature = "pg_test"))]
pub(crate) use kernels::{l2_sq_from_bytes_scalar, simd_x86};

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
/// L2 distance between two f32 slices, using the SAME AVX2+FMA kernel as the scan (M43). Reuses `l2_dist_from_bytes`
/// by reinterpreting `b`'s f32 slice as its own little-endian bytes — a zero-copy cast (an f32 slice IS its bytes on
/// x86_64 LE). Used by the `theodb_hnsw` BUILD (billions of 128-dim distances) to replace the scalar `l2_distance`.
/// NOT bit-identical to `l2_distance` (SIMD FMA rounds differently) — callers that need exact pgvector parity
/// (operators, scan rerank, knn) MUST keep using `l2_distance`; this is for the approximate graph build/search where
/// the recall gate is PARITY, not identity. Aligns the build's metric with the scan's (both SIMD → consistent).
pub(crate) fn l2_distance_simd(a: &[f32], b: &[f32]) -> f64 {
    check_dims(a, b);
    // The reinterpret feeds `b`'s NATIVE-endian bytes to `l2_dist_from_bytes`, which reloads them as f32 either via
    // AVX `_mm256_loadu_ps` (native f32) or the scalar `f32::from_le_bytes`. Both are only value-preserving when
    // native == little-endian. Guard it: on big-endian the reinterpret would corrupt values, so fall back to the
    // exact scalar `l2_distance` (correctness over the SIMD win on a target TheoDB does not ship — M43 review finding).
    #[cfg(target_endian = "little")]
    {
        // SAFETY: `b` is a valid `&[f32]`; its bytes are exactly `b.len()*4` contiguous LE bytes. The reinterpret is
        // read-only and lives only for the `l2_dist_from_bytes` call, which re-asserts `raw.len() == a.len()*4`.
        let b_bytes = unsafe { std::slice::from_raw_parts(b.as_ptr() as *const u8, b.len() * 4) };
        l2_dist_from_bytes(a, b_bytes)
    }
    #[cfg(not(target_endian = "little"))]
    {
        l2_distance(a, b)
    }
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
        // fall below the f32 ULP and are lost — an f64 accumulator would keep them. `big` MUST be a power of two
        // so that `big²` is EXACTLY representable in f32 (else the squaring itself rounds and `got` drifts off
        // `big` regardless of the accumulator width). big = 2^16 → big² = 2^32, ULP(2^32) = 512 ≫ 64, so the
        // 64 unit terms are lost in f32 (Σ = 2^32 exactly) but kept in f64 — the exact discriminator we assert.
        let big = 65536.0_f32; // 2^16
        let a: Vec<f32> = std::iter::once(big).chain(std::iter::repeat_n(1.0, 64)).collect();
        let b: Vec<f32> = std::iter::repeat_n(0.0, 65).collect();
        let got = l2_distance(&a, &b);
        assert_eq!(got, (big as f64) * 1.0, "f32 sum drops sub-ULP terms → exactly big"); // sqrt(big²)=big
        let f64_acc: f64 = a.iter().map(|x| (*x as f64) * (*x as f64)).sum::<f64>().sqrt();
        assert_ne!(got, f64_acc, "an f64 accumulator would NOT lose the +64");
    }

    #[pg_test(error = "theodb vector op: different vector dimensions 2 and 1")]
    fn dim_mismatch_rejected_22023() {
        // EC-1 negative: mismatched dims fail-fast with the typed 22023 (err_input).
        let _ = l2_distance(&[1.0, 2.0], &[3.0]);
    }

    // --- M31b: fused decode+distance from page bytes ---------------------------------------------------------

    fn to_le_bytes(v: &[f32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    /// Independent f32 Σ(q−c)² oracle (no sqrt roundtrip) — the honest reference for the from-bytes scalar sum.
    fn l2_sq_f32_oracle(q: &[f32], c: &[f32]) -> f32 {
        q.iter().zip(c).map(|(x, y)| (x - y) * (x - y)).sum()
    }

    #[pg_test]
    fn l2_from_bytes_scalar_is_bit_identical_to_f32_sum() {
        // The scalar fallback reads f32 from LE bytes and MUST equal a direct f32 Σd² (same summation order — same
        // f32 accumulator), bit-for-bit. Uses NON-perfect-square magnitudes so the equality is not an artifact of
        // a Pythagorean fixture surviving a sqrt/square roundtrip.
        let q = [0.3f32, 4.7, 1.1, 12.9, 2.2, 7.3, 0.05, 9.8];
        let c = [1.4f32, 0.2, 3.3, 5.6, 8.1, 0.9, 6.7, 2.0];
        let raw = to_le_bytes(&c);
        assert_eq!(l2_sq_from_bytes_scalar(&q, &raw), l2_sq_f32_oracle(&q, &c));
    }

    #[cfg(target_arch = "x86_64")]
    #[pg_test]
    fn l2_dist_from_bytes_scalar_dispatch_branch_covered() {
        // Force the dispatch to the scalar branch (on this x86 host it would otherwise take AVX) so BOTH arms of
        // l2_dist_from_bytes are exercised. Reset detection afterwards (no cross-test state).
        let q: Vec<f32> = (0..40).map(|i| (i as f32) * 0.13 - 2.0).collect();
        let c: Vec<f32> = (0..40).map(|i| 1.5 - (i as f32) * 0.07).collect();
        let raw = to_le_bytes(&c);
        simd_x86::force_for_test(false);
        let scalar = l2_dist_from_bytes(&q, &raw);
        simd_x86::force_for_test(true);
        let avx = l2_dist_from_bytes(&q, &raw);
        simd_x86::reset_for_test();
        let oracle = (l2_sq_f32_oracle(&q, &c) as f64).sqrt();
        assert_eq!(scalar, oracle, "scalar dispatch branch must equal the f32 oracle exactly");
        let eps = 1e-4 * (q.len() as f64).sqrt() * oracle.max(1.0);
        assert!((avx - oracle).abs() <= eps, "avx dispatch branch off by {}", (avx - oracle).abs());
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

    /// M58: the SIMD (AVX2+FMA) cosine/IP kernels match the exact scalar oracle within a tiny eps, across dims
    /// (including tails past the 8-lane boundary), for BOTH the AVX and scalar dispatch branches. Approximate
    /// (lane-reduce rounding), recall-preserving — NOT bit-identical (same rule as L2's SIMD; SQL operators stay exact).
    #[pg_test]
    fn cosine_and_ip_from_bytes_match_scalar_within_eps_across_dims() {
        for dim in [1usize, 7, 8, 9, 16, 17, 128, 768] {
            let q: Vec<f32> = (0..dim).map(|i| (i as f32) * 0.3 - 2.0).collect();
            let c: Vec<f32> = (0..dim).map(|i| 1.5 - (i as f32) * 0.2).collect();
            let raw = to_le_bytes(&c);
            let cos_oracle = cosine_distance(&q, &c);
            let dot_oracle: f64 = q.iter().zip(&c).map(|(a, b)| (*a as f64) * (*b as f64)).sum();
            for avx in [true, false] {
                simd_x86::force_for_test(avx);
                let cos = cosine_dist_from_bytes(&q, &raw);
                assert!(
                    (cos - cos_oracle).abs() <= 1e-4 * (dim as f64).sqrt(),
                    "cosine dim={dim} avx={avx}: {cos} vs {cos_oracle}"
                );
                let ip = ip_dist_from_bytes(&q, &raw);
                let eps = 1e-4 * (dim as f64).sqrt() * dot_oracle.abs().max(1.0);
                assert!(
                    (ip - (-dot_oracle)).abs() <= eps,
                    "ip dim={dim} avx={avx}: {ip} vs {}",
                    -dot_oracle
                );
            }
            simd_x86::reset_for_test();
        }
    }

    // B-023/B-053 — o micro-bench `cosine_simd_per_candidate_speedup` SAIU daqui para
    // `benches/simd_cosine.rs`, e o motivo está medido, não estimado: ele cronometrava parede dentro da suíte
    // funcional, e o mesmo binário dava `0,78×` após os outros 439 testes e `0,66×` com 3 contêineres ao lado —
    // monotônico com a carga acumulada. Era a redução de frequência por licença AVX2 num i7-1355U de 15 W: o
    // teste media a térmica do laptop e reportava como qualidade do kernel.
    //
    // A correção anterior (medianas de rodadas alternadas) melhorou a MEDIÇÃO e não removeu a CLASSE.
    // `rules/testing.md § 6` veda tempo em teste unitário sem isolamento, e um vermelho intermitente treina o
    // time a ignorar vermelho.
    //
    // O que ficou aqui é o que é determinístico e pertence: a CORREÇÃO do despacho — `simd_matches_scalar_*`
    // prova que os dois branches concordam. Velocidade virou `cargo bench --bench simd_cosine`, com o
    // `criterion` reportando variância em vez de uma asserção sobre o relógio.
}
