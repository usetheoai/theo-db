//! Scan hooks (M26 Phase 3): `ambeginscan` / `amrescan` / `amgettuple` / `amendscan`.
//!
//! `amrescan` deserializes the persisted `IvfflatIndex` from pages (once), reads the ORDER-BY query vector from
//! the scan key, and runs the reused search — producing `(encoded_tid, distance)` in ascending-distance order.
//! `amgettuple` hands the executor one heap TID at a time (in that order) until exhausted.
use crate::am::build::datum_to_vec_f32;
use crate::am::index::Persisted;
use crate::am::{page, tid};
use crate::ann::Metric;
use pgrx::prelude::*;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

// Lists probed per structured IVFFlat scan (bounds the pages read — the partial-read win, M31). M34: read from the
// `theodb_ivfflat.probes` GUC (default 10) instead of a fixed constant; still clamped to the actual list count.

/// One scored candidate: its distance-to-query key + heap TID. Ordered ascending by (distance, tid) — the exact
/// order the old `results.sort_by` produced, so the emitted top-K is byte-identical (recall unchanged, M36 ADR-1).
/// `f64::total_cmp` is a TOTAL order — under M49 cosine a zero-norm vector yields NaN, which `total_cmp` orders
/// LAST (EC-3), so a degenerate vector sinks to the end instead of panicking; the `tid` tiebreak reproduces the
/// stable order of the previous sort.
#[derive(PartialEq)]
struct Scored {
    d: f64,
    tid: i64,
}
impl Eq for Scored {}
impl Ord for Scored {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        self.d.total_cmp(&o.d).then(self.tid.cmp(&o.tid))
    }
}
impl PartialOrd for Scored {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}

/// M36: a lazy MIN-heap over the scan candidates instead of a fully-sorted Vec. `amrescan` heapifies in O(C)
/// (`BinaryHeap::from`); `amgettuple` pops the next-nearest in O(log C). The executor pulls ~k times for an
/// `ORDER BY <-> q LIMIT k`, so total work is O(C + k·log C) vs the old O(C·log C) full sort — the ~38% `sort`
/// phase the M36 measurement flagged. `Reverse` turns the max-heap into a min-heap (nearest first).
struct ScanState {
    heap: BinaryHeap<Reverse<Scored>>,
    // M52 iterative scan (HNSW only): when the heap exhausts and the executor keeps pulling (a selective WHERE
    // rejected the emitted tuples), re-search with a growing `ef`, dedup already-emitted tids, up to
    // `max_scan_tuples`. Empty query / non-HNSW index ⇒ `iterative = false` (pre-M52 behavior: at most ef_search).
    query: Vec<f32>,
    rel: pg_sys::Relation,
    ef: usize,
    // M87 — IVF iterative cursor: the `probes` the next re-search grows from. `> 0` ⇒ the index is IVF (grow
    // probes); `0` ⇒ HNSW (grow `ef`). Under a selective WHERE the executor keeps pulling, so we probe MORE lists
    // (the true NN may be in an unprobed list) AND widen the AQ rerank pool (once all lists are probed, the pool
    // caps the distinct emitted candidates — a selective filter needs more reranked to find k survivors) until
    // `max_scan_tuples` distinct tids are emitted (recall preserved).
    probes: usize,
    rerank_pool: usize,
    lists: usize, // M87 — total IVF list count; the iterative re-search grows `probes` up to this, then stops.
    last_total: usize, // M87 — candidates the last IVF scan returned; growth stops once it stops increasing.
    emitted: std::collections::HashSet<i64>,
    iterative: bool,
    exhausted: bool,
}

#[pg_guard]
pub extern "C-unwind" fn ambeginscan(
    index_relation: pg_sys::Relation,
    nkeys: ::std::os::raw::c_int,
    norderbys: ::std::os::raw::c_int,
) -> pg_sys::IndexScanDesc {
    let scandesc = unsafe { pg_sys::RelationGetIndexScan(index_relation, nkeys, norderbys) };
    let state = Box::new(ScanState {
        heap: BinaryHeap::new(),
        query: Vec::new(),
        rel: index_relation,
        ef: 0,
        probes: 0,
        rerank_pool: 0,
        lists: 0,
        last_total: 0,
        emitted: std::collections::HashSet::new(),
        iterative: false,
        exhausted: false,
    });
    unsafe { (*scandesc).opaque = Box::into_raw(state).cast::<std::os::raw::c_void>() };
    scandesc
}

/// Heapify the scan candidates into a lazy min-heap (M36) — O(C), replacing the old O(C·log C) full sort.
fn heapify(candidates: Vec<(i64, f64)>) -> BinaryHeap<Reverse<Scored>> {
    BinaryHeap::from(
        candidates.into_iter().map(|(tid, d)| Reverse(Scored { d, tid })).collect::<Vec<_>>(),
    )
}

