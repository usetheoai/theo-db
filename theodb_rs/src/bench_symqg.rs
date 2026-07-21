//! E2 — in-PG benchmark entrypoint for the SymphonyQG spike. Runs INSIDE the backend (so the crate's pgrx symbols
//! resolve — a plain `cargo test` binary cannot link them), reusing the real `ann::symqg_spike` + `HnswIndex` on
//! SIFT loaded from disk. Off-PG-equivalent measurement (pure in-RAM graph search; no heap/WAL/MVCC on the search
//! path — it never touches a table), exposed as SQL only because that is the friction-free way to link the crate.
//! Call: `SELECT theodb.symqg_spike_bench('/root', 1000000, 200, 1);` (returns the E2_RESULT lines as text).
use crate::ann::symqg_spike::{SymqgSpike, exact_beam_search};
use crate::ann::{HnswIndex, Metric};
use pgrx::prelude::*;
use std::io::Write;

fn read_fvecs(path: &str, limit: usize) -> Vec<Vec<f32>> {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let mut out = Vec::new();
    let mut off = 0usize;
    while off + 4 <= bytes.len() && out.len() < limit {
        let d = i32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()) as usize;
        off += 4;
        let mut v = Vec::with_capacity(d);
        for _ in 0..d {
            v.push(f32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()));
            off += 4;
        }
        out.push(v);
    }
    out
}
fn read_ivecs(path: &str, limit: usize) -> Vec<Vec<i32>> {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let mut out = Vec::new();
    let mut off = 0usize;
    while off + 4 <= bytes.len() && out.len() < limit {
        let d = i32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()) as usize;
        off += 4;
        let mut v = Vec::with_capacity(d);
        for _ in 0..d {
            v.push(i32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()));
            off += 4;
        }
        out.push(v);
    }
    out
}

/// Build the HNSW base graph on SIFT, co-locate per-parent RaBitQ codes, and A/B the SymphonyQG estimated
/// traversal vs the exact-distance traversal on the SAME graph across a beam sweep. Returns the report text; also
/// mirrors each line to stderr (`eprintln`) so a `tail -f` on the PG log shows progress during the long build.
#[pg_extern]
fn symqg_spike_bench(sift_dir: &str, n: i64, nq: i64, bits: i32) -> String {
    let n = n as usize;
    let nq = nq as usize;
    let bits = bits as u8;
    let k = 10usize;
    let mut report = String::new();
    let mut emit = |s: &str| {
        let _ = writeln!(std::io::stderr(), "{s}");
        report.push_str(s);
        report.push('\n');
    };
    let base = read_fvecs(&format!("{sift_dir}/sift_base.fvecs"), n);
    let queries = read_fvecs(&format!("{sift_dir}/sift_query.fvecs"), nq);
    let gt = read_ivecs(&format!("{sift_dir}/sift_groundtruth.ivecs"), nq);
    emit(&format!("E2_LOADED n={} nq={} bits={}", base.len(), queries.len(), bits));
    let corpus: Vec<(i64, Vec<f32>)> =
        base.iter().enumerate().map(|(i, v)| (i as i64, v.clone())).collect();
    let t0 = std::time::Instant::now();
    let g = HnswIndex::build(&corpus, 16, 200, Metric::L2, 42);
    emit(&format!("E2_BUILD hnsw_s={:.1}", t0.elapsed().as_secs_f64()));
    let t0 = std::time::Instant::now();
    let spike = SymqgSpike::build(&g, bits, 42);
    emit(&format!("E2_BUILD symqg_encode_s={:.1}", t0.elapsed().as_secs_f64()));
    let recall = |topk: &[usize], truth: &[i32]| -> usize {
        let set: std::collections::HashSet<i32> = truth.iter().take(k).copied().collect();
        topk.iter().filter(|&&nn| set.contains(&(nn as i32))).count()
    };
    for beam in [10usize, 20, 40, 80, 160, 320, 640] {
        pgrx::check_for_interrupts!();
        let t = std::time::Instant::now();
        let (mut hit_s, mut ex_s) = (0usize, 0usize);
        for (qi, q) in queries.iter().enumerate().take(nq) {
            let r = spike.search(&g, q, k, beam);
            ex_s += r.exact_dists;
            hit_s += recall(&r.topk, &gt[qi]);
        }
        let ms_s = t.elapsed().as_secs_f64() * 1000.0 / nq as f64;
        let t = std::time::Instant::now();
        let (mut hit_e, mut ex_e) = (0usize, 0usize);
        for (qi, q) in queries.iter().enumerate().take(nq) {
            let r = exact_beam_search(&g, q, k, beam);
            ex_e += r.exact_dists;
            hit_e += recall(&r.topk, &gt[qi]);
        }
        let ms_e = t.elapsed().as_secs_f64() * 1000.0 / nq as f64;
        let rec_s = hit_s as f64 / (nq * k) as f64;
        let rec_e = hit_e as f64 / (nq * k) as f64;
        emit(&format!(
            "E2_RESULT beam={beam} symqg_recall={rec_s:.4} symqg_exdists={} symqg_ms={ms_s:.3} exact_recall={rec_e:.4} exact_exdists={} exact_ms={ms_e:.3} exdist_ratio={:.2} speedup={:.2}",
            ex_s / nq,
            ex_e / nq,
            ex_e as f64 / ex_s.max(1) as f64,
            ms_e / ms_s.max(1e-9)
        ));
    }
    emit("E2_DONE");
    report
}
