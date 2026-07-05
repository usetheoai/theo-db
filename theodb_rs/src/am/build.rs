//! `ambuild` / `ambuildempty` (M26 Phase 2) — build the IVFFlat index from the heap and persist it to pages.
//!
//! Reuses the proven `crate::ann::IvfflatIndex` (no algorithm fork). The heap scan collects `(encoded_tid, vec)`
//! into a corpus, builds the index once, serializes it (`to_bytes`), and writes it via `crate::am::page`
//! (WAL-logged). The heap TID is encoded into the `i64` id slot so `amgettuple` can return it later.
use crate::am::index::Persisted;
use crate::am::page;
use crate::am::tid;
use crate::ann::{HnswIndex, IvfflatIndex, Metric};
use pgrx::prelude::*;

/// Default IVFFlat list count — the reloption default lives in `options::DEFAULT_LISTS` (M34). Re-exported here for
/// the empty/HNSW build paths that don't read the reloption.
use crate::am::options::{lists_from_relation, DEFAULT_LISTS};
/// HNSW build params for the persisted AM (mirror the SQL-callable defaults).
const HNSW_M: usize = 16;
const HNSW_EF_CONSTRUCTION: usize = 64;
const BUILD_SEED: u64 = 42;

/// Collected during the heap scan (one entry per live, non-NULL-vector heap tuple).
struct BuildState {
    corpus: Vec<(i64, Vec<f32>)>,
    dim: Option<usize>,
}

/// Scan the heap once, collecting `(encoded heap TID, vector)` for every live non-NULL-vector tuple. Shared by
/// both AMs' `ambuild`. Returns `(corpus, heap_tuple_count)`.
unsafe fn collect_corpus(
    heaprel: pg_sys::Relation,
    indexrel: pg_sys::Relation,
    index_info: *mut pg_sys::IndexInfo,
) -> (Vec<(i64, Vec<f32>)>, f64) {
    let mut state = BuildState { corpus: Vec::new(), dim: None };
    let ntuples = pg_sys::table_index_build_scan(
        heaprel,
        indexrel,
        index_info,
        true, // allow_sync
        true, // progress
        Some(build_callback),
        (&mut state as *mut BuildState).cast::<std::os::raw::c_void>(),
        std::ptr::null_mut(),
    );
    (state.corpus, ntuples as f64)
}

unsafe fn build_result(ntuples: f64, nindexed: usize) -> *mut pg_sys::IndexBuildResult {
    let mut result = PgBox::<pg_sys::IndexBuildResult>::alloc0();
    result.heap_tuples = ntuples;
    result.index_tuples = nindexed as f64;
    result.into_pg()
}

/// `theodb_ivfflat` build. The DEFAULT l2 opclass sets the metric; cosine/ip opclasses are a follow-up (pgrx 0.16
/// does not expose `get_opfamily_name` for opclass→metric resolution). The metric is persisted in the blob.
#[pg_guard]
pub extern "C-unwind" fn ambuild(
    heaprel: pg_sys::Relation,
    indexrel: pg_sys::Relation,
    index_info: *mut pg_sys::IndexInfo,
) -> *mut pg_sys::IndexBuildResult {
    unsafe {
        let (corpus, ntuples) = collect_corpus(heaprel, indexrel, index_info);
        let lists = lists_from_relation(indexrel); // M34 — WITH (lists=N), default 100
        let idx = IvfflatIndex::build(&corpus, lists, Metric::L2, BUILD_SEED);
        // M31: persist in the STRUCTURED layout (meta + centroids + per-list pages) so scans read only probed
        // lists (O(probes)), not the whole blob (O(N)).
        let dim = corpus.first().map(|(_, v)| v.len()).unwrap_or(0) as u32;
        page::write_ivf_structured(indexrel, dim, Metric::L2.tag(), idx.centroids(), &idx.list_entries());
        build_result(ntuples, corpus.len())
    }
}