#[pg_guard]
pub extern "C-unwind" fn amrescan(
    scan: pg_sys::IndexScanDesc,
    _keys: pg_sys::ScanKey,
    _nkeys: ::std::os::raw::c_int,
    orderbys: pg_sys::ScanKey,
    norderbys: ::std::os::raw::c_int,
) {
    unsafe {
        let scan_ref = &mut *scan;
        let state = &mut *scan_ref.opaque.cast::<ScanState>();
        // M52: reset the iterative-scan state on every rescan (the executor may rescan the same descriptor).
        state.heap.clear();
        state.emitted.clear();
        state.exhausted = false;
        state.iterative = false;

        if norderbys < 1 || orderbys.is_null() {
            return; // no ORDER BY <-> key → no index-ordered scan
        }
        // A NULL query vector (`ORDER BY col <-> NULL`) has SK_ISNULL set and a 0 sk_argument — dereferencing it
        // would segfault in pg_detoast_datum. Treat it as an empty scan (matches pgvector's ivfscan/hnswscan).
        if (*orderbys).sk_flags as u32 & pg_sys::SK_ISNULL != 0 {
            return;
        }
        // Serialize against a concurrent VACUUM fold (share mode — compatible with other scans/inserts, blocks the
        // exclusive rewrite). Prevents a torn read of the index/pending region mid-rewrite.
        crate::am::lock::index_shared(scan_ref.indexRelation);
        let query = datum_to_vec_f32((*orderbys).sk_argument);
        let rel = scan_ref.indexRelation;

        // Dispatch on the persisted layout: structured IVFFlat (M31 — partial-page read) vs the M26 blob (HNSW).
        let magic = match page::peek_magic(rel) {
            Ok(m) => m,
            Err(e) => pg_sys::error!("theodb am scan: {e}"),
        };
        if magic == 0 {
            return; // empty / unbuilt index
        }
        // M36: the scan functions heapify their candidates into a lazy min-heap (O(C)) so `amgettuple` pops the
        // top-K in O(k·log C) — replacing the old O(C·log C) full sort (the measured ~38% `sort` phase).
        state.heap = if magic == page::IVF_STRUCT_MAGIC {
            // M87: IVF (v3/v4/v5/v6) now participates in iterative scan — under a selective WHERE the executor keeps
            // pulling, so `amgettuple` re-searches with a GROWN `probes` (more lists) until `max_scan_tuples`
            // distinct tids are emitted (recall preserved). Armed only when max_scan_tuples > 0.
            let probes0 = crate::am::guc::probes();
            let pool0 = 64 * crate::am::guc::over_fetch();
            state.query = query.clone();
            state.rel = rel;
            state.probes = probes0;
            state.rerank_pool = pool0;
            state.lists = page::ivf_list_count(rel);
            state.iterative = crate::am::guc::max_scan_tuples() > 0;
            let init = scan_ivf_structured(rel, &query, probes0, pool0);
            state.last_total = init.len();
            heapify(init)
        } else if magic == crate::am::hnsw_page::HNSW_STRUCT_MAGIC {
            // M52: HNSW is the only layout with iterative scan. Record the query + starting ef so `amgettuple`
            // can grow the search under a selective WHERE. iterative is armed only when max_scan_tuples > 0.
            let ef = crate::am::guc::ef_search();
            state.query = query.clone();
            state.rel = rel;
            state.ef = ef;
            state.iterative = crate::am::guc::max_scan_tuples() > 0;
            scan_hnsw_structured(rel, &query, ef)
        } else {
            scan_blob(rel, &query)
        };
    }
}

/// M35 partial-read HNSW scan: read the meta (1 page), traverse the graph ON DEMAND reading only visited nodes'
/// element/neighbor tuples (∝ ef·M, flat in N — never the whole graph), then fold the pending region. Ascending
/// distance. Replaces the O(N) `scan_blob` path for structured `theodb_hnsw` indexes.
unsafe fn scan_hnsw_structured(rel: pg_sys::Relation, query: &[f32], ef: usize) -> BinaryHeap<Reverse<Scored>> {
    heapify(gather_hnsw_candidates(rel, query, ef))
}

/// M52: gather the HNSW scan candidates (traverse at `ef` + pending fold) as a raw `(tid, dist)` Vec. Extracted
/// so the iterative scan (`amgettuple`) can re-run it with a growing `ef` and dedup already-emitted tids, without
/// re-heapifying inside. `ef` is a parameter (not the GUC) precisely so the iterative loop can grow it.
unsafe fn gather_hnsw_candidates(rel: pg_sys::Relation, query: &[f32], ef: usize) -> Vec<(i64, f64)> {
    let meta = match crate::am::hnsw_page::read_meta(rel) {
        Ok(m) => m,
        Err(e) => pg_sys::error!("theodb am scan: {e}"),
    };
    let metric = match Metric::from_tag(meta.metric_tag) {
        Some(m) => m,
        None => pg_sys::error!("theodb am scan: unknown metric tag"),
    };
    // Fail-fast with a typed error (mirrors pgvector's "different vector dimensions") instead of letting a
    // cross-dim query reach the SIMD scorer's length assertion as a bare panic across the C boundary. Only when
    // the index has nodes: an empty index carries dim=0 and traverse short-circuits to [] regardless of the query.
    if meta.node_count > 0 && query.len() != meta.dim as usize {
        pg_sys::error!("theodb hnsw: query dim {} != index dim {}", query.len(), meta.dim);
    }
    let mut results = match crate::am::hnsw_page::traverse(rel, &meta, query, ef) {
        Ok(r) => r,
        Err(e) => pg_sys::error!("theodb am scan: {e}"),
    };
    // Fold in pending (INSERTed after build) — no rebuild (mirror the IVF path).
    let pending = match page::read_pending(rel) {
        Ok(p) => p,
        Err(e) => pg_sys::error!("theodb am scan: {e}"),
    };
    for (tidv, v) in pending {
        results.push((tidv, metric.dist(query, &v)));
    }
    results
}

