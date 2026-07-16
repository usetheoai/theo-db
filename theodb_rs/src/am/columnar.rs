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

use super::columnar_codec::{self as codec, StripeHeader};
use pgrx::prelude::*;

// The column-major byval encoding (`Datum::value().to_le_bytes()[..attlen]`) and the SET_VARSIZE_4B reconstruction
// assume a little-endian target (x86-64). Make the assumption a compile-time failure on a big-endian build rather
// than a silent wrong-answer at runtime (council-rust-pgrx review).
const _: () = assert!(cfg!(target_endian = "little"), "theodb_columnar column-major encoding requires a little-endian target");
use std::cell::RefCell;
use std::collections::HashMap;
use std::mem::size_of;
use std::sync::OnceLock;

/// Per-backend pending write state: relation OID → accumulated row blobs (each = a formed heap tuple's bytes). These
/// rows are visible ONLY to this backend's own transaction (a same-xact scan appends them directly — no MVCC leak);
/// they are flushed to a durable stripe + its `columnar.stripe` catalog row at xact pre-commit (and at COPY's
/// `finish_bulk_insert`).
thread_local! {
    static WRITE_STATES: RefCell<HashMap<u32, Vec<Vec<u8>>>> = RefCell::new(HashMap::new());
}

// ===========================================================================================================
// M99 Phase C2 — MVCC via a heap catalog (`columnar.stripe`), not the metapage (ADR-0042 D2)
// ===========================================================================================================
//
// A stripe becomes visible to a scan IFF its `columnar.stripe` catalog row is visible under the scan's snapshot —
// delegating snapshot isolation, WAL, crash recovery and abort-rollback to Postgres for free. The metapage tail is
// physical/WAL state: its buffer changes are durable regardless of the enclosing xact's commit/abort, so a stripe
// descriptor written there would be visible even for an uncommitted or aborted INSERT (the MVCC violation). Moving
// the stripe directory to an ordinary heap table makes the catalog row's own xmin/xmax the visibility gate. The
// metapage now keeps ONLY the monotonic reservation counters. The on-disk TCS1 header/directory already indexes
// chunk groups + columns, so ONE catalog table (not chunk_group/chunk tables) is the minimum (council-index-storage).

extension_sql!(
    r#"
CREATE SCHEMA IF NOT EXISTS columnar;
CREATE TABLE IF NOT EXISTS columnar.stripe (
    relid            oid      NOT NULL,   -- pg_class OID of the columnar table (every scan filters on this)
    stripe_id        bigint   NOT NULL,   -- from reserve(StripeId); stable identity + tie-break order
    header_block     integer  NOT NULL,   -- block of the stripe's TCS1 header (navigates everything below)
    row_count        integer  NOT NULL,   -- rows in the stripe (planner stat)
    first_row_number bigint   NOT NULL,   -- reserve(RowNumber) base; deterministic scan order
    ncols            smallint NOT NULL,   -- cross-check vs live tupdesc.natts
    PRIMARY KEY (relid, stripe_id)
);
CREATE INDEX IF NOT EXISTS columnar_stripe_relid_rownum ON columnar.stripe (relid, first_row_number);

-- Reclaim orphaned stripe rows when a columnar table is dropped, so a later OID reuse can never inherit stale
-- stripes (the catalog has no FK to pg_class — the object is already gone by row-delete time).
CREATE OR REPLACE FUNCTION columnar._drop_cleanup() RETURNS event_trigger LANGUAGE plpgsql AS $fn$
BEGIN
    DELETE FROM columnar.stripe s
    USING pg_event_trigger_dropped_objects() d
    WHERE d.classid = 'pg_catalog.pg_class'::regclass AND s.relid = d.objid;
END;
$fn$;
DROP EVENT TRIGGER IF EXISTS columnar_drop_cleanup;
CREATE EVENT TRIGGER columnar_drop_cleanup ON sql_drop EXECUTE FUNCTION columnar._drop_cleanup();
"#,
    name = "theodb_columnar_catalog",
);

/// A committed stripe's catalog metadata (the visibility-scoped row). The TCS1 header at `header_block` carries
/// everything below the stripe (directory, per-column chunks, min/max).
struct StripeMeta {
    header_block: u32,
}

