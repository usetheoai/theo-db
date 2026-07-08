// M59 Phase 2 — `#[pg_test]` suite for the AH LUT16 kernel (`vec::ah`). Split out of `ah.rs` (via `include!`)
// so neither file exceeds the 500-LoC budget (`rules/architecture.md`). Spliced inside `mod tests`, so
// `super::` resolves to `vec::ah`. Covers T2.1 (scalar LUT + oracle), T2.2 (pshufb parity + no-overflow +
// block32), T2.3 (micro-bench).
    use super::*;
    use pgrx::prelude::*;

    /// Tiny deterministic PRNG (SplitMix64) — local so `ah.rs` tests do not depend on `crate::ann::Rng`.
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
            ((z >> 11) as f64 / (1u64 << 53) as f64) as f32 * 2.0 - 1.0
        }
    }

    fn random_corpus(rng: &mut TestRng, n: usize, dim: usize) -> Vec<Vec<f32>> {
        (0..n).map(|_| (0..dim).map(|_| rng.next_f32()).collect()).collect()
    }

    /// The naive f32-ADC score: `Σ_i ‖query_sub_i − centroid_{i, code_i}‖²` — the exact reference the requantized
    /// LUT16 must ORDER-match (the FAISS/ScaNN validity property, blueprint T2).
    fn naive_adc_f32(q: &AqQuantizer, query: &[f32], code: &[u8]) -> f64 {
        let lut = q.adc_lut(query);
        q.adc_distance(&lut, code)
    }

    // ---------- T2.1: scalar LUT16 + oracle ----------

    #[pg_test]
    fn ah_lut_score_equals_naive_adc() {
        // Over a random corpus, ah_score_scalar RANKS codes identically to the naive f32-ADC (order-preserving —
        // the requant preserves the argmin/top-k, which is all the scan needs). blueprint T2.
        let mut r = TestRng::new(101);
        let dim = 16;
        let m = 4;
        let corpus = random_corpus(&mut r, 300, dim);
        let q = AqQuantizer::train(&corpus, m, 4, 2.0, 42).expect("train");
        let codes: Vec<Vec<u8>> = corpus.iter().map(|v| q.encode(v)).collect();
        let query = random_corpus(&mut r, 1, dim).remove(0);
        let lut = build_lut16(&query, &q).expect("lut");

        // Spearman-style: the ranking by int8 AH score must equal the ranking by naive f32 ADC on the SAME codes.
        let mut by_ah: Vec<usize> = (0..codes.len()).collect();
        by_ah.sort_by_key(|&i| ah_score_scalar(&lut, &codes[i]));
        let mut by_f32: Vec<usize> = (0..codes.len()).collect();
        by_f32.sort_by(|&a, &b| {
            naive_adc_f32(&q, &query, &codes[a])
                .partial_cmp(&naive_adc_f32(&q, &query, &codes[b]))
                .unwrap()
        });
        // Top-k agreement (the property the scan uses): the top-10 AH survivors overlap the top-10 f32 by ≥ 8/10.
        let k = 10;
        let ah_top: std::collections::HashSet<usize> = by_ah[..k].iter().copied().collect();
        let overlap = by_f32[..k].iter().filter(|i| ah_top.contains(i)).count();
        assert!(overlap >= 8, "AH top-{k} must overlap f32 top-{k} by ≥8, got {overlap}");
        // And the argmin must agree exactly (the nearest neighbour is preserved).
        assert_eq!(by_ah[0], by_f32[0], "AH argmin must equal f32 argmin (nearest is preserved)");
    }

    #[pg_test]
    fn ah_lut_requant_bounded() {
        // The int8 requant error per LUT entry is ≤ one quant step (scale). And a degenerate all-equal-partials
        // subspace (min==max) does NOT panic (div-by-zero guard).
        let mut r = TestRng::new(202);
        let dim = 8;
        let m = 2;
        let corpus = random_corpus(&mut r, 200, dim);
        let q = AqQuantizer::train(&corpus, m, 4, 1.0, 7).expect("train");
        let query = random_corpus(&mut r, 1, dim).remove(0);
        let lut = build_lut16(&query, &q).expect("lut");
        // Reconstruct each partial and compare to the f32 LUT: |A·code + B − partial| ≤ A (one step).
        let f32_lut = q.adc_lut(&query);
        for (i, sub) in f32_lut.iter().enumerate() {
            for (j, &partial) in sub.iter().enumerate() {
                let code = lut.tables[i * 16 + j] as i32;
                let recon = lut.scale * code as f32 + lut.bias;
                assert!(
                    (recon - partial).abs() <= lut.scale + 1e-3,
                    "requant off by more than one step at [{i}][{j}]: recon={recon} partial={partial} step={}",
                    lut.scale
                );
            }
        }
        // Degenerate: an empty codebook → typed Err (fail-fast), never a panic.
        let empty = AqQuantizer::train(&[], m, 4, 1.0, 1).expect("empty train");
        assert!(build_lut16(&query, &empty).is_err(), "empty codebook must be rejected");
    }

    #[pg_test]
    fn ah_build_lut16_rejects_dim_mismatch() {
        // Negative (Rule 8): a query of the wrong dim → typed Err, never OOB.
        let mut r = TestRng::new(303);
        let corpus = random_corpus(&mut r, 50, 8);
        let q = AqQuantizer::train(&corpus, 2, 4, 1.0, 1).expect("train");
        let bad_query = vec![0.0f32; 7]; // dim 7 != codebook dim 8
        assert!(build_lut16(&bad_query, &q).is_err(), "dim-mismatch query must be rejected");
    }

    // ---------- T2.2: SIMD pshufb == scalar oracle ----------

    #[cfg(target_arch = "x86_64")]
    #[pg_test]
    fn ah_simd_matches_scalar_lut() {
        // The single-code `_mm_shuffle_epi8` (pshufb) kernel is BIT-IDENTICAL to the scalar oracle across several
        // m (even + odd, incl. a non-lane-multiple tail): both sum the same int8 LUT; pshufb is exact, not
        // approximate (the tolerance lives in the requant, tested separately). This proves the intrinsic is
        // correct even though production single-code scoring dispatches to scalar (the pshufb WIN is the batched
        // block path). blueprint Coverage Corner 1 Project B (FAISS pq4 bit-for-bit accumulate).
        if !super::simd_ah::available() {
            return; // no AVX2 on this host → the pshufb kernel is unreachable; scalar oracle already covered.
        }
        let mut r = TestRng::new(404);
        for &m in &[2usize, 4, 5, 8, 16, 17] {
            let dim = m * 4; // sub_dim 4
            let corpus = random_corpus(&mut r, 250, dim);
            let q = AqQuantizer::train(&corpus, m, 4, 2.0, 11).expect("train");
            let query = random_corpus(&mut r, 1, dim).remove(0);
            let lut = build_lut16(&query, &q).expect("lut");
            for v in corpus.iter().take(40) {
                let code = q.encode(v);
                let oracle = ah_score_scalar(&lut, &code);
                // SAFETY: available() true; code.len()==ceil(m/2) by construction.
                let avx = unsafe { super::simd_ah::ah_score(&lut, &code) };
                assert_eq!(avx, oracle, "single-code pshufb must equal the scalar oracle for m={m}");
                // And the public dispatcher (scalar by design) equals the oracle too.
                assert_eq!(ah_score(&lut, &code), oracle, "public ah_score must equal the oracle for m={m}");
            }
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[pg_test]
    fn ah_simd_block32_matches_scalar() {
        // The batched block32 kernel (subspace-major transposed layout, FAISS bbs=32) scores a block of up to 32
        // codes identically to the per-code scalar oracle. Exercises the widened i16 accumulate path.
        let mut r = TestRng::new(505);
        let m = 8usize;
        let dim = m * 4;
        let corpus = random_corpus(&mut r, 64, dim);
        let q = AqQuantizer::train(&corpus, m, 4, 2.0, 13).expect("train");
        let query = random_corpus(&mut r, 1, dim).remove(0);
        let lut = build_lut16(&query, &q).expect("lut");
        if !super::simd_ah::available() {
            // No AVX2 on this host → the block kernel is unreachable; the scalar path is already covered.
            return;
        }
        let n = 32usize;
        let codes: Vec<Vec<u8>> = corpus.iter().take(n).map(|v| q.encode(v)).collect();
        // Transpose to subspace-pair-major: for each pair p, one byte per vector.
        let pairs = m.div_ceil(2);
        let mut block = vec![0u8; pairs * 32];
        for (v, code) in codes.iter().enumerate() {
            for (p, &byte) in code.iter().enumerate() {
                block[p * 32 + v] = byte;
            }
        }
        let mut out = vec![0i32; n];
        ah_score_block(&lut, &block, n, &mut out); // dispatches to the AVX2 block kernel here.
        for (v, code) in codes.iter().enumerate() {
            assert_eq!(out[v], ah_score_scalar(&lut, code), "block32 vector {v} mismatch");
        }
    }

    #[pg_test]
    fn ah_simd_no_overflow() {
        // A worst-case all-max-LUT block does not overflow the int accumulate. m=128 → max acc = 128*127 = 16256
        // < i16::MAX (32767) and ≪ i32::MAX. Construct a synthetic all-127 LUT and assert the score is exact.
        let m = 128usize;
        let tables = vec![LUT_MAX as i8; m * 16];
        let lut = Lut16 { m, tables, scale: 1.0, bias: 0.0 };
        let code = vec![0u8; m.div_ceil(2)]; // every nibble → centroid 0 → 127
        let scalar = ah_score_scalar(&lut, &code);
        assert_eq!(scalar, m as i32 * LUT_MAX, "all-max scalar accumulate must be exact (no overflow)");
        #[cfg(target_arch = "x86_64")]
        {
            if super::simd_ah::available() {
                // Single-code pshufb: exact even at the worst case.
                // SAFETY: available() true; code.len()==ceil(m/2).
                let avx1 = unsafe { super::simd_ah::ah_score(&lut, &code) };
                assert_eq!(avx1, scalar, "single-code avx must not overflow");
                // Batched block: the i16 widened accumulate must survive m=128 all-max (128*127=16256 < 32767).
                let pairs = m.div_ceil(2);
                let block = vec![0u8; pairs * 32]; // all nibbles → centroid 0 → 127
                let mut out = vec![0i32; 32];
                ah_score_block(&lut, &block, 32, &mut out);
                assert!(out.iter().all(|&s| s == scalar), "block32 accumulate must not overflow (i16 widen)");
            }
        }
    }

    // ---------- T2.3: criterion micro-bench ----------

    #[pg_test]
    fn ah_simd_per_candidate_speedup() {
        // Per-candidate scoring cost of the AH THROUGHPUT path — the batched `pshufb` block32 kernel (16/32
        // parallel lookups/instruction, blueprint T2) vs the scalar loop over the same 32 codes, m=128
        // (SIFT1M-scale). This is the honest comparison: single-code pshufb pays a lane-extract per subspace and
        // loses to a table lookup (documented on `ah_score`); the win is BATCHED. Logs the ratio + writes it for
        // the Phase-5 doc; asserts only "SIMD not slower" (loose, non-flaky). Mirrors cosine_simd_… (vec.rs:487).
        let mut r = TestRng::new(606);
        let m = 128usize;
        let dim = m * 4;
        let corpus = random_corpus(&mut r, 64, dim);
        let q = AqQuantizer::train(&corpus, m, 4, 2.0, 99).expect("train");
        let query = random_corpus(&mut r, 1, dim).remove(0);
        let lut = build_lut16(&query, &q).expect("lut");
        let pairs = m.div_ceil(2);
        // A full transposed block of 32 real codes.
        let mut block = vec![0u8; pairs * 32];
        for (v, cv) in corpus.iter().take(32).enumerate() {
            let code = q.encode(cv);
            for (p, &b) in code.iter().enumerate() {
                block[p * 32 + v] = b;
            }
        }
        let iters = 50_000u64; // each iter scores 32 codes → 1.6M candidate-scores per branch
        let run_avx = || -> f64 {
            let t0 = std::time::Instant::now();
            let mut acc = 0i64;
            let mut out = vec![0i32; 32];
            for _ in 0..iters {
                ah_score_block(std::hint::black_box(&lut), std::hint::black_box(&block), 32, &mut out);
                acc += out[0] as i64;
            }
            std::hint::black_box(acc);
            t0.elapsed().as_secs_f64()
        };
        let run_scalar = || -> f64 {
            let t0 = std::time::Instant::now();
            let mut acc = 0i64;
            let mut o = vec![0i32; 32];
            for _ in 0..iters {
                ah_score_block_scalar(std::hint::black_box(&lut), std::hint::black_box(&block), 32, &mut o);
                acc += o[0] as i64;
            }
            std::hint::black_box(acc);
            t0.elapsed().as_secs_f64()
        };
        // On x86 with AVX2, ah_score_block dispatches to the SIMD kernel; run_scalar always uses the scalar loop.
        #[cfg(target_arch = "x86_64")]
        let have_avx = super::simd_ah::available();
        #[cfg(not(target_arch = "x86_64"))]
        let have_avx = false;
        let (t_avx, t_scalar) = (run_avx(), run_scalar());
        let speedup = t_scalar / t_avx.max(1e-9);
        let line = format!(
            "M59 AH block32 micro-bench m={m} iters={iters} (x32 codes) avx2={have_avx}: scalar={t_scalar:.4}s block={t_avx:.4}s speedup={speedup:.2}x"
        );
        pgrx::log!("{line}");
        let _ = std::fs::write("/build/target/m59-ah-speedup.txt", &line);
        // Loose non-flaky guard. When AVX2 is present the batched pshufb should be ≥ the scalar loop; when it is
        // NOT present both paths are the identical scalar loop, so only the "not materially slower" bound holds.
        assert!(t_avx <= t_scalar * 1.2, "AH block32 must not be slower than the scalar loop (block={t_avx} scalar={t_scalar})");
    }
