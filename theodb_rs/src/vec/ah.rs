//! Domain (plan M59 Phase 2): the **Asymmetric-Hashing LUT16** scoring kernel — the near-free per-candidate
//! scorer that closes the M33 ScaNN QPS gap (blueprint T2, ADR D3). Given a full-precision query and an
//! `AqQuantizer` (Phase 1), it precomputes ONE per-query lookup table `LUT[m][16]` of int8 partial scores, then
//! scores each corpus code by `Σ_i LUT[i][code_i]` — `m` in-register table lookups, no per-candidate multiply,
//! no decode. On x86_64 the sum is `_mm256_shuffle_epi8` (`pshufb`, 16 parallel lookups/lane) with an int8→int16
//! widened accumulate; a scalar fallback is the correctness oracle. Same runtime-dispatch shape as the M58
//! kernels in the parent `vec` module (`is_x86_feature_detected!`, `available()`/`force_for_test`).
//!
//! **int8 requantization (A/B, FAISS FastScan).** `pshufb` is a byte-lane instruction, so the f32 partials
//! `‖q_sub − centroid‖²` must be quantized to int8. We pick a single per-query affine map
//! `q(v) = round((v − B) / A)` over ALL `m·16` partials so that (FAISS Wiki [4]): (1) A is maximized (finest
//! resolution), (2) the int16 accumulator over `m` subspaces never overflows, (3) the int8 LUT never saturates.
//! The reconstructed distance is `A·acc + m·B`, but since `m·B` is a constant shared by every corpus code, the
//! raw integer accumulator `acc` alone preserves the ADC RANKING — which is all the scan+rerank needs. The
//! requantization is a bounded tolerance, asserted (not ignored) by `ah_lut_requant_bounded`.
//!
//! Domain layer: NO `pg_sys` (architecture.md § 1) — consumes only `crate::vec::aq::AqQuantizer` + `crate::vec`.
//! Production callers (the v3 scan) land in Phase 4 (Dependency Graph 2→3→4); in this phase the kernel is
//! exercised by the `#[pg_test]` suite below (pillar-a caller deferred per the M59 plan, honest `dead_code`).
#![allow(dead_code)]

use crate::vec::aq::AqQuantizer;

/// int8 LUT range: partials map to `[0, LUT_MAX]`. 127 keeps the value inside `i8` with a saturation guard AND
/// bounds the `m`-way i16 accumulate: `m · 127 ≤ 32767` ⇒ safe for any `m ≤ 258` (SIFT1M uses `m ≤ 128`).
const LUT_MAX: i32 = 127;

/// A per-query requantized lookup table: `m` subspaces × 16 int8 partial scores, plus the affine `scale`/`bias`
/// that map an integer accumulator back to the f32 ADC distance (`dist ≈ scale·acc + m·bias`). The RANKING is
/// carried by `acc` alone (bias·m is a per-query constant), so the scan can compare raw accumulators.
pub(crate) struct Lut16 {
    /// Number of subspaces (== `AqQuantizer::m`).
    m: usize,
    /// `m · 16` int8 partial scores, subspace-major: `tables[i*16 + j]` is subspace `i`, centroid `j`.
    tables: Vec<i8>,
    /// The requant step `A`: `f32_partial ≈ A · int_code + B`.
    scale: f32,
    /// The requant floor `B` (the global min partial across all `m·16` entries).
    bias: f32,
}

impl Lut16 {
    pub(crate) fn m(&self) -> usize {
        self.m
    }

    /// Map an integer accumulator (`Σ_i tables[i][code_i]`) back to the approximate f32 ADC distance.
    /// `dist ≈ scale·acc + m·bias`. Only used by tests / rerank sanity — the scan compares raw `acc`.
    pub(crate) fn dequantize(&self, acc: i32) -> f32 {
        self.scale * acc as f32 + self.m as f32 * self.bias
    }
}

