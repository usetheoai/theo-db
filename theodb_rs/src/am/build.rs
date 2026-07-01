//! `ambuild` / `ambuildempty` (M26 Phase 2) — build the IVFFlat index from the heap and persist it to pages.
//!
//! Reuses the proven `crate::ann::IvfflatIndex` (no algorithm fork). The heap scan collects `(encoded_tid, vec)`
//! into a corpus, builds the index once, serializes it (`to_bytes`), and writes it via `crate::am::page`
//! (WAL-logged). The heap TID is encoded into the `i64` id slot so `amgettuple` can return it later.
use crate::am::page;
use crate::am::tid;
use crate::ann::{IvfflatIndex, Metric};
use pgrx::prelude::*;

/// Number of IVFFlat lists (centroids) for the persisted build. A fixed sensible default for the MVP (the
/// SQL-callable path exposes `lists`; a reloption follows in a later phase). Clamped to corpus size internally.
const DEFAULT_LISTS: usize = 100;
const BUILD_SEED: u64 = 42;

/// Collected during the heap scan (one entry per live, non-NULL-vector heap tuple).
struct BuildState {
    corpus: Vec<(i64, Vec<f32>)>,
    dim: Option<usize>,
}

#[pg_guard]
pub extern "C-unwind" fn ambuild(
    heaprel: pg_sys::Relation,
    indexrel: pg_sys::Relation,
    index_info: *mut pg_sys::IndexInfo,
) -> *mut pg_sys::IndexBuildResult {
    // Phase 2 supports the DEFAULT l2 opclass; cosine/ip opclasses are a follow-up once opclass→metric resolution
    // is wired (pgrx 0.16 does not expose `get_opfamily_name`). The metric is persisted in the blob for the scan.
    let metric = Metric::L2;
    let mut state = BuildState { corpus: Vec::new(), dim: None };

    let ntuples = unsafe {
        pg_sys::table_index_build_scan(
            heaprel,
            indexrel,
            index_info,
            true,  // allow_sync
            true,  // anyvisible=false in PG; use progress=true
            Some(build_callback),
            (&mut state as *mut BuildState).cast::<std::os::raw::c_void>(),
            std::ptr::null_mut(),
        )
    };

    // Build the index (reuse the proven algorithm) and persist it (WAL-logged pages).
    let idx = IvfflatIndex::build(&state.corpus, DEFAULT_LISTS, metric, BUILD_SEED);
    unsafe { page::write_blob(indexrel, &idx.to_bytes()) };

    let mut result = unsafe { PgBox::<pg_sys::IndexBuildResult>::alloc0() };
    result.heap_tuples = ntuples as f64;
    result.index_tuples = state.corpus.len() as f64;
    result.into_pg()
}

/// Called once per heap tuple during the build scan. Skips NULL vectors (pgvector index semantics).
#[pg_guard]
unsafe extern "C-unwind" fn build_callback(
    _indexrel: pg_sys::Relation,
    htid: pg_sys::ItemPointer,
    values: *mut pg_sys::Datum,
    isnull: *mut bool,
    _tuple_is_alive: bool,
    state: *mut std::os::raw::c_void,
) {
    if *isnull {
        return; // NULL vector — not indexed (EC: pgvector semantics)
    }
    let st = &mut *state.cast::<BuildState>();
    let v = datum_to_vec_f32(*values);
    match st.dim {
        Some(d) if d != v.len() => return, // dimension mismatch guard (defensive; the type enforces dim)
        None => st.dim = Some(v.len()),
        _ => {}
    }
    st.corpus.push((tid::encode(htid), v));
}

/// Incremental insert (M26 Phase 5, ADR-2): append the new `(heap TID, vector)` to the pending region — O(1)
/// amortized, NO index rebuild. Scans fold the pending region into the ranking; VACUUM later folds it into the
/// main index. NULL vectors are not indexed (returns false).
#[allow(clippy::too_many_arguments)]
#[pg_guard]
pub unsafe extern "C-unwind" fn aminsert(
    indexrel: pg_sys::Relation,
    values: *mut pg_sys::Datum,
    isnull: *mut bool,
    heap_tid: pg_sys::ItemPointer,
    _heaprel: pg_sys::Relation,
    _check_unique: pg_sys::IndexUniqueCheck::Type,
    _index_unchanged: bool,
    _index_info: *mut pg_sys::IndexInfo,
) -> bool {
    if *isnull {
        return false;
    }
    let v = datum_to_vec_f32(*values);
    let encoded = tid::encode(heap_tid);
    match page::append_pending(indexrel, encoded, &v) {
        Ok(()) => true,
        Err(e) => pg_sys::error!("theodb am insert: {e}"),
    }
}

/// Rebuild the main index over only the live heap TIDs (M26 Phase 5 — called by VACUUM's `ambulkdelete`). Reads
/// the current main index + pending, keeps entries the `dead` predicate rejects, rebuilds, and rewrites the blob
/// (folding pending in + dropping dead TIDs). Returns the number of live entries.
pub(crate) unsafe fn vacuum_rebuild(
    indexrel: pg_sys::Relation,
    dead: &mut dyn FnMut(i64) -> bool,
) -> usize {
    let blob = match page::read_blob(indexrel) {
        Ok(b) => b,
        Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
    };
    if blob.is_empty() {
        return 0;
    }
    let idx = match IvfflatIndex::from_bytes(&blob) {
        Ok(i) => i,
        Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
    };
    let pending = match page::read_pending(indexrel) {
        Ok(p) => p,
        Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
    };
    let metric = idx.metric();
    let mut live: Vec<(i64, Vec<f32>)> = Vec::new();
    for (id, v) in idx.entries().into_iter().chain(pending) {
        if !dead(id) {
            live.push((id, v));
        }
    }
    let rebuilt = IvfflatIndex::build(&live, DEFAULT_LISTS, metric, BUILD_SEED);
    page::rewrite_blob(indexrel, &rebuilt.to_bytes());
    live.len()
}

/// Empty index (unlogged/empty table): persist an empty index blob so scans read cleanly.
#[pg_guard]
pub extern "C-unwind" fn ambuildempty(indexrel: pg_sys::Relation) {
    let idx = IvfflatIndex::build(&[], DEFAULT_LISTS, Metric::L2, BUILD_SEED);
    unsafe { page::write_blob(indexrel, &idx.to_bytes()) };
}

/// Convert a pgvector `vector` Datum into a `Vec<f32>`. pgvector's on-disk layout (studied in M20): a varlena
/// header, then `int16 dim`, `int16 unused`, then `dim × float4`. Detoast first (vectors are plain-stored but be
/// safe). Ported to a raw reader because the AM callback receives the raw Datum (no SQL `::real[]` cast here).
pub(crate) unsafe fn datum_to_vec_f32(datum: pg_sys::Datum) -> Vec<f32> {
    let detoasted = pg_sys::pg_detoast_datum(datum.cast_mut_ptr::<pg_sys::varlena>());
    let base = detoasted.cast::<u8>();
    // VARHDRSZ (4-byte varlena header) — pgvector always uses the 4-byte header for `vector`.
    let data = base.add(4);
    let dim = i16::from_ne_bytes([*data, *data.add(1)]) as usize;
    let floats = data.add(4).cast::<f32>(); // skip dim(2) + unused(2)
    let mut out = Vec::with_capacity(dim);
    for i in 0..dim {
        out.push(floats.add(i).read_unaligned());
    }
    out
}