/// M31 partial-page scan: read the meta + centroids (∝ nlists), pick the `SCAN_PROBES` nearest centroids, and read
/// ONLY those lists' pages (∝ probes) — never the whole index. Merge the pending region. Ascending distance.
// M87 — returns the candidate `(tid, distance)` Vec (NOT a heap) so the caller can dedup already-emitted tids on
// an iterative re-search under a selective WHERE. `probes` is passed in (the iterative cursor grows it), not read
// from the GUC. Dispatches to the v4/v5/v6 AQ gathers or the v3 f32 path.
unsafe fn scan_ivf_structured(
    rel: pg_sys::Relation,
    query: &[f32],
    probes: usize,
    rerank_pool: usize,
) -> Vec<(i64, f64)> {
    // M77 (pg_scann): an IVF-AQ v4 index (built WITH pq_subspaces) takes the batched-AH + rerank path.
    if page::ivf_is_v4(rel) {
        return scan_ivf_aq(rel, query, probes, rerank_pool);
    }
    // M83 (Roadmap v7): an IVF-AQ v5 index (built WITH separate_storage=1) takes the storage-separated path.
    if page::ivf_is_v5(rel) {
        return scan_ivf_aq_split(rel, query, probes, rerank_pool);
    }
    // M85 (Roadmap v7): an IVF-AQ v6 index (built WITH separate_storage=1, refine=sq8) reranks on SQ8 codes.
    if page::ivf_is_v6(rel) {
        return scan_ivf_aq_split_sq8(rel, query, probes, rerank_pool);
    }
    // v3 f32 IVF reranks ALL probed candidates exactly (no AH prune pool) — `rerank_pool` unused here.
    let _ = rerank_pool;
    let meta = match page::read_ivf_meta(rel) {
        Ok(m) => m,
        Err(e) => pg_sys::error!("theodb am scan: {e}"),
    };
    let metric = match Metric::from_tag(meta.metric_tag) {
        Some(m) => m,
        None => pg_sys::error!("theodb am scan: unknown metric tag"),
    };
    // NOTE: do NOT early-return on empty centroids — an index built (or vacuumed) empty still has a pending
    // region with INSERTed rows that must be folded in (else those rows are silently dropped). The probe loop
    // below is simply empty when there are no centroids, and the pending fold still runs.
    let mut cd: Vec<(f64, usize)> =
        meta.centroids.iter().enumerate().map(|(i, c)| (metric.dist(query, c), i)).collect();
    cd.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let probes = probes.clamp(1, meta.centroids.len().max(1));
    let dim = meta.dim as usize;
    let entry = 8 + dim * 4;

    let mut results: Vec<(i64, f64)> = Vec::new();
    // M49: every metric (L2/IP/cosine) scores DIRECTLY off the page bytes via a fused zero-alloc kernel — no
    // per-entry `scratch` decode. (L2 keeps its AVX2+FMA path; IP/cosine are scalar-from-bytes, still zero-alloc.)
    // Opt-in phase profiler (THEODB_SCAN_PROFILE=1): attribute the scan latency across {reads, score, sort} so the
    // optimization targets the REAL bottleneck, not an assumed one (measurement-first — ADR D3/D4). Off by default:
    // std::time::Instant is only sampled at list granularity (≤ probes pairs), never per-candidate.
    let profile = std::env::var("THEODB_SCAN_PROFILE").is_ok_and(|v| v == "1");
    let mut read_us = 0u128;
    let mut score_us = 0u128;
    let mut cand = 0usize;
    for &(_, ci) in cd.iter().take(probes) {
        let (fb, np, cnt) = meta.dir[ci];
        let t_read = std::time::Instant::now();
        let bytes = match page::read_ivf_list_bytes(rel, fb, np) {
            Ok(b) => b,
            Err(e) => pg_sys::error!("theodb am scan: {e}"),
        };
        if profile {
            read_us += t_read.elapsed().as_micros();
        }
        let t_score = std::time::Instant::now();
        for i in 0..cnt as usize {
            let o = i * entry;
            if bytes.len() < o + entry {
                break; // page shorter than the directory count claims — stop this list
            }
            let tidv = i64::from_le_bytes(bytes[o..o + 8].try_into().unwrap());
            // M49: 3-way fused, zero-alloc for all metrics (was: L2 fused; cosine/ip decoded `scratch` per entry).
            let raw = &bytes[o + 8..o + entry];
            let d = match metric {
                Metric::L2 => crate::vec::l2_dist_from_bytes(query, raw),
                Metric::Ip => crate::vec::ip_dist_from_bytes(query, raw),
                Metric::Cosine => crate::vec::cosine_dist_from_bytes(query, raw),
            };
            results.push((tidv, d));
            cand += 1;
        }
        if profile {
            score_us += t_score.elapsed().as_micros();
        }
    }
    // Fold in pending (INSERTed after build) — no rebuild.
    let pending = match page::read_pending(rel) {
        Ok(p) => p,
        Err(e) => pg_sys::error!("theodb am scan: {e}"),
    };
    for (tidv, v) in pending {
        results.push((tidv, metric.dist(query, &v)));
    }
    if profile {
        // Phase attribution for the scan (opt-in observability — the wiring-triad runtime metric). `nonempty`
        // surfaces list-balance health: a near-1 value on distinct data signals a degenerate build/corpus. M87:
        // heapify moved to the caller (the iterative re-search needs the raw Vec to dedup already-emitted tids).
        let nonempty = meta.dir.iter().filter(|(_, _, c)| *c > 0).count();
        pgrx::log!(
            "theodb scan profile: cand={cand} nonempty_lists={nonempty}/{} probes={probes} \
             reads={read_us}us score={score_us}us",
            meta.centroids.len()
        );
    }
    results
}

