//! M99 — the `theodb_columnar` append-only columnar Table Access Method.
//!
//! **Phase A (this file, initial):** the de-risk registration spike — register a real `TableAmRoutine` (pgrx 0.19,
//! pg17) so `CREATE TABLE ... USING theodb_columnar` loads end-to-end and an empty seqscan returns zero rows. This
//! proves the TAM FFI/registration path on THIS toolchain BEFORE the large write/read/MVCC build (ROADMAP M99
//! ALTO-risk guard), exactly as `am/mod.rs` did for the IndexAmRoutine in M26 Phase 0.
//!
//! The real write path (stripe/chunk/zstd, `columnar.stripe` catalog) is Phase B (`columnar/writer.rs`); the read
//! path + MVCC-via-catalog visibility + min/max chunk-group pruning is Phase C (`columnar/reader.rs`,
//! `columnar/meta.rs`); the isolation permutation proofs + crash-safety + benchmark are Phase D.
//!
//! **License (ADR-0042):** own code. Hydra columnar (AGPLv3) + Citus columnar (AGPLv3) are studied as design
//! literature ONLY (Rule 9) — no source copied, no library linked. `cstore_fdw` (Apache-2.0, an FDW) + `arrow-rs`
//! codecs (Apache-2.0) are the permissive reuse. Scope: append-only analytical (D4) — INSERT/seqscan/aggregate real;
//! UPDATE/DELETE/tuple-lock/parallel/bitmap/sample are typed-`error!` stubs (the Citus base surface).
//!
//! **Unwind boundary (M98 established, `build.rs` discipline):** every callback is `#[pg_guard] extern "C-unwind"`;
//! every unsupported callback `pg_sys::error!(...)` (which is `!`, coerces to any return type) — never `panic!`/
//! `unimplemented!` (which would be a panic-across-C even though pg_guard catches it; the ereport path gives a clean
//! SQLSTATE). A corrupt-on-disk decode becomes a typed `error!`, never a panic.
#![allow(non_snake_case)]

use pgrx::prelude::*;
use std::mem::size_of;
use std::sync::OnceLock;

/// The `theodb_columnar` table_am_handler. Idempotent install (skips if `pg_am` already has it — safe re-`CREATE
/// EXTENSION`). Mirrors the `theodb_ivfflat_amhandler` SQL shape in `am/mod.rs`, but `TYPE TABLE`.
#[pg_extern(sql = "
    CREATE OR REPLACE FUNCTION theodb_columnar_tam_handler(internal) RETURNS table_am_handler
        LANGUAGE c AS '@MODULE_PATHNAME@', '@FUNCTION_NAME@';
    DO $$
    BEGIN
        IF NOT EXISTS (SELECT 1 FROM pg_catalog.pg_am WHERE amname = 'theodb_columnar') THEN
            CREATE ACCESS METHOD theodb_columnar TYPE TABLE HANDLER theodb_columnar_tam_handler;
        END IF;
    END;
    $$;
")]
fn theodb_columnar_tam_handler(_fcinfo: pg_sys::FunctionCallInfo) -> PgBox<pg_sys::TableAmRoutine> {
    // CRITICAL (TableAM contract): `RelationInitTableAccessMethod` stores the returned routine pointer DIRECTLY in
    // `rel->rd_tableam` — unlike index AMs, PG does NOT memcpy it into the relcache context. The routine must
    // therefore outlive the current (transient) memory context, or `rd_tableam` dangles the moment that context
    // resets and the next callback segfaults. So we build it ONCE in `TopMemoryContext` (backend lifetime) and return
    // the SAME stateless routine for every columnar relation — exactly as heapam returns `&heapam_methods`.
    static ROUTINE: OnceLock<usize> = OnceLock::new();
    let ptr = *ROUTINE.get_or_init(|| build_columnar_amroutine_in_top() as usize);
    unsafe { PgBox::from_pg(ptr as *mut pg_sys::TableAmRoutine) }
}

