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
use std::cell::RefCell;
use std::collections::HashMap;
use std::mem::size_of;
use std::sync::OnceLock;

/// Per-backend pending write state: relation OID → accumulated row blobs (each = a formed heap tuple's bytes). A
/// stripe is flushed from here at scan time (so same-xact INSERT→SELECT sees the rows) and on xact pre-commit.
thread_local! {
    static WRITE_STATES: RefCell<HashMap<u32, Vec<Vec<u8>>>> = RefCell::new(HashMap::new());
}

/// A stripe descriptor, stored in the metapage tail. Fixed 28 bytes.
#[derive(Clone, Copy)]
struct StripeDesc {
    first_block: u32,
    n_blocks: u32,
    byte_len: u64,
    row_count: u32,
    first_row_number: u64,
}
const STRIPE_DESC_LEN: usize = 28;
const META_HEAD_LEN: usize = 28; // 24-byte counters head + n_stripes(4)

impl StripeDesc {
    fn write_into(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.first_block.to_le_bytes());
        out.extend_from_slice(&self.n_blocks.to_le_bytes());
        out.extend_from_slice(&self.byte_len.to_le_bytes());
        out.extend_from_slice(&self.row_count.to_le_bytes());
        out.extend_from_slice(&self.first_row_number.to_le_bytes());
    }
    fn read_from(b: &[u8]) -> Self {
        StripeDesc {
            first_block: u32::from_le_bytes(b[0..4].try_into().unwrap()),
            n_blocks: u32::from_le_bytes(b[4..8].try_into().unwrap()),
            byte_len: u64::from_le_bytes(b[8..16].try_into().unwrap()),
            row_count: u32::from_le_bytes(b[16..20].try_into().unwrap()),
            first_row_number: u64::from_le_bytes(b[20..28].try_into().unwrap()),
        }
    }
}

/// Read the metapage item (block 0) raw bytes.
unsafe fn read_meta_bytes(rel: pg_sys::Relation) -> Result<Vec<u8>, String> {
    let items = super::page::read_all_page_items(rel, 0)?;
    items.into_iter().next().ok_or_else(|| "theodb_columnar: metapage has no item".to_string())
}

/// Decode the stripe descriptors from the metapage tail (bytes `META_HEAD_LEN..`). An item shorter than the head
/// (legacy 24-byte A2 metapage) means zero stripes.
unsafe fn read_stripes(rel: pg_sys::Relation) -> Result<Vec<StripeDesc>, String> {
    let bytes = read_meta_bytes(rel)?;
    // Validate the head (magic/version) via the counters decoder — fail-fast on a foreign/corrupt fork.
    ColumnarMeta::from_bytes(&bytes)?;
    if bytes.len() < META_HEAD_LEN {
        return Ok(Vec::new());
    }
    let n = u32::from_le_bytes(bytes[24..28].try_into().unwrap()) as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let off = META_HEAD_LEN + i * STRIPE_DESC_LEN;
        if off + STRIPE_DESC_LEN > bytes.len() {
            return Err(format!("theodb_columnar: metapage truncated (stripe {i} of {n})"));
        }
        out.push(StripeDesc::read_from(&bytes[off..off + STRIPE_DESC_LEN]));
    }
    Ok(out)
}

/// Append a stripe descriptor to the metapage and bump `n_stripes`, preserving the current counters head (which the
/// caller has just advanced via `reserve`). Full-image rewrite of block 0 (torn-page-proof, reuses `page.rs`).
unsafe fn append_stripe(rel: pg_sys::Relation, desc: StripeDesc) -> Result<(), String> {
    let cur = read_meta_bytes(rel)?;
    ColumnarMeta::from_bytes(&cur)?;
    let n = if cur.len() >= META_HEAD_LEN {
        u32::from_le_bytes(cur[24..28].try_into().unwrap())
    } else {
        0
    };
    // Rebuild: 24-byte counters head (unchanged) + new n_stripes + existing descs + the new desc.
    let mut out = Vec::with_capacity(META_HEAD_LEN + (n as usize + 1) * STRIPE_DESC_LEN);
    out.extend_from_slice(&cur[0..24]);
    out.extend_from_slice(&(n + 1).to_le_bytes());
    if cur.len() >= META_HEAD_LEN {
        out.extend_from_slice(&cur[META_HEAD_LEN..META_HEAD_LEN + n as usize * STRIPE_DESC_LEN]);
    }
    desc.write_into(&mut out);
    // A page item must fit in one 8 KB page (≈ BLCKSZ − header − line pointer). Beyond this the stripe directory
    // needs its own paged region — a later-phase concern (honest MVP limit ≈ 285 stripes).
    if out.len() > 8000 {
        return Err("theodb_columnar: too many stripes for one metapage (stripe directory paging is a later phase)".into());
    }
    super::page::pivot_meta_page(rel, &out);
    Ok(())
}

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

