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
const _: () = assert!(
    cfg!(target_endian = "little"),
    "theodb_columnar column-major encoding requires a little-endian target"
);
use std::cell::RefCell;
use std::collections::HashMap;
use std::mem::size_of;
use std::sync::OnceLock;

/// Per-backend pending write state: relation OID → accumulated row blobs (each = a formed heap tuple's bytes) + a
/// running byte counter. These rows are visible ONLY to this backend's own transaction (a same-xact scan appends them
/// directly — no MVCC leak). M104 (#99): the pending set is flushed to a durable stripe **incrementally** once its
/// bytes exceed `maintenance_work_mem` (bounded write memory, N-independent — the DuckDB row-group / ClickHouse
/// one-part-per-INSERT pattern), plus a final drain at xact pre-commit and COPY's `finish_bulk_insert`. Every stripe's
/// `columnar.stripe` catalog row carries the same xact xid, so all stripes of one INSERT commit/abort atomically —
/// the crash-safety invariant (pages durable → catalog row LAST) is preserved per-stripe by construction.
#[derive(Default)]
struct PendingWrite {
    rows: Vec<Vec<u8>>,
    bytes: usize,
}
thread_local! {
    static WRITE_STATES: RefCell<HashMap<u32, PendingWrite>> = RefCell::new(HashMap::new());
    // M150 — last-scan chunk-group skip counters `(skipped, scanned)`, reset at `columnar_scan_begin`. Wiring
    // metric (pillar c): the A/B evidence that the min/max directory is being consumed. Best-effort under nested
    // scans (a shared thread_local, same as the agg-path `THEODB_SCAN_PROFILE` log) — a single top-level query
    // reads its own counts; SQL accessors `theodb_columnar_chunks_{skipped,scanned}()` expose them for tests.
    static SKIP_STATS: std::cell::Cell<(u64, u64)> = const { std::cell::Cell::new((0, 0)) };
}

/// M150 — chunk groups the zone-map pruned in the most recent `theodb_columnar` general scan (wiring metric).
#[pg_extern]
fn theodb_columnar_chunks_skipped() -> i64 {
    SKIP_STATS.with(|s| s.get().0 as i64)
}