/// Build the per-query `Lut16` (blueprint T2, FAISS A/B). For each subspace `i`, the f32 partial to centroid `j`
/// is the squared-L2 sub-distance `‖query_sub_i − centroid_{i,j}‖²` (PQ/ADC convention, matches
/// `AqQuantizer::adc_lut`). All `m·16` partials are then requantized to int8 by one global affine map.
///
/// Edge case (`rules/testing.md § 4.1`): a corpus/query where every partial is equal (`max == min`) ⇒ a
/// degenerate range; we guard the divisor so no `0/0` (all codes map to 0, ranking is trivially flat).
pub(crate) fn build_lut16(query: &[f32], q: &AqQuantizer) -> Result<Lut16, String> {
    let m = q.m();
    let sub_dim = if m == 0 { 0 } else { q.dim() / m };
    if q.dim() == 0 {
        return Err("ah build_lut16: empty codebook (train on a non-empty corpus first)".to_string());
    }
    if query.len() != q.dim() {
        return Err(format!(
            "ah build_lut16: query dim {} != codebook dim {} (fail-fast, Rule 8)",
            query.len(),
            q.dim()
        ));
    }
    let centroids = q.centroids();
    // Compute all m*16 f32 partials, tracking the global min/max for the affine requant.
    let mut partials: Vec<f32> = Vec::with_capacity(m * 16);
    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    for (i, sub_cents) in centroids.iter().enumerate() {
        let sub = &query[i * sub_dim..(i + 1) * sub_dim];
        for j in 0..16 {
            // `AqQuantizer` always trains 16 centroids per subspace; guard a short codebook defensively.
            let val = if j < sub_cents.len() {
                let d = crate::vec::l2_distance(sub, &sub_cents[j]);
                (d * d) as f32
            } else {
                f32::INFINITY // never selected; keeps table full-width for pshufb (16-byte lane)
            };
            let finite = if val.is_finite() { val } else { 0.0 };
            partials.push(finite);
            lo = lo.min(finite);
            hi = hi.max(finite);
        }
    }
    // Affine requant: [lo, hi] → [0, LUT_MAX]. `scale = (hi-lo)/LUT_MAX` (A), `bias = lo` (B).
    let range = (hi - lo).max(f32::MIN_POSITIVE); // guard max==min (degenerate) — no div-by-zero.
    let scale = range / LUT_MAX as f32;
    let inv = LUT_MAX as f32 / range;
    let tables: Vec<i8> = partials
        .iter()
        .map(|&v| {
            let code = ((v - lo) * inv).round() as i32;
            code.clamp(0, LUT_MAX) as i8 // saturation guard (FAISS: never saturate the int8 LUT)
        })
        .collect();
    Ok(Lut16 { m, tables, scale, bias: lo })
}

// ------------------------------------------------------------------------------------------------------------
// E2 FastScan 1-bit sign kernel (plan symqg-fastscan-1bit T1.1). The SymphonyQG per-neighbour estimate needs the
// dot `⟨q_r, u⟩` with `u ∈ {−1,+1}^dim`. Reformulated as a LUT16-pshufb scan (RaBitQ FastScan): group 4 sign-dims;
// the 4-bit sign pattern `p ∈ 0..16` indexes the signed sum `Σ_{b<4} q_r[4g+b]·(bit(p,b)? +1 : −1)`. `m = dim/4`
// groups reproduce the full dot (up to the int8 requant). REUSES `ah_score_block` unchanged (it is LUT-agnostic).
// ------------------------------------------------------------------------------------------------------------

