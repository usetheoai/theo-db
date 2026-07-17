//! E2 — SymphonyQG spike (CLEAN-ROOM from arXiv:2411.12229, SIGMOD'25; the NTUITIVE-licensed C++ is study-only,
//! never copied — D1). Measures the core SymphonyQG claim on OUR stack: does folding a quantized distance estimate
//! INTO the graph traversal (co-located neighbor codes + no explicit re-rank) reach the same recall while doing
//! FAR fewer exact distance computations than exact-distance traversal on the SAME graph?
//!
//! Key parsimony win (rung 4): the SymphonyQG estimator IS our RaBitQ estimator (`vec/rabitq.rs::estimate_l2_sq`)
//! with the reference point `c` = the PARENT vertex instead of an IVF centroid. A neighbor `x_i` of parent `p` is
//! encoded as `encode(x_i − x_p)` (residual relative to the parent) — which is exactly why SymphonyQG replicates a
//! vertex's code at every parent that points to it. At query time, for a popped center `p` we compute
//! `q_r = P·(q − x_p)` and `qc2 = ‖q − x_p‖²` once, then `estimate_l2_sq(code_i, q_r, qc2) ≈ ‖q − x_i‖²`.
//!
//! The gate proxy is **exact distance computations per query at matched recall**: SymphonyQG pays ONE exact per
//! POPPED vertex (the center, which doubles as its own refinement — no separate re-rank); the exact baseline pays
//! ONE exact per CANDIDATE (every neighbor of every popped vertex). Both use the same base proximity graph (our
//! HNSW layer-0), so the delta isolates the mechanism, not the graph.
use super::hnsw::HnswIndex;
use crate::vec::rabitq::{RabitqCode, RabitqQuantizer};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};

/// Order f32 by bits for the heaps (all distances are ≥ 0 and finite here). NaN would sort high; guarded by the
/// finite inputs (SIFT vectors + squared-L2 estimates clamped ≥ 0).
#[derive(Clone, Copy, PartialEq)]
struct OrdF(f64);
impl Eq for OrdF {}
impl PartialOrd for OrdF {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for OrdF {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        self.0.partial_cmp(&o.0).unwrap_or(std::cmp::Ordering::Equal)
    }
}

/// Result of one search: the top-k node indices (ascending exact distance) and the number of EXACT distance
/// computations spent (the gate proxy).
pub(crate) struct SearchStat {
    pub topk: Vec<usize>,
    pub exact_dists: usize,
}

/// A TRUE 1-bit RaBitQ code (the SymphonyQG variant): `u[d] = sign(P·(x−c)[d]) ∈ {−1,+1}` (our multi-bit
/// `RabitqQuantizer` is DEGENERATE at bits=1 — `L = 2^0−1 = 0` gives all-zero codes). Stored relative to the
/// parent `c`. `nr = ‖x−c‖`, `w = ⟨u, o'⟩ = Σ|o'[d]|` (o' = unit rotated residual). Estimate (squared-L2):
/// `‖q−x‖² ≈ qc2 + nr² − 2·nr·(⟨q_r,u⟩ / w)`, `q_r = P·(q−c)`, `qc2 = ‖q_r‖²` — same shape as E1's estimator,
/// with the code specialized to signs so the per-neighbor dot `⟨q_r,u⟩` becomes a FastScan-friendly signed sum.
#[derive(Clone)]
pub(crate) struct SignCode {
    pub u: Vec<i8>, // ±1 per dim
    pub nr: f32,
    pub w: f32,
}

/// Encode `residual = x − c` to a 1-bit sign code, reusing `rq`'s rotation `P`.
fn encode_sign(rq: &RabitqQuantizer, residual: &[f32]) -> SignCode {
    let rr = rq.rotate(residual); // P·(x−c)
    let nr = (rr.iter().map(|&v| (v as f64) * (v as f64)).sum::<f64>()).sqrt() as f32;
    if nr == 0.0 {
        return SignCode { u: vec![0i8; rr.len()], nr: 0.0, w: 0.0 };
    }
    let u: Vec<i8> = rr.iter().map(|&v| if v >= 0.0 { 1i8 } else { -1i8 }).collect();
    // w = ⟨u, o'⟩ = Σ u·(rr/nr) = (Σ|rr|)/nr
    let w = (rr.iter().map(|&v| v.abs() as f64).sum::<f64>() as f32) / nr;
    SignCode { u, nr, w }
}

