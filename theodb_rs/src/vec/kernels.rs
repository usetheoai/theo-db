//! Núcleo numérico de distância vetorial — PURO por contrato.
//!
//! # Por que este módulo existe separado de `vec.rs`
//!
//! `vec.rs` tem UMA dependência de PostgreSQL: `use crate::pg::err_input`, usada só por `check_dims` para o
//! erro tipado 22023 de dimensão incompatível. Essa única linha impedia que o núcleo fosse `#[path]`-incluído
//! num binário de `criterion`, que é o padrão do `benches/scan_hot_path.rs` — e o `Cargo.toml` (`[[bench]]`)
//! registra o custo de ignorar isso: sob `pg_test`, código dependente de símbolos do PG não linka num bench
//! standalone, e tentar já bloqueou **todos** os `#[pg_test]` do crate no M144.
//!
//! A consequência prática era o [[B-023]]: o micro-bench de SIMD morava na suíte funcional porque não havia
//! para onde movê-lo, e `rules/testing.md § 6` proíbe teste dependente de tempo sem isolamento.
//!
//! # A fronteira, e por que ela cai exatamente aqui
//!
//! A família `*_from_bytes` **nunca** chamou `check_dims` — ela valida com `assert_eq!` sobre o comprimento
//! bruto, que é invariante de layout e não regra de negócio. Então a separação não move nenhuma verificação:
//! ela apenas revela uma fronteira que já existia. `l2_distance`, `inner_product`, `cosine_distance` e
//! `l2_distance_simd` ficam em `vec.rs` porque recebem dois `&[f32]` de fonte SQL e precisam do erro tipado.
//!
//! É o mesmo movimento que o `ann/scan_core.rs` (FU-1): extrair o núcleo puro para que o bench meça o código
//! REAL, em vez de uma cópia divergente — que é como o pgvectorscale contorna o mesmo problema.
//!
//! # SRP
//!
//! Este módulo sabe **calcular distância a partir de bytes empacotados**. Não sabe o que é uma transação, um
//! erro SQL ou uma página. Quem sabe disso é `vec.rs`.

// ---------------------------------------------------------------------------------------------------------------
// M31b — fused decode+distance for the index-AM SCAN hot loop. Reads the candidate vector's f32 DIRECTLY from the
// page bytes (little-endian, native on x86) and computes L2 in ONE pass — eliminating the separate byte-decode
// (Phase 0 profile: decode 45% + scalar distance 55% of the ~38 ms scan). AVX2+FMA (8-wide) with a runtime
// dispatch + scalar fallback (portability). Used ONLY by `scan_ivf_structured`'s L2 branch (am/scan.rs); the M20
// SQL-callable ops above are untouched (byte-parity contract). Numeric: SIMD sums in a different f32 order → ~1
// ULP·√dim off the scalar sum — recall-preserving (ranking unchanged), NOT bit-identical. pgvector has the same
// property (FMA).
// ---------------------------------------------------------------------------------------------------------------

