//! TheoDB vector Index Access Method (M26) — promotes the in-memory rebuild-per-query ANN to a persisted
//! Postgres index AM (`theodb_ivfflat` / `theodb_hnsw`).
//!
//! **Phase 0 (this file, initial):** the de-risk spike — register a real `IndexAmRoutine` via pgrx 0.16.1 with
//! minimal no-op hooks, so `CREATE ACCESS METHOD` + `CREATE INDEX … USING theodb_ivfflat` load end-to-end. This
//! proves the FFI/registration path on THIS toolchain BEFORE the large build (ROADMAP M26 ALTO-risk guard).
//! The real `ambuild`/scan/maintenance land in later phases (`build.rs`, `scan.rs`, `vacuum.rs`, `cost.rs`).
//!
//! Pattern source (pgrx =0.16.1, identical to ours): pgvectorscale `src/access_method/mod.rs:45` (amhandler →
//! `PgBox<IndexAmRoutine>`), `build.rs:296` (ambuild sig), `scan.rs:309` (scan sigs), `vacuum.rs:24`,
//! `cost_estimate.rs:6`. Parsimony: the opclass name selects the metric (l2/cosine/ip) — no opclass support
//! functions needed, because `ambuild` will use `crate::ann::Metric` directly.
use pgrx::*;

/// The IndexAmRoutine handler. Idempotent install of the AM (skips if `pg_am` already has it — safe re-`CREATE
/// EXTENSION`). Mirrors pgvectorscale's amhandler SQL shape (`access_method/mod.rs:27`).
#[pg_extern(sql = "
    CREATE OR REPLACE FUNCTION theodb_ivfflat_amhandler(internal) RETURNS index_am_handler
        PARALLEL SAFE IMMUTABLE STRICT COST 0.0001 LANGUAGE c AS '@MODULE_PATHNAME@', '@FUNCTION_NAME@';
    DO $$
    BEGIN
        IF NOT EXISTS (SELECT 1 FROM pg_catalog.pg_am WHERE amname = 'theodb_ivfflat') THEN
            CREATE ACCESS METHOD theodb_ivfflat TYPE INDEX HANDLER theodb_ivfflat_amhandler;
        END IF;
    END;
    $$;
")]
fn theodb_ivfflat_amhandler(_fcinfo: pg_sys::FunctionCallInfo) -> PgBox<pg_sys::IndexAmRoutine> {
    let mut amroutine =
        unsafe { PgBox::<pg_sys::IndexAmRoutine>::alloc_node(pg_sys::NodeTag::T_IndexAmRoutine) };

    amroutine.amstrategies = 0;
    amroutine.amsupport = 0; // metric is implied by the opclass name; no opclass support procs (parsimony)
    amroutine.amcanorder = false;
    amroutine.amcanorderbyop = true; // enables the `ORDER BY embedding <-> $1 LIMIT k` pushdown (Phase 4)
    amroutine.amcanbackward = false;
    amroutine.amcanunique = false;
    amroutine.amcanmulticol = false;
    amroutine.amoptionalkey = true;
    amroutine.amsearcharray = false;
    amroutine.amsearchnulls = false;
    amroutine.amstorage = false;
    amroutine.amclusterable = false;
    amroutine.ampredlocks = false;
    amroutine.amcanparallel = false;
    amroutine.amcaninclude = false;
    amroutine.amusemaintenanceworkmem = false;
    amroutine.amkeytype = pg_sys::InvalidOid;

    amroutine.amvalidate = Some(amvalidate);
    amroutine.ambuild = Some(ambuild);
    amroutine.ambuildempty = Some(ambuildempty);
    amroutine.aminsert = Some(aminsert);
    amroutine.ambulkdelete = Some(ambulkdelete);
    amroutine.amvacuumcleanup = Some(amvacuumcleanup);
    amroutine.amcostestimate = Some(amcostestimate);
    amroutine.amoptions = None; // no reloptions in Phase 0 (added in Phase 2)
    amroutine.ambeginscan = Some(ambeginscan);
    amroutine.amrescan = Some(amrescan);
    amroutine.amgettuple = Some(amgettuple);
    amroutine.amgetbitmap = None;
    amroutine.amendscan = Some(amendscan);

    amroutine.into_pg_boxed()
}

#[pg_guard]
pub extern "C-unwind" fn amvalidate(_opclassoid: pg_sys::Oid) -> bool {
    true
}

/// Phase-0 no-op build: return an empty `IndexBuildResult` (0 tuples). The real page-persisted build lands in
/// Phase 2. The heap is not scanned yet — the spike only proves CREATE INDEX reaches the AM.
#[pg_guard]
pub extern "C-unwind" fn ambuild(
    _heaprel: pg_sys::Relation,
    _indexrel: pg_sys::Relation,
    _index_info: *mut pg_sys::IndexInfo,
) -> *mut pg_sys::IndexBuildResult {
    let mut result = unsafe { PgBox::<pg_sys::IndexBuildResult>::alloc0() };
    result.heap_tuples = 0.0;
    result.index_tuples = 0.0;
    result.into_pg()
}

#[pg_guard]
pub extern "C-unwind" fn ambuildempty(_indexrel: pg_sys::Relation) {}