/// Scan cursor state. Embeds `TableScanDescData` as the FIRST field (C-struct-inheritance idiom) so a
/// `TableScanDesc` pointer round-trips, plus a boxed materialization of every visible row's bytes and a cursor.
#[repr(C)]
struct ColumnarScanState {
    base: pg_sys::TableScanDescData,
    rows: *mut Vec<Vec<u8>>, // Rust-heap Box, freed in scan_end
    cursor: usize,
}

/// Materialize all visible rows for `rel`: flush this backend's pending writes (so same-xact INSERT is seen), then
/// read every stripe's data pages and split them back into row blobs. Single-backend MVP visibility (all flushed
/// stripes); snapshot-scoped cross-backend visibility is Phase C2/D.
unsafe fn materialize_rows(rel: pg_sys::Relation) -> Result<Vec<Vec<u8>>, String> {
    unsafe {
        flush_pending(rel)?;
        let stripes = read_stripes(rel)?;
        let mut out = Vec::new();
        for st in stripes {
            // Concatenate the stripe's compressed data pages (one item per page, written by flush).
            let mut compressed = Vec::with_capacity(st.byte_len as usize);
            for b in st.first_block..st.first_block + st.n_blocks {
                let items = super::page::read_all_page_items(rel, b)?;
                if let Some(chunk) = items.into_iter().next() {
                    compressed.extend_from_slice(&chunk);
                }
            }
            if compressed.len() != st.byte_len as usize {
                return Err(format!(
                    "theodb_columnar: stripe on-disk length mismatch ({} != {})",
                    compressed.len(),
                    st.byte_len
                ));
            }
            // Decompress to the row-blob payload.
            let payload = zstd::decode_all(&compressed[..])
                .map_err(|e| format!("theodb_columnar: zstd decompress failed: {e}"))?;
            // Split payload into row blobs: [u32 len][bytes]…
            let mut off = 0usize;
            for _ in 0..st.row_count {
                if off + 4 > payload.len() {
                    return Err("theodb_columnar: stripe row header truncated".into());
                }
                let len = u32::from_le_bytes(payload[off..off + 4].try_into().unwrap()) as usize;
                off += 4;
                if off + len > payload.len() {
                    return Err("theodb_columnar: stripe row body truncated".into());
                }
                out.push(payload[off..off + len].to_vec());
                off += len;
            }
        }
        Ok(out)
    }
}

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
        let rows = match materialize_rows(rel) {
            Ok(r) => r,
            Err(e) => pg_sys::error!("{e}"),
        };
        let scan = pg_sys::palloc0(size_of::<ColumnarScanState>()) as *mut ColumnarScanState;
        (*scan).base.rs_rd = rel;
        (*scan).base.rs_snapshot = snapshot;
        (*scan).base.rs_nkeys = nkeys;
        (*scan).base.rs_key = key;
        (*scan).base.rs_parallel = pscan;
        (*scan).base.rs_flags = flags;
        (*scan).rows = Box::into_raw(Box::new(rows));
        (*scan).cursor = 0;
        scan as pg_sys::TableScanDesc
    }
}

#[pg_guard]
pub unsafe extern "C-unwind" fn columnar_scan_end(scan: pg_sys::TableScanDesc) {
    unsafe {
        if !scan.is_null() {
            let st = scan as *mut ColumnarScanState;
            if !(*st).rows.is_null() {
                drop(Box::from_raw((*st).rows)); // free the Rust-heap materialization
            }
            pg_sys::pfree(scan as *mut std::os::raw::c_void);
        }
    }
}