/// Run `f` with a guaranteed active snapshot. SPI needs one, but a flush point (`finish_bulk_insert` /
/// pre-commit `XactCallback`) runs where the executor has not pushed a snapshot → SPI raises "cannot execute SQL
/// without an outer snapshot or portal". During a scan an active snapshot already exists (the query's), so this is a
/// no-op there and the SPI SELECT correctly reads under that snapshot (respecting the xact's isolation level).
unsafe fn with_active_snapshot<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let pushed = !pg_sys::ActiveSnapshotSet();
    if pushed {
        pg_sys::PushActiveSnapshot(pg_sys::GetTransactionSnapshot());
    }
    let r = f();
    if pushed {
        pg_sys::PopActiveSnapshot();
    }
    r
}

/// Read the stripes VISIBLE to the current snapshot for `rel_oid`, from the heap catalog. SPI runs under the active
/// snapshot, so an uncommitted/aborted/committed-after stripe is filtered out by MVCC for free — no visibility code
/// here. Ordered by `first_row_number` for a deterministic, heap-matching scan order.
unsafe fn read_visible_stripes(rel_oid: pg_sys::Oid) -> Result<Vec<StripeMeta>, String> {
    with_active_snapshot(|| Spi::connect(|c| {
        let t = c
            .select(
                "SELECT header_block FROM columnar.stripe WHERE relid = $1 ORDER BY first_row_number, stripe_id",
                None,
                &[rel_oid.into()],
            )
            .map_err(|e| format!("theodb_columnar: stripe catalog read failed: {e:?}"))?;
        let mut out = Vec::new();
        for row in t {
            let hb = row
                .get::<i32>(1)
                .map_err(|e| format!("theodb_columnar: header_block read: {e:?}"))?
                .ok_or("theodb_columnar: null header_block in catalog")?;
            out.push(StripeMeta { header_block: hb as u32 });
        }
        Ok(out)
    }))
}

/// Insert the visibility-granting catalog row for a just-written stripe. Runs inside the current xact via SPI, so the
/// row inherits that xact's xid as its `xmin` — the stripe becomes visible exactly when/whom the INSERT's commit
/// becomes visible, and an abort makes it invisible forever (its data pages become recoverable orphans). Called from
/// `flush_pending` AFTER every referenced data page is durable and every buffer lock is released (no SPI under a
/// buffer lock — council-rust-pgrx).
unsafe fn insert_stripe_row(
    rel_oid: pg_sys::Oid,
    stripe_id: i64,
    header_block: u32,
    row_count: u32,
    first_row_number: i64,
    ncols: i16,
) -> Result<(), String> {
    with_active_snapshot(|| {
        Spi::run_with_args(
            "INSERT INTO columnar.stripe (relid, stripe_id, header_block, row_count, first_row_number, ncols) \
             VALUES ($1, $2, $3, $4, $5, $6)",
            &[
                rel_oid.into(),
                stripe_id.into(),
                (header_block as i32).into(),
                (row_count as i32).into(),
                first_row_number.into(),
                ncols.into(),
            ],
        )
        .map_err(|e| format!("theodb_columnar: stripe catalog insert failed: {e:?}"))
    })
}