/// Build the `TableAmRoutine` for `theodb_columnar` in `TopMemoryContext` and return the raw pointer (leaked into
/// that lasting context on purpose — the backend keeps exactly one). Every required callback is a non-NULL pointer
/// (else `GetTableAmRoutine`'s asserts crash an assert-build). Phase A: lifecycle + empty-scan are real; the
/// write/read path is stubbed (real in Phase B/C); UPDATE/DELETE/parallel/bitmap/sample are typed-`error!` (D4).
fn build_columnar_amroutine_in_top() -> *mut pg_sys::TableAmRoutine {
    let old = unsafe { pg_sys::MemoryContextSwitchTo(pg_sys::TopMemoryContext) };
    let mut amr = unsafe { PgBox::<pg_sys::TableAmRoutine>::alloc_node(pg_sys::NodeTag::T_TableAmRoutine) };

    // --- slot / scan lifecycle (real; empty scan for Phase A) ---
    amr.slot_callbacks = Some(columnar_slot_callbacks);
    amr.scan_begin = Some(columnar_scan_begin);
    amr.scan_end = Some(columnar_scan_end);
    amr.scan_rescan = Some(columnar_scan_rescan);
    amr.scan_getnextslot = Some(columnar_scan_getnextslot);

    // --- tid-range scan (append-only: not supported) ---
    amr.scan_set_tidrange = Some(columnar_scan_set_tidrange);
    amr.scan_getnextslot_tidrange = Some(columnar_scan_getnextslot_tidrange);

    // --- parallel scan (not supported in M99) ---
    amr.parallelscan_estimate = Some(columnar_parallelscan_estimate);
    amr.parallelscan_initialize = Some(columnar_parallelscan_initialize);
    amr.parallelscan_reinitialize = Some(columnar_parallelscan_reinitialize);

    // --- index fetch (Phase C+ for index-scan; Phase A: not supported) ---
    amr.index_fetch_begin = Some(columnar_index_fetch_begin);
    amr.index_fetch_reset = Some(columnar_index_fetch_reset);
    amr.index_fetch_end = Some(columnar_index_fetch_end);
    amr.index_fetch_tuple = Some(columnar_index_fetch_tuple);

    // --- tuple fetch / visibility ---
    amr.tuple_fetch_row_version = Some(columnar_tuple_fetch_row_version);
    amr.tuple_tid_valid = Some(columnar_tuple_tid_valid);
    amr.tuple_get_latest_tid = Some(columnar_tuple_get_latest_tid);
    amr.tuple_satisfies_snapshot = Some(columnar_tuple_satisfies_snapshot);
    amr.index_delete_tuples = Some(columnar_index_delete_tuples);

    // --- insert / modify (Phase B for real; Phase A: not supported) ---
    amr.tuple_insert = Some(columnar_tuple_insert);
    amr.tuple_insert_speculative = Some(columnar_tuple_insert_speculative);
    amr.tuple_complete_speculative = Some(columnar_tuple_complete_speculative);
    amr.multi_insert = Some(columnar_multi_insert);
    amr.tuple_delete = Some(columnar_tuple_delete);
    amr.tuple_update = Some(columnar_tuple_update);
    amr.tuple_lock = Some(columnar_tuple_lock);
    amr.finish_bulk_insert = Some(columnar_finish_bulk_insert);

    // --- relation lifecycle (real: create storage + metapage) ---
    amr.relation_set_new_filelocator = Some(columnar_relation_set_new_filelocator);
    amr.relation_nontransactional_truncate = Some(columnar_relation_nontransactional_truncate);
    amr.relation_copy_data = Some(columnar_relation_copy_data);
    amr.relation_copy_for_cluster = Some(columnar_relation_copy_for_cluster);
    amr.relation_vacuum = Some(columnar_relation_vacuum);

    // --- analyze (real: empty for Phase A) ---
    amr.scan_analyze_next_block = Some(columnar_scan_analyze_next_block);
    amr.scan_analyze_next_tuple = Some(columnar_scan_analyze_next_tuple);

    // --- index build (Phase C+: not supported in A) ---
    amr.index_build_range_scan = Some(columnar_index_build_range_scan);
    amr.index_validate_scan = Some(columnar_index_validate_scan);

    // --- size / toast (real) ---
    amr.relation_size = Some(columnar_relation_size);
    amr.relation_needs_toast_table = Some(columnar_relation_needs_toast_table);
    amr.relation_toast_am = Some(columnar_relation_toast_am);
    amr.relation_fetch_toast_slice = Some(columnar_relation_fetch_toast_slice);
    amr.relation_estimate_size = Some(columnar_relation_estimate_size);

    // --- bitmap / sample scan (not supported in M99) ---
    amr.scan_bitmap_next_block = Some(columnar_scan_bitmap_next_block);
    amr.scan_bitmap_next_tuple = Some(columnar_scan_bitmap_next_tuple);
    amr.scan_sample_next_block = Some(columnar_scan_sample_next_block);
    amr.scan_sample_next_tuple = Some(columnar_scan_sample_next_tuple);

    // Leak the routine into TopMemoryContext (backend keeps exactly one) and restore the caller's context.
    let raw = amr.into_pg_boxed().into_pg();
    unsafe { pg_sys::MemoryContextSwitchTo(old) };
    raw
}