/// Scalar 1-bit estimate (the FastScan oracle for Stage 2). `q_r = P·(q−c)`, `qc2 = ‖q_r‖²`. Reused by the in-PG
/// scan (`scan_symqg_structured`) — the SAME estimator the off-PG spike validated.
#[inline]
pub(crate) fn estimate_sign(code: &SignCode, q_r: &[f32], qc2: f64) -> f64 {
    if code.w == 0.0 || code.nr == 0.0 {
        return qc2 + (code.nr as f64) * (code.nr as f64);
    }
    let mut dot = 0.0f64; // ⟨q_r, u⟩
    for (i, &qi) in q_r.iter().enumerate() {
        dot += (qi as f64) * (code.u[i] as f64);
    }
    let nr = code.nr as f64;
    qc2 + nr * nr - 2.0 * nr * (dot / code.w as f64)
}

pub(crate) struct SymqgSpike {
    /// Per parent node: its (neighbor_node, code-encoded-relative-to-this-parent). Replicated per parent — the
    /// SymphonyQG co-location. Squared-L2 semantics via `estimate_l2_sq`.
    codes: Vec<Vec<(usize, RabitqCode)>>,
    /// Sign-code variant (used when `bits == 1`) — the true 1-bit path that the FastScan kernel targets.
    sign_codes: Vec<Vec<(usize, SignCode)>>,
    sign_mode: bool,
    /// `P·x_p` per node — the rotated vertex vector, precomputed at build so the per-hop query residual is a cheap
    /// O(D) subtraction `rotate(q) − rot_vec[p]` (rotation is linear) instead of an O(D²) rotate per hop. This is
    /// the SymphonyQG "rotate the query once" lever — without it the naive spike is I/O-cheap but compute-bound.
    rot_vec: Vec<Vec<f32>>,
    rq: RabitqQuantizer,
}

impl SymqgSpike {
    /// Encode every vertex's layer-0 neighbors relative to that vertex (`encode(x_i − x_p)`). 1-bit per the paper
    /// (`bits=1`); higher bits available for the honest bit-sweep. O(N·degree·D²) — the dense O(D²) rotate is the
    /// spike's build cost (the paper uses Fast-JL O(D log D); noted as a build-time caveat, not a recall factor).
    pub(crate) fn build(g: &HnswIndex, bits: u8, seed: u64) -> Self {
        Self::build_cancellable(g, bits, seed, &|| {})
    }

