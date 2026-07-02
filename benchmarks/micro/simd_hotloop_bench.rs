// M31b micro-bench (committed, reproducible): validates the fused AVX2+FMA decode+distance vs the scalar
// decode+distance over the scan hot-loop, and asserts parity within eps across dims (incl. the 8-lane tail).
// Standalone (no pgrx / no PGRX_HOME) so it runs anywhere:  rustc -O --edition 2021 simd_hotloop_bench.rs && ./simd_hotloop_bench
// Portable SSE2 baseline build (no target-cpu=native) — matches the extension. Prints PARITY + before/after + speedup.

// Standalone validation of the M31b fused decode+distance (mirrors theodb_rs/src/vec.rs), portable SSE2 baseline
// build (rustc -O, no target-cpu=native) — matches the extension's portability. Asserts parity vs scalar oracle
// across dims (incl. 8-lane tail) and micro-benches scalar-decode+distance vs fused-SIMD over the scan hot-loop.

fn l2_sq_scalar_decode(query: &[f32], raw: &[u8]) -> f32 {
    // The M31 path: decode each f32 into a scratch then subtract (what we're replacing).
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
    static AVX2_FMA: AtomicU8 = AtomicU8::new(2);
    pub fn available() -> bool {
        match AVX2_FMA.load(Ordering::Relaxed) {
            1 => true, 0 => false,
            _ => { let ok = is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma");
                   AVX2_FMA.store(u8::from(ok), Ordering::Relaxed); ok }
        }
    }
    #[target_feature(enable = "avx2,fma")]
    pub unsafe fn l2_sq(query: &[f32], raw: &[u8]) -> f32 {
        let dim = query.len(); let qp = query.as_ptr(); let rp = raw.as_ptr();
        let mut acc = _mm256_setzero_ps(); let mut i = 0usize;
        while i + 8 <= dim {
            let q = _mm256_loadu_ps(qp.add(i));
            let r = _mm256_loadu_ps(rp.add(i * 4) as *const f32);
            let d = _mm256_sub_ps(q, r);
            acc = _mm256_fmadd_ps(d, d, acc); i += 8;
        }
        let mut lanes = [0f32; 8]; _mm256_storeu_ps(lanes.as_mut_ptr(), acc);
        let mut s: f32 = lanes.iter().sum();
        while i < dim {
            let o = i * 4;
            let r = f32::from_le_bytes([*rp.add(o), *rp.add(o+1), *rp.add(o+2), *rp.add(o+3)]);
            let d = *qp.add(i) - r; s += d * d; i += 1;
        }
        s
    }
}

fn l2_dist_from_bytes(query: &[f32], raw: &[u8]) -> f64 {
    #[cfg(target_arch = "x86_64")]
    let sq = if simd_x86::available() { unsafe { simd_x86::l2_sq(query, raw) } }
             else { l2_sq_scalar_decode(query, raw) };
    #[cfg(not(target_arch = "x86_64"))]
    let sq = l2_sq_scalar_decode(query, raw);
    (sq as f64).sqrt()
}

fn now() -> std::time::Instant { std::time::Instant::now() }

fn main() {
    // 1) Parity across dims (incl. tail).
    for dim in [1usize,7,8,9,16,17,128,129] {
        let q: Vec<f32> = (0..dim).map(|i| (i as f32)*0.5 - 3.0).collect();
        let c: Vec<f32> = (0..dim).map(|i| 2.0 - (i as f32)*0.3).collect();
        let raw: Vec<u8> = c.iter().flat_map(|x| x.to_le_bytes()).collect();
        let got = l2_dist_from_bytes(&q, &raw);
        let oracle = (l2_sq_scalar_decode(&q, &raw) as f64).sqrt();
        let eps = 1e-4 * (dim as f64).sqrt() * oracle.max(1.0);
        assert!((got-oracle).abs() <= eps, "dim={dim} fused={got} oracle={oracle} eps={eps}");
    }
    println!("PARITY OK (dims 1..129, within eps)");
    println!("AVX2+FMA available on this host: {}",
        { #[cfg(target_arch="x86_64")] { simd_x86::available() } #[cfg(not(target_arch="x86_64"))] { false } });

    // 2) Micro-bench: scan hot-loop, N=65000 candidates dim=128.
    let dim = 128usize; let n = 65000usize;
    let q: Vec<f32> = (0..dim).map(|i| (i as f32)*0.01).collect();
    let mut buf: Vec<u8> = Vec::with_capacity(n*dim*4);
    for k in 0..n { for j in 0..dim { buf.extend_from_slice(&(((k*7+j) as f32)*0.001).to_le_bytes()); } }
    let entry = dim*4;
    let iters = 60;
    // scalar decode+distance (M31 path)
    let mut acc = 0f64; let t = now();
    for _ in 0..iters { for k in 0..n { let o=k*entry; acc += (l2_sq_scalar_decode(&q, &buf[o..o+entry]) as f64).sqrt(); } }
    let scalar_ms = t.elapsed().as_secs_f64()*1000.0/(iters as f64);
    // fused SIMD (M31b path)
    let mut acc2 = 0f64; let t = now();
    for _ in 0..iters { for k in 0..n { let o=k*entry; acc2 += l2_dist_from_bytes(&q, &buf[o..o+entry]); } }
    let simd_ms = t.elapsed().as_secs_f64()*1000.0/(iters as f64);
    println!("scan hot-loop (N={n} dim={dim}), mean/iter over {iters}:");
    println!("  scalar decode+dist (M31): {scalar_ms:.3} ms   [checksum {acc:.1}]");
    println!("  fused AVX2+FMA (M31b):     {simd_ms:.3} ms   [checksum {acc2:.1}]");
    println!("  speedup: {:.2}x", scalar_ms/simd_ms);
}