/// Phase-0 no-op insert (returns false = "not inserted"). Real pending-buffer insert lands in Phase 5.
// The 8-arg signature is dictated by Postgres's `aminsert_function` FFI contract — irreducible.
#[allow(clippy::too_many_arguments)]
#[pg_guard]
pub unsafe extern "C-unwind" fn aminsert(
    _indexrel: pg_sys::Relation,
    _values: *mut pg_sys::Datum,
    _isnull: *mut bool,
    _heap_tid: pg_sys::ItemPointer,
    _heaprel: pg_sys::Relation,
    _check_unique: pg_sys::IndexUniqueCheck::Type,
    _index_unchanged: bool,
    _index_info: *mut pg_sys::IndexInfo,
) -> bool {
    false
}

#[pg_guard]
pub extern "C-unwind" fn ambeginscan(
    index_relation: pg_sys::Relation,
    nkeys: ::std::os::raw::c_int,
    norderbys: ::std::os::raw::c_int,
) -> pg_sys::IndexScanDesc {
    unsafe { pg_sys::RelationGetIndexScan(index_relation, nkeys, norderbys) }
}

#[pg_guard]
pub extern "C-unwind" fn amrescan(
    _scan: pg_sys::IndexScanDesc,
    _keys: pg_sys::ScanKey,
    _nkeys: ::std::os::raw::c_int,
    _orderbys: pg_sys::ScanKey,
    _norderbys: ::std::os::raw::c_int,
) {
}

/// Phase-0 no-op scan: return false = "no more tuples". Real deserialize+search lands in Phase 3.
#[pg_guard]
pub extern "C-unwind" fn amgettuple(
    _scan: pg_sys::IndexScanDesc,
    _direction: pg_sys::ScanDirection::Type,
) -> bool {
    false
}

#[pg_guard]
pub extern "C-unwind" fn amendscan(_scan: pg_sys::IndexScanDesc) {}

/// Phase-0 cost: mark the index as usable only when order-bys are present; keep costs modest so the planner MAY
/// choose it (tuned in Phase 4). When there is no order-by key, refuse (infinite cost).
// The 8-arg signature is dictated by Postgres's `amcostestimate_function` FFI contract — irreducible.
#[allow(clippy::too_many_arguments)]
#[pg_guard]
pub unsafe extern "C-unwind" fn amcostestimate(
    _root: *mut pg_sys::PlannerInfo,
    path: *mut pg_sys::IndexPath,
    _loop_count: f64,
    index_startup_cost: *mut pg_sys::Cost,
    index_total_cost: *mut pg_sys::Cost,
    index_selectivity: *mut pg_sys::Selectivity,
    index_correlation: *mut f64,
    index_pages: *mut f64,
) {
    if (*path).indexorderbys.is_null() {
        *index_startup_cost = f64::MAX;
        *index_total_cost = f64::MAX;
        *index_selectivity = 0.0;
        *index_correlation = 0.0;
        *index_pages = 0.0;
        return;
    }
    *index_startup_cost = 0.0;
    *index_total_cost = 0.0;
    *index_selectivity = 1.0;
    *index_correlation = 1.0;
    *index_pages = 1.0;
}

#[pg_guard]
pub extern "C-unwind" fn ambulkdelete(
    _info: *mut pg_sys::IndexVacuumInfo,
    stats: *mut pg_sys::IndexBulkDeleteResult,
    _callback: pg_sys::IndexBulkDeleteCallback,
    _callback_state: *mut ::std::os::raw::c_void,
) -> *mut pg_sys::IndexBulkDeleteResult {
    if stats.is_null() {
        unsafe { PgBox::<pg_sys::IndexBulkDeleteResult>::alloc0().into_pg() }
    } else {
        stats
    }
}

#[pg_guard]
pub extern "C-unwind" fn amvacuumcleanup(
    _vinfo: *mut pg_sys::IndexVacuumInfo,
    stats: *mut pg_sys::IndexBulkDeleteResult,
) -> *mut pg_sys::IndexBulkDeleteResult {
    stats
}

// The DEFAULT operator classes — one per metric; the opclass name selects the metric at build time (Phase 2).
// Minimal (no support procs): only the ORDER-BY operator binding the distance op to this AM. Requires pgvector's
// `vector` type + operators (present in the TheoDB image). `requires` the amhandler so the AM exists first.
extension_sql!(
    r#"
    CREATE OPERATOR CLASS theodb_ivfflat_l2_ops DEFAULT FOR TYPE vector USING theodb_ivfflat AS
        OPERATOR 1 <-> (vector, vector) FOR ORDER BY float_ops;
    CREATE OPERATOR CLASS theodb_ivfflat_cosine_ops FOR TYPE vector USING theodb_ivfflat AS
        OPERATOR 1 <=> (vector, vector) FOR ORDER BY float_ops;
    CREATE OPERATOR CLASS theodb_ivfflat_ip_ops FOR TYPE vector USING theodb_ivfflat AS
        OPERATOR 1 <#> (vector, vector) FOR ORDER BY float_ops;
    "#,
    name = "theodb_ivfflat_opclasses",
    requires = [theodb_ivfflat_amhandler],
);