/// Pre-commit flush: a plain `INSERT ... VALUES` never triggers `finish_bulk_insert`, so its accumulated rows would
/// be lost at commit without this. At `PRE_COMMIT`/`PREPARE` flush every pending relation into a durable stripe +
/// its catalog row (still inside the committing xact → correct MVCC). On abort, discard the pending rows (the stripe
/// never existed). An `ereport(ERROR)` here safely converts the commit to an abort (pre-commit is before the commit
/// record) — never a panic (council-index-storage + council-rust-pgrx).
#[pg_guard]
unsafe extern "C-unwind" fn columnar_xact_flush(event: pg_sys::XactEvent::Type, _arg: *mut std::ffi::c_void) {
    use pg_sys::XactEvent as XE;
    if event == XE::XACT_EVENT_PRE_COMMIT
        || event == XE::XACT_EVENT_PARALLEL_PRE_COMMIT
        || event == XE::XACT_EVENT_PREPARE
    {
        let oids: Vec<u32> = WRITE_STATES
            .with(|w| w.borrow().iter().filter(|(_, v)| !v.is_empty()).map(|(k, _)| *k).collect());
        for oid in oids {
            let relid = pg_sys::Oid::from_u32_unchecked(oid);
            let rel = pg_sys::relation_open(relid, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            let res = flush_pending(rel);
            pg_sys::relation_close(rel, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            if let Err(e) = res {
                pg_sys::error!("{e}");
            }
        }
    } else if event == XE::XACT_EVENT_ABORT || event == XE::XACT_EVENT_PARALLEL_ABORT {
        WRITE_STATES.with(|w| w.borrow_mut().clear());
    }
}

/// Register the columnar pre-commit flush callback. Called once from `_PG_init`.
pub(crate) fn init() {
    unsafe { pg_sys::RegisterXactCallback(Some(columnar_xact_flush), std::ptr::null_mut()) };
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

/// Per-column layout descriptor derived from the live tupdesc (never from disk): fixed width (`Some(attlen)`) vs
/// varlena (`None`), by-value vs by-reference, and the min/max comparison domain for skip-pruning.
#[derive(Clone, Copy)]
struct ColDesc {
    attlen_fixed: Option<usize>,
    byval: bool,
    mm: codec::MinMaxKind,
}

/// Read the i-th column's descriptor from the flex-array `attrs` (council-rust-pgrx idiom: `attrs.as_ptr().add(i)`,
/// always over `0..natts`). Builtin type OIDs (pg_type.dat, ABI-stable) map to a min/max domain; everything else
/// gets `None` (the pruner then cannot skip that column — fail-safe).
unsafe fn coldesc(tupdesc: pg_sys::TupleDesc, i: usize) -> Result<ColDesc, String> {
    let attr = (*tupdesc).attrs.as_ptr().add(i);
    let attlen = (*attr).attlen;
    let byval = (*attr).attbyval;
    let typid = (*attr).atttypid.to_u32();
    let attlen_fixed = if attlen > 0 {
        Some(attlen as usize)
    } else if attlen == -1 {
        None // varlena
    } else {
        return Err(format!("theodb_columnar: unsupported attlen {attlen} at column {i} (cstring/expanded)"));
    };
    let mm = match typid {
        16 => codec::MinMaxKind::Bool, // BOOLOID
        20 => codec::MinMaxKind::I8,   // INT8OID
        21 => codec::MinMaxKind::I2,   // INT2OID
        23 => codec::MinMaxKind::I4,   // INT4OID
        700 => codec::MinMaxKind::F4,  // FLOAT4OID
        701 => codec::MinMaxKind::F8,  // FLOAT8OID
        _ => codec::MinMaxKind::None,
    };
    Ok(ColDesc { attlen_fixed, byval, mm })
}

/// Extract the storable value bytes of a NON-NULL datum (caller checked `isnull` first — detoasting a null-garbage
/// datum would segfault). Fixed by-value: the low `attlen` bytes of the Datum word (LE, x86-64). Fixed by-reference:
/// `attlen` bytes at the pointer. Varlena: detoast to a private copy, take its logical payload, free the copy.
unsafe fn extract_value_bytes(col: &ColDesc, datum: pg_sys::Datum) -> Result<Vec<u8>, String> {
    match col.attlen_fixed {
        Some(len) => {
            if col.byval {
                let raw = datum.value() as u64;
                Ok(raw.to_le_bytes()[..len].to_vec())
            } else {
                let p = datum.cast_mut_ptr::<u8>();
                Ok(std::slice::from_raw_parts(p, len).to_vec())
            }
        }
        None => {
            // `pg_detoast_datum_copy` ALWAYS returns a fresh palloc → we always own it → always pfree (no double-free
            // ambiguity — dtype.rs idiom). Store the logical payload (header-format-independent).
            let dt = pg_sys::pg_detoast_datum_copy(datum.cast_mut_ptr::<pg_sys::varlena>());
            let payload = varlena_payload(dt as *const u8);
            pg_sys::pfree(dt as *mut std::os::raw::c_void);
            payload
        }
    }
}

/// The logical bytes (no varlena header) of a detoasted varlena, handling both the 1-byte (short) and 4-byte header
/// formats — `pg_detoast_datum_copy` leaves short values short. Length comes from the self-describing header only.
unsafe fn varlena_payload(p: *const u8) -> Result<Vec<u8>, String> {
    let b0 = *p;
    if b0 & 0x01 == 0x01 {
        let total = ((b0 >> 1) & 0x7F) as usize; // VARSIZE_1B: total incl. the 1-byte header
        if total < 1 {
            return Err("theodb_columnar: corrupt 1B varlena".into());
        }
        Ok(std::slice::from_raw_parts(p.add(1), total - 1).to_vec())
    } else {
        let hdr = (p as *const u32).read_unaligned();
        let total = ((hdr >> 2) & 0x3FFF_FFFF) as usize; // VARSIZE_4B: total incl. the 4-byte header
        if total < 4 {
            return Err("theodb_columnar: corrupt 4B varlena".into());
        }
        Ok(std::slice::from_raw_parts(p.add(4), total - 4).to_vec())
    }
}

/// Rebuild a heap Datum from stored value bytes. Returns the Datum plus an optional palloc'd pointer the caller frees
/// after `heap_form_tuple` has copied it. Fixed by-value: zero-extend the LE bytes into a Datum word. Varlena: build a
/// canonical 4-byte varlena (SET_VARSIZE_4B — the dtype.rs idiom).
unsafe fn rebuild_datum(
    col: &ColDesc,
    bytes: &[u8],
) -> Result<(pg_sys::Datum, Option<*mut std::os::raw::c_void>), String> {
    match col.attlen_fixed {
        Some(len) => {
            if bytes.len() < len {
                return Err(format!("theodb_columnar: fixed value {} bytes < attlen {len}", bytes.len()));
            }
            if col.byval {
                let mut buf = [0u8; 8];
                buf[..len].copy_from_slice(&bytes[..len]);
                Ok((pg_sys::Datum::from(u64::from_le_bytes(buf) as usize), None))
            } else {
                let p = pg_sys::palloc(len) as *mut u8;
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, len);
                Ok((pg_sys::Datum::from(p), Some(p as *mut std::os::raw::c_void)))
            }
        }
        None => {
            let total = 4 + bytes.len();
            let p = pg_sys::palloc(total) as *mut u8;
            (p as *mut u32).write((total as u32) << 2); // SET_VARSIZE_4B (LE on x86-64, low 2 bits = 0)
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), p.add(4), bytes.len());
            Ok((pg_sys::Datum::from(p), Some(p as *mut std::os::raw::c_void)))
        }
    }
}