/// M77 (pg_scann) — the IVF-AQ v4 scan: probe nprobe lists → batched AH-LUT scan over the block32 codes (the
/// ~5-7× QPS lever the M75 spike proved) → rerank the `over_fetch` best by exact f32. O(probes) page reads, like
/// the f32 IVF path, but the per-candidate score is a near-free LUT lookup instead of 128 mults. Ports
/// `ann::ivf_aqah::search` to read from the persisted v4 pages.
unsafe fn scan_ivf_aq(rel: pg_sys::Relation, query: &[f32], probes: usize, rerank_pool: usize) -> Vec<(i64, f64)> {
    let meta = match page::read_ivf_aq_meta(rel) {
        Ok(m) => m,
        Err(e) => pg_sys::error!("theodb am scan: {e}"),
    };
    let quant = match crate::am::aq::AqQuantizer::from_meta_bytes(&meta.codebook) {
        Ok(q) => q,
        Err(e) => pg_sys::error!("theodb am scan (aq codebook): {e}"),
    };
    let lut = match crate::vec::ah::build_lut16(query, &quant) {
        Ok(l) => l,
        Err(e) => pg_sys::error!("theodb am scan (aq lut): {e}"),
    };
    let metric = match Metric::from_tag(meta.metric_tag) {
        Some(m) => m,
        None => pg_sys::error!("theodb am scan: unknown metric tag"),
    };
    let dim = meta.dim as usize;
    let entry_f32 = dim * 4;
    let pairs = (meta.m as usize).div_ceil(2);

    let mut cd: Vec<(f64, usize)> =
        meta.centroids.iter().enumerate().map(|(i, c)| (metric.dist(query, c), i)).collect();
    cd.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let probes = probes.clamp(1, meta.centroids.len().max(1));
    // M84 — AQ rerank pool = base 64 × over_fetch factor. Fixes a latent no-op: the old `over_fetch().max(64)`
    // was ALWAYS 64 (over_fetch ≤ 64, so the .max(64) floor always won), so `theodb_hnsw.over_fetch` never
    // widened the AQ rerank pool. `64 * over_fetch()` keeps the default (over_fetch=1 → 64) but lets a wider pool
    // recover recall when the AH prune quality caps it (the M83 recall-ceiling lever).
    // M87: `rerank_pool` is a parameter — the iterative re-search widens it (once all lists are probed, the pool
    // caps the distinct emitted; a selective filter needs more reranked). Floored at 1 for a degenerate call.
    let rerank_pool = rerank_pool.max(1);

    // Stage 1 — batched AH scan of probed lists. Cache each list's bytes for the rerank (read once, O(probes)).
    let mut listbytes: Vec<(usize, Vec<u8>)> = Vec::new(); // (n, bytes)
    let mut cands: Vec<(i32, usize, usize)> = Vec::new(); // (ah_score, listbytes_idx, entry)
    for &(_, ci) in cd.iter().take(probes) {
        let (fb, np, cnt) = meta.dir[ci];
        let n = cnt as usize;
        if n == 0 {
            continue;
        }
        let bytes = match page::read_ivf_list_bytes(rel, fb, np) {
            Ok(b) => b,
            Err(e) => pg_sys::error!("theodb am scan: {e}"),
        };
        let codes_off = 8 * n + entry_f32 * n;
        let nblocks = n.div_ceil(32);
        let lbidx = listbytes.len();
        let mut out = [0i32; 32];
        for b in 0..nblocks {
            let bn = (n - b * 32).min(32);
            let base = codes_off + b * pairs * 32;
            if bytes.len() < base + pairs * 32 {
                break; // page shorter than the directory count claims — stop this list
            }
            crate::vec::ah::ah_score_block(&lut, &bytes[base..base + pairs * 32], bn, &mut out[..bn]);
            for (j, &score) in out.iter().enumerate().take(bn) {
                cands.push((score, lbidx, b * 32 + j));
            }
        }
        listbytes.push((n, bytes));
    }

    // Stage 2 — rerank the `rerank_pool` best (smaller AH score = closer) with the exact f32 distance.
    let rn = rerank_pool.min(cands.len());
    if rn < cands.len() {
        cands.select_nth_unstable_by_key(rn, |c| c.0);
        cands.truncate(rn);
    }
    let mut results: Vec<(i64, f64)> = Vec::with_capacity(cands.len());
    for (_, lbidx, entry) in &cands {
        let (n, bytes) = &listbytes[*lbidx];
        let tid = i64::from_le_bytes(bytes[entry * 8..entry * 8 + 8].try_into().unwrap());
        let f32_off = 8 * n + entry * entry_f32;
        let raw = &bytes[f32_off..f32_off + entry_f32];
        let d = match metric {
            Metric::L2 => crate::vec::l2_dist_from_bytes(query, raw),
            Metric::Ip => crate::vec::ip_dist_from_bytes(query, raw),
            Metric::Cosine => crate::vec::cosine_dist_from_bytes(query, raw),
        };
        results.push((tid, d));
    }
    // M81 — fold in pending (rows INSERTed after build): f32, scored EXACTLY (never quantized), no rebuild. Same
    // contract as the v3 f32 path — else post-build INSERTs are silently dropped from a WITH (pq_subspaces) index.
    match page::read_pending(rel) {
        Ok(pending) => {
            for (tidv, v) in pending {
                results.push((tidv, metric.dist(query, &v)));
            }
        }
        Err(e) => pg_sys::error!("theodb am scan (pending): {e}"),
    }
    results
}