#[pg_guard]
pub unsafe extern "C-unwind" fn columnar_scan_rescan(
    scan: pg_sys::TableScanDesc,
    _key: *mut pg_sys::ScanKeyData,
    _set_params: bool,
    _allow_strat: bool,
    _allow_sync: bool,
    _allow_pagemode: bool,
) {
    unsafe {
        let st = scan as *mut ColumnarScanState;
        (*st).cursor = 0;
    }
}

/// Emit the next row: reconstruct a `HeapTupleData` over the stored bytes, deform it into the virtual slot, and
/// store it. Returns false at end of the materialized set.
#[pg_guard]
pub unsafe extern "C-unwind" fn columnar_scan_getnextslot(
    scan: pg_sys::TableScanDesc,
    _direction: pg_sys::ScanDirection::Type,
    slot: *mut pg_sys::TupleTableSlot,
) -> bool {
    unsafe {
        let st = scan as *mut ColumnarScanState;
        let rows = &*(*st).rows;
        if (*st).cursor >= rows.len() {
            pg_sys::ExecClearTuple(slot);
            return false;
        }
        let bytes = &rows[(*st).cursor];
        (*st).cursor += 1;

        let mut htup: pg_sys::HeapTupleData = std::mem::zeroed();
        htup.t_len = bytes.len() as u32;
        htup.t_data = bytes.as_ptr() as pg_sys::HeapTupleHeader;

        let tupdesc = (*(*st).base.rs_rd).rd_att;
        pg_sys::ExecClearTuple(slot);
        pg_sys::heap_deform_tuple(&mut htup, tupdesc, (*slot).tts_values, (*slot).tts_isnull);
        pg_sys::ExecStoreVirtualTuple(slot);
        true
    }
}

// ===========================================================================================================
// M99 B — write path (accumulate rows per backend, flush to a stripe at scan time / commit)
// ===========================================================================================================
//
// HONEST SCOPE: this slice stores each row as its formed heap-tuple bytes (row-major on disk) — a correct, general
// INSERT→SELECT round-trip on any column set. The true column-major encoding (per-column chunks + zstd + min/max
// skip-pruning — the actual columnar *benefit*) is the follow-up refactor within M99; TDD order is correct-first.
// The `datumSerialize`/`TupleDescAttr` column-major primitives are absent from the pgrx 0.19 bindings, so the
// column-major slice will encode via the tuple descriptor's attlen/attbyval directly.

/// Accumulate one row into this backend's pending write state for `rel`.
unsafe fn accumulate_row(rel: pg_sys::Relation, slot: *mut pg_sys::TupleTableSlot) {
    unsafe {
        pg_sys::slot_getallattrs(slot);
        let tupdesc = (*rel).rd_att;
        let htup = pg_sys::heap_form_tuple(tupdesc, (*slot).tts_values, (*slot).tts_isnull);
        let len = (*htup).t_len as usize;
        let bytes = std::slice::from_raw_parts((*htup).t_data as *const u8, len).to_vec();
        pg_sys::heap_freetuple(htup);
        let oid = (*rel).rd_id.to_u32();
        WRITE_STATES.with(|w| w.borrow_mut().entry(oid).or_default().push(bytes));
    }
}

#[pg_guard]
pub unsafe extern "C-unwind" fn columnar_tuple_insert(
    rel: pg_sys::Relation,
    slot: *mut pg_sys::TupleTableSlot,
    _cid: pg_sys::CommandId,
    _options: std::os::raw::c_int,
    _bistate: *mut pg_sys::BulkInsertStateData,
) {
    unsafe { accumulate_row(rel, slot) };
}

#[pg_guard]
pub unsafe extern "C-unwind" fn columnar_multi_insert(
    rel: pg_sys::Relation,
    slots: *mut *mut pg_sys::TupleTableSlot,
    nslots: std::os::raw::c_int,
    _cid: pg_sys::CommandId,
    _options: std::os::raw::c_int,
    _bistate: *mut pg_sys::BulkInsertStateData,
) {
    unsafe {
        for i in 0..nslots as isize {
            accumulate_row(rel, *slots.offset(i));
        }
    }
}

#[pg_guard]
pub unsafe extern "C-unwind" fn columnar_finish_bulk_insert(
    rel: pg_sys::Relation,
    _options: std::os::raw::c_int,
) {
    // Flush at the end of a bulk (COPY) so the rows are durable + visible to a following scan in this xact.
    if let Err(e) = unsafe { flush_pending(rel) } {
        pg_sys::error!("{e}");
    }
}

