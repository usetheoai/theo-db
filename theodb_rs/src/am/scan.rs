//! Scan hooks (M26 Phase 3): `ambeginscan` / `amrescan` / `amgettuple` / `amendscan`.
//!
//! `amrescan` deserializes the persisted `IvfflatIndex` from pages (once), reads the ORDER-BY query vector from
//! the scan key, and runs the reused search — producing `(encoded_tid, distance)` in ascending-distance order.
//! `amgettuple` hands the executor one heap TID at a time (in that order) until exhausted.
use crate::am::build::datum_to_vec_f32;
use crate::am::index::Persisted;
use crate::am::{page, tid};
use pgrx::prelude::*;

/// Default lists probed per scan (mirrors ivfflat's `probes`; a GUC/reloption follows later). Larger = higher
/// recall, slower. `SCAN_K` caps how many candidates we materialize (the executor applies the real LIMIT).
const SCAN_PROBES: usize = 10;
const SCAN_K: usize = 10_000;

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
        let query = datum_to_vec_f32((*orderbys).sk_argument);

        let blob = match page::read_blob(scan_ref.indexRelation) {
            Ok(b) => b,
            Err(e) => pg_sys::error!("theodb am scan: {e}"),
        };
        if blob.is_empty() {
            return; // empty index
        }
        let idx = match Persisted::from_bytes(&blob) {
            Ok(i) => i,
            Err(e) => pg_sys::error!("theodb am scan: {e}"),
        };
        // Fold in tuples inserted after the build (pending region) so new rows surface without a rebuild.
        let pending = match page::read_pending(scan_ref.indexRelation) {
            Ok(p) => p,
            Err(e) => pg_sys::error!("theodb am scan: {e}"),
        };
        // The metric is baked into the persisted index (from_bytes restores it) — search uses it directly.
        state.results = idx.search_merged(&query, SCAN_K, SCAN_PROBES, &pending);
    }
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