    /// EC-1: the encode loop calls `check_interrupt` every 4096 vertices so a long `CREATE INDEX` responds to
    /// `pg_cancel_backend` (the AM injects `pgrx::check_for_interrupts!`; the pure `ann/` layer only declares the
    /// `Fn()` seam — same DIP pattern as `HnswIndex::build_cancellable`). Without it, a 32M-iteration encode ignores
    /// cancel until it finishes (the exact E1 k-means bug where only a postmaster kill worked).
    pub(crate) fn build_cancellable(
        g: &HnswIndex,
        bits: u8,
        seed: u64,
        check_interrupt: &(dyn Fn() + Sync),
    ) -> Self {
        let dim = g.spike_vector(0).len();
        let rq = RabitqQuantizer::train(dim, bits, seed);
        let sign_mode = bits == 1; // true 1-bit sign path (multi-bit rq is degenerate at bits=1)
        let n = g.spike_len();
        let mut codes: Vec<Vec<(usize, RabitqCode)>> = Vec::with_capacity(if sign_mode { 0 } else { n });
        let mut sign_codes: Vec<Vec<(usize, SignCode)>> = Vec::with_capacity(if sign_mode { n } else { 0 });
        let mut rot_vec: Vec<Vec<f32>> = Vec::with_capacity(n);
        for p in 0..n {
            if p % 4096 == 0 {
                check_interrupt();
            }
            let pv = g.spike_vector(p);
            rot_vec.push(rq.rotate(pv)); // P·x_p — precomputed so per-hop q_r is a subtraction, not a rotate
            let nbrs = g.spike_base_neighbors(p);
            if sign_mode {
                let mut row = Vec::with_capacity(nbrs.len());
                for &nb in nbrs {
                    let resid: Vec<f32> = g.spike_vector(nb).iter().zip(pv).map(|(&x, &c)| x - c).collect();
                    row.push((nb, encode_sign(&rq, &resid)));
                }
                sign_codes.push(row);
            } else {
                let mut row = Vec::with_capacity(nbrs.len());
                for &nb in nbrs {
                    let resid: Vec<f32> = g.spike_vector(nb).iter().zip(pv).map(|(&x, &c)| x - c).collect();
                    row.push((nb, rq.encode(&resid)));
                }
                codes.push(row);
            }
        }
        SymqgSpike { codes, sign_codes, sign_mode, rot_vec, rq }
    }

    /// Persistence accessors (T2.1 `ambuild_symqg` reads these to pack the co-located page rows).
    pub(crate) fn is_sign_mode(&self) -> bool {
        self.sign_mode
    }
    pub(crate) fn rq(&self) -> &RabitqQuantizer {
        &self.rq
    }
    /// Vertex `p`'s co-located neighbours as `(neighbour_node, sign_code)` (sign_mode only).
    pub(crate) fn sign_codes_of(&self, p: usize) -> &[(usize, SignCode)] {
        &self.sign_codes[p]
    }
    /// Vertex `p`'s ROTATED vector `P·x_p` (stored per-row so the in-PG scan gets exact-dist + q_r in one O(D)
    /// subtraction, no per-hop rotate).
    pub(crate) fn rot_vec_of(&self, p: usize) -> &[f32] {
        &self.rot_vec[p]
    }