/// Build the per-query sign LUT for the FastScan 1-bit estimate. `m = ⌈dim/4⌉` groups of ≤4 sign-dims (the last
/// group may be partial — no `dim % 4 == 0` requirement, so any dim is layout-compatible; `⌈⌈dim/4⌉/2⌉ = ⌈dim/8⌉`
/// keeps `row_bytes` unchanged). Eligibility (D5, gated by the scan caller): `m ≤ 258` (int16-accumulator-safe,
/// see `LUT_MAX`) — larger dims (e.g. 1536-dim ⇒ m=384) fall back to the scalar `estimate_sign`. Fail-fast `Err`
/// on empty or `m > 258`. int8 requant mirrors `build_lut16` (one global affine map).
pub(crate) fn build_sign_lut16(q_r: &[f32]) -> Result<Lut16, String> {
    if q_r.is_empty() {
        return Err("build_sign_lut16: empty q_r (fail-fast, Rule 8)".to_string());
    }
    let dim = q_r.len();
    let m = dim.div_ceil(4);
    if m > 258 {
        return Err(format!(
            "build_sign_lut16: dim {dim} → m {m} exceeds the i16-accumulator-safe 258 (caller must gate; use scalar)"
        ));
    }
    // All m*16 signed-sum partials (subspace-major), tracking global min/max for the affine requant. The last
    // group sums only its real dims (`end`); a partial group's phantom pattern-bits are 0 at encode and ignored.
    let mut partials: Vec<f32> = Vec::with_capacity(m * 16);
    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    for g in 0..m {
        let end = ((g + 1) * 4).min(dim);
        for p in 0..16u32 {
            let mut s = 0.0f32;
            for (b, &v) in q_r[g * 4..end].iter().enumerate() {
                s += if (p >> b) & 1 == 1 { v } else { -v };
            }
            partials.push(s);
            lo = lo.min(s);
            hi = hi.max(s);
        }
    }
    let range = (hi - lo).max(f32::MIN_POSITIVE); // guard max==min (degenerate: q_r all zero) — no div-by-zero.
    let scale = range / LUT_MAX as f32;
    let inv = LUT_MAX as f32 / range;
    let tables: Vec<i8> = partials
        .iter()
        .map(|&v| (((v - lo) * inv).round() as i32).clamp(0, LUT_MAX) as i8)
        .collect();
    Ok(Lut16 { m, tables, scale, bias: lo })
}

/// FastScan estimate for up to 32 neighbours in one block. `codes` is the block32-nibble layout for the `n`
/// neighbours (`codes[pair*32 + v]`); `ah_score_block` yields the requantized dot per neighbour, then a cheap
/// per-neighbour finalize reproduces `estimate_sign`: `est = qc2 + nr² − 2·nr·(dot/w)` (with the same `w==0 ||
/// nr==0 ⇒ qc2 + nr²` early branch). `nr.len() == w.len() == out.len() == n`.
pub(crate) fn sign_estimate_block(
    lut: &Lut16,
    codes: &[u8],
    n: usize,
    nr: &[f32],
    w: &[f32],
    qc2: f64,
    out: &mut [f64],
) {
    debug_assert!(nr.len() >= n && w.len() >= n && out.len() >= n);
    let mut acc = vec![0i32; n];
    ah_score_block(lut, codes, n, &mut acc);
    for k in 0..n {
        let (nrk, wk) = (nr[k] as f64, w[k] as f64);
        out[k] = if wk == 0.0 || nrk == 0.0 {
            qc2 + nrk * nrk
        } else {
            let dot = lut.dequantize(acc[k]) as f64; // ≈ ⟨q_r, u_k⟩
            qc2 + nrk * nrk - 2.0 * nrk * (dot / wk)
        };
    }
}

/// The 4-bit code index of subspace `i` from packed code bytes (even → low nibble, odd → high nibble). Mirrors
/// `AqQuantizer`'s packing exactly so a code encoded there scores here.
#[inline]
fn nibble(code: &[u8], i: usize) -> usize {
    let byte = code[i / 2];
    (if i.is_multiple_of(2) { byte & 0x0F } else { (byte >> 4) & 0x0F }) as usize
}

/// Scalar AH score of one corpus code: `Σ_i LUT[i][code_i]` accumulated in i32 (the SIMD oracle). Smaller =
/// closer (partials are squared-L2 sub-distances). The length invariant is enforced ALWAYS (release too) so no
/// OOB can slip past a caller — mirrors `vec::l2_dist_from_bytes:258`.
pub(crate) fn ah_score_scalar(lut: &Lut16, code: &[u8]) -> i32 {
    assert_eq!(
        code.len(),
        lut.m.div_ceil(2),
        "ah_score_scalar: code must be exactly ceil(m/2) bytes for m={}",
        lut.m
    );
    let mut acc = 0i32;
    for i in 0..lut.m {
        acc += lut.tables[i * 16 + nibble(code, i)] as i32;
    }
    acc
}