// ===========================================================================================================
// M99 A2 — columnar metapage (block 0): monotonic row_number / stripe_id reservation counters
// ===========================================================================================================
//
// The metapage lives in block 0 of the relation's MAIN fork as a single fixed-size page item. It holds the two
// monotonic counters the writer reserves from (row_number for synthetic TIDs, stripe_id for the catalog). The
// reservation is a read-modify-write of block 0 under a buffer EXCLUSIVE lock, WAL-logged full-image via GenericXLog
// — so two concurrent inserters can never get overlapping ranges (proven under concurrency in Phase D). This mirrors
// Hydra's `ColumnarStorageReserveRowNumber`/`ReserveStripeId` (studied only — ADR-0042), reusing our own `page.rs`
// GenericXLog discipline (Rule 9).

const META_MAGIC: u32 = 0x54_43_4F_4C; // "TCOL" (Theo COLumnar) — distinguishes a columnar fork from the IVF/HNSW blob.
const META_VERSION: u32 = 1;
const META_LEN: usize = 24; // magic(4) + version(4) + reserved_row_number(8) + reserved_stripe_id(8)

#[derive(Clone, Copy)]
struct ColumnarMeta {
    reserved_row_number: u64,
    reserved_stripe_id: u64,
}

impl ColumnarMeta {
    fn to_bytes(&self) -> [u8; META_LEN] {
        let mut b = [0u8; META_LEN];
        b[0..4].copy_from_slice(&META_MAGIC.to_le_bytes());
        b[4..8].copy_from_slice(&META_VERSION.to_le_bytes());
        b[8..16].copy_from_slice(&self.reserved_row_number.to_le_bytes());
        b[16..24].copy_from_slice(&self.reserved_stripe_id.to_le_bytes());
        b
    }

    /// Decode + validate the metapage bytes. A wrong magic/version is a corrupt or foreign fork → typed error
    /// (fail-fast at the trust boundary, never a silent wrong answer).
    fn from_bytes(b: &[u8]) -> Result<Self, String> {
        if b.len() < META_LEN {
            return Err(format!("theodb_columnar: metapage too short ({} < {META_LEN})", b.len()));
        }
        let magic = u32::from_le_bytes(b[0..4].try_into().unwrap());
        if magic != META_MAGIC {
            return Err(format!("theodb_columnar: bad metapage magic {magic:#x} (expected {META_MAGIC:#x})"));
        }
        let version = u32::from_le_bytes(b[4..8].try_into().unwrap());
        if version != META_VERSION {
            return Err(format!("theodb_columnar: unsupported metapage version {version}"));
        }
        Ok(ColumnarMeta {
            reserved_row_number: u64::from_le_bytes(b[8..16].try_into().unwrap()),
            reserved_stripe_id: u64::from_le_bytes(b[16..24].try_into().unwrap()),
        })
    }
}

/// Initialize the metapage (block 0) with both counters at 0. Called from `relation_set_new_filelocator` right after
/// the storage is created, so block 0 always exists before any reservation. Reuses `page::extend_page_with_item`
/// (WAL-logged extend under the relation-extension lock).
unsafe fn init_metapage(rel: pg_sys::Relation) {
    let meta = ColumnarMeta { reserved_row_number: 0, reserved_stripe_id: 0 };
    unsafe { super::page::extend_page_with_item(rel, pg_sys::ForkNumber::MAIN_FORKNUM, &meta.to_bytes()) };
}

/// Which counter a reservation bumps.
#[derive(Clone, Copy)]
enum Counter {
    RowNumber,
    StripeId,
}