/// Scalar squared-L2 between `query` (`&[f32]`) and a candidate vector stored as little-endian f32 bytes `raw`
/// (`raw.len() == query.len()*4`). The portable fallback + the correctness oracle for the AVX2 path.
pub(crate) fn l2_sq_from_bytes_scalar(query: &[f32], raw: &[u8]) -> f32 {
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
pub(crate) mod simd_x86 {
    use std::arch::x86_64::*;
    use std::sync::atomic::{AtomicU8, Ordering};

    static AVX2_FMA: AtomicU8 = AtomicU8::new(2); // 2 = unknown, 1 = yes, 0 = no

    /// Test-only: pin the dispatch to scalar (`false`) or AVX (`true`) so both branches of `l2_dist_from_bytes`
    /// are coverable on the same host; `reset_for_test` restores runtime detection. Callers MUST reset in the same
    /// test (no cross-test state — pgrx pg_tests run sequentially in one backend).
    #[cfg(any(test, feature = "pg_test"))]
    pub(crate) fn force_for_test(available: bool) {
        AVX2_FMA.store(u8::from(available), Ordering::Relaxed);
    }

    #[cfg(any(test, feature = "pg_test"))]
    pub(crate) fn reset_for_test() {
        AVX2_FMA.store(2, Ordering::Relaxed);
    }

    /// Detect AVX2+FMA once, cache in an atomic (idempotent — any thread writes the same value).
    pub(crate) fn available() -> bool {
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

    /// Squared-L2 with AVX2+FMA, reading `raw` as unaligned little-endian f32. SAFETY (both halves required —
    /// the loop is driven by `dim = query.len()` and indexes BOTH `query[0..dim]` and `raw[0..dim*4]`): the caller
    /// MUST ensure (1) AVX2+FMA is available (via `available()`), AND (2) `raw.len() == query.len()*4` — i.e. the
    /// candidate vector byte-slice has exactly `dim` f32. `_mm256_loadu_ps` handles unaligned addresses, so reading
    /// `&[u8]` page bytes as `*const f32` is sound; only the LENGTH invariant is the caller's obligation.
    #[target_feature(enable = "avx2,fma")]
    pub(super) unsafe fn l2_sq(query: &[f32], raw: &[u8]) -> f32 {
        unsafe {
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
                let r = f32::from_le_bytes([
                    *rp.add(o),
                    *rp.add(o + 1),
                    *rp.add(o + 2),
                    *rp.add(o + 3),
                ]);
                let d = *qp.add(i) - r;
                s += d * d;
                i += 1;
            }
            s
        }
    }

    /// M58: the cosine kernel — `(Σq·r, Σq², Σr²)` with AVX2+FMA over unaligned LE-f32 `raw`, three lane
    /// accumulators reduced once. Same SAFETY contract as [`l2_sq`] (caller ensures AVX2+FMA available AND
    /// `raw.len() == query.len()*4`). Feeds `cosine_dist_from_bytes` / `ip_dist_from_bytes` — the scan hot path for
    /// real (OpenAI/Cohere) cosine/IP embeddings, which until now ran scalar (the M58 P2 gap). Approximate (SIMD FMA
    /// rounds differently than the scalar sum) — same parity-not-identity rule as L2's SIMD (operators stay scalar).
    #[target_feature(enable = "avx2,fma")]
    pub(super) unsafe fn cosine_terms(query: &[f32], raw: &[u8]) -> (f32, f32, f32) {
        unsafe {
            let dim = query.len();
            let qp = query.as_ptr();
            let rp = raw.as_ptr();
            let (mut adot, mut anq, mut anr) =
                (_mm256_setzero_ps(), _mm256_setzero_ps(), _mm256_setzero_ps());
            let mut i = 0usize;
            while i + 8 <= dim {
                let q = _mm256_loadu_ps(qp.add(i));
                let r = _mm256_loadu_ps(rp.add(i * 4) as *const f32);
                adot = _mm256_fmadd_ps(q, r, adot);
                anq = _mm256_fmadd_ps(q, q, anq);
                anr = _mm256_fmadd_ps(r, r, anr);
                i += 8;
            }
            let mut ld = [0f32; 8];
            let mut lq = [0f32; 8];
            let mut lr = [0f32; 8];
            _mm256_storeu_ps(ld.as_mut_ptr(), adot);
            _mm256_storeu_ps(lq.as_mut_ptr(), anq);
            _mm256_storeu_ps(lr.as_mut_ptr(), anr);
            let (mut dot, mut nq, mut nr) =
                (ld.iter().sum::<f32>(), lq.iter().sum::<f32>(), lr.iter().sum::<f32>());
            while i < dim {
                let o = i * 4;
                let r = f32::from_le_bytes([
                    *rp.add(o),
                    *rp.add(o + 1),
                    *rp.add(o + 2),
                    *rp.add(o + 3),
                ]);
                let q = *qp.add(i);
                dot += q * r;
                nq += q * q;
                nr += r * r;
                i += 1;
            }
            (dot, nq, nr)
        }
    }

    /// M58: fused dot `Σq·r` with AVX2+FMA (the inner-product kernel). Same SAFETY contract as [`l2_sq`].
    #[target_feature(enable = "avx2,fma")]
    pub(super) unsafe fn dot(query: &[f32], raw: &[u8]) -> f32 {
        unsafe {
            let dim = query.len();
            let qp = query.as_ptr();
            let rp = raw.as_ptr();
            let mut acc = _mm256_setzero_ps();
            let mut i = 0usize;
            while i + 8 <= dim {
                let q = _mm256_loadu_ps(qp.add(i));
                let r = _mm256_loadu_ps(rp.add(i * 4) as *const f32);
                acc = _mm256_fmadd_ps(q, r, acc);
                i += 8;
            }
            let mut lanes = [0f32; 8];
            _mm256_storeu_ps(lanes.as_mut_ptr(), acc);
            let mut s: f32 = lanes.iter().sum();
            while i < dim {
                let o = i * 4;
                let r = f32::from_le_bytes([
                    *rp.add(o),
                    *rp.add(o + 1),
                    *rp.add(o + 2),
                    *rp.add(o + 3),
                ]);
                s += *qp.add(i) * r;
                i += 1;
            }
            s
        }
    }
}

/// L2 distance between `query` and a candidate stored as little-endian f32 bytes `raw` — the SCAN hot-path entry
/// (M31b). Dispatches to AVX2+FMA when available (cached), else the scalar fallback. The length invariant
/// `raw.len() == query.len()*4` is enforced ALWAYS (not just in debug) so the `unsafe` AVX2 path can never OOB in
/// release regardless of caller — one compare per candidate, negligible vs the SIMD work.
pub(crate) fn l2_dist_from_bytes(query: &[f32], raw: &[u8]) -> f64 {
    assert_eq!(raw.len(), query.len() * 4, "l2_dist_from_bytes: raw must be exactly dim*4 bytes");
    #[cfg(target_arch = "x86_64")]
    let sq = if simd_x86::available() {
        // SAFETY: `available()` confirmed AVX2+FMA and the `assert_eq!` above guarantees `raw.len()==query.len()*4`.
        unsafe { simd_x86::l2_sq(query, raw) }
    } else {
        l2_sq_from_bytes_scalar(query, raw)
    };
    #[cfg(not(target_arch = "x86_64"))]
    let sq = l2_sq_from_bytes_scalar(query, raw);
    (sq as f64).sqrt()
}

/// M49: fused dot product `Σ query·raw` over little-endian f32 bytes — ZERO per-node `Vec<f32>` alloc (the mine
/// the ROADMAP flagged for cosine/ip). Reads each f32 inline from `raw`. The length invariant is enforced ALWAYS
/// so no OOB. (AVX2 for the dot term is a Phase-4 parity optimization if the benchmark shows cosine/ip lag L2's
/// AVX2 kernel; the zero-alloc contract — the M49 DoD — is already met by this scalar path.)
pub(crate) fn dot_from_bytes_scalar(query: &[f32], raw: &[u8]) -> f32 {
    let mut dot = 0f32;
    for (i, &qi) in query.iter().enumerate() {
        let r = f32::from_le_bytes([raw[i * 4], raw[i * 4 + 1], raw[i * 4 + 2], raw[i * 4 + 3]]);
        dot += qi * r;
    }
    dot
}

/// M58: dispatches to the AVX2+FMA `dot` kernel when available (cached), else the scalar fallback. Length
/// invariant enforced ALWAYS so the `unsafe` SIMD path can never OOB. Approximate (SIMD rounding) — the scan
/// walk/rerank tolerate it (parity-not-identity, like L2); the SQL `<#>` operator uses the exact scalar path.
pub(crate) fn dot_from_bytes(query: &[f32], raw: &[u8]) -> f32 {
    assert_eq!(raw.len(), query.len() * 4, "dot_from_bytes: raw must be exactly dim*4 bytes");
    #[cfg(target_arch = "x86_64")]
    if simd_x86::available() {
        // SAFETY: `available()` confirmed AVX2+FMA; the `assert_eq!` guarantees `raw.len()==query.len()*4`.
        return unsafe { simd_x86::dot(query, raw) };
    }
    dot_from_bytes_scalar(query, raw)
}

/// M49: negative inner product `-Σ q·r` from raw bytes (the `<#>` ORDER BY key — smaller = closer, pgvector
/// `vector_negative_inner_product` convention). Zero-alloc.
pub(crate) fn ip_dist_from_bytes(query: &[f32], raw: &[u8]) -> f64 {
    -(dot_from_bytes(query, raw) as f64)
}

/// M49: cosine distance `1 - dot/sqrt(‖q‖²·‖r‖²)` from raw bytes, one pass, clamp to [-1,1]. Zero-alloc. A
/// zero-norm `raw` yields NaN (0/0) — ordered LAST by the scan's `Cand` "NaN LAST" comparator (`ann/mod.rs`).
pub(crate) fn cosine_terms_scalar(query: &[f32], raw: &[u8]) -> (f32, f32, f32) {
    let (mut dot, mut nq, mut nr) = (0f32, 0f32, 0f32);
    for (i, &qi) in query.iter().enumerate() {
        let r = f32::from_le_bytes([raw[i * 4], raw[i * 4 + 1], raw[i * 4 + 2], raw[i * 4 + 3]]);
        dot += qi * r;
        nq += qi * qi;
        nr += r * r;
    }
    (dot, nq, nr)
}

pub(crate) fn cosine_dist_from_bytes(query: &[f32], raw: &[u8]) -> f64 {
    assert_eq!(
        raw.len(),
        query.len() * 4,
        "cosine_dist_from_bytes: raw must be exactly dim*4 bytes"
    );
    // M58: AVX2+FMA cosine kernel when available (the real-embedding scan hot path); scalar fallback otherwise.
    #[cfg(target_arch = "x86_64")]
    let (dot, nq, nr) = if simd_x86::available() {
        // SAFETY: `available()` confirmed AVX2+FMA; the `assert_eq!` guarantees `raw.len()==query.len()*4`.
        unsafe { simd_x86::cosine_terms(query, raw) }
    } else {
        cosine_terms_scalar(query, raw)
    };
    #[cfg(not(target_arch = "x86_64"))]
    let (dot, nq, nr) = cosine_terms_scalar(query, raw);
    let sim = (dot as f64) / ((nq as f64) * (nr as f64)).sqrt();
    1.0 - sim.clamp(-1.0, 1.0)
}