    /// SymphonyQG traversal (Algorithm 1): pop the min-ESTIMATED candidate, compute its EXACT distance (the
    /// center/refinement, counted), estimate all its neighbors via FastScan-class RaBitQ, push. The answer is the
    /// k smallest EXACT distances among popped vertices — NO separate re-rank pass. `beam` is the working-set /
    /// ef bound.
    pub(crate) fn search(&self, g: &HnswIndex, query: &[f32], k: usize, beam: usize) -> SearchStat {
        let metric = g.spike_metric();
        let entry = match g.spike_entry() {
            Some(e) => e,
            None => return SearchStat { topk: vec![], exact_dists: 0 },
        };
        // Faithful SymphonyQG Algorithm-1 ef-search. Two SEPARATE structures, each keyed CONSISTENTLY:
        //   cand  — min-heap by ESTIMATE (pick next to expand),
        //   beam  — max-heap by ESTIMATE, capacity `beam` (the working set; its max is the prune/stop threshold),
        //   nn    — max-heap by EXACT, capacity `k` (the answer; the exact of each expanded center refines it).
        // Stop = the nearest candidate's ESTIMATE exceeds the beam's worst ESTIMATE (estimate-vs-estimate, never
        // mixing scales — the bug the first spike run exposed). NN is the k best EXACT among expanded centers.
        let mut visited: HashSet<usize> = HashSet::new();
        let mut cand: BinaryHeap<Reverse<(OrdF, usize)>> = BinaryHeap::new();
        let mut beamw: BinaryHeap<OrdF> = BinaryHeap::new();
        let mut nn: BinaryHeap<(OrdF, usize)> = BinaryHeap::new();
        let mut exact_dists = 0usize;
        // Rotate the query ONCE (O(D²)); every per-hop residual is then `rot_q − rot_vec[p]` (O(D) subtraction).
        let rot_q = self.rq.rotate(query);
        visited.insert(entry);
        let d0 = metric.dist(query, g.spike_vector(entry)); // entry seed: use exact as its own estimate
        exact_dists += 1;
        cand.push(Reverse((OrdF(d0), entry)));
        beamw.push(OrdF(d0));
        while let Some(Reverse((OrdF(est_p), p))) = cand.pop() {
            if beamw.len() >= beam {
                if let Some(&OrdF(worst)) = beamw.peek() {
                    if est_p > worst {
                        break; // nothing in the beam can be improved
                    }
                }
            }
            // expand p: one EXACT (the center/refinement), then estimate every neighbor.
            let pv = g.spike_vector(p);
            let dp = metric.dist(query, pv);
            exact_dists += 1;
            nn.push((OrdF(dp), p));
            if nn.len() > k {
                nn.pop();
            }
            // q_r = P·(q − x_p) = rot_q − P·x_p (linearity) — O(D) subtraction, NOT an O(D²) rotate per hop.
            let rp = &self.rot_vec[p];
            let q_r: Vec<f32> = rot_q.iter().zip(rp).map(|(&a, &b)| a - b).collect();
            let qc2: f64 = q_r.iter().map(|&d| (d as f64) * (d as f64)).sum(); // ‖q−c‖²=‖q_r‖² (rotation preserves norm)
            let admit_neighbor = |nb: usize, est: f64,
                                      cand: &mut BinaryHeap<Reverse<(OrdF, usize)>>,
                                      beamw: &mut BinaryHeap<OrdF>| {
                let admit = beamw.len() < beam || beamw.peek().map(|&OrdF(w)| est < w).unwrap_or(true);
                if admit {
                    cand.push(Reverse((OrdF(est), nb)));
                    beamw.push(OrdF(est));
                    if beamw.len() > beam {
                        beamw.pop();
                    }
                }
            };
            if self.sign_mode {
                for (nb, code) in &self.sign_codes[p] {
                    if !visited.insert(*nb) {
                        continue;
                    }
                    let est = estimate_sign(code, &q_r, qc2).max(0.0);
                    admit_neighbor(*nb, est, &mut cand, &mut beamw);
                }
            } else {
                for (nb, code) in &self.codes[p] {
                    if !visited.insert(*nb) {
                        continue;
                    }
                    let est = self.rq.estimate_l2_sq(code, &q_r, qc2).max(0.0);
                    admit_neighbor(*nb, est, &mut cand, &mut beamw);
                }
            }
        }
        let mut out: Vec<(f64, usize)> = nn.into_iter().map(|(OrdF(d), n)| (d, n)).collect();
        out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        out.truncate(k);
        SearchStat { topk: out.into_iter().map(|(_, n)| n).collect(), exact_dists }
    }
}

/// Exact-distance baseline beam search on the SAME graph: every candidate neighbor is scored with an EXACT
/// distance (counted). This is the control the SymphonyQG spike must beat on exact-distance count at matched
/// recall.
pub(crate) fn exact_beam_search(g: &HnswIndex, query: &[f32], k: usize, beam: usize) -> SearchStat {
    let metric = g.spike_metric();
    let entry = match g.spike_entry() {
        Some(e) => e,
        None => return SearchStat { topk: vec![], exact_dists: 0 },
    };
    let mut visited: HashSet<usize> = HashSet::new();
    let mut cand: BinaryHeap<Reverse<(OrdF, usize)>> = BinaryHeap::new();
    let mut results: BinaryHeap<(OrdF, usize)> = BinaryHeap::new();
    let mut exact_dists = 0usize;
    visited.insert(entry);
    let d0 = metric.dist(query, g.spike_vector(entry));
    exact_dists += 1;
    cand.push(Reverse((OrdF(d0), entry)));
    results.push((OrdF(d0), entry));
    while let Some(Reverse((OrdF(dp), p))) = cand.pop() {
        if results.len() >= beam {
            if let Some((OrdF(worst), _)) = results.peek() {
                if dp > *worst {
                    break;
                }
            }
        }
        for &nb in g.spike_base_neighbors(p) {
            if !visited.insert(nb) {
                continue;
            }
            let d = metric.dist(query, g.spike_vector(nb));
            exact_dists += 1;
            cand.push(Reverse((OrdF(d), nb)));
            results.push((OrdF(d), nb));
            if results.len() > beam {
                results.pop();
            }
        }
    }
    let mut out: Vec<(f64, usize)> = results.into_iter().map(|(OrdF(d), n)| (d, n)).collect();
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(k);
    SearchStat { topk: out.into_iter().map(|(_, n)| n).collect(), exact_dists }
}