/// M83 (Roadmap v7 D3 spike) — the IVF-AQ v5 STORAGE-SEPARATED scan: Stage 1 reads ONLY each probed list's
/// compact CODE pages (ids + block32 codes, ~24 B/vec) and AH-prunes; Stage 2 random-reads the f32 for the
/// `over_fetch` survivors ONLY (~512 B/vec × R). Fixes the v4 interleaving (ADR-0037) that forced the full f32
/// I/O on every candidate. Recall is byte-identical to v4/v3 (same AH prune + exact rerank); the win is the far
/// smaller working set paged in during the scan-heavy Stage 1.
unsafe fn scan_ivf_aq_split(
    rel: pg_sys::Relation,
    query: &[f32],
    probes: usize,
    rerank_pool: usize,
) -> Vec<(i64, f64)> {
    let meta = match page::read_ivf_aq_meta_split(rel) {
        Ok(m) => m,
        Err(e) => pg_sys::error!("theodb am scan: {e}"),
    };
    let quant = match crate::am::aq::AqQuantizer::from_meta_bytes(&meta.codebook) {
        Ok(q) => q,
        Err(e) => pg_sys::error!("theodb am scan (aq codebook): {e}"),
    };
    let lut = match crate::vec::ah::build_lut16(query, &quant) {
        Ok(l) => l,
        Err(e) => pg_sys::error!("theodb am scan (aq lut): {e}"),
    };
    let metric = match Metric::from_tag(meta.metric_tag) {
        Some(m) => m,
        None => pg_sys::error!("theodb am scan: unknown metric tag"),
    };
    let dim = meta.dim as usize;
    let pairs = (meta.m as usize).div_ceil(2);

    let mut cd: Vec<(f64, usize)> =
        meta.centroids.iter().enumerate().map(|(i, c)| (metric.dist(query, c), i)).collect();
    cd.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let probes = probes.clamp(1, meta.centroids.len().max(1));
    // M84 — AQ rerank pool = base 64 × over_fetch factor. Fixes a latent no-op: the old `over_fetch().max(64)`
    // was ALWAYS 64 (over_fetch ≤ 64, so the .max(64) floor always won), so `theodb_hnsw.over_fetch` never
    // widened the AQ rerank pool. `64 * over_fetch()` keeps the default (over_fetch=1 → 64) but lets a wider pool
    // recover recall when the AH prune quality caps it (the M83 recall-ceiling lever).
    // M87: `rerank_pool` is a parameter — the iterative re-search widens it (once all lists are probed, the pool
    // caps the distinct emitted; a selective filter needs more reranked). Floored at 1 for a degenerate call.
    let rerank_pool = rerank_pool.max(1);

    // Stage 1 — read ONLY the CODE pages of each probed list; AH-score; keep (score, tid, list_ci, ordinal).
    let mut cands: Vec<(i32, i64, usize, usize)> = Vec::new();
    for &(_, ci) in cd.iter().take(probes) {
        let (cfb, cnp, _vfb, _vnp, cnt) = meta.dir[ci];
        let n = cnt as usize;
        if n == 0 {
            continue;
        }
        let cbytes = match page::read_ivf_list_bytes(rel, cfb, cnp) {
            Ok(b) => b,
            Err(e) => pg_sys::error!("theodb am scan: {e}"),
        };
        let codes_off = 8 * n; // codes start right after the n ids
        let nblocks = n.div_ceil(32);
        let mut out = [0i32; 32];
        for b in 0..nblocks {
            let bn = (n - b * 32).min(32);
            let base = codes_off + b * pairs * 32;
            if cbytes.len() < base + pairs * 32 {
                break;
            }
            crate::vec::ah::ah_score_block(&lut, &cbytes[base..base + pairs * 32], bn, &mut out[..bn]);
            for (j, &score) in out.iter().enumerate().take(bn) {
                let ordinal = b * 32 + j;
                let tid = i64::from_le_bytes(cbytes[ordinal * 8..ordinal * 8 + 8].try_into().unwrap());
                cands.push((score, tid, ci, ordinal));
            }
        }
    }

    // Stage 2 — rerank the `rerank_pool` best (smaller AH score = closer); random-read f32 for survivors ONLY.
    let rn = rerank_pool.min(cands.len());
    if rn < cands.len() {
        cands.select_nth_unstable_by_key(rn, |c| c.0);
        cands.truncate(rn);
    }
    let mut results: Vec<(i64, f64)> = Vec::with_capacity(cands.len());
    for (_, tid, ci, ordinal) in &cands {
        let (_cfb, _cnp, vfb, vnp, _cnt) = meta.dir[*ci];
        let raw = match page::read_vec_at(rel, vfb, vnp, *ordinal, dim) {
            Ok(b) => b,
            Err(e) => pg_sys::error!("theodb am scan (v5 rerank): {e}"),
        };
        let d = match metric {
            Metric::L2 => crate::vec::l2_dist_from_bytes(query, &raw),
            Metric::Ip => crate::vec::ip_dist_from_bytes(query, &raw),
            Metric::Cosine => crate::vec::cosine_dist_from_bytes(query, &raw),
        };
        results.push((*tid, d));
    }
    // M81 — fold in pending (rows INSERTed after build): f32, scored EXACTLY, no rebuild. Same contract as v3/v4.
    match page::read_pending(rel) {
        Ok(pending) => {
            for (tidv, v) in pending {
                results.push((tidv, metric.dist(query, &v)));
            }
        }
        Err(e) => pg_sys::error!("theodb am scan (pending): {e}"),
    }
    results
}