/// Atomically reserve `n` ids from a metapage counter and return the FIRST reserved id (the range is `[base, base+n)`).
/// Read-modify-write of block 0 under a buffer EXCLUSIVE lock + GenericXLog full-image — the range is durable and
/// non-overlapping across concurrent backends.
unsafe fn reserve(rel: pg_sys::Relation, counter: Counter, n: u64) -> Result<u64, String> {
    let buf = pg_sys::ReadBufferExtended(
        rel,
        pg_sys::ForkNumber::MAIN_FORKNUM,
        0,
        pg_sys::ReadBufferMode::RBM_NORMAL,
        std::ptr::null_mut(),
    );
    pg_sys::LockBuffer(buf, pg_sys::BUFFER_LOCK_EXCLUSIVE as i32);
    let state = pg_sys::GenericXLogStart(rel);
    let page = pg_sys::GenericXLogRegisterBuffer(state, buf, pg_sys::GENERIC_XLOG_FULL_IMAGE as i32);

    // The metapage is the single item at FirstOffsetNumber.
    let itemid = pg_sys::PageGetItemId(page, pg_sys::FirstOffsetNumber);
    let item = pg_sys::PageGetItem(page, itemid) as *mut u8;
    let cur = std::slice::from_raw_parts(item, META_LEN);
    let mut meta = match ColumnarMeta::from_bytes(cur) {
        Ok(m) => m,
        Err(e) => {
            // Abort the xlog + release before failing loud (no half-written WAL record).
            pg_sys::GenericXLogAbort(state);
            pg_sys::UnlockReleaseBuffer(buf);
            return Err(e);
        }
    };

    let field = match counter {
        Counter::RowNumber => &mut meta.reserved_row_number,
        Counter::StripeId => &mut meta.reserved_stripe_id,
    };
    let base = *field;
    *field = base + n;

    // Overwrite the item in place (same length) — no PageAddItem churn.
    let bytes = meta.to_bytes();
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), item, META_LEN);

    pg_sys::MarkBufferDirty(buf);
    pg_sys::GenericXLogFinish(state);
    pg_sys::UnlockReleaseBuffer(buf);
    Ok(base)
}

// ===========================================================================================================
// Real: slot + scan lifecycle (Phase A — empty scan)
// ===========================================================================================================

/// Columnar returns virtual tuples (materialized from decompressed column chunks) — the same slot type Hydra uses.
#[pg_guard]
pub unsafe extern "C-unwind" fn columnar_slot_callbacks(
    _rel: pg_sys::Relation,
) -> *const pg_sys::TupleTableSlotOps {
    &raw const pg_sys::TTSOpsVirtual
}

/// Begin a scan: allocate a minimal `TableScanDescData`, record the relation + snapshot. Phase A returns no rows;
/// the real column-chunk reader is Phase C.
#[pg_guard]
pub unsafe extern "C-unwind" fn columnar_scan_begin(
    rel: pg_sys::Relation,
    snapshot: pg_sys::Snapshot,
    nkeys: std::os::raw::c_int,
    key: *mut pg_sys::ScanKeyData,
    pscan: pg_sys::ParallelTableScanDesc,
    flags: u32,
) -> pg_sys::TableScanDesc {
    unsafe {
        let scan = pg_sys::palloc0(size_of::<pg_sys::TableScanDescData>()) as *mut pg_sys::TableScanDescData;
        (*scan).rs_rd = rel;
        (*scan).rs_snapshot = snapshot;
        (*scan).rs_nkeys = nkeys;
        (*scan).rs_key = key;
        (*scan).rs_parallel = pscan;
        (*scan).rs_flags = flags;
        scan as pg_sys::TableScanDesc
    }
}

#[pg_guard]
pub unsafe extern "C-unwind" fn columnar_scan_end(scan: pg_sys::TableScanDesc) {
    unsafe {
        if !scan.is_null() {
            pg_sys::pfree(scan as *mut std::os::raw::c_void);
        }
    }
}

#[pg_guard]
pub unsafe extern "C-unwind" fn columnar_scan_rescan(
    _scan: pg_sys::TableScanDesc,
    _key: *mut pg_sys::ScanKeyData,
    _set_params: bool,
    _allow_strat: bool,
    _allow_sync: bool,
    _allow_pagemode: bool,
) {
    // Phase A: empty scan has no cursor state to reset.
}

/// Phase A: no rows yet (INSERT is Phase B, the real chunk reader is Phase C). Returning false = end of scan.
#[pg_guard]
pub unsafe extern "C-unwind" fn columnar_scan_getnextslot(
    _scan: pg_sys::TableScanDesc,
    _direction: pg_sys::ScanDirection::Type,
    _slot: *mut pg_sys::TupleTableSlot,
) -> bool {
    false
}

// ===========================================================================================================
// Real: relation lifecycle (create storage + metapage init)
// ===========================================================================================================