/// Reconstruct the heap-tuple bytes of row `r` from the decoded per-column values of a chunk group, so the existing
/// `scan_getnextslot` (which `heap_deform_tuple`s a stored blob) needs no change.
unsafe fn form_row(
    tupdesc: pg_sys::TupleDesc,
    cols: &[ColDesc],
    cgcols: &[Vec<Option<Vec<u8>>>],
    r: usize,
) -> Result<Vec<u8>, String> {
    let natts = cols.len();
    let mut values = vec![pg_sys::Datum::from(0usize); natts];
    let mut isnull = vec![false; natts];
    let mut to_free: Vec<*mut std::os::raw::c_void> = Vec::new();
    for col in 0..natts {
        match &cgcols[col][r] {
            None => isnull[col] = true,
            Some(bytes) => {
                let (datum, freeable) = rebuild_datum(&cols[col], bytes)?;
                values[col] = datum;
                if let Some(p) = freeable {
                    to_free.push(p);
                }
            }
        }
    }
    let htup = pg_sys::heap_form_tuple(tupdesc, values.as_mut_ptr(), isnull.as_mut_ptr());
    let len = (*htup).t_len as usize;
    let bytes = std::slice::from_raw_parts((*htup).t_data as *const u8, len).to_vec();
    pg_sys::heap_freetuple(htup);
    for p in to_free {
        pg_sys::pfree(p);
    }
    Ok(bytes)
}

/// Decode one column-major stripe into heap-tuple byte blobs: read its TCS1 header (at `header_block`, from the
/// catalog row), its directory, then for each chunk group decode every column and transpose back to rows.
unsafe fn decode_stripe(
    rel: pg_sys::Relation,
    header_block: u32,
    tupdesc: pg_sys::TupleDesc,
    cols: &[ColDesc],
    natts: usize,
    out: &mut Vec<Vec<u8>>,
) -> Result<(), String> {
    let hdr_items = super::page::read_all_page_items(rel, header_block)?;
    let hdr_bytes = hdr_items.into_iter().next().ok_or("theodb_columnar: stripe header page empty")?;
    let header = StripeHeader::from_bytes(&hdr_bytes)?;
    if header.ncols as usize != natts {
        return Err(format!("theodb_columnar: stripe ncols {} != relation natts {natts}", header.ncols));
    }
    let dir_bytes = super::page::read_chunked(rel, header.dir_first_block, header.dir_n_pages)?;
    if dir_bytes.len() < header.dir_len as usize {
        return Err("theodb_columnar: stripe directory truncated on disk".into());
    }
    let n_entries = header.n_chunk_groups as usize * natts;
    let entries = codec::deserialize_directory(&dir_bytes, n_entries)?;
    for cg in 0..header.n_chunk_groups as usize {
        let cg_rows = entries[cg * natts].row_count as usize;
        let mut cgcols: Vec<Vec<Option<Vec<u8>>>> = Vec::with_capacity(natts);
        for col in 0..natts {
            let e = &entries[cg * natts + col];
            let comp = super::page::read_chunked(rel, e.first_block, e.n_pages)?;
            if comp.len() < e.comp_len as usize {
                return Err("theodb_columnar: column chunk truncated on disk".into());
            }
            let raw = zstd::decode_all(&comp[..e.comp_len as usize])
                .map_err(|x| format!("theodb_columnar: zstd decode failed: {x}"))?;
            cgcols.push(codec::decode_column(&raw, cols[col].attlen_fixed, cg_rows, e.has_nulls)?);
        }
        for r in 0..cg_rows {
            out.push(form_row(tupdesc, cols, &cgcols, r)?);
        }
    }
    Ok(())
}