/// `theodb_hnsw` build — same persistence layer, an HNSW graph instead of IVFFlat lists (M26 Phase 6).
#[pg_guard]
pub extern "C-unwind" fn ambuild_hnsw(
    heaprel: pg_sys::Relation,
    indexrel: pg_sys::Relation,
    index_info: *mut pg_sys::IndexInfo,
) -> *mut pg_sys::IndexBuildResult {
    unsafe {
        let (corpus, ntuples) = collect_corpus(heaprel, indexrel, index_info);
        // T4.1: inject the cancellation seam so a long `CREATE INDEX` responds to `pg_cancel_backend` within one
        // parallel batch. `check_for_interrupts!` runs on the leader between batches (all workers joined) — safe
        // to longjmp. Under `#[pg_guard]` (this callback), the ereport(ERROR) unwinds cleanly across the C boundary.
        let idx = HnswIndex::build_cancellable(
            &corpus, HNSW_M, HNSW_EF_CONSTRUCTION, Metric::L2, BUILD_SEED, &|| { pgrx::check_for_interrupts!(); },
        );
        // M35: persist the STRUCTURED page-native layout (meta + element + neighbor tuples) so scans traverse the
        // graph ON DEMAND (O(ef·M) pages), not deserialize the whole blob (O(N)).
        match crate::am::hnsw_page::pack(&idx) {
            Ok(packed) => crate::am::hnsw_page::write_structured(indexrel, pg_sys::ForkNumber::MAIN_FORKNUM, &packed),
            Err(e) => pg_sys::error!("theodb hnsw build: {e}"),
        }
        build_result(ntuples, corpus.len())
    }
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
    // Share the fold lock — a concurrent VACUUM rewrite (exclusive) must not run while we append to pending.
    crate::am::lock::index_shared(indexrel);
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
    // Exclusive the fold lock — wait for all concurrent scans/inserts (share) to finish, then rewrite alone.
    crate::am::lock::index_exclusive(indexrel);
    let magic = match page::peek_magic(indexrel) {
        Ok(m) => m,
        Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
    };
    if magic == 0 {
        return 0; // unbuilt
    }
    if magic == page::IVF_STRUCT_MAGIC {
        return vacuum_rebuild_structured(indexrel, dead);
    }
    if magic == crate::am::hnsw_page::HNSW_STRUCT_MAGIC {
        return vacuum_rebuild_hnsw_structured(indexrel, dead);
    }
    // Blob (M26 legacy) path.
    let blob = match page::read_blob(indexrel) {
        Ok(b) => b,
        Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
    };
    if blob.is_empty() {
        return 0;
    }
    let idx = match Persisted::from_bytes(&blob) {
        Ok(i) => i,
        Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
    };
    let pending = match page::read_pending(indexrel) {
        Ok(p) => p,
        Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
    };
    let mut live: Vec<(i64, Vec<f32>)> = Vec::new();
    for (id, v) in idx.entries().into_iter().chain(pending) {
        if !dead(id) {
            live.push((id, v));
        }
    }
    let rebuilt = idx.rebuilt_with(&live, BUILD_SEED);
    page::rewrite_blob(indexrel, &rebuilt.to_bytes());
    live.len()
}

/// M35 VACUUM fold for the structured HNSW layout: enumerate every element tuple + pending, drop dead, rebuild the
/// graph, and rewrite the structured layout in place (folding pending in, dropping dead TIDs). Returns live count.
unsafe fn vacuum_rebuild_hnsw_structured(indexrel: pg_sys::Relation, dead: &mut dyn FnMut(i64) -> bool) -> usize {
    let meta = match crate::am::hnsw_page::read_meta(indexrel) {
        Ok(m) => m,
        Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
    };
    let metric = match Metric::from_tag(meta.metric_tag) {
        Some(m) => m,
        None => pg_sys::error!("theodb am vacuum: unknown metric tag"),
    };
    let mut all = match crate::am::hnsw_page::enumerate_entries(indexrel, &meta) {
        Ok(e) => e,
        Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
    };
    match page::read_pending(indexrel) {
        Ok(p) => all.extend(p),
        Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
    }
    let live: Vec<(i64, Vec<f32>)> = all.into_iter().filter(|(id, _)| !dead(*id)).collect();
    // HNSW build params are fixed consts (no reloption), so rebuild with them; preserve the metric from meta.
    // T4.1: the fold's rebuild is also cancellable (a VACUUM of a huge index responds to cancel per batch).
    let idx = HnswIndex::build_cancellable(
        &live, HNSW_M, HNSW_EF_CONSTRUCTION, metric, BUILD_SEED, &|| { pgrx::check_for_interrupts!(); },
    );
    // M48 (#47): crash-safe fold — pack the new generation at a fresh base, write it to inert pages, then pivot
    // block 0. `pack_at` resolves the graph's pointers relative to `base`, so the packed image is position-
    // independent and readers (which follow meta.elem_first/nbr_first/entry_blkno) need no change. T2.2: pack once
    // at base 1 to count pages (the count is base-independent), pick a base that reuses the dead low region when
    // it fits (bounded growth), and repack only if the base changed.
    let probe = match crate::am::hnsw_page::pack_at(&idx, 1) {
        Ok(p) => p,
        Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
    };
    let need = probe.pages.len() as u32;
    let nblocks = pg_sys::RelationGetNumberOfBlocksInFork(indexrel, pg_sys::ForkNumber::MAIN_FORKNUM);
    let base = crate::am::fold::free_region(crate::am::fold::cur_gen_start(indexrel), nblocks, need);
    let packed = if base == 1 {
        probe
    } else {
        match crate::am::hnsw_page::pack_at(&idx, base as usize) {
            Ok(p) => p,
            Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
        }
    };
    crate::am::fold::fold(indexrel, &packed.meta, &packed.pages, base);
    live.len()
}