/// Create the relation's physical storage (main fork) + initialize the columnar metapage (block 0) so `CREATE
/// TABLE` succeeds, `relation_size` has a fork to measure, and the reservation counters exist before the first
/// INSERT (M99 A2). The stripe/chunk catalog rows are inserted by the writer (Phase B).
#[pg_guard]
pub unsafe extern "C-unwind" fn columnar_relation_set_new_filelocator(
    rel: pg_sys::Relation,
    newrlocator: *const pg_sys::RelFileLocator,
    persistence: std::os::raw::c_char,
    freezeXid: *mut pg_sys::TransactionId,
    minmulti: *mut pg_sys::MultiXactId,
) {
    unsafe {
        // Set the relation's frozen-xid horizon exactly as heapam does. Even though columnar delegates row
        // visibility to the catalog, pg_class.relfrozenxid/relminmxid MUST be valid xids — an Invalid (0)
        // frozenxid corrupts the vacuum/wraparound bookkeeping and crashes downstream. (heapam_handler.c:
        // heapam_relation_set_new_filelocator.)
        *freezeXid = pg_sys::RecentXmin;
        *minmulti = pg_sys::GetOldestMultiXactId();

        // UNLOGGED columnar is out of M99 scope (the init-fork reset-on-crash machinery is a later bet). Permanent
        // and temp relations use the normal create path.
        if persistence == pg_sys::RELPERSISTENCE_UNLOGGED as std::os::raw::c_char {
            pg_sys::error!("theodb_columnar: UNLOGGED tables are not supported in M99");
        }

        // Create the main-fork storage (WAL-logged for a permanent relation). register_delete=true so an aborted
        // CREATE cleans up the file.
        let srel = pg_sys::RelationCreateStorage(*newrlocator, persistence, true);
        pg_sys::smgrclose(srel);

        // Initialize the metapage (block 0) with both reservation counters at 0 (M99 A2), so block 0 always exists
        // before the first INSERT reserves from it.
        init_metapage(rel);
    }
}

// ===========================================================================================================
// Real: size / toast / analyze (Phase A — empty)
// ===========================================================================================================

#[pg_guard]
pub unsafe extern "C-unwind" fn columnar_relation_size(
    rel: pg_sys::Relation,
    forkNumber: pg_sys::ForkNumber::Type,
) -> u64 {
    unsafe {
        let smgr = pg_sys::RelationGetSmgr(rel);
        if pg_sys::smgrexists(smgr, forkNumber) {
            (pg_sys::smgrnblocks(smgr, forkNumber) as u64) * (pg_sys::BLCKSZ as u64)
        } else {
            0
        }
    }
}

#[pg_guard]
pub unsafe extern "C-unwind" fn columnar_relation_needs_toast_table(_rel: pg_sys::Relation) -> bool {
    // M99: values are stored inline in column chunks (no out-of-line TOAST). Matches the Citus base surface.
    false
}

#[pg_guard]
pub unsafe extern "C-unwind" fn columnar_relation_estimate_size(
    rel: pg_sys::Relation,
    _attr_widths: *mut i32,
    pages: *mut pg_sys::BlockNumber,
    tuples: *mut f64,
    allvisfrac: *mut f64,
) {
    unsafe {
        let smgr = pg_sys::RelationGetSmgr(rel);
        let nblocks = if pg_sys::smgrexists(smgr, pg_sys::ForkNumber::MAIN_FORKNUM) {
            pg_sys::smgrnblocks(smgr, pg_sys::ForkNumber::MAIN_FORKNUM)
        } else {
            0
        };
        *pages = nblocks;
        // Phase A: no catalog row-count yet; a real estimate reads columnar.stripe (Phase C ANALYZE).
        *tuples = 0.0;
        *allvisfrac = 0.0;
    }
}

/// Analyze: Phase A has no rows to sample. Return false (no more blocks / tuples).
#[pg_guard]
pub unsafe extern "C-unwind" fn columnar_scan_analyze_next_block(
    _scan: pg_sys::TableScanDesc,
    _stream: *mut pg_sys::ReadStream,
) -> bool {
    false
}

#[pg_guard]
pub unsafe extern "C-unwind" fn columnar_scan_analyze_next_tuple(
    _scan: pg_sys::TableScanDesc,
    _OldestXmin: pg_sys::TransactionId,
    _liverows: *mut f64,
    _deadrows: *mut f64,
    _slot: *mut pg_sys::TupleTableSlot,
) -> bool {
    false
}

// ===========================================================================================================
// Typed-error stubs (D4 — append-only; unsupported callbacks fail loud with a clean SQLSTATE, never panic)
// ===========================================================================================================