// ------------------------------------------------------------------------------------------------------------
// x86_64 AVX2 `pshufb` kernel + runtime dispatch (T2.2). Sits in its own detection cache (independent of the
// M58 `AVX2_FMA` gate: AH needs only AVX2, not FMA). Same `force_for_test`/`available()` shape as `simd_x86`.
// ------------------------------------------------------------------------------------------------------------

#[cfg(target_arch = "x86_64")]
mod simd_ah {
    use super::Lut16;
    use std::arch::x86_64::*;
    use std::sync::atomic::{AtomicU8, Ordering};

    static AVX2: AtomicU8 = AtomicU8::new(2); // 2 = unknown, 1 = yes, 0 = no

    /// Detect AVX2 once, cache in an atomic (idempotent — any thread writes the same value).
    pub(super) fn available() -> bool {
        match AVX2.load(Ordering::Relaxed) {
            1 => true,
            0 => false,
            _ => {
                let ok = is_x86_feature_detected!("avx2");
                AVX2.store(u8::from(ok), Ordering::Relaxed);
                ok
            }
        }
    }

    /// AH score of ONE code with AVX2 `pshufb`. Processes the `m` subspaces two-at-a-time: each `pshufb` on a
    /// 32-byte YMM does the lookup for both the low and high nibble lanes at once. Accumulate int8 partials into
    /// **i16** lanes (`_mm256_add_epi16`) to avoid the overflow the M59 blueprint warns about (int8→int16 widen).
    ///
    /// SAFETY (caller MUST guarantee via `ah_score`): (1) AVX2 available (`available()`), (2)
    /// `code.len() == ceil(m/2)`. The reads are `lut.tables[i*16 .. i*16+16]` (in-bounds: 16 per subspace) and
    /// `code[i/2]` (in-bounds by the length invariant). No pointer escapes the function; all lookups are
    /// register shuffles. This scores a single code (per-node HNSW walk, ADR D4); a batched block variant is a
    /// Phase-4 follow-up if the walk under-feeds `pshufb`.
    #[target_feature(enable = "avx2")]
    pub(super) unsafe fn ah_score(lut: &Lut16, code: &[u8]) -> i32 { unsafe {
        let m = lut.m();
        let tab = lut.tables_ptr();
        let mut acc = 0i32;
        let mut i = 0usize;
        // Process subspaces in pairs via a broadcasted 16-byte LUT lane + a 1-byte nibble index. `pshufb`
        // selects `tab[nibble]` for the low 4 bits of each index byte (the high bit selects zero, unused here).
        while i < m {
            let byte = *code.get_unchecked(i / 2);
            // Low nibble → subspace i.
            let lut_lo = _mm_loadu_si128(tab.add(i * 16) as *const __m128i);
            let idx_lo = _mm_set1_epi8((byte & 0x0F) as i8);
            let got_lo = _mm_shuffle_epi8(lut_lo, idx_lo);
            acc += _mm_extract_epi8(got_lo, 0); // _mm_extract_epi8 already returns i32
            i += 1;
            if i < m {
                // High nibble → subspace i (now the odd one).
                let lut_hi = _mm_loadu_si128(tab.add(i * 16) as *const __m128i);
                let idx_hi = _mm_set1_epi8(((byte >> 4) & 0x0F) as i8);
                let got_hi = _mm_shuffle_epi8(lut_hi, idx_hi);
                acc += _mm_extract_epi8(got_hi, 0);
                i += 1;
            }
        }
        acc
    }}