/// M85 (Roadmap v7) — the IVF-AQ v6 SQ8-REFINE scan: identical to the v5 split scan (Stage 1 AH prune over
/// codes-only pages) but Stage 2 reranks the `over_fetch` survivors on SQ8 codes (`dim` B/vec) instead of raw f32
/// (`dim·4` B/vec) — 4× less Stage-2 random-read I/O at the high-recall frontier (M84). Rerank is asymmetric: the
/// SQ8 code is decoded to an approximate f32, then the EXACT `Metric::dist` is applied against the f32 query, so
/// the query is never quantized (recall degrades on one side only). Recall is SQ8-approximate for built rows,
/// EXACT for pending rows (the pending region stays f32).
unsafe fn scan_ivf_aq_split_sq8(
    rel: pg_sys::Relation,
    query: &[f32],
    probes: usize,
    rerank_pool: usize,
) -> Vec<(i64, f64)> {
    let meta = match page::read_ivf_aq_meta_split_sq8(rel) {
        Ok(m) => m,
        Err(e) => pg_sys::error!("theodb am scan: {e}"),
    };
    let quant = match crate::am::aq::AqQuantizer::from_meta_bytes(&meta.aq_codebook) {
        Ok(q) => q,
        Err(e) => pg_sys::error!("theodb am scan (aq codebook): {e}"),
    };
    let sq8 = match crate::sq8::Sq8Quantizer::from_meta_bytes(&meta.sq8_codebook) {
        Ok(q) => q,
        Err(e) => pg_sys::error!("theodb am scan (sq8 codebook): {e}"),
    };
    let lut = match crate::vec::ah::build_lut16(query, &quant) {
        Ok(l) => l,
        Err(e) => pg_sys::error!("theodb am scan (aq lut): {e}"),
    };
    let metric = match Metric::from_tag(meta.metric_tag) {
        Some(m) => m,
        None => pg_sys::error!("theodb am scan: unknown metric tag"),
    };
    let dim = meta.dim as usize;
    let pairs = (meta.m as usize).div_ceil(2);

    let mut cd: Vec<(f64, usize)> =
        meta.centroids.iter().enumerate().map(|(i, c)| (metric.dist(query, c), i)).collect();
    cd.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let probes = probes.clamp(1, meta.centroids.len().max(1));
    // M87: `rerank_pool` is a parameter — the iterative re-search widens it (once all lists are probed, the pool
    // caps the distinct emitted; a selective filter needs more reranked). Floored at 1 for a degenerate call.
    let rerank_pool = rerank_pool.max(1);

    // Stage 1 — read ONLY the CODE pages of each probed list; AH-score; keep (score, tid, list_ci, ordinal).
    let mut cands: Vec<(i32, i64, usize, usize)> = Vec::new();
    for &(_, ci) in cd.iter().take(probes) {
        let (cfb, cnp, _sfb, _snp, cnt) = meta.dir[ci];
        let n = cnt as usize;
        if n == 0 {
            continue;
        }
        let cbytes = match page::read_ivf_list_bytes(rel, cfb, cnp) {
            Ok(b) => b,
            Err(e) => pg_sys::error!("theodb am scan: {e}"),
        };
        let codes_off = 8 * n;
        let nblocks = n.div_ceil(32);
        let mut out = [0i32; 32];
        for b in 0..nblocks {
            let bn = (n - b * 32).min(32);
            let base = codes_off + b * pairs * 32;
            if cbytes.len() < base + pairs * 32 {
                break;
            }
            crate::vec::ah::ah_score_block(&lut, &cbytes[base..base + pairs * 32], bn, &mut out[..bn]);
            for (j, &score) in out.iter().enumerate().take(bn) {
                let ordinal = b * 32 + j;
                let tid = i64::from_le_bytes(cbytes[ordinal * 8..ordinal * 8 + 8].try_into().unwrap());
                cands.push((score, tid, ci, ordinal));
            }
        }
    }

    // Stage 2 — rerank the `rerank_pool` best; random-read the SQ8 code for those survivors ONLY, decode, and
    // apply the exact metric against the f32 query (asymmetric). 128 B/survivor at dim=128, not 512 B.
    let rn = rerank_pool.min(cands.len());
    if rn < cands.len() {
        cands.select_nth_unstable_by_key(rn, |c| c.0);
        cands.truncate(rn);
    }
    let mut results: Vec<(i64, f64)> = Vec::with_capacity(cands.len());
    for (_, tid, ci, ordinal) in &cands {
        let (_cfb, _cnp, sfb, snp, _cnt) = meta.dir[*ci];
        let code = match page::read_sq8_at(rel, sfb, snp, *ordinal, dim) {
            Ok(b) => b,
            Err(e) => pg_sys::error!("theodb am scan (v6 sq8 rerank): {e}"),
        };
        let approx = sq8.decode(&code);
        results.push((*tid, metric.dist(query, &approx)));
    }
    // M81 — fold in pending (rows INSERTed after build): f32, scored EXACTLY (never quantized). Same as v3/v4/v5.
    match page::read_pending(rel) {
        Ok(pending) => {
            for (tidv, v) in pending {
                results.push((tidv, metric.dist(query, &v)));
            }
        }
        Err(e) => pg_sys::error!("theodb am scan (pending): {e}"),
    }
    results
}