#[cfg(test)]
mod tests {
    use super::{encode_sign, estimate_sign, exact_beam_search, SymqgSpike};
    use crate::vec::rabitq::RabitqQuantizer;

    // EC-6: a neighbour identical to its parent → residual 0 → nr=0, w=0; estimate_sign returns exactly qc2
    // (no div-by-zero). Pins the encode_sign guard so a refactor cannot regress it.
    #[test]
    fn symqg_encode_sign_zero_residual() {
        let dim = 32;
        let rq = RabitqQuantizer::train(dim, 1, 5);
        let zero = vec![0.0f32; dim];
        let c = encode_sign(&rq, &zero);
        assert_eq!(c.nr, 0.0);
        assert_eq!(c.w, 0.0);
        let q_r: Vec<f32> = (0..dim).map(|i| i as f32 * 0.1).collect();
        let qc2: f64 = q_r.iter().map(|&x| (x as f64) * (x as f64)).sum();
        assert!((estimate_sign(&c, &q_r, qc2) - qc2).abs() < 1e-9);
    }

    use crate::ann::{HnswIndex, Metric};
    use std::time::Instant;

    fn read_fvecs(path: &str, limit: usize) -> Vec<Vec<f32>> {
        let bytes = std::fs::read(path).expect("read fvecs");
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
        let bytes = std::fs::read(path).expect("read ivecs");
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

    // Hermetic sanity: on a small random corpus, the SymphonyQG estimated traversal must reach recall comparable
    // to the exact-distance traversal on the SAME graph at a generous beam. Catches estimator-plumbing bugs (sign,
    // per-parent encoding, qc2 scale) BEFORE the expensive SIFT run — the E1 lesson (a scan bug tanked recall).
    #[test]
    fn symqg_spike_recall_matches_exact_on_small_corpus() {
        // deterministic pseudo-random corpus (SplitMix-ish; no Date/rand dep).
        let (n, dim) = (2000usize, 24usize);
        let mut s: u64 = 0x9E3779B97F4A7C15;
        let mut next = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            ((s >> 11) as f64 / (1u64 << 53) as f64) as f32
        };
        let data: Vec<Vec<f32>> = (0..n).map(|_| (0..dim).map(|_| next() - 0.5).collect()).collect();
        let corpus: Vec<(i64, Vec<f32>)> = data.iter().enumerate().map(|(i, v)| (i as i64, v.clone())).collect();
        let g = HnswIndex::build(&corpus, 12, 100, Metric::L2, 7);
        let spike = SymqgSpike::build(&g, 7, 7); // 7-bit: estimator is near-exact, so recall must track the baseline
        let queries: Vec<Vec<f32>> = (0..40).map(|_| (0..dim).map(|_| next() - 0.5).collect()).collect();
        let (mut hs, mut he) = (0usize, 0usize);
        for q in &queries {
            // brute-force truth
            let mut all: Vec<(f64, usize)> =
                (0..n).map(|i| (Metric::L2.dist(q, &data[i]), i)).collect();
            all.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            let truth: std::collections::HashSet<usize> = all.iter().take(10).map(|&(_, i)| i).collect();
            let rs = spike.search(&g, q, 10, 80);
            let re = exact_beam_search(&g, q, 10, 80);
            hs += rs.topk.iter().filter(|&&nn| truth.contains(&nn)).count();
            he += re.topk.iter().filter(|&&nn| truth.contains(&nn)).count();
        }
        let (rec_s, rec_e) = (hs as f64 / 400.0, he as f64 / 400.0);
        // the estimated traversal must not collapse: within 15pp of the exact baseline on the same graph.
        assert!(
            rec_s >= rec_e - 0.15,
            "symqg estimated recall {rec_s:.3} collapsed vs exact {rec_e:.3} — estimator/plumbing bug"
        );
        assert!(rec_s > 0.5, "symqg recall {rec_s:.3} implausibly low — traversal is not finding neighbors");
    }