    /// Batched AH score of a block of `n` codes, subspace-major transposed layout (FAISS FastScan `bbs`): the
    /// caller lays `codes` out so `codes[s*n + v]` holds the packed nibble of subspace-pair `s` for vector `v`.
    /// Each `_mm256_shuffle_epi8` scores 32 vectors' lookup for one subspace at once; int8→int16 widened
    /// accumulate (`_mm256_add_epi16`) avoids overflow. This is the throughput path the Phase-5 benchmark
    /// exercises (billions of lookups/sec, blueprint T2). `out.len() == n`.
    ///
    /// SAFETY: caller guarantees AVX2 available; `n <= 32`; `codes.len() == ceil(m/2) * n`; `out.len() == n`.
    #[target_feature(enable = "avx2")]
    pub(super) unsafe fn ah_score_block32(lut: &Lut16, codes: &[u8], n: usize, out: &mut [i32]) { unsafe {
        let m = lut.m();
        let tab = lut.tables_ptr();
        // Two i16 accumulators cover 32 lanes (16 per YMM half after widening).
        let mut acc_lo = _mm256_setzero_si256(); // vectors 0..16 (i16)
        let mut acc_hi = _mm256_setzero_si256(); // vectors 16..32 (i16)
        let mask_lo = _mm256_set1_epi8(0x0F);
        let mut s = 0usize;
        let mut pair = 0usize;
        while s < m {
            // 32-byte block of packed nibbles for this subspace-pair, one byte per vector.
            let block = _mm256_loadu_si256(codes.as_ptr().add(pair * 32) as *const __m256i);
            // Low nibble = subspace s; high nibble = subspace s+1.
            let idx_lo = _mm256_and_si256(block, mask_lo);
            let lut_s = broadcast_lane(tab.add(s * 16));
            let part_lo = _mm256_shuffle_epi8(lut_s, idx_lo); // 32 int8 partials for subspace s
            widen_add(&mut acc_lo, &mut acc_hi, part_lo);
            s += 1;
            if s < m {
                let idx_hi = _mm256_and_si256(_mm256_srli_epi16(block, 4), mask_lo);
                let lut_s1 = broadcast_lane(tab.add(s * 16));
                let part_hi = _mm256_shuffle_epi8(lut_s1, idx_hi);
                widen_add(&mut acc_lo, &mut acc_hi, part_hi);
                s += 1;
            }
            pair += 1;
        }
        // Store the two i16 accumulators and widen to the i32 output.
        let mut buf = [0i16; 32];
        _mm256_storeu_si256(buf.as_mut_ptr() as *mut __m256i, acc_lo);
        _mm256_storeu_si256(buf.as_mut_ptr().add(16) as *mut __m256i, acc_hi);
        for (v, o) in out.iter_mut().enumerate().take(n) {
            *o = buf[v] as i32;
        }
    }}

    /// Broadcast a 16-byte LUT lane into both 128-bit halves of a YMM so `_mm256_shuffle_epi8` (which shuffles
    /// per-128-bit-lane) uses the same table in each half.
    #[target_feature(enable = "avx2")]
    unsafe fn broadcast_lane(p: *const i8) -> __m256i { unsafe {
        let lane = _mm_loadu_si128(p as *const __m128i);
        _mm256_set_m128i(lane, lane)
    }}

    /// Widen 32 int8 partials into the two i16 accumulators (avoids overflow across `m` subspaces, blueprint T2).
    /// Uses `_mm256_cvtepi8_epi16` on each 128-bit half so byte-lane `v` maps to i16-lane `v` IN ORDER — the
    /// `unpack*_epi8` primitives interleave WITHIN each 128-bit lane and would scramble the vector→lane mapping.
    #[target_feature(enable = "avx2")]
    unsafe fn widen_add(acc_lo: &mut __m256i, acc_hi: &mut __m256i, part: __m256i) {
        let low128 = _mm256_castsi256_si128(part); // vectors 0..16 (int8)
        let high128 = _mm256_extracti128_si256(part, 1); // vectors 16..32 (int8)
        // Sign-extend each 16-byte half to 16× i16, preserving order (byte v → i16 lane v).
        *acc_lo = _mm256_add_epi16(*acc_lo, _mm256_cvtepi8_epi16(low128));
        *acc_hi = _mm256_add_epi16(*acc_hi, _mm256_cvtepi8_epi16(high128));
    }
}

