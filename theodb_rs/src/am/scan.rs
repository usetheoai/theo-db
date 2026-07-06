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
/// `f64::total_cmp` is a TOTAL order (no NaN hazard — distances are finite), and the `tid` tiebreak reproduces the
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
}

#[pg_guard]
pub extern "C-unwind" fn ambeginscan(
    index_relation: pg_sys::Relation,
    nkeys: ::std::os::raw::c_int,
    norderbys: ::std::os::raw::c_int,
) -> pg_sys::IndexScanDesc {
    let scandesc = unsafe { pg_sys::RelationGetIndexScan(index_relation, nkeys, norderbys) };
    let state = Box::new(ScanState { heap: BinaryHeap::new() });
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
        state.heap.clear();

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
            scan_ivf_structured(rel, &query)
        } else if magic == crate::am::hnsw_page::HNSW_STRUCT_MAGIC {
            scan_hnsw_structured(rel, &query)
        } else {
            scan_blob(rel, &query)
        };
    }
}

/// M35 partial-read HNSW scan: read the meta (1 page), traverse the graph ON DEMAND reading only visited nodes'
/// element/neighbor tuples (∝ ef·M, flat in N — never the whole graph), then fold the pending region. Ascending
/// distance. Replaces the O(N) `scan_blob` path for structured `theodb_hnsw` indexes.
unsafe fn scan_hnsw_structured(rel: pg_sys::Relation, query: &[f32]) -> BinaryHeap<Reverse<Scored>> {
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
    let ef = crate::am::guc::ef_search();
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
    // M36: heapify (O(C)) instead of the old O(C·log C) sort — `amgettuple` pops the top-K lazily.
    heapify(results)
}

/// M31 partial-page scan: read the meta + centroids (∝ nlists), pick the `SCAN_PROBES` nearest centroids, and read
/// ONLY those lists' pages (∝ probes) — never the whole index. Merge the pending region. Ascending distance.
unsafe fn scan_ivf_structured(rel: pg_sys::Relation, query: &[f32]) -> BinaryHeap<Reverse<Scored>> {
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
    let probes = crate::am::guc::probes().clamp(1, meta.centroids.len().max(1));
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
    // M36: heapify (O(C)) instead of the old O(C·log C) full sort — the lazy min-heap the executor pops top-K from.
    let t_heapify = std::time::Instant::now();
    let heap = heapify(results);
    if profile {
        // Phase attribution for the scan (opt-in observability — the wiring-triad runtime metric). `nonempty`
        // surfaces list-balance health: a near-1 value on distinct data signals a degenerate build/corpus.
        let heapify_us = t_heapify.elapsed().as_micros();
        let nonempty = meta.dir.iter().filter(|(_, _, c)| *c > 0).count();
        // LOG (server log, not client) — a diagnostic is not a WARNING; keeps client output + warn-as-error tooling
        // clean while `THEODB_SCAN_PROFILE=1`. Read via the server log (`docker logs`). `heapify` replaced `sort`
        // in M36 (O(C) vs O(C·log C)); per-pop cost moved to `amgettuple` (bounded by the executor's LIMIT).
        pgrx::log!(
            "theodb scan profile: cand={cand} nonempty_lists={nonempty}/{} probes={probes} \
             reads={read_us}us score={score_us}us heapify={heapify_us}us",
            meta.centroids.len()
        );
    }
    heap
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
        // M36: pop the next-nearest candidate from the lazy min-heap — O(log C) per call, and the executor only
        // pulls ~k times for a `LIMIT k`, so the scan never pays the full O(C·log C) sort.
        let Some(Reverse(scored)) = state.heap.pop() else {
            return false;
        };
        tid::set_on(scored.tid, &mut scan_ref.xs_heaptid);
        // Our stored vectors are the heap vectors, so the emitted distance order is exact — no recheck needed.
        scan_ref.xs_recheckorderby = false;
        scan_ref.xs_recheck = false;
        true
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
mod scan_heap_tests {
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