    // E2 gate harness. Runs off-PG on SIFT1M via env paths. Prints `E2_RESULT` lines:
    //   SIFT=/root N=1000000 NQ=200 BITS=1 cargo test --release symqg_spike_sift_ab -- --ignored --nocapture
    #[test]
    #[ignore]
    fn symqg_spike_sift_ab() {
        let sift = std::env::var("SIFT").unwrap_or_else(|_| "/root".into());
        let n: usize = std::env::var("N").ok().and_then(|s| s.parse().ok()).unwrap_or(1_000_000);
        let nq: usize = std::env::var("NQ").ok().and_then(|s| s.parse().ok()).unwrap_or(200);
        let bits: u8 = std::env::var("BITS").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
        let k = 10usize;
        let base = read_fvecs(&format!("{sift}/sift_base.fvecs"), n);
        let queries = read_fvecs(&format!("{sift}/sift_query.fvecs"), nq);
        let gt = read_ivecs(&format!("{sift}/sift_groundtruth.ivecs"), nq);
        println!("E2_LOADED n={} nq={} bits={}", base.len(), queries.len(), bits);
        let corpus: Vec<(i64, Vec<f32>)> = base.iter().enumerate().map(|(i, v)| (i as i64, v.clone())).collect();
        let t = Instant::now();
        let g = HnswIndex::build(&corpus, 16, 200, Metric::L2, 42);
        println!("E2_BUILD hnsw_s={:.1}", t.elapsed().as_secs_f64());
        let t = Instant::now();
        let spike = SymqgSpike::build(&g, bits, 42);
        println!("E2_BUILD symqg_encode_s={:.1}", t.elapsed().as_secs_f64());
        let recall = |topk: &[usize], truth: &[i32]| -> usize {
            let set: std::collections::HashSet<i32> = truth.iter().take(k).copied().collect();
            topk.iter().filter(|&&nn| set.contains(&(nn as i32))).count()
        };
        for beam in [10usize, 20, 40, 80, 160, 320, 640] {
            // SymphonyQG spike (estimated traversal)
            let t = Instant::now();
            let (mut hit_s, mut ex_s) = (0usize, 0usize);
            for qi in 0..nq {
                let r = spike.search(&g, &queries[qi], k, beam);
                hit_s += recall(&r.topk, &gt[qi]);
                ex_s += r.exact_dists;
            }
            let ms_s = t.elapsed().as_secs_f64() * 1000.0 / nq as f64;
            // exact-distance baseline (same graph)
            let t = Instant::now();
            let (mut hit_e, mut ex_e) = (0usize, 0usize);
            for qi in 0..nq {
                let r = exact_beam_search(&g, &queries[qi], k, beam);
                hit_e += recall(&r.topk, &gt[qi]);
                ex_e += r.exact_dists;
            }
            let ms_e = t.elapsed().as_secs_f64() * 1000.0 / nq as f64;
            let rec_s = hit_s as f64 / (nq * k) as f64;
            let rec_e = hit_e as f64 / (nq * k) as f64;
            println!(
                "E2_RESULT beam={beam} symqg_recall={rec_s:.4} symqg_exdists={} symqg_ms={ms_s:.3} exact_recall={rec_e:.4} exact_exdists={} exact_ms={ms_e:.3} exdist_ratio={:.2} speedup={:.2}",
                ex_s / nq,
                ex_e / nq,
                ex_e as f64 / ex_s.max(1) as f64,
                ms_e / ms_s.max(1e-9)
            );
        }
        println!("E2_DONE");
    }
}