/// The M26 blob scan path — HNSW (and any legacy blob index): deserialize the whole index + search. O(N).
unsafe fn scan_blob(rel: pg_sys::Relation, query: &[f32]) -> BinaryHeap<Reverse<Scored>> {
    let blob = match page::read_blob(rel) {
        Ok(b) => b,
        Err(e) => pg_sys::error!("theodb am scan: {e}"),
    };
    if blob.is_empty() {
        return BinaryHeap::new();
    }
    let idx = match Persisted::from_bytes(&blob) {
        Ok(i) => i,
        Err(e) => pg_sys::error!("theodb am scan: {e}"),
    };
    let pending = match page::read_pending(rel) {
        Ok(p) => p,
        Err(e) => pg_sys::error!("theodb am scan: {e}"),
    };
    // `search_merged` already returns ascending-sorted; heapify is O(C) and keeps the uniform pop path (M36).
    heapify(idx.search_merged(query, &pending))
}

#[pg_guard]
pub extern "C-unwind" fn amgettuple(
    scan: pg_sys::IndexScanDesc,
    _direction: pg_sys::ScanDirection::Type,
) -> bool {
    unsafe {
        let scan_ref = &mut *scan;
        let state = &mut *scan_ref.opaque.cast::<ScanState>();
        loop {
            // M36: pop the next-nearest candidate from the lazy min-heap — O(log C) per call, and the executor
            // only pulls ~k times for a `LIMIT k`, so the scan never pays the full O(C·log C) sort.
            if let Some(Reverse(scored)) = state.heap.pop() {
                // M52: dedup across iterative re-searches (a grown-ef search re-emits earlier candidates).
                if !state.emitted.insert(scored.tid) {
                    continue;
                }
                tid::set_on(scored.tid, &mut scan_ref.xs_heaptid);
                // Our stored vectors are the heap vectors, so within a batch the order is exact. Across iterative
                // batches the order is RELAXED (pgvector-0.8 default) — acceptable for a filtered scan; no recheck.
                scan_ref.xs_recheckorderby = false;
                scan_ref.xs_recheck = false;
                return true;
            }
            // M52 iterative scan: the heap is empty. Under a selective WHERE the executor keeps pulling, so grow
            // `ef` and re-search until `max_scan_tuples` distinct candidates are emitted (recall preserved).
            if !state.iterative || state.exhausted {
                return false;
            }
            let cap = crate::am::guc::max_scan_tuples();
            if state.emitted.len() >= cap {
                state.exhausted = true;
                return false;
            }
            // M87: grow the search. HNSW (state.probes == 0) grows `ef`; IVF (state.probes > 0) LOOPS growing both
            // `probes` (reach unprobed lists) and the AQ rerank pool (once all lists are probed the pool caps the
            // distinct emitted — a selective filter needs more reranked to find k). An empty intermediate list (or
            // a grow whose candidates were all already emitted) must NOT terminate — keep growing until fresh
            // candidates appear OR the scan saturates (all lists probed AND the returned total stops increasing).
            if state.probes == 0 {
                let new_ef = state.ef.saturating_mul(2).min(cap);
                if new_ef <= state.ef {
                    state.exhausted = true; // ef already at the ceiling — no wider search possible
                    return false;
                }
                state.ef = new_ef;
                let fresh: Vec<(i64, f64)> = gather_hnsw_candidates(state.rel, &state.query, new_ef)
                    .into_iter()
                    .filter(|(tid, _)| !state.emitted.contains(tid))
                    .collect();
                if fresh.is_empty() {
                    state.exhausted = true; // grew ef but found nothing new — the whole graph is emitted (recall 1.0)
                    return false;
                }
                state.heap = heapify(fresh);
            } else {
                loop {
                    if state.emitted.len() >= cap {
                        state.exhausted = true;
                        return false;
                    }
                    state.probes = state.probes.saturating_mul(2);
                    state.rerank_pool = state.rerank_pool.saturating_mul(2);
                    let all = scan_ivf_structured(state.rel, &state.query, state.probes, state.rerank_pool);
                    let total = all.len();
                    let fresh: Vec<(i64, f64)> =
                        all.into_iter().filter(|(tid, _)| !state.emitted.contains(tid)).collect();
                    if !fresh.is_empty() {
                        state.heap = heapify(fresh);
                        break;
                    }
                    // Nothing new. Exhaust only when fully saturated: every list probed AND the returned total
                    // stopped growing (the pool now covers all candidates). Else keep growing.
                    if state.probes >= state.lists.max(1) && total <= state.last_total {
                        state.exhausted = true;
                        return false;
                    }
                    state.last_total = total;
                }
            }
        }
    }
}