// NOTE: these stubs are intentionally NOT `#[pg_guard]`. They only call `pg_sys::error!` — a C `ereport`/siglongjmp
// with a `&'static str` message (no allocation, no Rust code that can panic), so there is no Rust unwind to guard
// against crossing the C boundary. (Combining a `macro_rules!`-generated signature with the `#[pg_guard]` proc-macro
// also breaks hygiene — pg_guard cannot see the macro-generated parameter idents. The real callbacks above ARE
// `#[pg_guard]` because they run real Rust that could panic.)
macro_rules! columnar_unsupported {
    ($name:ident ( $($arg:ident : $ty:ty),* $(,)? ) $( -> $ret:ty )? , $msg:literal) => {
        pub unsafe extern "C-unwind" fn $name( $( $arg : $ty ),* ) $( -> $ret )? {
            let _ = ( $( &$arg ),* );
            pg_sys::error!(concat!("theodb_columnar: ", $msg, " is not supported (M99 is append-only analytical)"));
        }
    };
}

columnar_unsupported!(columnar_scan_set_tidrange(_s: pg_sys::TableScanDesc, _mn: pg_sys::ItemPointer, _mx: pg_sys::ItemPointer), "TID-range scan");
columnar_unsupported!(columnar_scan_getnextslot_tidrange(_s: pg_sys::TableScanDesc, _d: pg_sys::ScanDirection::Type, _sl: *mut pg_sys::TupleTableSlot) -> bool, "TID-range scan");
columnar_unsupported!(columnar_parallelscan_estimate(_r: pg_sys::Relation) -> pg_sys::Size, "parallel scan");
columnar_unsupported!(columnar_parallelscan_initialize(_r: pg_sys::Relation, _p: pg_sys::ParallelTableScanDesc) -> pg_sys::Size, "parallel scan");
columnar_unsupported!(columnar_parallelscan_reinitialize(_r: pg_sys::Relation, _p: pg_sys::ParallelTableScanDesc), "parallel scan");
columnar_unsupported!(columnar_index_fetch_begin(_r: pg_sys::Relation) -> *mut pg_sys::IndexFetchTableData, "index fetch");
columnar_unsupported!(columnar_index_fetch_reset(_d: *mut pg_sys::IndexFetchTableData), "index fetch");
columnar_unsupported!(columnar_index_fetch_end(_d: *mut pg_sys::IndexFetchTableData), "index fetch");
columnar_unsupported!(columnar_index_fetch_tuple(_s: *mut pg_sys::IndexFetchTableData, _t: pg_sys::ItemPointer, _sn: pg_sys::Snapshot, _sl: *mut pg_sys::TupleTableSlot, _ca: *mut bool, _ad: *mut bool) -> bool, "index fetch");
columnar_unsupported!(columnar_tuple_fetch_row_version(_r: pg_sys::Relation, _t: pg_sys::ItemPointer, _sn: pg_sys::Snapshot, _sl: *mut pg_sys::TupleTableSlot) -> bool, "tuple fetch by TID");
columnar_unsupported!(columnar_tuple_tid_valid(_s: pg_sys::TableScanDesc, _t: pg_sys::ItemPointer) -> bool, "tuple TID validity");
columnar_unsupported!(columnar_tuple_get_latest_tid(_s: pg_sys::TableScanDesc, _t: pg_sys::ItemPointer), "latest-TID lookup");
columnar_unsupported!(columnar_tuple_satisfies_snapshot(_r: pg_sys::Relation, _sl: *mut pg_sys::TupleTableSlot, _sn: pg_sys::Snapshot) -> bool, "tuple visibility by TID");
columnar_unsupported!(columnar_index_delete_tuples(_r: pg_sys::Relation, _d: *mut pg_sys::TM_IndexDeleteOp) -> pg_sys::TransactionId, "index delete");
columnar_unsupported!(columnar_tuple_insert(_r: pg_sys::Relation, _sl: *mut pg_sys::TupleTableSlot, _c: pg_sys::CommandId, _o: std::os::raw::c_int, _b: *mut pg_sys::BulkInsertStateData), "INSERT (wired in Phase B)");
columnar_unsupported!(columnar_tuple_insert_speculative(_r: pg_sys::Relation, _sl: *mut pg_sys::TupleTableSlot, _c: pg_sys::CommandId, _o: std::os::raw::c_int, _b: *mut pg_sys::BulkInsertStateData, _t: u32), "speculative insert");
columnar_unsupported!(columnar_tuple_complete_speculative(_r: pg_sys::Relation, _sl: *mut pg_sys::TupleTableSlot, _t: u32, _s: bool), "speculative insert");
columnar_unsupported!(columnar_multi_insert(_r: pg_sys::Relation, _sl: *mut *mut pg_sys::TupleTableSlot, _n: std::os::raw::c_int, _c: pg_sys::CommandId, _o: std::os::raw::c_int, _b: *mut pg_sys::BulkInsertStateData), "COPY multi-insert (wired in Phase B)");
columnar_unsupported!(columnar_tuple_delete(_r: pg_sys::Relation, _t: pg_sys::ItemPointer, _c: pg_sys::CommandId, _sn: pg_sys::Snapshot, _cr: pg_sys::Snapshot, _w: bool, _f: *mut pg_sys::TM_FailureData, _cp: bool) -> pg_sys::TM_Result::Type, "DELETE");
columnar_unsupported!(columnar_tuple_update(_r: pg_sys::Relation, _o: pg_sys::ItemPointer, _sl: *mut pg_sys::TupleTableSlot, _c: pg_sys::CommandId, _sn: pg_sys::Snapshot, _cr: pg_sys::Snapshot, _w: bool, _f: *mut pg_sys::TM_FailureData, _lm: *mut pg_sys::LockTupleMode::Type, _ui: *mut pg_sys::TU_UpdateIndexes::Type) -> pg_sys::TM_Result::Type, "UPDATE");
columnar_unsupported!(columnar_tuple_lock(_r: pg_sys::Relation, _t: pg_sys::ItemPointer, _sn: pg_sys::Snapshot, _sl: *mut pg_sys::TupleTableSlot, _c: pg_sys::CommandId, _m: pg_sys::LockTupleMode::Type, _wp: pg_sys::LockWaitPolicy::Type, _fl: u8, _f: *mut pg_sys::TM_FailureData) -> pg_sys::TM_Result::Type, "SELECT FOR UPDATE / row lock");
columnar_unsupported!(columnar_finish_bulk_insert(_r: pg_sys::Relation, _o: std::os::raw::c_int), "finish bulk insert (wired in Phase B)");
columnar_unsupported!(columnar_relation_nontransactional_truncate(_r: pg_sys::Relation), "TRUNCATE (wired in Phase B)");
columnar_unsupported!(columnar_relation_copy_data(_r: pg_sys::Relation, _n: *const pg_sys::RelFileLocator), "ALTER TABLE SET TABLESPACE");
columnar_unsupported!(columnar_relation_copy_for_cluster(_ot: pg_sys::Relation, _nt: pg_sys::Relation, _oi: pg_sys::Relation, _us: bool, _ox: pg_sys::TransactionId, _xc: *mut pg_sys::TransactionId, _mc: *mut pg_sys::MultiXactId, _nt2: *mut f64, _tv: *mut f64, _trd: *mut f64), "CLUSTER / VACUUM FULL");
columnar_unsupported!(columnar_relation_vacuum(_r: pg_sys::Relation, _p: *mut pg_sys::VacuumParams, _b: pg_sys::BufferAccessStrategy), "VACUUM (wired in a later phase)");
columnar_unsupported!(columnar_index_build_range_scan(_tr: pg_sys::Relation, _ir: pg_sys::Relation, _ii: *mut pg_sys::IndexInfo, _as: bool, _av: bool, _pr: bool, _sb: pg_sys::BlockNumber, _nb: pg_sys::BlockNumber, _cb: pg_sys::IndexBuildCallback, _cs: *mut std::os::raw::c_void, _s: pg_sys::TableScanDesc) -> f64, "index build over columnar (wired in a later phase)");
columnar_unsupported!(columnar_index_validate_scan(_tr: pg_sys::Relation, _ir: pg_sys::Relation, _ii: *mut pg_sys::IndexInfo, _sn: pg_sys::Snapshot, _st: *mut pg_sys::ValidateIndexState), "concurrent index validate");
columnar_unsupported!(columnar_relation_toast_am(_r: pg_sys::Relation) -> pg_sys::Oid, "TOAST (columnar stores inline)");
columnar_unsupported!(columnar_relation_fetch_toast_slice(_tr: pg_sys::Relation, _v: pg_sys::Oid, _as: i32, _so: i32, _sl: i32, _res: *mut pg_sys::varlena), "TOAST slice fetch");
columnar_unsupported!(columnar_scan_bitmap_next_block(_s: pg_sys::TableScanDesc, _t: *mut pg_sys::TBMIterateResult) -> bool, "bitmap heap scan");
columnar_unsupported!(columnar_scan_bitmap_next_tuple(_s: pg_sys::TableScanDesc, _t: *mut pg_sys::TBMIterateResult, _sl: *mut pg_sys::TupleTableSlot) -> bool, "bitmap heap scan");
columnar_unsupported!(columnar_scan_sample_next_block(_s: pg_sys::TableScanDesc, _ss: *mut pg_sys::SampleScanState) -> bool, "TABLESAMPLE");
columnar_unsupported!(columnar_scan_sample_next_tuple(_s: pg_sys::TableScanDesc, _ss: *mut pg_sys::SampleScanState, _sl: *mut pg_sys::TupleTableSlot) -> bool, "TABLESAMPLE");

