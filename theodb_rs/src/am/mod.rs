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
//! functions needed, because `ambuild` uses `crate::ann::Metric` directly.
use pgrx::*;

mod build; // ambuild / ambuildempty (Phase 2) + shared datum/metric helpers
mod page; // page persistence (Phase 1)
mod scan; // ambeginscan / amrescan / amgettuple / amendscan (Phase 3)
mod tid; // heap TID ⇄ i64 codec

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
    amroutine.ambuild = Some(build::ambuild);
    amroutine.ambuildempty = Some(build::ambuildempty);
    amroutine.aminsert = Some(build::aminsert);
    amroutine.ambulkdelete = Some(ambulkdelete);
    amroutine.amvacuumcleanup = Some(amvacuumcleanup);
    amroutine.amcostestimate = Some(amcostestimate);
    amroutine.amoptions = None; // no reloptions yet (added in a later phase)
    amroutine.ambeginscan = Some(scan::ambeginscan);
    amroutine.amrescan = Some(scan::amrescan);
    amroutine.amgettuple = Some(scan::amgettuple);
    amroutine.amgetbitmap = None;
    amroutine.amendscan = Some(scan::amendscan);

    amroutine.into_pg_boxed()
}

#[pg_guard]
pub extern "C-unwind" fn amvalidate(_opclassoid: pg_sys::Oid) -> bool {
    true
}

/// Cost: mark the index usable only when order-bys are present; keep costs modest so the planner MAY
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

/// VACUUM bulk-delete (M26 Phase 5): rebuild the main index over only the TIDs the `callback` reports as LIVE,
/// folding in the pending region and dropping dead tuples. A rebuild-on-vacuum (periodic, not per-query) — the
/// per-INSERT path stays O(1) (pending append), so this does not violate "no total rebuild on insert".
#[pg_guard]
pub extern "C-unwind" fn ambulkdelete(
    info: *mut pg_sys::IndexVacuumInfo,
    stats: *mut pg_sys::IndexBulkDeleteResult,
    callback: pg_sys::IndexBulkDeleteCallback,
    callback_state: *mut ::std::os::raw::c_void,
) -> *mut pg_sys::IndexBulkDeleteResult {
    unsafe {
        let results = if stats.is_null() {
            PgBox::<pg_sys::IndexBulkDeleteResult>::alloc0().into_pg()
        } else {
            stats
        };
        let indexrel = (*info).index;
        // `dead(id)` decodes the packed TID and asks Postgres's callback whether that heap tuple is dead.
        let mut dead = |id: i64| -> bool {
            let mut itid = pg_sys::ItemPointerData::default();
            tid::set_on(id, &mut itid);
            match callback {
                Some(cb) => cb(&mut itid, callback_state),
                None => false,
            }
        };
        let live = build::vacuum_rebuild(indexrel, &mut dead);
        (*results).num_index_tuples = live as f64;
        results
    }
}

/// VACUUM cleanup (M26 Phase 5): report the final page count. The heavy lifting happened in `ambulkdelete`.
#[pg_guard]
pub extern "C-unwind" fn amvacuumcleanup(
    vinfo: *mut pg_sys::IndexVacuumInfo,
    stats: *mut pg_sys::IndexBulkDeleteResult,
) -> *mut pg_sys::IndexBulkDeleteResult {
    unsafe {
        if stats.is_null() || (*vinfo).analyze_only {
            return stats;
        }
        (*stats).num_pages = pg_sys::RelationGetNumberOfBlocksInFork(
            (*vinfo).index,
            pg_sys::ForkNumber::MAIN_FORKNUM,
        );
        stats
    }
}

// The DEFAULT l2 operator class — the ORDER-BY operator binding `<->` to this AM (no support procs; the metric
// is L2, baked into the persisted index). cosine/ip opclasses are a follow-up (need opclass→metric resolution,
// which pgrx 0.16 does not expose via get_opfamily_name). Requires pgvector's `vector` type + `<->` operator
// (present in the TheoDB image). `requires` the amhandler so the AM exists first.
extension_sql!(
    r#"
    CREATE OPERATOR CLASS theodb_ivfflat_l2_ops DEFAULT FOR TYPE vector USING theodb_ivfflat AS
        OPERATOR 1 <-> (vector, vector) FOR ORDER BY float_ops;
    "#,
    name = "theodb_ivfflat_opclasses",
    requires = [theodb_ivfflat_amhandler],
);