/// M150 — chunk groups the most recent `theodb_columnar` general scan examined (skipped + decoded).
#[pg_extern]
fn theodb_columnar_chunks_scanned() -> i64 {
    SKIP_STATS.with(|s| s.get().1 as i64)
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
    with_active_snapshot(|| {
        Spi::connect(|c| {
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
        })
    })
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
unsafe extern "C-unwind" fn columnar_xact_flush(
    event: pg_sys::XactEvent::Type,
    _arg: *mut std::ffi::c_void,
) {
    use pg_sys::XactEvent as XE;
    if event == XE::XACT_EVENT_PRE_COMMIT
        || event == XE::XACT_EVENT_PARALLEL_PRE_COMMIT
        || event == XE::XACT_EVENT_PREPARE
    {
        let oids: Vec<u32> = WRITE_STATES.with(|w| {
            w.borrow().iter().filter(|(_, v)| !v.rows.is_empty()).map(|(k, _)| *k).collect()
        });
        for oid in oids {
            let relid = pg_sys::Oid::from_u32_unchecked(oid);
            // M144 T2.1: the table may have been DROPped in THIS same txn (INSERT + DROP TABLE before
            // COMMIT). `relation_open` would ERROR on the now-invisible OID and abort the user's whole
            // COMMIT. `try_relation_open` returns NULL instead — skip the flush for a dropped relation
            // (its columnar data is being removed anyway) and drop its pending WRITE_STATES entry so a
            // future txn that reuses the OID does not inherit a stale buffer.
            let rel =
                pg_sys::try_relation_open(relid, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            if rel.is_null() {
                WRITE_STATES.with(|w| {
                    w.borrow_mut().remove(&oid);
                });
                continue;
            }
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
    let mut amr =
        unsafe { PgBox::<pg_sys::TableAmRoutine>::alloc_node(pg_sys::NodeTag::T_TableAmRoutine) };

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
    // M135/ADR-2: bitmap callbacks stay NULL — deliberately, not by omission. PG18 removed
    // `scan_bitmap_next_block` from `TableAmRoutine` entirely, and registering an erroring stub for
    // `scan_bitmap_next_tuple` would tell the planner we support bitmap scans, so it would plan one and fail at
    // runtime. Citus reaches the same conclusion for its columnar AM (`columnar_tableam.c:2527` NULL, with the
    // planner consequence documented at `columnar_customscan.c:435-443`). Leaving it NULL makes the planner route
    // around us instead.
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
            return Err(format!(
                "theodb_columnar: bad metapage magic {magic:#x} (expected {META_MAGIC:#x})"
            ));
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
    unsafe {
        super::page::extend_page_with_item(rel, pg_sys::ForkNumber::MAIN_FORKNUM, &meta.to_bytes())
    };
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
    let page =
        pg_sys::GenericXLogRegisterBuffer(state, buf, pg_sys::GENERIC_XLOG_FULL_IMAGE as i32);

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
// M104 (Q2) — a LAZY, one-stripe-at-a-time scan cursor. `columnar_scan_begin` resolves the visible-stripe SET
// once (MVCC-correct, snapshot-fixed for the scan's life), then `getnextslot` decodes ONE stripe into `current`
// only when the previous one is exhausted, and drains the same-xact WRITE_STATES pending rows as the FINAL batch.
// Peak scan memory is O(one stripe ≈ maintenance_work_mem), not O(the whole visible table) — the Arrow
// RecordBatch / DuckDB row-group-at-a-time streaming pattern. Row ORDER is byte-identical to the old eager
// materialization (stripes in catalog order, then pending), so scan results are unchanged.
struct ColumnarScanState {
    base: pg_sys::TableScanDescData,
    stripes: *mut Vec<StripeMeta>, // the visible stripe set (resolved once at begin — MVCC-fixed)
    stripe_idx: usize,
    current: *mut Vec<Vec<u8>>, // the currently-decoded stripe's rows (or the pending tail); freed on advance/end
    cursor: usize,
    pending_loaded: bool, // the WRITE_STATES same-xact tail has been loaded as the final batch
}

/// Per-column layout descriptor derived from the live tupdesc (never from disk): fixed width (`Some(attlen)`) vs
/// varlena (`None`), by-value vs by-reference, and the min/max comparison domain for skip-pruning.
#[derive(Clone, Copy)]
struct ColDesc {
    attlen_fixed: Option<usize>,
    byval: bool,
    mm: codec::MinMaxKind,
    typid: u32,
}

/// Read the i-th column's descriptor via `super::tupdesc_attr` (M135/ADR-1 — never touch `attrs`/`compact_attrs`
/// directly: PG18 moved the array and the naive access compiles while reading out of bounds). Builtin type OIDs (pg_type.dat, ABI-stable) map to a min/max domain; everything else
/// gets `None` (the pruner then cannot skip that column — fail-safe).
unsafe fn coldesc(tupdesc: pg_sys::TupleDesc, i: usize) -> Result<ColDesc, String> {
    let attr = super::tupdesc_attr(tupdesc, i);
    let attlen = (*attr).attlen;
    let byval = (*attr).attbyval;
    let typid = (*attr).atttypid.to_u32();
    let attlen_fixed = if attlen > 0 {
        Some(attlen as usize)
    } else if attlen == -1 {
        None // varlena
    } else {
        return Err(format!(
            "theodb_columnar: unsupported attlen {attlen} at column {i} (cstring/expanded)"
        ));
    };
    let mm = minmax_kind_of(typid);
    Ok(ColDesc { attlen_fixed, byval, mm, typid })
}

/// The min/max comparison domain for a Postgres type OID (shared by `coldesc` on the write/decode side and by the
/// zone-map predicate extraction on the plan side — DRY). `None` for any type without a cheap native order.
pub(crate) fn minmax_kind_of(typid: u32) -> codec::MinMaxKind {
    match typid {
        16 => codec::MinMaxKind::Bool, // BOOLOID
        20 => codec::MinMaxKind::I8,   // INT8OID
        21 => codec::MinMaxKind::I2,   // INT2OID
        23 => codec::MinMaxKind::I4,   // INT4OID
        700 => codec::MinMaxKind::F4,  // FLOAT4OID
        701 => codec::MinMaxKind::F8,  // FLOAT8OID
        // Temporal types share an integer min/max domain (the stored bytes ARE the internal int): timestamp /
        // timestamptz are int64 microseconds → I8; date is int32 days → I4. Numeric-order compare (chunk_can_match)
        // + compute_minmax + encode_const_bits all reuse the proven I8/I4 path unchanged. The Arrow-facing type
        // (build_arrow / build_filter_expr) still branches on the OID so the DataFusion Filter stays type-correct.
        1114 | 1184 => codec::MinMaxKind::I8, // TIMESTAMPOID / TIMESTAMPTZOID (int64 μs since 2000-01-01)
        1082 => codec::MinMaxKind::I4,        // DATEOID (int32 days since 2000-01-01)
        _ => codec::MinMaxKind::None,
    }
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

/// Deform buffered rows (heap-tuple bytes) into per-column byte streams, for the columns in `wanted`.
///
/// #190 (ADR-2): the three call-sites that did this inline — the pending-aware scan, the min/max fast path and
/// `flush_pending` — differed ONLY in which columns they wanted (a subset, exactly one, all of them). Keeping three
/// copies of the same rule meant a change to the conversion had to be applied three times; the TOAST materialisation
/// of #190 is exactly such a change, and two stale copies would have silently kept the old semantics.
///
/// Returns `out[i]` for `wanted[i]`, one entry per row (`None` = SQL NULL).
unsafe fn deform_rows_into_columns(
    rows: &[Vec<u8>],
    tupdesc: pg_sys::TupleDesc,
    natts: usize,
    cols: &[ColDesc],
    wanted: &[usize],
) -> Result<Vec<Vec<Option<Vec<u8>>>>, String> {
    unsafe {
        let mut out: Vec<Vec<Option<Vec<u8>>>> = vec![Vec::with_capacity(rows.len()); wanted.len()];
        let mut values = vec![pg_sys::Datum::from(0usize); natts];
        let mut isnull = vec![false; natts];
        for rbytes in rows {
            let mut htup: pg_sys::HeapTupleData = std::mem::zeroed();
            htup.t_len = rbytes.len() as u32;
            htup.t_data = rbytes.as_ptr() as pg_sys::HeapTupleHeader;
            pg_sys::heap_deform_tuple(&mut htup, tupdesc, values.as_mut_ptr(), isnull.as_mut_ptr());
            for (wi, &col) in wanted.iter().enumerate() {
                // `isnull` is checked before touching the datum — detoasting a NULL is a segfault.
                if isnull[col] {
                    out[wi].push(None);
                } else {
                    out[wi].push(Some(extract_value_bytes(&cols[col], values[col])?));
                }
            }
        }
        Ok(out)
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
                return Err(format!(
                    "theodb_columnar: fixed value {} bytes < attlen {len}",
                    bytes.len()
                ));
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
///
/// M149 — `want_mask[col] == false` marks a column projected AWAY: it is written NULL (its `cgcols[col]` was never
/// decoded, so it MUST NOT be indexed). When every entry is true (the plain all-columns path) the behavior is
/// byte-identical to pre-M149. The row COUNT is unaffected, so scan order stays byte-identical.
unsafe fn form_row(
    tupdesc: pg_sys::TupleDesc,
    cols: &[ColDesc],
    cgcols: &[Vec<Option<Vec<u8>>>],
    r: usize,
    want_mask: &[bool],
) -> Result<Vec<u8>, String> {
    let natts = cols.len();
    let mut values = vec![pg_sys::Datum::from(0usize); natts];
    let mut isnull = vec![false; natts];
    let mut to_free: Vec<*mut std::os::raw::c_void> = Vec::new();
    for col in 0..natts {
        if !want_mask[col] {
            isnull[col] = true; // projected away — never materialized, never read by any upper node
            continue;
        }
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
///
/// M149 — `want_mask` (length `natts`) selects which columns to materialize. For a masked-out column the
/// per-chunk `read_chunked` + zstd decode is SKIPPED (the ~7% decode win) and its `cgcols` slot is left empty
/// (a placeholder never indexed — `form_row` writes NULL for it). `want_mask` all-true reproduces the exact
/// pre-M149 all-columns decode.
unsafe fn decode_stripe(
    rel: pg_sys::Relation,
    header_block: u32,
    tupdesc: pg_sys::TupleDesc,
    cols: &[ColDesc],
    natts: usize,
    out: &mut Vec<Vec<u8>>,
    want_mask: &[bool],
    predicates: &[super::zonemap::ZonePredicate],
    skip: bool,
) -> Result<(), String> {
    let hdr_items = super::page::read_all_page_items(rel, header_block)?;
    let hdr_bytes =
        hdr_items.into_iter().next().ok_or("theodb_columnar: stripe header page empty")?;
    let header = StripeHeader::from_bytes(&hdr_bytes)?;
    if header.ncols as usize != natts {
        return Err(format!(
            "theodb_columnar: stripe ncols {} != relation natts {natts}",
            header.ncols
        ));
    }
    let dir_bytes = super::page::read_chunked(rel, header.dir_first_block, header.dir_n_pages)?;
    if dir_bytes.len() < header.dir_len as usize {
        return Err("theodb_columnar: stripe directory truncated on disk".into());
    }
    let n_entries = header.n_chunk_groups as usize * natts;
    let entries = codec::deserialize_directory(&dir_bytes, n_entries)?;
    for cg in 0..header.n_chunk_groups as usize {
        let cg_rows = entries[cg * natts].row_count as usize;
        // M150 — zone-map chunk-group skip in the GENERAL scan path (mirror of `decode_columns`'s agg-path skip,
        // ADR D3). Skip the WHOLE chunk group (never a single column — the row cursor advances per chunk group, so
        // a partial skip would misalign) when any pushed `col op const` predicate's min/max PROVES no row can
        // match. Fail-safe: `p.col < natts` guards OOB, `chunk_can_match` returns "must scan" on `has_minmax=false`
        // / `MinMaxKind::None` / NaN. The skip is an ADMISSION filter — `ExecScan` re-checks the full qual over the
        // surviving rows (the final authority), so the result is byte-identical to skip-off (A/B gate, Rule 5).
        SKIP_STATS.with(|s| {
            let (sk, sc) = s.get();
            s.set((sk, sc + 1));
        });
        if skip
            && predicates.iter().any(|p| {
                p.col < natts && {
                    let e = &entries[cg * natts + p.col];
                    !super::zonemap::chunk_can_match(
                        e.has_minmax,
                        e.min_bits,
                        e.max_bits,
                        cols[p.col].mm,
                        p,
                    )
                }
            })
        {
            SKIP_STATS.with(|s| {
                let (sk, sc) = s.get();
                s.set((sk + 1, sc));
            });
            continue;
        }
        let mut cgcols: Vec<Vec<Option<Vec<u8>>>> = Vec::with_capacity(natts);
        for col in 0..natts {
            if !want_mask[col] {
                cgcols.push(Vec::new()); // projected away — skip read_chunked/zstd; placeholder never indexed
                continue;
            }
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
            out.push(form_row(tupdesc, cols, &cgcols, r, want_mask)?);
        }
    }
    Ok(())
}

// M104 (Q2): the eager `materialize_rows` (decode ALL visible stripes + pending into one Vec up front) was replaced
// by the lazy one-stripe-at-a-time `load_next_batch` (peak memory O(one stripe), not O(the whole table)). The MVCC
// visibility model is unchanged (the stripe set is resolved once at scan_begin under the scan snapshot; pending rows
// are the same-xact tail); the row ORDER (stripes in catalog order, then pending) is byte-identical.

/// M100 — resolve a column name to its 0-based attribute index (for projection pushdown). Returns None if absent.
pub(crate) unsafe fn column_index(rel: pg_sys::Relation, name: &str) -> Option<usize> {
    let tupdesc = (*rel).rd_att;
    let natts = (*tupdesc).natts as usize;
    (0..natts).find(|&i| {
        std::ffi::CStr::from_ptr((*super::tupdesc_attr(tupdesc, i)).attname.data.as_ptr())
            .to_string_lossy()
            == name
    })
}

/// M100 — decode visible stripes (+ this backend's pending rows) into per-column value vectors, BEFORE the row
/// transposition — the input the DataFusion Arrow batch builder needs (`df_executor.rs`). Returns one tuple per
/// RETURNED column: (name, atttypid, values[row] = Some(stored bytes) | None). `projection = Some(&[col_idx])`
/// decodes + returns ONLY those columns (projection pushdown — skips `read_chunked`/zstd on unprojected columns, the
/// columnar performance lever); `None` returns all. The stored bytes are the codec encoding (fixed: attlen LE bytes;
/// varlena: logical payload) — `df_executor` maps them to Arrow arrays.
///
/// BOUNDARY (M104): this is the INTENTIONAL, documented columnar-READ API — the one seam through which higher-level
/// consumers (`df_executor` M100, `vindex` M103) read column bytes; it is NOT an internal leak. The raw-bytes shape
/// is deliberate: the caller (Arrow/vector layer) owns the type interpretation, keeping the codec (`columnar_codec`)
/// free of Arrow/vector dependencies. A narrower typed accessor would re-encode the same bytes for each consumer;
/// this single read seam is the DRY boundary. Consumers MUST go through here (never `read_chunked` directly).
/// M160 — a decoded column in one of two shapes. `Cells` is the legacy per-cell boxed representation (nullable /
/// varlena / text / pending-present). `FixedRaw` is the M160 fast path: the contiguous little-endian value stream of a
/// NON-NULL fixed-width column (the whole zstd-decompressed buffer concatenated across chunk-groups), which `build_arrow`
/// turns into an Arrow `PrimitiveArray` via ONE typed `Vec<T>` (no per-cell `Vec<u8>` alloc storm — the M148-twin cost
/// the deep-dive flamegraph measured on the pushdown path). `width` is the fixed byte width (matches the Arrow native
/// buffer layout); `row_count` is the number of values (`bytes.len() == width * row_count`, asserted by build_arrow).
pub(crate) enum DecodedColumn {
    Cells(Vec<Option<Vec<u8>>>),
    FixedRaw { bytes: Vec<u8>, width: usize, row_count: usize },
}

/// M160 — is `typid` a fixed-width type whose stored little-endian bytes are byte-identical to the Arrow native
/// primitive buffer, so it can take the zero-copy `FixedRaw` fast path? Excludes bool (Arrow bit-packs it) and all
/// varlena/text. Widths match `decode_column`'s `attlen_fixed` and `build_arrow`'s per-type readers.
pub(crate) fn fixed_arrow_width(typid: u32) -> Option<usize> {
    match typid {
        21 => Some(2),                 // int2
        23 | 700 | 1082 => Some(4),    // int4 / float4 / date (Date32)
        20 | 701 | 1114 | 1184 => Some(8), // int8 / float8 / timestamp / timestamptz (all i64/f64)
        _ => None,                     // bool (bit-packed), varlena/text → cell path
    }
}

/// M160 fast decode for the pushdown path (`decode_to_batch`). Same stripe-walk + zone-map skip + directory contract as
/// `decode_columns`, but a wanted column whose type is `fixed_arrow_width` AND has NO nulls in ANY visible chunk-group
/// (decided from the directory in a cheap first pass) AND has no same-xact pending rows accumulates as `FixedRaw` (one
/// bulk `extend_from_slice` per chunk-group — O(bytes), not O(cells)); every other column stays `Cells` (fail-safe,
/// byte-identical to `decode_columns`). Returns `(name, typid, DecodedColumn)` per wanted column, in `wanted` order.
/// One visible stripe's directory, plus how many chunk-groups it holds. Lifted to module scope (it used to be
/// declared inside `decode_columns_v2`) so the per-chunk-group decode can be a shared helper — M168 needs the same
/// unit both accumulated (the existing path) and streamed (the O(k) top-k path).
struct StripePlan {
    entries: Vec<codec::ChunkDirEntry>,
    n_chunk_groups: usize,
}

/// Everything a chunk-group decode needs that does NOT change between chunk-groups. Bundled so the helper takes a
/// context rather than eleven positional arguments.
struct CgDecodeCtx<'a> {
    rel: pg_sys::Relation,
    natts: usize,
    wanted: &'a [usize],
    cols: &'a [ColDesc],
    mode_fixed: &'a [Option<usize>],
    predicates: &'a [super::zonemap::ZonePredicate],
    skip: bool,
}

/// Decode ONE chunk-group into the caller's accumulators.
///
/// This is the unit of work the columnar decode has always done — it was inlined in `decode_columns_v2`'s nested
/// loop. Extracting it changes nothing about *how* a chunk-group is decoded; it changes only who owns the
/// accumulators. `decode_columns_v2` passes buffers that live across every chunk-group (so the whole relation lands
/// in one batch, the O(N) behaviour M167 measured at 772 MiB); the M168 streaming path passes buffers it resets and
/// drains per chunk-group, which is what makes the peak independent of N.
///
/// Returns `Ok(false)` when the zone-map proved this chunk-group cannot match (nothing was read or decoded).
unsafe fn decode_one_chunk_group(
    ctx: &CgDecodeCtx,
    pl: &StripePlan,
    cg: usize,
    fixed_bytes: &mut [Vec<u8>],
    fixed_rows: &mut [usize],
    cell_cols: &mut [Vec<Option<Vec<u8>>>],
) -> Result<bool, String> {
    let natts = ctx.natts;
    let cg_rows = pl.entries[cg * natts].row_count as usize;
    if ctx.skip
        && ctx.predicates.iter().any(|p| {
            p.col < natts && {
                let e = &pl.entries[cg * natts + p.col];
                !super::zonemap::chunk_can_match(
                    e.has_minmax,
                    e.min_bits,
                    e.max_bits,
                    ctx.cols[p.col].mm,
                    p,
                )
            }
        })
    {
        return Ok(false);
    }
    for (wi, &col) in ctx.wanted.iter().enumerate() {
        let e = &pl.entries[cg * natts + col];
        let comp = super::page::read_chunked(ctx.rel, e.first_block, e.n_pages)?;
        if comp.len() < e.comp_len as usize {
            return Err("theodb_columnar: column chunk truncated on disk".into());
        }
        let raw = zstd::decode_all(&comp[..e.comp_len as usize])
            .map_err(|x| format!("theodb_columnar: zstd decode failed: {x}"))?;
        match ctx.mode_fixed[wi] {
            Some(w) => {
                // FixedRaw: has_nulls=false ⇒ the whole `raw` is the dense contiguous LE value stream.
                let expect = w * cg_rows;
                if raw.len() != expect {
                    return Err(format!(
                        "theodb_columnar: fixed chunk size {} != {w}*{cg_rows} (col {col})",
                        raw.len()
                    ));
                }
                fixed_bytes[wi].extend_from_slice(&raw); // one bulk copy per chunk-group (O(bytes))
                fixed_rows[wi] += cg_rows;
            }
            None => {
                let mut vals =
                    codec::decode_column(&raw, ctx.cols[col].attlen_fixed, cg_rows, e.has_nulls)?;
                cell_cols[wi].append(&mut vals);
            }
        }
    }
    Ok(true)
}

/// A columnar scan planned but not yet decoded: stripe directories, the projection, and the per-column FixedRaw
/// decision. Pass 1 of the decode — cheap (headers + directories only, no value chunks).
///
/// Extracted so the accumulating path (`decode_columns_v2`) and the M168 streaming path share ONE definition of
/// "what this scan is". Duplicating pass 1 would duplicate knowledge, and the FixedRaw decision in particular is
/// subtle: a column is only eligible if NO chunk-group anywhere has nulls for it, which is a whole-relation
/// property that a per-chunk-group loop cannot rediscover.
pub(crate) struct ScanPlan {
    plans: Vec<StripePlan>,
    wanted: Vec<usize>,
    cols: Vec<ColDesc>,
    mode_fixed: Vec<Option<usize>>,
    names: Vec<String>,
    natts: usize,
}

impl ScanPlan {
    /// Total chunk-groups across every visible stripe — the number of batches a streaming consumer will see
    /// (minus whatever the zone-map skips).
    pub(crate) fn n_chunk_groups(&self) -> usize {
        self.plans.iter().map(|p| p.n_chunk_groups).sum()
    }
}

/// Pass 1: plan the scan without decoding a single value chunk.
pub(crate) unsafe fn plan_columnar_scan(
    rel: pg_sys::Relation,
    projection: Option<&[usize]>,
) -> Result<ScanPlan, String> {
    let tupdesc = (*rel).rd_att;
    let natts = (*tupdesc).natts as usize;
    let cols = (0..natts).map(|i| coldesc(tupdesc, i)).collect::<Result<Vec<_>, _>>()?;
    let wanted: Vec<usize> = match projection {
        Some(p) => {
            for &i in p {
                if i >= natts {
                    return Err(format!(
                        "theodb_columnar: projection column {i} out of range (natts {natts})"
                    ));
                }
            }
            p.to_vec()
        }
        None => (0..natts).collect(),
    };
    let name_of = |i: usize| -> String {
        std::ffi::CStr::from_ptr((*super::tupdesc_attr(tupdesc, i)).attname.data.as_ptr())
            .to_string_lossy()
            .into_owned()
    };
    let mut plans: Vec<StripePlan> = Vec::new();
    for sm in read_visible_stripes((*rel).rd_id)? {
        let hdr_items = super::page::read_all_page_items(rel, sm.header_block)?;
        let header_bytes =
            hdr_items.into_iter().next().ok_or("theodb_columnar: stripe header page empty")?;
        let header = StripeHeader::from_bytes(&header_bytes)?;
        if header.ncols as usize != natts {
            return Err(format!("theodb_columnar: stripe ncols {} != natts {natts}", header.ncols));
        }
        let dir_bytes = super::page::read_chunked(rel, header.dir_first_block, header.dir_n_pages)?;
        let entries =
            codec::deserialize_directory(&dir_bytes, header.n_chunk_groups as usize * natts)?;
        plans.push(StripePlan { entries, n_chunk_groups: header.n_chunk_groups as usize });
    }
    let fast_decode = super::columnar_agg::ENABLE_FAST_DECODE.get();
    let mode_fixed: Vec<Option<usize>> = wanted
        .iter()
        .map(|&col| {
            if !fast_decode {
                return None;
            }
            let w = fixed_arrow_width(cols[col].typid)?;
            let any_null = plans
                .iter()
                .any(|pl| (0..pl.n_chunk_groups).any(|cg| pl.entries[cg * natts + col].has_nulls));
            if any_null { None } else { Some(w) }
        })
        .collect();
    let names = wanted.iter().map(|&c| name_of(c)).collect();
    Ok(ScanPlan { plans, wanted, cols, mode_fixed, names, natts })
}

/// The backend thread that a raw-`Relation` holder was created on.
///
/// M168 ADR-2. `ColumnarChunkStream` holds a `pg_sys::Relation` and is handed to DataFusion, whose
/// `PartitionStream` trait demands `Send + Sync`. That `unsafe impl` is TRUE under the executor's
/// `new_current_thread` runtime with `target_partitions(1)`: the stream is polled on the backend thread and
/// nowhere else. But it is true by configuration, not by construction — someone switching to `new_multi_thread`
/// would get silent memory corruption rather than a compile error, because PostgreSQL relation access is not
/// thread-safe.
///
/// So the invariant is asserted rather than commented. Precedent in this project: M139 found Tantivy calling
/// `Directory` from four threads, and the fix was to make the constraint explicit instead of hoping.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ThreadAffinity(std::thread::ThreadId);

impl ThreadAffinity {
    pub(crate) fn capture() -> Self {
        Self(std::thread::current().id())
    }

    /// Panics if called from any thread other than the one that captured it.
    pub(crate) fn assert_owned(&self, what: &str) {
        let now = std::thread::current().id();
        assert_eq!(
            self.0, now,
            "{what}: touched from {now:?} but owned by {:?}. A pg_sys::Relation is only valid on its backend \
             thread; this means the DataFusion runtime is no longer single-threaded (see M168 ADR-2).",
            self.0
        );
    }
}

/// Streams a planned columnar scan ONE chunk-group at a time.
///
/// This is the M168 answer to the M167 DoD bullet 2b. `decode_columns_v2` hands the same helper accumulators that
/// live across the whole relation, so its peak is O(N) — measured at 809,738,352 bytes for ClickBench q23. This
/// type hands the helper accumulators it drains after every chunk-group, so the peak is one chunk-group.
///
/// It holds a raw `pg_sys::Relation` and therefore MUST be advanced only on the backend thread that created it.
/// That is not a comment: `next` asserts it (see `ThreadAffinity`), turning a runtime/threading change from silent
/// memory corruption into an immediate panic.
pub(crate) struct ColumnarChunkStream {
    plan: ScanPlan,
    rel: pg_sys::Relation,
    affinity: ThreadAffinity,
    pl_idx: usize,
    cg_idx: usize,
    skipped: usize,
    emitted: usize,
}

impl ColumnarChunkStream {
    pub(crate) fn new(rel: pg_sys::Relation, plan: ScanPlan) -> Self {
        Self {
            plan,
            rel,
            affinity: ThreadAffinity::capture(),
            pl_idx: 0,
            cg_idx: 0,
            skipped: 0,
            emitted: 0,
        }
    }

    pub(crate) fn column_names(&self) -> &[String] {
        &self.plan.names
    }

    pub(crate) fn column_typids(&self) -> Vec<u32> {
        self.plan.wanted.iter().map(|&c| self.plan.cols[c].typid).collect()
    }

    pub(crate) fn stats(&self) -> (usize, usize) {
        (self.emitted, self.skipped)
    }

    /// Decode the next non-skipped chunk-group. `Ok(None)` = the scan is exhausted.
    ///
    /// SAFETY: the caller guarantees this runs on the backend thread that owns `rel` (see the type's doc). Every
    /// buffer is freshly allocated per call and moved out, so nothing accumulates across chunk-groups.
    pub(crate) unsafe fn next(
        &mut self,
        predicates: &[super::zonemap::ZonePredicate],
        skip: bool,
    ) -> Result<Option<Vec<(String, u32, DecodedColumn)>>, String> {
        self.affinity.assert_owned("ColumnarChunkStream::next");
        let n = self.plan.wanted.len();
        let ctx = CgDecodeCtx {
            rel: self.rel,
            natts: self.plan.natts,
            wanted: &self.plan.wanted,
            cols: &self.plan.cols,
            mode_fixed: &self.plan.mode_fixed,
            predicates,
            skip,
        };
        while self.pl_idx < self.plan.plans.len() {
            let pl = &self.plan.plans[self.pl_idx];
            if self.cg_idx >= pl.n_chunk_groups {
                self.pl_idx += 1;
                self.cg_idx = 0;
                continue;
            }
            let cg = self.cg_idx;
            self.cg_idx += 1;
            // Fresh per chunk-group — this is the whole point. `decode_columns_v2` reuses buffers across every
            // chunk-group; draining them here is what makes the peak independent of N.
            let mut fixed_bytes: Vec<Vec<u8>> = vec![Vec::new(); n];
            let mut fixed_rows: Vec<usize> = vec![0; n];
            let mut cell_cols: Vec<Vec<Option<Vec<u8>>>> = vec![Vec::new(); n];
            if !decode_one_chunk_group(
                &ctx,
                pl,
                cg,
                &mut fixed_bytes,
                &mut fixed_rows,
                &mut cell_cols,
            )? {
                self.skipped += 1;
                continue;
            }
            self.emitted += 1;
            let out = self
                .plan
                .wanted
                .iter()
                .enumerate()
                .map(|(wi, &col)| {
                    let dc = match self.plan.mode_fixed[wi] {
                        Some(w) => DecodedColumn::FixedRaw {
                            bytes: std::mem::take(&mut fixed_bytes[wi]),
                            width: w,
                            row_count: fixed_rows[wi],
                        },
                        None => DecodedColumn::Cells(std::mem::take(&mut cell_cols[wi])),
                    };
                    (self.plan.names[wi].clone(), self.plan.cols[col].typid, dc)
                })
                .collect();
            return Ok(Some(out));
        }
        Ok(None)
    }
}

pub(crate) unsafe fn decode_columns_v2(
    rel: pg_sys::Relation,
    projection: Option<&[usize]>,
    predicates: &[super::zonemap::ZonePredicate],
    skip: bool,
) -> Result<Vec<(String, u32, DecodedColumn)>, String> {
    let tupdesc = (*rel).rd_att;
    let natts = (*tupdesc).natts as usize;
    let cols = (0..natts).map(|i| coldesc(tupdesc, i)).collect::<Result<Vec<_>, _>>()?;
    let wanted: Vec<usize> = match projection {
        Some(p) => {
            for &i in p {
                if i >= natts {
                    return Err(format!(
                        "theodb_columnar: projection column {i} out of range (natts {natts})"
                    ));
                }
            }
            p.to_vec()
        }
        None => (0..natts).collect(),
    };
    let name_of = |i: usize| -> String {
        std::ffi::CStr::from_ptr((*super::tupdesc_attr(tupdesc, i)).attname.data.as_ptr())
            .to_string_lossy()
            .into_owned()
    };

    // Same-xact pending rows force the whole result onto the legacy cell path (fail-safe: merging FixedRaw bytes with
    // pending cell rows is out of M160 scope — pending is empty for a read-only benchmark query, the measured regime).
    let oid = (*rel).rd_id.to_u32();
    let has_pending = WRITE_STATES.with(|w| w.borrow().get(&oid).is_some_and(|p| !p.rows.is_empty()));
    if has_pending {
        return Ok(decode_columns(rel, projection, predicates, skip)?
            .into_iter()
            .map(|(n, t, v)| (n, t, DecodedColumn::Cells(v)))
            .collect());
    }

    // Pass 1 — read every visible stripe's header + directory (cheap; no value chunks) so we can (a) decide per-wanted
    // column whether it can take the FixedRaw fast path (fixed-width type AND no nulls anywhere) and (b) reuse the
    // directories in pass 2 without re-reading. Also carries the zone-map skip decision per chunk-group.
    let mut plans: Vec<StripePlan> = Vec::new();
    for sm in read_visible_stripes((*rel).rd_id)? {
        let hdr_items = super::page::read_all_page_items(rel, sm.header_block)?;
        let hdr_bytes =
            hdr_items.into_iter().next().ok_or("theodb_columnar: stripe header page empty")?;
        let header = StripeHeader::from_bytes(&hdr_bytes)?;
        if header.ncols as usize != natts {
            return Err(format!("theodb_columnar: stripe ncols {} != natts {natts}", header.ncols));
        }
        let dir_bytes = super::page::read_chunked(rel, header.dir_first_block, header.dir_n_pages)?;
        let entries =
            codec::deserialize_directory(&dir_bytes, header.n_chunk_groups as usize * natts)?;
        plans.push(StripePlan { entries, n_chunk_groups: header.n_chunk_groups as usize });
    }

    // Per-wanted-column mode: FixedRaw-eligible iff the M160 GUC is on AND the type is fixed-width AND no visible
    // chunk-group has nulls for it. GUC off ⇒ every column takes the legacy cell path (the A/B "before").
    let fast_decode = super::columnar_agg::ENABLE_FAST_DECODE.get();
    let mode_fixed: Vec<Option<usize>> = wanted
        .iter()
        .map(|&col| {
            if !fast_decode {
                return None;
            }
            let w = fixed_arrow_width(cols[col].typid)?;
            let any_null = plans.iter().any(|pl| {
                (0..pl.n_chunk_groups).any(|cg| pl.entries[cg * natts + col].has_nulls)
            });
            if any_null { None } else { Some(w) }
        })
        .collect();

    // Accumulators: FixedRaw columns get a byte buffer + a running row count; cell columns get the boxed vec.
    let mut fixed_bytes: Vec<Vec<u8>> = vec![Vec::new(); wanted.len()];
    let mut fixed_rows: Vec<usize> = vec![0; wanted.len()];
    let mut cell_cols: Vec<Vec<Option<Vec<u8>>>> = vec![Vec::new(); wanted.len()];
    let (mut skipped_cg, mut total_cg) = (0usize, 0usize);

    // Pass 2 — decode value chunks (the expensive part, done once), routing per column mode.
    let ctx = CgDecodeCtx {
        rel,
        natts,
        wanted: &wanted,
        cols: &cols,
        mode_fixed: &mode_fixed,
        predicates,
        skip,
    };
    for pl in &plans {
        for cg in 0..pl.n_chunk_groups {
            total_cg += 1;
            // Accumulators live across every chunk-group here — that is exactly what makes this path O(N) in the
            // decoded batch. The streaming caller passes per-chunk-group buffers to the same helper instead.
            if !decode_one_chunk_group(
                &ctx,
                pl,
                cg,
                &mut fixed_bytes,
                &mut fixed_rows,
                &mut cell_cols,
            )? {
                skipped_cg += 1;
            }
        }
    }
    if skip
        && !predicates.is_empty()
        && std::env::var("THEODB_SCAN_PROFILE").is_ok_and(|v| v == "1")
    {
        pgrx::log!("theodb_columnar zonemap: skipped {skipped_cg}/{total_cg} chunk groups");
    }

    Ok(wanted
        .iter()
        .enumerate()
        .map(|(wi, &col)| {
            let dc = match mode_fixed[wi] {
                Some(w) => DecodedColumn::FixedRaw {
                    bytes: std::mem::take(&mut fixed_bytes[wi]),
                    width: w,
                    row_count: fixed_rows[wi],
                },
                None => DecodedColumn::Cells(std::mem::take(&mut cell_cols[wi])),
            };
            (name_of(col), cols[col].typid, dc)
        })
        .collect())
}

pub(crate) unsafe fn decode_columns(
    rel: pg_sys::Relation,
    projection: Option<&[usize]>,
    predicates: &[super::zonemap::ZonePredicate],
    skip: bool,
) -> Result<Vec<(String, u32, Vec<Option<Vec<u8>>>)>, String> {
    let tupdesc = (*rel).rd_att;
    let natts = (*tupdesc).natts as usize;
    let cols = (0..natts).map(|i| coldesc(tupdesc, i)).collect::<Result<Vec<_>, _>>()?;
    let wanted: Vec<usize> = match projection {
        Some(p) => {
            for &i in p {
                if i >= natts {
                    return Err(format!(
                        "theodb_columnar: projection column {i} out of range (natts {natts})"
                    ));
                }
            }
            p.to_vec()
        }
        None => (0..natts).collect(),
    };
    let name_of = |i: usize| -> String {
        std::ffi::CStr::from_ptr((*super::tupdesc_attr(tupdesc, i)).attname.data.as_ptr())
            .to_string_lossy()
            .into_owned()
    };

    // Per WANTED column, its accumulated values (indexed positionally with `wanted`).
    let mut columns: Vec<Vec<Option<Vec<u8>>>> = vec![Vec::new(); wanted.len()];
    let (mut skipped_cg, mut total_cg) = (0usize, 0usize); // zone-map skip-ratio metric (wiring pillar c)
    for sm in read_visible_stripes((*rel).rd_id)? {
        let hdr_items = super::page::read_all_page_items(rel, sm.header_block)?;
        let hdr_bytes =
            hdr_items.into_iter().next().ok_or("theodb_columnar: stripe header page empty")?;
        let header = StripeHeader::from_bytes(&hdr_bytes)?;
        if header.ncols as usize != natts {
            return Err(format!("theodb_columnar: stripe ncols {} != natts {natts}", header.ncols));
        }
        let dir_bytes = super::page::read_chunked(rel, header.dir_first_block, header.dir_n_pages)?;
        let entries =
            codec::deserialize_directory(&dir_bytes, header.n_chunk_groups as usize * natts)?;
        for cg in 0..header.n_chunk_groups as usize {
            let cg_rows = entries[cg * natts].row_count as usize;
            total_cg += 1;
            // Zone-map skip-pruning (ADR D3): skip the WHOLE chunk group (all wanted columns together → the value
            // vectors stay aligned) when any pushed predicate's min/max PROVES no row can match. Fail-safe —
            // `p.col < natts` guards OOB (EC-2), `chunk_can_match` returns "must scan" on `has_minmax=false`. The
            // skip is an admission filter: surviving over-admitted rows are still filtered by the executor's real
            // predicate (the final authority), so the aggregate is byte-identical to skip-off.
            if skip
                && predicates.iter().any(|p| {
                    p.col < natts && {
                        let e = &entries[cg * natts + p.col];
                        !super::zonemap::chunk_can_match(
                            e.has_minmax,
                            e.min_bits,
                            e.max_bits,
                            cols[p.col].mm,
                            p,
                        )
                    }
                })
            {
                skipped_cg += 1;
                continue;
            }
            for (wi, &col) in wanted.iter().enumerate() {
                let e = &entries[cg * natts + col];
                let comp = super::page::read_chunked(rel, e.first_block, e.n_pages)?;
                if comp.len() < e.comp_len as usize {
                    return Err("theodb_columnar: column chunk truncated on disk".into());
                }
                let raw = zstd::decode_all(&comp[..e.comp_len as usize])
                    .map_err(|x| format!("theodb_columnar: zstd decode failed: {x}"))?;
                let mut vals =
                    codec::decode_column(&raw, cols[col].attlen_fixed, cg_rows, e.has_nulls)?;
                columns[wi].append(&mut vals);
            }
        }
    }
    // Zone-map skip-ratio (wiring pillar c, opt-in THEODB_SCAN_PROFILE=1): how many chunk groups the predicate
    // pruned. `skipped/total` is the A/B evidence that the min/max directory is being consumed (0 when skip off).
    if skip
        && !predicates.is_empty()
        && std::env::var("THEODB_SCAN_PROFILE").is_ok_and(|v| v == "1")
    {
        pgrx::log!("theodb_columnar zonemap: skipped {skipped_cg}/{total_cg} chunk groups");
    }

    // Same-xact pending rows (heap-tuple bytes) → deform ALL atts (heap_deform is all-or-nothing), keep the wanted.
    let oid = (*rel).rd_id.to_u32();
    let pending: Option<Vec<Vec<u8>>> =
        WRITE_STATES.with(|w| w.borrow().get(&oid).map(|p| p.rows.clone()));
    if let Some(rows) = pending {
        let pend = deform_rows_into_columns(&rows, tupdesc, natts, &cols, &wanted)?;
        for (wi, col_rows) in pend.into_iter().enumerate() {
            columns[wi].extend(col_rows);
        }
    }

    Ok(wanted
        .iter()
        .enumerate()
        .map(|(wi, &col)| (name_of(col), cols[col].typid, std::mem::take(&mut columns[wi])))
        .collect())
}

/// Fold two min/max candidates in the `MinMaxKind` BIT domain (ints stored as `i64 as u64`, floats as `f64::to_bits`).
/// NEVER a raw `u64` compare — negatives would order as huge (columnar-minmax blueprint trap B).
fn fold_minmax_bits(a: u64, b: u64, mm: codec::MinMaxKind, want_max: bool) -> u64 {
    use codec::MinMaxKind::*;
    let pick_b = match mm {
        // I2/I4/I8/Bool + temporal (timestamp→I8, date→I4): compare in the signed integer domain.
        I2 | I4 | I8 | Bool => {
            let (ai, bi) = (a as i64, b as i64);
            if want_max { bi > ai } else { bi < ai }
        }
        // Only float MIN reaches the fold (float MAX is gated out; NaN groups are has_minmax=false → excluded).
        F4 | F8 => {
            let (af, bf) = (f64::from_bits(a), f64::from_bits(b));
            if want_max { bf > af } else { bf < af }
        }
        None => false,
    };
    if pick_b { b } else { a }
}

/// Decode a folded min/max bit value into the column's native PG datum (reverse of `encode_const_bits`). I4 covers
/// int4 AND date (both 4-byte by-value); I8 covers int8 AND timestamp/timestamptz (8-byte by-value μs).
unsafe fn decode_minmax_datum(bits: u64, mm: codec::MinMaxKind) -> Result<pg_sys::Datum, String> {
    use codec::MinMaxKind::*;
    let d = match mm {
        I2 => (bits as i64 as i16).into_datum(),
        I4 => (bits as i64 as i32).into_datum(),
        I8 => (bits as i64).into_datum(),
        Bool => (bits != 0).into_datum(),
        F4 => (f64::from_bits(bits) as f32).into_datum(),
        F8 => f64::from_bits(bits).into_datum(),
        None => return Err("directory_minmax: unordered kind".into()),
    };
    d.ok_or_else(|| "directory_minmax: null min/max datum".into())
}

/// M-minmax Phase B — answer a scalar `min(col)`/`max(col)` (no WHERE, no GROUP BY) by folding the zone-map directory
/// `min_bits`/`max_bits` over the VISIBLE stripes + the same-xact pending rows, WITHOUT decoding any column chunk.
/// Returns `Ok(Some(cell))` when the fast-path is byte-identical-safe, `Ok(None)` when the caller MUST fall back to the
/// full DataFusion scan (Phase A). Gating (verified by council-index-storage — see blueprint § 7 conditions):
///   - unordered type → None; `max` on a float kind → None (compute_minmax skips NaN, PG max returns NaN);
///   - any VISIBLE chunk-group with `has_minmax==false` on the column → None (all-NULL or NaN-float, indistinguishable);
///   - pending rows with non-null values but no usable min/max (all-NaN float min) → None.
/// Empty (no visible groups + no pending) → `Some(NULL)`. MVCC-correct: append-only + stripe-atomic visibility mean
/// every row of a visible stripe is visible, so folding the visible directory == the snapshot-visible min/max.
pub(crate) unsafe fn directory_minmax(
    rel: pg_sys::Relation,
    col_name: &str,
    typid: u32,
    want_max: bool,
) -> Result<Option<(pg_sys::Datum, bool)>, String> {
    let mm = minmax_kind_of(typid);
    if mm == codec::MinMaxKind::None {
        return Ok(None);
    }
    if want_max && matches!(mm, codec::MinMaxKind::F4 | codec::MinMaxKind::F8) {
        return Ok(None); // NaN gate: directory max_bits skipped NaN; native max(float) returns NaN
    }
    let tupdesc = (*rel).rd_att;
    let natts = (*tupdesc).natts as usize;
    let col_idx = match (0..natts).find(|&i| {
        std::ffi::CStr::from_ptr((*super::tupdesc_attr(tupdesc, i)).attname.data.as_ptr())
            .to_string_lossy()
            == col_name
    }) {
        Some(x) => x,
        None => return Ok(None),
    };

    // Fold the directory min/max over VISIBLE stripes only (snapshot-correct — never all physical stripes).
    let mut acc: Option<u64> = None;
    for sm in read_visible_stripes((*rel).rd_id)? {
        let hdr_items = super::page::read_all_page_items(rel, sm.header_block)?;
        let hdr_bytes =
            hdr_items.into_iter().next().ok_or("theodb_columnar: stripe header page empty")?;
        let header = StripeHeader::from_bytes(&hdr_bytes)?;
        if header.ncols as usize != natts {
            return Err(format!("theodb_columnar: stripe ncols {} != natts {natts}", header.ncols));
        }
        let dir_bytes = super::page::read_chunked(rel, header.dir_first_block, header.dir_n_pages)?;
        let entries =
            codec::deserialize_directory(&dir_bytes, header.n_chunk_groups as usize * natts)?;
        for cg in 0..header.n_chunk_groups as usize {
            let e = &entries[cg * natts + col_idx];
            if e.row_count == 0 {
                continue;
            }
            if !e.has_minmax {
                return Ok(None); // all-NULL or NaN-float group → fall back to the full scan (byte-safe)
            }
            let cand = if want_max { e.max_bits } else { e.min_bits };
            acc = Some(match acc {
                None => cand,
                Some(a) => fold_minmax_bits(a, cand, mm, want_max),
            });
        }
    }

    // Fold the same-xact pending rows (no directory entry) via compute_minmax — the identical bit domain.
    let oid = (*rel).rd_id.to_u32();
    let pending: Option<Vec<Vec<u8>>> =
        WRITE_STATES.with(|w| w.borrow().get(&oid).map(|p| p.rows.clone()));
    if let Some(rows) = pending {
        if !rows.is_empty() {
            let cols = (0..natts).map(|i| coldesc(tupdesc, i)).collect::<Result<Vec<_>, _>>()?;
            let colvals: Vec<Option<Vec<u8>>> =
                deform_rows_into_columns(&rows, tupdesc, natts, &cols, &[col_idx])?
                    .into_iter()
                    .next()
                    .unwrap_or_default();
            let (has, pmin, pmax) = codec::compute_minmax(&colvals, mm);
            if has {
                let cand = if want_max { pmax } else { pmin };
                acc = Some(match acc {
                    None => cand,
                    Some(a) => fold_minmax_bits(a, cand, mm, want_max),
                });
            } else if colvals.iter().any(|v| v.is_some()) {
                return Ok(None); // pending has non-null but no usable min/max (all-NaN float) → fall back
            }
            // else: all pending NULL → contributes nothing, keep acc
        }
    }

    match acc {
        None => Ok(Some((pg_sys::Datum::from(0usize), true))), // no visible/pending rows → SQL NULL
        Some(bits) => Ok(Some((decode_minmax_datum(bits, mm)?, false))),
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
        // Resolve the visible stripe SET once, under this scan's snapshot (MVCC-correct + fixed for the scan life).
        // Decode is deferred to getnextslot — peak memory is one stripe, not the whole table.
        let stripes = match read_visible_stripes((*rel).rd_id) {
            Ok(s) => s,
            Err(e) => pg_sys::error!("{e}"),
        };
        let scan = pg_sys::palloc0(size_of::<ColumnarScanState>()) as *mut ColumnarScanState;
        (*scan).base.rs_rd = rel;
        (*scan).base.rs_snapshot = snapshot;
        (*scan).base.rs_nkeys = nkeys;
        (*scan).base.rs_key = key;
        (*scan).base.rs_parallel = pscan;
        (*scan).base.rs_flags = flags;
        (*scan).stripes = Box::into_raw(Box::new(stripes));
        (*scan).stripe_idx = 0;
        (*scan).current = Box::into_raw(Box::new(Vec::new()));
        (*scan).cursor = 0;
        (*scan).pending_loaded = false;
        SKIP_STATS.with(|s| s.set((0, 0))); // M150 — fresh skip counters for this scan
        scan as pg_sys::TableScanDesc
    }
}

/// Decode the next batch into `(*st).current`: the next visible stripe (catalog order), or — when all stripes are
/// consumed — the same-xact WRITE_STATES pending rows as the FINAL batch. Returns false when there is nothing left.
/// The row order matches the old eager materialization exactly (byte-identical scan results).
unsafe fn load_next_batch(st: *mut ColumnarScanState) -> bool {
    unsafe {
        let rel = (*st).base.rs_rd;
        let tupdesc = (*rel).rd_att;
        let natts = (*tupdesc).natts as usize;
        let stripes = &*(*st).stripes;
        // free the previous batch
        drop(Box::from_raw((*st).current));
        let mut batch: Vec<Vec<u8>> = Vec::new();
        let loaded_a_source;
        if (*st).stripe_idx < stripes.len() {
            let hb = stripes[(*st).stripe_idx].header_block;
            (*st).stripe_idx += 1;
            let cols = match (0..natts).map(|i| coldesc(tupdesc, i)).collect::<Result<Vec<_>, _>>()
            {
                Ok(c) => c,
                Err(e) => pg_sys::error!("{e}"),
            };
            // M149 — the projection for THIS scan (keyed by the scandesc pointer == `st`). `None` (no projection
            // node, or fallback whole-row/system-col) ⇒ decode ALL columns (the exact pre-M149 plain path). A
            // nested/unrelated columnar scan sees `None` here because the key is this scan's descriptor.
            let want_mask: Vec<bool> =
                match crate::am::columnar_project::scan_projection(st as usize) {
                    Some(w) => {
                        let mut m = vec![false; natts];
                        for &c in w.iter() {
                            if c < natts {
                                m[c] = true;
                            }
                        }
                        m
                    }
                    None => vec![true; natts],
                };
            // M150 — the zone-map predicates pushed for THIS scan (keyed by the scandesc pointer == `st`, the same
            // side-channel discipline as the projection above). `None` (no projection node, no pushable qual, or a
            // nested/unrelated scan) ⇒ empty slice ⇒ no chunk-group is skipped (the exact pre-M150 path). Gated by
            // `theodb.enable_chunk_skip` so the A/B OFF-vs-ON ablation isolates the skip on the SAME binary.
            let preds: Vec<super::zonemap::ZonePredicate> =
                match crate::am::columnar_project::scan_predicates(st as usize) {
                    Some(p) => (*p).clone(),
                    None => Vec::new(),
                };
            let skip = crate::am::columnar_project::ENABLE_CHUNK_SKIP.get() && !preds.is_empty();
            if let Err(e) =
                decode_stripe(rel, hb, tupdesc, &cols, natts, &mut batch, &want_mask, &preds, skip)
            {
                pg_sys::error!("{e}");
            }
            loaded_a_source = true;
        } else if !(*st).pending_loaded {
            (*st).pending_loaded = true;
            let oid = (*rel).rd_id.to_u32();
            WRITE_STATES.with(|w| {
                if let Some(p) = w.borrow().get(&oid) {
                    batch = p.rows.clone();
                }
            });
            loaded_a_source = true;
        } else {
            loaded_a_source = false; // all stripes consumed + pending drained → terminal
        }
        (*st).current = Box::into_raw(Box::new(batch));
        (*st).cursor = 0;
        loaded_a_source
    }
}

#[pg_guard]
pub unsafe extern "C-unwind" fn columnar_scan_end(scan: pg_sys::TableScanDesc) {
    unsafe {
        if !scan.is_null() {
            let st = scan as *mut ColumnarScanState;
            if !(*st).stripes.is_null() {
                drop(Box::from_raw((*st).stripes));
            }
            if !(*st).current.is_null() {
                drop(Box::from_raw((*st).current)); // free the current stripe batch
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
        // reset ALL cursors so the scan restarts from the first stripe (R3 — a partial reset would skip rows).
        let st = scan as *mut ColumnarScanState;
        (*st).stripe_idx = 0;
        (*st).pending_loaded = false;
        drop(Box::from_raw((*st).current));
        (*st).current = Box::into_raw(Box::new(Vec::new()));
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
        // Advance to a batch that has a row to emit (decoding the next stripe / the pending tail lazily). An
        // empty stripe is skipped by looping; when no source remains, the scan is done.
        while (*st).cursor >= (&*(*st).current).len() {
            if !load_next_batch(st) {
                pg_sys::ExecClearTuple(slot);
                return false;
            }
        }
        let current = &*(*st).current;
        let bytes = &current[(*st).cursor];
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
        let natts = (*tupdesc).natts as usize;

        // #190 (ADR-1) — MATERIALISE every varlena HERE, where the executor's snapshot is guaranteed.
        //
        // `heap_form_tuple` keeps an out-of-line TOAST value as an 18-byte pointer. That pointer used to be
        // resolved much later, inside `flush_pending`, which also runs from the pre-commit XACT callback —
        // where there is NO active snapshot. Result: `cannot fetch toast data without an active snapshot`,
        // and no real workload could be loaded at all. Detoasting at ingestion removes the dependency rather
        // than borrowing a snapshot at flush time: it also closes the window where a long transaction's
        // buffered pointer outlives the toast chunk it references (VACUUM), and immunises every future
        // flush call-site by construction.
        //
        // Materialise EVERY non-null varlena, with no `VARATT_IS_EXTERNAL` pre-test: arrays reach the
        // executor in *expanded* form (`VARATT_IS_EXTERNAL_EXPANDED`) — a pointer into per-tuple memory that
        // is reset each row. Skipping those would leave a dangling pointer in the buffer (use-after-free),
        // strictly worse than the bug being fixed. `pg_detoast_datum_copy` flattens external, compressed and
        // expanded alike, and always returns a fresh palloc we own.
        //
        // The `attlen != -1` guard mirrors `extract_value_bytes` (fixed-length types are read by length, and
        // a by-value datum treated as a pointer is a segfault); `tts_isnull` is checked first because
        // detoasting a NULL is likewise a segfault.
        let isnull = std::slice::from_raw_parts((*slot).tts_isnull, natts);
        // NEVER mutate `(*slot).tts_values` (#190 v2 — use-after-free fix). The executor reads the SAME
        // slot AFTER `table_tuple_insert` returns — `ExecInsertIndexTuples`, `ExecARInsertTriggers`, and
        // `ExecProcessReturning` all project from it (PG18 `nodeModifyTable.c`). Because we called
        // `slot_getallattrs` above (`tts_nvalid == natts`), `slot_getattr` hands those readers the cached
        // `tts_values[i]` WITHOUT re-deforming — so if we point the slot at a flat we `pfree` below, an
        // `INSERT ... RETURNING <varlena col>` (or an AFTER-ROW trigger) dereferences freed memory. Detoast
        // into a LOCAL copy of the datum array and form the tuple from that; the slot stays pristine.
        let mut form_values: Vec<pg_sys::Datum> =
            std::slice::from_raw_parts((*slot).tts_values, natts).to_vec();
        let mut owned: Vec<*mut pg_sys::varlena> = Vec::new();
        for i in 0..natts {
            if isnull[i] {
                continue;
            }
            // `attlen == -1` is the varlena marker — the SAME criterion `coldesc` uses to build
            // `attlen_fixed`, so ingestion and flush agree on what is a varlena.
            if (*super::tupdesc_attr(tupdesc, i)).attlen != -1 {
                continue; // fixed-length: nothing to detoast
            }
            let flat = pg_sys::pg_detoast_datum_copy(form_values[i].cast_mut_ptr::<pg_sys::varlena>());
            owned.push(flat);
            form_values[i] = pg_sys::Datum::from(flat);
        }

        // ORDER IS LOAD-BEARING (#190): form the tuple and copy its bytes BEFORE freeing the flattened
        // copies. `heap_form_tuple` copies the values into the new tuple, and `bytes` copies again into the
        // buffer; freeing earlier would build the row from freed memory — silent corruption, no error.
        let htup = pg_sys::heap_form_tuple(tupdesc, form_values.as_mut_ptr(), (*slot).tts_isnull);
        let len = (*htup).t_len as usize;
        let bytes = std::slice::from_raw_parts((*htup).t_data as *const u8, len).to_vec();
        pg_sys::heap_freetuple(htup);
        for flat in owned {
            pg_sys::pfree(flat as *mut std::os::raw::c_void);
        }
        let oid = (*rel).rd_id.to_u32();

        // M104 (#99): flush a stripe once the pending set exceeds `maintenance_work_mem`, so a big
        // INSERT...SELECT holds O(maintenance_work_mem) — not O(rows-in-xact) — in RAM. `flush_pending` is
        // the SAME atomic pages→catalog-row-LAST unit used at pre-commit. Every stripe carries this xact's
        // xid, so a crash/abort mid-multi-stripe INSERT leaves ALL stripes invisible (H3, by construction).
        //
        // #190: the budget check happens BEFORE the row is pushed. It used to run after, which was harmless
        // while an out-of-line value occupied 18 bytes in the buffer — but rows are materialised now, so a
        // single wide value (varlena allows up to 1 GB) would be copied in whole before any check, and the
        // flush would only fire on the NEXT row. That would trade a loud INSERT error for a backend OOM.
        // A row that is bigger than the budget on its own still goes through (it cannot be split), but it
        // never piles on top of the rows already buffered.
        let mwm = (pg_sys::maintenance_work_mem as usize).saturating_mul(1024).max(1);
        let needs_flush_first = WRITE_STATES.with(|w| {
            let m = w.borrow();
            m.get(&oid)
                .is_some_and(|p| !p.rows.is_empty() && p.bytes.saturating_add(bytes.len()) > mwm)
        });
        if needs_flush_first {
            if let Err(e) = flush_pending(rel) {
                pg_sys::error!("{e}");
            }
        }

        // Accumulate the row + its (materialised) bytes; report the pending total for the next decision.
        let pending_bytes = WRITE_STATES.with(|w| {
            let mut m = w.borrow_mut();
            let p = m.entry(oid).or_default();
            p.bytes += bytes.len();
            p.rows.push(bytes);
            p.bytes
        });
        // A single row above the budget: flush it right away so the buffer returns to empty.
        if pending_bytes > mwm {
            if let Err(e) = flush_pending(rel) {
                pg_sys::error!("{e}");
            }
        }
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
    let rows = WRITE_STATES.with(|w| w.borrow_mut().remove(&oid).map(|p| p.rows));
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
        let all: Vec<usize> = (0..natts).collect();
        let columns = deform_rows_into_columns(&rows, tupdesc, natts, &cols, &all)?;

        // Encode + write each (chunk_group, column) chunk; build the directory in grid order [cg][col].
        let n_cg = row_count.div_ceil(codec::CHUNK_GROUP_ROWS);
        let mut dir = Vec::with_capacity(n_cg * natts);
        for cg in 0..n_cg {
            let lo = cg * codec::CHUNK_GROUP_ROWS;
            let hi = (lo + codec::CHUNK_GROUP_ROWS).min(row_count);
            for col in 0..natts {
                let enc = codec::encode_column(
                    &columns[col][lo..hi],
                    cols[col].attlen_fixed,
                    cols[col].mm,
                );
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
        let header_block = super::page::extend_page_with_item(
            rel,
            pg_sys::ForkNumber::MAIN_FORKNUM,
            &header.to_bytes(),
        );

        // Publish the stripe LAST via its heap-catalog row (the MVCC visibility root) — after every referenced data
        // page is durable and every buffer lock is released. Its xmin ties visibility to this xact's commit.
        insert_stripe_row(
            (*rel).rd_id,
            stripe_id,
            header_block,
            row_count as u32,
            base as i64,
            natts as i16,
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
pub unsafe extern "C-unwind" fn columnar_relation_needs_toast_table(
    _rel: pg_sys::Relation,
) -> bool {
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

// HISTORY (#143): a note used to sit here claiming these stubs were "intentionally NOT `#[pg_guard]`" because
// `pg_sys::error!` was "a C ereport/siglongjmp ... so there is no Rust unwind to guard against". That belief was
// FALSE and it cost us a postmaster abort on `CREATE INDEX` over a columnar table: in pgrx 0.19 a PG ERROR is
// raised as `panic_any`, which unwinds. The accurate rationale — including why the `#[pg_guard]` attribute cannot
// be used from inside a `macro_rules!` — now lives in the macro body itself, next to the guard it explains.
macro_rules! columnar_unsupported {
    ($name:ident ( $($arg:ident : $ty:ty),* $(,)? ) $( -> $ret:ty )? , $msg:literal) => {
        // M135/#143 — `#[pg_guard]` is LOAD-BEARING here, not decoration. In pgrx 0.19 a PG `ERROR` is raised as
        // `panic_any` (pgrx-pg-sys panic.rs:155-160), so Rust frames unwind before `ereport` fires at a guard
        // boundary. These stubs are called DIRECTLY by PostgreSQL C code (table-AM callbacks); without a guard
        // frame the unwinder walks off the end of the stack — `_URC_END_OF_STACK`, reported as
        // "failed to initiate panic, error 5" — and aborts the whole postmaster. `CREATE INDEX` on a columnar
        // table was a 3-statement server crash. The macro generates 30 callbacks, so the omission was 30 latent
        // crashes; the file's own header (line 17) already stated the rule this macro was the one place to skip.
        pub unsafe extern "C-unwind" fn $name( $( $arg : $ty ),* ) $( -> $ret )? {
            let _ = ( $( &$arg ),* );
            // The guard is applied HERE, as an explicit call, rather than via `#[pg_guard]`. The attribute is a
            // proc macro that re-emits a call to an inner fn using the parameter names it parsed; those names come
            // from THIS macro's `$arg` fragments, and the hygiene context does not survive, so `#[pg_guard]` on the
            // generated fn fails to compile ("cannot find value `_s` in this scope"). `pgrx_extern_c_guard` is what
            // the attribute expands to anyway (pgrx lib.rs:126 re-exports it), so calling it directly is the same
            // boundary with none of the hygiene coupling. Nothing with a destructor is live in this frame, so the
            // ereport longjmp that leaves the guard cannot skip a Drop.
            pgrx::pgrx_extern_c_guard(|| {
                pg_sys::error!(concat!("theodb_columnar: ", $msg, " is not supported (M99 is append-only analytical)"))
            })
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
            let dir_bytes =
                super::page::read_chunked(rel, header.dir_first_block, header.dir_n_pages)?;
            let entries =
                codec::deserialize_directory(&dir_bytes, header.n_chunk_groups as usize * natts)?;
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

    // M104 (#99) — a big INSERT with a small maintenance_work_mem flushes INCREMENTALLY: it produces MULTIPLE stripes
    // (not one whole-transaction buffer), and the committed rows are correct. Proves the O(maintenance_work_mem)
    // write-memory bound (the peak pending set never holds the whole INSERT).
    #[pg_test]
    fn m104_incremental_flush_produces_multiple_stripes() {
        Spi::run("SET maintenance_work_mem = '1MB'").unwrap();
        Spi::run("CREATE TABLE m104_inc (a int, b text) USING theodb_columnar").unwrap();
        // ~50k rows of ~40-byte tuples ≈ 2MB > 1MB mwm → at least 2 incremental stripes.
        Spi::run("INSERT INTO m104_inc SELECT g, repeat('x', 30) FROM generate_series(1, 50000) g")
            .unwrap();
        let cnt = Spi::get_one::<i64>("SELECT count(*) FROM m104_inc").unwrap().unwrap();
        assert_eq!(cnt, 50000, "all rows committed and readable");
        let sum = Spi::get_one::<i64>("SELECT sum(a) FROM m104_inc").unwrap().unwrap();
        assert_eq!(sum, 50000i64 * 50001 / 2, "values intact across incremental stripes");
        let stripes = Spi::get_one::<i64>(
            "SELECT count(*) FROM columnar.stripe WHERE relid = 'm104_inc'::regclass",
        )
        .unwrap()
        .unwrap();
        assert!(
            stripes > 1,
            "incremental flush produced MULTIPLE stripes (got {stripes}) — write memory is bounded, not O(N)"
        );
        Spi::run("DROP TABLE m104_inc").unwrap();
    }

    // M104 (Q2) — the LAZY streaming scan returns the identical rows (count, sum, order) as an eager scan would,
    // across MULTIPLE stripes (via a low mwm) + same-xact pending rows. Peak scan memory is one stripe, not the
    // whole table, but the result is byte-identical.
    #[pg_test]
    fn m104_streaming_scan_matches_full_result() {
        Spi::run("SET maintenance_work_mem = '1MB'").unwrap();
        Spi::run("CREATE TABLE m104_scan (a int, b text) USING theodb_columnar").unwrap();
        // committed rows across multiple stripes
        Spi::run("INSERT INTO m104_scan SELECT g, 'r'||g FROM generate_series(1, 60000) g")
            .unwrap();
        let cnt = Spi::get_one::<i64>("SELECT count(*) FROM m104_scan").unwrap().unwrap();
        assert_eq!(cnt, 60000, "streaming scan over multiple stripes returns every row");
        let sum = Spi::get_one::<i64>("SELECT sum(a) FROM m104_scan").unwrap().unwrap();
        assert_eq!(sum, 60000i64 * 60001 / 2, "values intact across lazily-decoded stripes");
        // a specific row (order + content) and the last row
        let mid =
            Spi::get_one::<String>("SELECT b FROM m104_scan WHERE a = 30000").unwrap().unwrap();
        assert_eq!(mid, "r30000");
        // ordered read still monotonic (streaming preserves stripe/catalog order → row order)
        let ordered = Spi::get_one::<i64>(
            "SELECT count(*) FROM (SELECT a, lag(a) OVER () AS prev FROM m104_scan) t WHERE prev IS NOT NULL AND a <= prev",
        )
        .unwrap()
        .unwrap();
        assert_eq!(ordered, 0, "streaming scan preserves the stored row order");
        Spi::run("DROP TABLE m104_scan").unwrap();
    }

    // M104 H1 — self-referential INSERT under incremental flush: `INSERT INTO c SELECT FROM c` must honor INSERT
    // snapshot semantics — it reads the rows committed BEFORE the statement, NOT the stripes it incrementally flushes
    // mid-statement. Result = exactly 2× the pre-statement rows (no self-visible mid-flush stripes, no runaway).
    #[pg_test]
    fn m104_self_referential_insert_snapshot_safe() {
        Spi::run("SET maintenance_work_mem = '1MB'").unwrap();
        Spi::run("CREATE TABLE m104_self (a int, b text) USING theodb_columnar").unwrap();
        Spi::run(
            "INSERT INTO m104_self SELECT g, repeat('y', 30) FROM generate_series(1, 40000) g",
        )
        .unwrap();
        let before = Spi::get_one::<i64>("SELECT count(*) FROM m104_self").unwrap().unwrap();
        assert_eq!(before, 40000);
        // self-referential: even though this INSERT incrementally flushes stripes mid-statement, its SELECT reads the
        // pre-statement snapshot (40000), so the table ends at exactly 80000 — not more (mid-flush stripes not re-read).
        Spi::run("INSERT INTO m104_self SELECT a, b FROM m104_self").unwrap();
        let after = Spi::get_one::<i64>("SELECT count(*) FROM m104_self").unwrap().unwrap();
        assert_eq!(
            after, 80000,
            "self-referential INSERT honors its snapshot (2x), not its own mid-flush stripes"
        );
        Spi::run("DROP TABLE m104_self").unwrap();
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

        let nulls =
            Spi::get_one::<i64>("SELECT count(*) FROM m99_rt2 WHERE b IS NULL").unwrap().unwrap();
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
        Spi::get_one_with_args::<String>(
            "SELECT theodb_columnar_test_stripe_info($1)",
            &[oid.into()],
        )
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
        Spi::run(
            "INSERT INTO m99_cm SELECT g, 'row-' || g, g::float8 FROM generate_series(1, 25000) g",
        )
        .unwrap();
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
        assert!(
            (sumc - (1..=25000i64).map(|g| g as f64).sum::<f64>()).abs() < 1e-3,
            "sum(c) matches: {sumc}"
        );
        let sample =
            Spi::get_one::<String>("SELECT b FROM m99_cm WHERE a = 12345").unwrap().unwrap();
        assert_eq!(
            sample, "row-12345",
            "text round-trips across a chunk-group boundary via disk decode"
        );
        let nulls =
            Spi::get_one::<i64>("SELECT count(*) FROM m99_cm WHERE b IS NULL").unwrap().unwrap();
        assert_eq!(
            nulls, 1,
            "the NULL-text row round-trips through the column null-bitmap on disk"
        );
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
        let pre = Spi::get_one::<i64>(
            "SELECT count(*) FROM columnar.stripe WHERE relid = 'm99_mv'::regclass",
        )
        .unwrap()
        .unwrap();
        assert_eq!(pre, 0, "no catalog row before flush");
        let cnt_pending = Spi::get_one::<i64>("SELECT count(*) FROM m99_mv").unwrap().unwrap();
        assert_eq!(cnt_pending, 100, "pending rows visible to same xact before flush");
        // Flush → exactly one catalog stripe row appears (the visibility root).
        Spi::get_one_with_args::<String>(
            "SELECT theodb_columnar_test_stripe_info($1)",
            &[oid.into()],
        )
        .unwrap()
        .unwrap();
        let post = Spi::get_one::<i64>(
            "SELECT count(*) FROM columnar.stripe WHERE relid = 'm99_mv'::regclass",
        )
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
        Spi::get_one_with_args::<String>(
            "SELECT theodb_columnar_test_stripe_info($1)",
            &[oid.into()],
        )
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

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod m168_affinity_tests {
    use super::ThreadAffinity;

    /// M168 ADR-2 RED→GREEN. The whole point of `ThreadAffinity` is to fail loudly when a `pg_sys::Relation`
    /// holder is touched off its backend thread. An assertion never observed failing is not known to be an
    /// assertion — this observes it.
    ///
    /// No PostgreSQL state is touched from the spawned thread: only the affinity check runs there, so the test
    /// cannot itself corrupt anything. The panic is contained by `join()` returning `Err`.
    #[pgrx::pg_test]
    fn affinity_panics_off_owning_thread() {
        let aff = ThreadAffinity::capture();
        let joined = std::thread::spawn(move || aff.assert_owned("m168 test")).join();
        assert!(
            joined.is_err(),
            "ThreadAffinity did NOT panic when touched from another thread — the M168 ADR-2 guard is inert, \
             which means a multi-threaded runtime would corrupt memory silently instead of failing fast"
        );
    }

    /// The other half: on the owning thread it must be a no-op, or the guard would break the real path.
    #[pgrx::pg_test]
    fn affinity_is_silent_on_owning_thread() {
        let aff = ThreadAffinity::capture();
        aff.assert_owned("m168 test"); // must not panic
    }
}