/// M99 A2 test helper — reserve `n` row_numbers from a columnar table's metapage and return the FIRST reserved id.
/// Test-only (gated behind `pg_test`) so the monotonicity test can drive the reservation RMW from SQL.
#[cfg(any(test, feature = "pg_test"))]
#[pg_extern]
fn theodb_columnar_test_reserve_rows(rel_oid: pg_sys::Oid, n: i64) -> i64 {
    unsafe {
        let rel = pg_sys::relation_open(rel_oid, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
        let base = reserve(rel, Counter::RowNumber, n as u64);
        pg_sys::relation_close(rel, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
        match base {
            Ok(b) => b as i64,
            Err(e) => pgrx::error!("{e}"),
        }
    }
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    /// M99 A2 — the metapage reservation counter is monotonic + gap-free (row_number → synthetic TID uniqueness).
    /// 1000 single reservations return 0,1,…,999; a batch reserve of 5 returns 1000 and advances the counter by 5.
    /// Each reservation reads back the value the previous one wrote (a round-trip through block 0 under the buffer
    /// lock), so this proves the RMW is correct within a session; cross-backend non-overlap + crash-durability are
    /// proven in Phase D (isolation permutations + WAL replay).
    #[pg_test]
    fn m99_reserve_row_number_monotonic() {
        Spi::run("CREATE TABLE m99_rt (a int) USING theodb_columnar").unwrap();
        let oid = Spi::get_one::<pg_sys::Oid>("SELECT 'm99_rt'::regclass::oid").unwrap().unwrap();
        for i in 0..1000i64 {
            let r = Spi::get_one_with_args::<i64>(
                "SELECT theodb_columnar_test_reserve_rows($1, 1)",
                &[oid.into()],
            )
            .unwrap()
            .unwrap();
            assert_eq!(r, i, "reservation #{i} must return {i} (monotonic, gap-free)");
        }
        let base = Spi::get_one_with_args::<i64>(
            "SELECT theodb_columnar_test_reserve_rows($1, 5)",
            &[oid.into()],
        )
        .unwrap()
        .unwrap();
        assert_eq!(base, 1000, "batch reserve base after 1000 singles must be 1000");
        let next = Spi::get_one_with_args::<i64>(
            "SELECT theodb_columnar_test_reserve_rows($1, 1)",
            &[oid.into()],
        )
        .unwrap()
        .unwrap();
        assert_eq!(next, 1005, "after reserving a batch of 5, the next single must be 1005");
        Spi::run("DROP TABLE m99_rt").unwrap();
    }

    /// M99 A1 — `CREATE TABLE ... USING theodb_columnar` loads end-to-end and registers in `pg_am`, and an empty
    /// seqscan returns zero rows (INSERT is Phase B). Proves the TableAM FFI/registration path on pgrx 0.19 / pg17.
    #[pg_test]
    fn m99_columnar_am_creates_table() {
        // The AM is registered by the extension_sql on the handler.
        let am_exists = Spi::get_one::<bool>(
            "SELECT EXISTS (SELECT 1 FROM pg_catalog.pg_am WHERE amname = 'theodb_columnar' AND amtype = 't')",
        )
        .unwrap()
        .unwrap();
        assert!(am_exists, "theodb_columnar table AM must be registered in pg_am");

        Spi::run("CREATE TABLE m99_ct (a int, b text) USING theodb_columnar").unwrap();
        let relam_ok = Spi::get_one::<bool>(
            "SELECT am.amname = 'theodb_columnar' FROM pg_class c JOIN pg_am am ON am.oid = c.relam \
             WHERE c.relname = 'm99_ct'",
        )
        .unwrap()
        .unwrap();
        assert!(relam_ok, "m99_ct must be stored with the theodb_columnar AM");

        // Empty seqscan returns zero rows (Phase A: no write path yet).
        let n = Spi::get_one::<i64>("SELECT count(*) FROM m99_ct").unwrap().unwrap();
        assert_eq!(n, 0, "an empty columnar table must scan zero rows in Phase A");

        Spi::run("DROP TABLE m99_ct").unwrap();
    }
}