/// Materialize the rows VISIBLE to this scan: (1) the committed stripes visible under the current snapshot (MVCC
/// delegated to the `columnar.stripe` heap catalog), decoded column-major; then (2) this backend's not-yet-flushed
/// pending rows (thread-local — visible only to its own xact, so no cross-xact leak). Flushing is done at pre-commit,
/// NOT here — reading is side-effect-free.
unsafe fn materialize_rows(rel: pg_sys::Relation) -> Result<Vec<Vec<u8>>, String> {
    unsafe {
        let tupdesc = (*rel).rd_att;
        let natts = (*tupdesc).natts as usize;
        let cols = (0..natts).map(|i| coldesc(tupdesc, i)).collect::<Result<Vec<_>, _>>()?;
        let mut out = Vec::new();
        for sm in read_visible_stripes((*rel).rd_id)? {
            decode_stripe(rel, sm.header_block, tupdesc, &cols, natts, &mut out)?;
        }
        let oid = (*rel).rd_id.to_u32();
        WRITE_STATES.with(|w| {
            if let Some(rows) = w.borrow().get(&oid) {
                out.extend(rows.iter().cloned());
            }
        });
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

/// Write `bytes` across one-item pages (chunked at `page::CHUNK`) and return the actual `(first_block, n_pages)` the
/// extends received. A single backend flushes a whole stripe back-to-back with no yield, so these pages are contiguous
/// (read back via `read_chunked`); the cross-backend interleave is proven/handled in Phase D. An empty payload still
/// takes one page (so an all-null column's directory entry has a real block to point at).
unsafe fn write_chunk(rel: pg_sys::Relation, bytes: &[u8]) -> (u32, u32) {
    if bytes.is_empty() {
        let b = super::page::extend_page_with_item(rel, pg_sys::ForkNumber::MAIN_FORKNUM, &[]);
        return (b, 1);
    }
    let mut first: Option<u32> = None;
    let mut n = 0u32;
    for chunk in bytes.chunks(super::page::CHUNK) {
        let b = super::page::extend_page_with_item(rel, pg_sys::ForkNumber::MAIN_FORKNUM, chunk);
        first.get_or_insert(b);
        n += 1;
    }
    (first.unwrap(), n)
}

/// Flush this backend's pending rows for `rel` into a new COLUMN-MAJOR stripe: deform each row into per-column value
/// streams, split into 10k-row chunk groups, zstd-compress each `(chunk_group, column)` chunk with its per-chunk
/// min/max, write the chunks → directory → header, reserve the row_number range + stripe id, and append the stripe
/// descriptor to the metapage LAST. Because the metapage `StripeDesc` (the visibility root) is pivoted only after
/// every referenced page is durable, an aborted/crashed flush leaves invisible orphan pages, never a visible stripe
/// over garbage (the crash-safety invariant; cross-backend snapshot visibility is Phase C2/D).
unsafe fn flush_pending(rel: pg_sys::Relation) -> Result<(), String> {
    let oid = unsafe { (*rel).rd_id.to_u32() };
    let rows = WRITE_STATES.with(|w| w.borrow_mut().remove(&oid));
    let Some(rows) = rows else { return Ok(()) };
    if rows.is_empty() {
        return Ok(());
    }
    unsafe {
        let tupdesc = (*rel).rd_att;
        let natts = (*tupdesc).natts as usize;
        let cols = (0..natts).map(|i| coldesc(tupdesc, i)).collect::<Result<Vec<_>, _>>()?;

        // Deform every pending row (stored as heap-tuple bytes) into per-column value streams. `columns[c][r]` is
        // `Some(raw bytes)` or `None` for SQL NULL. `isnull` is checked before touching a datum (detoast of a null
        // is a segfault).
        let row_count = rows.len();
        let mut columns: Vec<Vec<Option<Vec<u8>>>> = vec![Vec::with_capacity(row_count); natts];
        let mut values = vec![pg_sys::Datum::from(0usize); natts];
        let mut isnull = vec![false; natts];
        for rbytes in &rows {
            let mut htup: pg_sys::HeapTupleData = std::mem::zeroed();
            htup.t_len = rbytes.len() as u32;
            htup.t_data = rbytes.as_ptr() as pg_sys::HeapTupleHeader;
            pg_sys::heap_deform_tuple(&mut htup, tupdesc, values.as_mut_ptr(), isnull.as_mut_ptr());
            for i in 0..natts {
                if isnull[i] {
                    columns[i].push(None);
                } else {
                    columns[i].push(Some(extract_value_bytes(&cols[i], values[i])?));
                }
            }
        }

        // Encode + write each (chunk_group, column) chunk; build the directory in grid order [cg][col].
        let n_cg = row_count.div_ceil(codec::CHUNK_GROUP_ROWS);
        let mut dir = Vec::with_capacity(n_cg * natts);
        for cg in 0..n_cg {
            let lo = cg * codec::CHUNK_GROUP_ROWS;
            let hi = (lo + codec::CHUNK_GROUP_ROWS).min(row_count);
            for col in 0..natts {
                let enc = codec::encode_column(&columns[col][lo..hi], cols[col].attlen_fixed, cols[col].mm);
                let compressed = zstd::encode_all(&enc.raw[..], 3)
                    .map_err(|e| format!("theodb_columnar: zstd compress failed: {e}"))?;
                let (first_block, n_pages) = write_chunk(rel, &compressed);
                dir.push(codec::ChunkDirEntry {
                    first_block,
                    n_pages,
                    comp_len: compressed.len() as u32,
                    raw_len: enc.raw.len() as u32,
                    row_count: (hi - lo) as u32,
                    null_count: enc.null_count,
                    has_nulls: enc.has_nulls,
                    has_minmax: enc.has_minmax,
                    all_null: enc.all_null,
                    min_bits: enc.min_bits,
                    max_bits: enc.max_bits,
                });
            }
        }

        // Reserve the row_number range (base needed by the header) + a stripe id.
        let base = reserve(rel, Counter::RowNumber, row_count as u64)?;
        let stripe_id = reserve(rel, Counter::StripeId, 1)? as i64;

        // Write the directory, then the header (single item pointing at the directory).
        let dir_bytes = codec::serialize_directory(&dir);
        let (dir_first_block, dir_n_pages) = write_chunk(rel, &dir_bytes);
        let header = StripeHeader {
            ncols: natts as u16,
            n_chunk_groups: n_cg as u32,
            row_count: row_count as u32,
            first_row_number: base,
            dir_first_block,
            dir_n_pages,
            dir_len: dir_bytes.len() as u32,
        };
        let header_block =
            super::page::extend_page_with_item(rel, pg_sys::ForkNumber::MAIN_FORKNUM, &header.to_bytes());

        // Publish the stripe LAST via its heap-catalog row (the MVCC visibility root) — after every referenced data
        // page is durable and every buffer lock is released. Its xmin ties visibility to this xact's commit.
        insert_stripe_row((*rel).rd_id, stripe_id, header_block, row_count as u32, base as i64, natts as i16)?;
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

/// M99 Phase C test helper — introspect the first stripe's on-disk column-major format: the magic, chunk-group /
/// column counts, and chunk-group-0/column-0 min/max. Flushes pending writes first so a fresh INSERT is observable.
#[cfg(any(test, feature = "pg_test"))]
#[pg_extern]
fn theodb_columnar_test_stripe_info(rel_oid: pg_sys::Oid) -> String {
    unsafe {
        let rel = pg_sys::relation_open(rel_oid, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
        let res = (|| -> Result<String, String> {
            flush_pending(rel)?;
            let stripes = read_visible_stripes((*rel).rd_id)?;
            if stripes.is_empty() {
                return Ok("empty".into());
            }
            let tupdesc = (*rel).rd_att;
            let natts = (*tupdesc).natts as usize;
            let st = &stripes[0];
            let hdr_items = super::page::read_all_page_items(rel, st.header_block)?;
            let hdr_bytes = hdr_items.into_iter().next().ok_or("no header item")?;
            let header = StripeHeader::from_bytes(&hdr_bytes)?; // validates the real "TCS1" magic
            let dir_bytes = super::page::read_chunked(rel, header.dir_first_block, header.dir_n_pages)?;
            let entries = codec::deserialize_directory(&dir_bytes, header.n_chunk_groups as usize * natts)?;
            let e0 = &entries[0]; // chunk group 0, column 0
            let (minr, maxr) = if e0.has_minmax {
                match coldesc(tupdesc, 0)?.mm {
                    codec::MinMaxKind::F4 | codec::MinMaxKind::F8 => (
                        format!("{}", f64::from_bits(e0.min_bits)),
                        format!("{}", f64::from_bits(e0.max_bits)),
                    ),
                    codec::MinMaxKind::None => ("na".into(), "na".into()),
                    _ => (format!("{}", e0.min_bits as i64), format!("{}", e0.max_bits as i64)),
                }
            } else {
                ("none".into(), "none".into())
            };
            Ok(format!(
                "magic=TCS1;stripes={};cg={};ncols={};col0_hasmm={};col0_min={};col0_max={}",
                stripes.len(),
                header.n_chunk_groups,
                header.ncols,
                e0.has_minmax,
                minr,
                maxr
            ))
        })();
        pg_sys::relation_close(rel, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
        match res {
            Ok(s) => s,
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

        // Force the columnar flush (the introspection helper flushes pending → durable stripe) then measure on-disk
        // size — the pending rows live in memory until flush, so we must materialize the stripe before pg_relation_size.
        let oid = Spi::get_one::<pg_sys::Oid>("SELECT 'm99_cz'::regclass::oid").unwrap().unwrap();
        Spi::get_one_with_args::<String>("SELECT theodb_columnar_test_stripe_info($1)", &[oid.into()])
            .unwrap()
            .unwrap();
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

    /// M99 Phase C — the stripe is stored COLUMN-MAJOR (magic "TCS1", per-column chunks in a `[chunk_group][col]`
    /// directory) with per-chunk min/max, and the INSERT→SELECT round-trip still holds through the new encode/decode.
    /// 25000 rows span 3 chunk groups (10k granule); chunk-group-0 / column-0 (`a int`, rows 1..10000) carries
    /// min=1/max=10000. This is the RED test that fails against the retired row-major zstd-blob format.
    #[pg_test]
    fn m99_stripe_is_column_major() {
        Spi::run("CREATE TABLE m99_cm (a int, b text, c float8) USING theodb_columnar").unwrap();
        Spi::run("INSERT INTO m99_cm SELECT g, 'row-' || g, g::float8 FROM generate_series(1, 25000) g").unwrap();
        Spi::run("INSERT INTO m99_cm VALUES (25001, NULL, NULL)").unwrap(); // NULL text + NULL float
        let oid = Spi::get_one::<pg_sys::Oid>("SELECT 'm99_cm'::regclass::oid").unwrap().unwrap();
        // The introspection helper flushes the 25001 pending rows into ONE durable column-major stripe + its MVCC
        // catalog row, then reports the on-disk format — so the subsequent SELECTs read back through the real
        // disk-decode + catalog-read path (not the in-memory pending buffer).
        let info = Spi::get_one_with_args::<String>(
            "SELECT theodb_columnar_test_stripe_info($1)",
            &[oid.into()],
        )
        .unwrap()
        .unwrap();
        assert!(info.contains("magic=TCS1"), "stripe must be the column-major TCS1 format: {info}");
        assert!(info.contains("ncols=3"), "3 columns: {info}");
        assert!(info.contains("cg=3"), "25001 rows / 10000-row chunk groups = 3: {info}");
        assert!(info.contains("col0_hasmm=true"), "int column must carry min/max: {info}");
        assert!(info.contains("col0_min=1;"), "chunk-group 0 (rows 1..10000) min(a) = 1: {info}");
        assert!(info.contains("col0_max=10000"), "chunk-group 0 max(a) = 10000: {info}");

        // Result-equivalence through the column-major encode → disk → decode + MVCC-catalog read path.
        let cnt = Spi::get_one::<i64>("SELECT count(*) FROM m99_cm").unwrap().unwrap();
        assert_eq!(cnt, 25001, "all rows read back from the durable stripe");
        let suma = Spi::get_one::<i64>("SELECT sum(a)::bigint FROM m99_cm").unwrap().unwrap();
        assert_eq!(suma, (1..=25001i64).sum::<i64>(), "sum(a) matches");
        let sumc = Spi::get_one::<f64>("SELECT sum(c) FROM m99_cm").unwrap().unwrap();
        assert!((sumc - (1..=25000i64).map(|g| g as f64).sum::<f64>()).abs() < 1e-3, "sum(c) matches: {sumc}");
        let sample = Spi::get_one::<String>("SELECT b FROM m99_cm WHERE a = 12345").unwrap().unwrap();
        assert_eq!(sample, "row-12345", "text round-trips across a chunk-group boundary via disk decode");
        let nulls = Spi::get_one::<i64>("SELECT count(*) FROM m99_cm WHERE b IS NULL").unwrap().unwrap();
        assert_eq!(nulls, 1, "the NULL-text row round-trips through the column null-bitmap on disk");
        Spi::run("DROP TABLE m99_cm").unwrap();
    }

    /// M99 Phase C2 — MVCC delegation to the `columnar.stripe` heap catalog: a stripe committed by another session
    /// becomes visible; the catalog row's own xmin/xmax is the visibility gate. This single-session test proves the
    /// catalog is the visibility root (flushed stripe visible via the catalog; a pending-only table shows via the
    /// in-memory buffer). The full cross-xact permutations (uncommitted invisible / RR holds snapshot / abort) are
    /// the Phase D `pg_isolation_regress` proof.
    #[pg_test]
    fn m99_mvcc_catalog_is_visibility_root() {
        Spi::run("CREATE TABLE m99_mv (a int) USING theodb_columnar").unwrap();
        Spi::run("INSERT INTO m99_mv SELECT g FROM generate_series(1, 100) g").unwrap();
        let oid = Spi::get_one::<pg_sys::Oid>("SELECT 'm99_mv'::regclass::oid").unwrap().unwrap();
        // Before flush: nothing in the catalog, rows visible only via the same-xact pending buffer.
        let pre = Spi::get_one::<i64>("SELECT count(*) FROM columnar.stripe WHERE relid = 'm99_mv'::regclass")
            .unwrap()
            .unwrap();
        assert_eq!(pre, 0, "no catalog row before flush");
        let cnt_pending = Spi::get_one::<i64>("SELECT count(*) FROM m99_mv").unwrap().unwrap();
        assert_eq!(cnt_pending, 100, "pending rows visible to same xact before flush");
        // Flush → exactly one catalog stripe row appears (the visibility root).
        Spi::get_one_with_args::<String>("SELECT theodb_columnar_test_stripe_info($1)", &[oid.into()])
            .unwrap()
            .unwrap();
        let post = Spi::get_one::<i64>("SELECT count(*) FROM columnar.stripe WHERE relid = 'm99_mv'::regclass")
            .unwrap()
            .unwrap();
        assert_eq!(post, 1, "exactly one catalog stripe row after flush");
        let cnt_disk = Spi::get_one::<i64>("SELECT count(*) FROM m99_mv").unwrap().unwrap();
        assert_eq!(cnt_disk, 100, "rows visible via the catalog stripe after flush");
        Spi::run("DROP TABLE m99_mv").unwrap();
    }

    /// M99 Phase C2 — DROP TABLE reclaims the table's `columnar.stripe` rows (the `sql_drop` event trigger), so a
    /// later OID reuse can never inherit stale stripes. Without this the catalog (which has no FK to pg_class) would
    /// leak orphan rows.
    #[pg_test]
    fn m99_drop_table_reclaims_catalog_rows() {
        Spi::run("CREATE TABLE m99_dc (a int) USING theodb_columnar").unwrap();
        Spi::run("INSERT INTO m99_dc SELECT g FROM generate_series(1, 50) g").unwrap();
        let oid = Spi::get_one::<pg_sys::Oid>("SELECT 'm99_dc'::regclass::oid").unwrap().unwrap();
        Spi::get_one_with_args::<String>("SELECT theodb_columnar_test_stripe_info($1)", &[oid.into()])
            .unwrap()
            .unwrap(); // flush → one catalog row
        let before = Spi::get_one_with_args::<i64>(
            "SELECT count(*) FROM columnar.stripe WHERE relid = $1",
            &[oid.into()],
        )
        .unwrap()
        .unwrap();
        assert_eq!(before, 1, "one catalog row before drop");
        Spi::run("DROP TABLE m99_dc").unwrap();
        // The dropped table's OID must have no surviving catalog rows.
        let after = Spi::get_one_with_args::<i64>(
            "SELECT count(*) FROM columnar.stripe WHERE relid = $1",
            &[oid.into()],
        )
        .unwrap()
        .unwrap();
        assert_eq!(after, 0, "DROP TABLE reclaimed the columnar.stripe rows (event trigger)");
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