#[pg_guard]
pub extern "C-unwind" fn amendscan(scan: pg_sys::IndexScanDesc) {
    unsafe {
        let scan_ref = &mut *scan;
        if !scan_ref.opaque.is_null() {
            drop(Box::from_raw(scan_ref.opaque.cast::<ScanState>()));
            scan_ref.opaque = std::ptr::null_mut();
        }
    }
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use super::*;

    /// The lazy min-heap MUST emit candidates in the exact order the old `results.sort_by` did — ascending by
    /// (distance, tid) — so the top-K (and thus recall) is byte-identical (M36 ADR-1). This is THE recall-preserved
    /// gate for the sort→heap change.
    #[pgrx::pg_test]
    fn heap_pops_same_order_as_sort_with_ties() {
        // Candidates with distance ties (2.0 twice) + an out-of-order input — the heap must sort them.
        let candidates: Vec<(i64, f64)> =
            vec![(30, 2.0), (10, 1.0), (50, 3.0), (20, 2.0), (40, 0.5)];

        // Reference: the exact comparator the old scan used (partial_cmp by dist, then tid).
        let mut expected = candidates.clone();
        expected.sort_by(|a, b| {
            a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0))
        });

        // Under test: heapify + pop-all.
        let mut heap = heapify(candidates);
        let mut popped: Vec<(i64, f64)> = Vec::new();
        while let Some(Reverse(s)) = heap.pop() {
            popped.push((s.tid, s.d));
        }

        assert_eq!(popped, expected, "heap pop order must equal the old sort order (ties broken by tid)");
        // Spot-check the tie is broken by tid ascending: 20 (d=2.0) before 30 (d=2.0).
        let pos20 = popped.iter().position(|&(t, _)| t == 20).unwrap();
        let pos30 = popped.iter().position(|&(t, _)| t == 30).unwrap();
        assert!(pos20 < pos30, "distance tie must break by tid ascending");
    }

    /// An empty candidate set → empty heap → the first pop returns None (empty scan; same as before).
    #[pgrx::pg_test]
    fn empty_heap_pops_none() {
        let mut heap = heapify(Vec::new());
        assert!(heap.pop().is_none());
    }
}