/// VACUUM fold for the structured IVFFlat layout (M31): enumerate all list entries + pending, drop dead, rebuild,
/// and rewrite the structured layout in place.
unsafe fn vacuum_rebuild_structured(indexrel: pg_sys::Relation, dead: &mut dyn FnMut(i64) -> bool) -> usize {
    let meta = match page::read_ivf_meta(indexrel) {
        Ok(m) => m,
        Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
    };
    let metric = match Metric::from_tag(meta.metric_tag) {
        Some(m) => m,
        None => pg_sys::error!("theodb am vacuum: unknown metric tag"),
    };
    let mut all: Vec<(i64, Vec<f32>)> = Vec::new();
    for ci in 0..meta.centroids.len() {
        let (fb, np, cnt) = meta.dir[ci];
        match page::read_ivf_list(indexrel, fb, np, cnt, meta.dim) {
            Ok(e) => all.extend(e),
            Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
        }
    }
    match page::read_pending(indexrel) {
        Ok(p) => all.extend(p),
        Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
    }
    let live: Vec<(i64, Vec<f32>)> = all.into_iter().filter(|(id, _)| !dead(*id)).collect();
    let dim = live.first().map(|(_, v)| v.len()).unwrap_or(meta.dim as usize) as u32;
    // M34 — a VACUUM fold preserves the built list count (WITH (lists=N)); reverting to the default would silently
    // re-partition a tuned index.
    let idx = IvfflatIndex::build(&live, lists_from_relation(indexrel), metric, BUILD_SEED);
    // M48 (#47): crash-safe fold — the v3 items carry gen_base = the fresh base, so the relocated directory /
    // centroids / lists resolve correctly after block 0 is pivoted. One item per page ⇒ wrap each as a 1-item
    // page. T2.2: the per-page count is base-independent, so build items at base 1 to count, choose a base that
    // reuses the dead low region when it fits, and rebuild the items only if the base changed.
    let probe = page::ivf_structured_items(1, dim, metric.tag(), idx.centroids(), &idx.list_entries());
    let need = probe.len() as u32 - 1; // minus the meta (item 0, written to block 0)
    let nblocks = pg_sys::RelationGetNumberOfBlocksInFork(indexrel, pg_sys::ForkNumber::MAIN_FORKNUM);
    let base = crate::am::fold::free_region(crate::am::fold::cur_gen_start(indexrel), nblocks, need);
    let items = if base == 1 {
        probe
    } else {
        page::ivf_structured_items(base, dim, metric.tag(), idx.centroids(), &idx.list_entries())
    };
    let (meta, body_items) = items.split_first().expect("ivf structured items always include the meta");
    let body: Vec<Vec<Vec<u8>>> = body_items.iter().map(|it| vec![it.clone()]).collect();
    crate::am::fold::fold(indexrel, meta, &body, base);
    live.len()
}

/// Empty index for an UNLOGGED table: Postgres calls `ambuildempty` to populate the INIT fork (the template
/// copied to the main fork on crash-recovery reset) — NOT the main fork. Writing MAIN here would append spurious
/// pages to the already-built main fork (pgvector writes INIT_FORKNUM too, `ivfbuild.c:1084`).
#[pg_guard]
pub extern "C-unwind" fn ambuildempty(indexrel: pg_sys::Relation) {
    let idx = IvfflatIndex::build(&[], DEFAULT_LISTS, Metric::L2, BUILD_SEED);
    unsafe {
        page::write_blob(indexrel, pg_sys::ForkNumber::INIT_FORKNUM, &idx.to_bytes());
        wal_log_init_fork(indexrel);
    }
}

#[pg_guard]
pub extern "C-unwind" fn ambuildempty_hnsw(indexrel: pg_sys::Relation) {
    let idx = HnswIndex::build(&[], HNSW_M, HNSW_EF_CONSTRUCTION, Metric::L2, BUILD_SEED);
    unsafe {
        // M35: an empty structured graph is meta-only (entry_level = -1).
        match crate::am::hnsw_page::pack(&idx) {
            Ok(packed) => {
                crate::am::hnsw_page::write_structured(indexrel, pg_sys::ForkNumber::INIT_FORKNUM, &packed);
                wal_log_init_fork(indexrel);
            }
            Err(e) => pg_sys::error!("theodb hnsw buildempty: {e}"),
        }
    }
}

/// Issue #46: WAL-log every INIT-fork page unconditionally. `GenericXLog` is a WAL no-op when
/// `RelationNeedsWAL()` is false (always the case for the UNLOGGED relations that get an INIT fork), so
/// without this the crash-recovery reset copies a fork that never reached the WAL — the reset main fork
/// comes up empty/zeroed and `aminsert` fails with "truncated meta page" until REINDEX. Pattern is the
/// upstream fix verbatim: pgvector `hnswbuild.c:1137-1138` / gist `gist.c:133-150` (`log_newpage_range`
/// with FPIs for the whole fork). Called as the LAST step of buildempty so the range covers every page.
unsafe fn wal_log_init_fork(indexrel: pg_sys::Relation) {
    let fork = pg_sys::ForkNumber::INIT_FORKNUM;
    let nblocks = pg_sys::RelationGetNumberOfBlocksInFork(indexrel, fork);
    if nblocks > 0 {
        pg_sys::log_newpage_range(indexrel, fork, 0, nblocks, true);
    }
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