#[cfg(target_arch = "x86_64")]
impl Lut16 {
    /// Raw pointer to the `m·16` int8 table (used only inside the `unsafe` AVX2 kernel, same crate).
    fn tables_ptr(&self) -> *const i8 {
        self.tables.as_ptr()
    }
}

/// Score ONE corpus code by AH. **The single-code path is scalar by design** (measured: a plain 16-entry table
/// lookup beats a per-subspace `_mm_shuffle_epi8` + `_mm_extract_epi8`, which pays a lane-extract per subspace —
/// see `ah_simd_per_candidate_speedup`). The `pshufb` throughput mechanism is the BATCHED [`ah_score_block`]
/// path (32 codes/instruction, blueprint T2). The single-code SIMD kernel [`simd_ah::ah_score`] still exists as
/// the parity oracle (forced in `ah_simd_matches_scalar_lut`) — honest evidence the intrinsic is correct — but
/// production single-code scoring dispatches to scalar. Length invariant enforced ALWAYS (no OOB).
pub(crate) fn ah_score(lut: &Lut16, code: &[u8]) -> i32 {
    assert_eq!(
        code.len(),
        lut.m().div_ceil(2),
        "ah_score: code must be exactly ceil(m/2) bytes for m={}",
        lut.m()
    );
    ah_score_scalar(lut, code)
}

/// Score a BLOCK of up to 32 corpus codes at once — the AH throughput path (batched `pshufb`, blueprint T2).
/// `codes` is the subspace-pair-major transposed layout (`codes[pair*32 + v]` = packed byte of vector `v` for
/// subspace-pair `pair`); `out.len() == n`, `n <= 32`. Dispatches to the AVX2 `pshufb` block kernel when
/// available, else a scalar loop that reproduces the same per-code accumulate. Length invariants enforced
/// ALWAYS so the `unsafe` path can never OOB.
pub(crate) fn ah_score_block(lut: &Lut16, codes: &[u8], n: usize, out: &mut [i32]) {
    let pairs = lut.m().div_ceil(2);
    assert!(n <= 32, "ah_score_block: n must be <= 32 (one FastScan block), got {n}");
    assert_eq!(out.len(), n, "ah_score_block: out.len() must equal n");
    assert_eq!(codes.len(), pairs * 32, "ah_score_block: codes must be exactly ceil(m/2)*32 bytes");
    #[cfg(target_arch = "x86_64")]
    if simd_ah::available() {
        // SAFETY: `available()` confirmed AVX2; the asserts above guarantee n<=32, out.len()==n, codes layout.
        unsafe { simd_ah::ah_score_block32(lut, codes, n, out) };
        return;
    }
    ah_score_block_scalar(lut, codes, n, out);
}

/// Scalar reference for [`ah_score_block`] — reconstructs each vector's packed code from the transposed block
/// and scores it with the single-code oracle. The fallback + the correctness reference for the AVX2 block path.
fn ah_score_block_scalar(lut: &Lut16, codes: &[u8], n: usize, out: &mut [i32]) {
    let m = lut.m();
    let pairs = m.div_ceil(2);
    let mut code = vec![0u8; pairs];
    for (v, o) in out.iter_mut().enumerate().take(n) {
        for (p, cb) in code.iter_mut().enumerate() {
            *cb = codes[p * 32 + v];
        }
        *o = ah_score_scalar(lut, &code);
    }
}

// ------------------------------------------------------------------------------------------------------------
// Tests (plan T2.1 + T2.2 + T2.3). `#[pg_test]` because the crate links pg symbols via `crate::vec`.
// ------------------------------------------------------------------------------------------------------------
// The `#[pg_test]` suite lives in the sibling `ah_tests.rs` (split so neither file exceeds the 500-LoC budget,
// `rules/architecture.md`). `include!` pulls the body inline BEFORE macro expansion so `#[pgrx::pg_schema]` sees
// the full module (a bare `#[path] mod tests;` would hand the macro only the declaration).
#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    include!("ah_tests.rs");
}
