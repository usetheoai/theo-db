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

/// Lists probed per structured IVFFlat scan (bounds the pages read — the partial-read win, M31).
const SCAN_PROBES: usize = 10;

struct ScanState {
    results: Vec<(i64, f64)>,
    pos: usize,
}

#[pg_guard]
pub extern "C-unwind" fn ambeginscan(
    index_relation: pg_sys::Relation,
    nkeys: ::std::os::raw::c_int,
    norderbys: ::std::os::raw::c_int,
) -> pg_sys::IndexScanDesc {
    let scandesc = unsafe { pg_sys::RelationGetIndexScan(index_relation, nkeys, norderbys) };
    let state = Box::new(ScanState { results: Vec::new(), pos: 0 });
    unsafe { (*scandesc).opaque = Box::into_raw(state).cast::<std::os::raw::c_void>() };
    scandesc
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
        state.results.clear();
        state.pos = 0;

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
        state.results = if magic == page::IVF_STRUCT_MAGIC {
            scan_ivf_structured(rel, &query)
        } else {
            scan_blob(rel, &query)
        };
    }
}

/// M31 partial-page scan: read the meta + centroids (∝ nlists), pick the `SCAN_PROBES` nearest centroids, and read
/// ONLY those lists' pages (∝ probes) — never the whole index. Merge the pending region. Ascending distance.
unsafe fn scan_ivf_structured(rel: pg_sys::Relation, query: &[f32]) -> Vec<(i64, f64)> {
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
    let probes = SCAN_PROBES.clamp(1, meta.centroids.len().max(1));
    let dim = meta.dim as usize;
    let entry = 8 + dim * 4;

    let mut results: Vec<(i64, f64)> = Vec::new();
    // Reused scratch — the vector of the entry currently being scored (avoids a Vec<f32> alloc per entry).
    let mut scratch = vec![0f32; dim];
    for &(_, ci) in cd.iter().take(probes) {
        let (fb, np, cnt) = meta.dir[ci];
        let bytes = match page::read_ivf_list_bytes(rel, fb, np) {
            Ok(b) => b,
            Err(e) => pg_sys::error!("theodb am scan: {e}"),
        };
        for i in 0..cnt as usize {
            let o = i * entry;
            if bytes.len() < o + entry {
                break; // page shorter than the directory count claims — stop this list
            }
            let tidv = i64::from_le_bytes(bytes[o..o + 8].try_into().unwrap());
            for (j, s) in scratch.iter_mut().enumerate() {
                let p = o + 8 + j * 4;
                *s = f32::from_le_bytes(bytes[p..p + 4].try_into().unwrap());
            }
            results.push((tidv, metric.dist(query, &scratch)));
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
    results.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
    results
}

/// The M26 blob scan path — HNSW (and any legacy blob index): deserialize the whole index + search. O(N).
unsafe fn scan_blob(rel: pg_sys::Relation, query: &[f32]) -> Vec<(i64, f64)> {
    let blob = match page::read_blob(rel) {
        Ok(b) => b,
        Err(e) => pg_sys::error!("theodb am scan: {e}"),
    };
    if blob.is_empty() {
        return Vec::new();
    }
    let idx = match Persisted::from_bytes(&blob) {
        Ok(i) => i,
        Err(e) => pg_sys::error!("theodb am scan: {e}"),
    };
    let pending = match page::read_pending(rel) {
        Ok(p) => p,
        Err(e) => pg_sys::error!("theodb am scan: {e}"),
    };
    idx.search_merged(query, &pending)
}

#[pg_guard]
pub extern "C-unwind" fn amgettuple(
    scan: pg_sys::IndexScanDesc,
    _direction: pg_sys::ScanDirection::Type,
) -> bool {
    unsafe {
        let scan_ref = &mut *scan;
        let state = &mut *scan_ref.opaque.cast::<ScanState>();
        if state.pos >= state.results.len() {
            return false;
        }
        let (encoded_tid, _dist) = state.results[state.pos];
        state.pos += 1;
        tid::set_on(encoded_tid, &mut scan_ref.xs_heaptid);
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