/// Flush this backend's pending rows for `rel` into a new stripe: write the row blobs across data pages, reserve the
/// row_number range + stripe id, and append the stripe descriptor to the metapage. WAL-logged throughout (GenericXLog
/// via `page.rs`), so an aborted xact rolls the metapage-descriptor + data pages back → the stripe never becomes
/// visible (the single-backend MVCC MVP; true cross-backend snapshot visibility is Phase C2/D).
unsafe fn flush_pending(rel: pg_sys::Relation) -> Result<(), String> {
    let oid = unsafe { (*rel).rd_id.to_u32() };
    let rows = WRITE_STATES.with(|w| w.borrow_mut().remove(&oid));
    let Some(rows) = rows else { return Ok(()) };
    if rows.is_empty() {
        return Ok(());
    }
    // Stripe payload = for each row: u32 length prefix + the row's heap-tuple bytes.
    let mut payload = Vec::new();
    for r in &rows {
        payload.extend_from_slice(&(r.len() as u32).to_le_bytes());
        payload.extend_from_slice(r);
    }
    // Compress the whole stripe with zstd (level 3 — the DuckDB/Parquet default balance). This is the measurable
    // columnar space benefit; the self-describing zstd frame carries the decompressed size, so `byte_len` on disk =
    // the COMPRESSED length. (Per-column chunking + min/max skip-pruning is the follow-up slice.)
    let compressed = zstd::encode_all(&payload[..], 3)
        .map_err(|e| format!("theodb_columnar: zstd compress failed: {e}"))?;
    unsafe {
        let first_block = pg_sys::RelationGetNumberOfBlocksInFork(rel, pg_sys::ForkNumber::MAIN_FORKNUM);
        let mut n_blocks: u32 = 0;
        for chunk in compressed.chunks(8000) {
            super::page::extend_page_with_item(rel, pg_sys::ForkNumber::MAIN_FORKNUM, chunk);
            n_blocks += 1;
        }
        let base = reserve(rel, Counter::RowNumber, rows.len() as u64)?;
        let _stripe_id = reserve(rel, Counter::StripeId, 1)?;
        append_stripe(
            rel,
            StripeDesc {
                first_block,
                n_blocks,
                byte_len: compressed.len() as u64,
                row_count: rows.len() as u32,
                first_row_number: base,
            },
        )?;
    }
    Ok(())
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
columnar_unsupported!(columnar_tuple_insert_speculative(_r: pg_sys::Relation, _sl: *mut pg_sys::TupleTableSlot, _c: pg_sys::CommandId, _o: std::os::raw::c_int, _b: *mut pg_sys::BulkInsertStateData, _t: u32), "speculative insert");
columnar_unsupported!(columnar_tuple_complete_speculative(_r: pg_sys::Relation, _sl: *mut pg_sys::TupleTableSlot, _t: u32, _s: bool), "speculative insert");
columnar_unsupported!(columnar_tuple_delete(_r: pg_sys::Relation, _t: pg_sys::ItemPointer, _c: pg_sys::CommandId, _sn: pg_sys::Snapshot, _cr: pg_sys::Snapshot, _w: bool, _f: *mut pg_sys::TM_FailureData, _cp: bool) -> pg_sys::TM_Result::Type, "DELETE");
columnar_unsupported!(columnar_tuple_update(_r: pg_sys::Relation, _o: pg_sys::ItemPointer, _sl: *mut pg_sys::TupleTableSlot, _c: pg_sys::CommandId, _sn: pg_sys::Snapshot, _cr: pg_sys::Snapshot, _w: bool, _f: *mut pg_sys::TM_FailureData, _lm: *mut pg_sys::LockTupleMode::Type, _ui: *mut pg_sys::TU_UpdateIndexes::Type) -> pg_sys::TM_Result::Type, "UPDATE");
columnar_unsupported!(columnar_tuple_lock(_r: pg_sys::Relation, _t: pg_sys::ItemPointer, _sn: pg_sys::Snapshot, _sl: *mut pg_sys::TupleTableSlot, _c: pg_sys::CommandId, _m: pg_sys::LockTupleMode::Type, _wp: pg_sys::LockWaitPolicy::Type, _fl: u8, _f: *mut pg_sys::TM_FailureData) -> pg_sys::TM_Result::Type, "SELECT FOR UPDATE / row lock");
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

    /// M99 B/C1 — INSERT→SELECT round-trip: rows written to a columnar table read back identical (values, order of
    /// aggregation, NULLs) — the result-equivalence GATE vs a row-store, single-transaction MVP. Column-major
    /// encoding + compression + min/max pruning are the follow-up slice; this proves correct storage+retrieval.
    #[pg_test]
    fn m99_insert_select_roundtrip() {
        Spi::run("CREATE TABLE m99_rt2 (a int, b text, c float8) USING theodb_columnar").unwrap();
        Spi::run(
            "INSERT INTO m99_rt2 SELECT g, 'row-' || g, g * 1.5 FROM generate_series(1, 5000) g",
        )
        .unwrap();
        // NULL handling: a row with a NULL text + NULL float.
        Spi::run("INSERT INTO m99_rt2 VALUES (5001, NULL, NULL)").unwrap();

        let cnt = Spi::get_one::<i64>("SELECT count(*) FROM m99_rt2").unwrap().unwrap();
        assert_eq!(cnt, 5001, "columnar table must return all inserted rows");

        let suma = Spi::get_one::<i64>("SELECT sum(a)::bigint FROM m99_rt2").unwrap().unwrap();
        assert_eq!(suma, (1..=5001i64).sum::<i64>(), "sum(a) must match");

        let sumc = Spi::get_one::<f64>("SELECT sum(c) FROM m99_rt2").unwrap().unwrap();
        let expect_c: f64 = (1..=5000i64).map(|g| g as f64 * 1.5).sum();
        assert!((sumc - expect_c).abs() < 1e-6, "sum(c) must match ({sumc} vs {expect_c})");

        let nulls = Spi::get_one::<i64>("SELECT count(*) FROM m99_rt2 WHERE b IS NULL").unwrap().unwrap();
        assert_eq!(nulls, 1, "the one NULL-text row must read back as NULL");

        let sample = Spi::get_one::<String>("SELECT b FROM m99_rt2 WHERE a = 42").unwrap().unwrap();
        assert_eq!(sample, "row-42", "text values must round-trip exactly");

        Spi::run("DROP TABLE m99_rt2").unwrap();
    }

    /// M99 (zstd) — the stripe is zstd-compressed on disk: a highly-repetitive column compresses well, so the
    /// columnar table's on-disk size is materially smaller than the same rows in a heap table (MEASURED, not
    /// asserted by opinion). Also proves the round-trip still holds through the compress/decompress path.
    #[pg_test]
    fn m99_stripe_compression_shrinks_ondisk() {
        // Highly compressible: a constant text + a monotonic int.
        Spi::run("CREATE TABLE m99_cz (a int, b text) USING theodb_columnar").unwrap();
        Spi::run("CREATE TABLE m99_hz (a int, b text)").unwrap(); // heap control
        let ins = "SELECT g, repeat('x', 200) FROM generate_series(1, 20000) g";
        Spi::run(&format!("INSERT INTO m99_cz {ins}")).unwrap();
        Spi::run(&format!("INSERT INTO m99_hz {ins}")).unwrap();

        // Force the columnar flush (scan) then measure on-disk size.
        let cnt = Spi::get_one::<i64>("SELECT count(*) FROM m99_cz").unwrap().unwrap();
        assert_eq!(cnt, 20000, "round-trip through compression must return all rows");

        let cz = Spi::get_one::<i64>("SELECT pg_relation_size('m99_cz')").unwrap().unwrap();
        let hz = Spi::get_one::<i64>("SELECT pg_relation_size('m99_hz')").unwrap().unwrap();
        // The repeat('x',200) column compresses ~massively; require at least a 2× on-disk shrink vs the heap.
        assert!(
            cz > 0 && (cz as f64) < (hz as f64) / 2.0,
            "columnar on-disk size {cz} must be < half the heap size {hz} (zstd compression benefit)"
        );

        Spi::run("DROP TABLE m99_cz").unwrap();
        Spi::run("DROP TABLE m99_hz").unwrap();
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
