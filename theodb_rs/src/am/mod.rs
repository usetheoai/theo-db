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

pub(crate) mod aq; // M59 — anisotropic product quantizer (Phase 1 domain); the Phase 2 AH kernel (vec::ah) consumes AqQuantizer
pub(crate) mod autotune; // M67 — deterministic ef_search recommender + scan-stats collector
mod build; // ambuild / ambuildempty (Phase 2) + shared datum/metric helpers
mod build_stream; // M96 — tuplesort-streaming ambuild (bounded-memory build spool)
mod datafusion_probe; // M98 — DataFusion coexistence smoke (the pillar GATE)
mod cost; // M48 T5.1 — honest amcostestimate visit-ratio (pgvector cost model)
mod columnar; // M99 — theodb_columnar append-only columnar Table Access Method (Phase A: registration spike)
pub(crate) mod customscan; // M92 spike — arbitrary-WHERE Custom Scan Provider (pathlist hook + custom node)
mod fold; // M48 — crash-safe VACUUM fold (meta-pivot, issue #47)
pub(crate) mod guc; // M34 — theodb_ivfflat.probes scan GUC
mod hnsw_page; // M35 — page-native structured persistence for theodb_hnsw
mod index; // polymorphic persisted index (ivf|hnsw) dispatch (Phase 6)
mod lock; // advisory index-fold lock (serialize VACUUM rewrite vs scan/insert)
pub(crate) mod options; // M34 — theodb_ivfflat WITH (lists=N) reloption
mod page; // page persistence (Phase 1)
mod scan; // ambeginscan / amrescan / amgettuple / amendscan (Phase 3)
mod tid; // heap TID ⇄ i64 codec

/// Type of the two per-algorithm build callbacks (the only hooks that differ between the AMs).
type AmBuildFn = unsafe extern "C-unwind" fn(
    pg_sys::Relation,
    pg_sys::Relation,
    *mut pg_sys::IndexInfo,
) -> *mut pg_sys::IndexBuildResult;
type AmBuildEmptyFn = unsafe extern "C-unwind" fn(pg_sys::Relation);

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
    make_amroutine(build::ambuild, build::ambuildempty)
}

/// The `theodb_hnsw` handler — the SAME plumbing (scan/insert/vacuum/cost dispatch on the persisted blob's
/// magic), only the build callbacks differ (an HNSW graph instead of IVFFlat lists). M26 Phase 6.
#[pg_extern(sql = "
    CREATE OR REPLACE FUNCTION theodb_hnsw_amhandler(internal) RETURNS index_am_handler
        PARALLEL SAFE IMMUTABLE STRICT COST 0.0001 LANGUAGE c AS '@MODULE_PATHNAME@', '@FUNCTION_NAME@';
    DO $$
    BEGIN
        IF NOT EXISTS (SELECT 1 FROM pg_catalog.pg_am WHERE amname = 'theodb_hnsw') THEN
            CREATE ACCESS METHOD theodb_hnsw TYPE INDEX HANDLER theodb_hnsw_amhandler;
        END IF;
    END;
    $$;
")]
fn theodb_hnsw_amhandler(_fcinfo: pg_sys::FunctionCallInfo) -> PgBox<pg_sys::IndexAmRoutine> {
    make_amroutine(build::ambuild_hnsw, build::ambuildempty_hnsw)
}

/// Fill an `IndexAmRoutine` with the shared hooks + the given per-algorithm build callbacks.
fn make_amroutine(ambuild: AmBuildFn, ambuildempty: AmBuildEmptyFn) -> PgBox<pg_sys::IndexAmRoutine> {
    let mut amroutine =
        unsafe { PgBox::<pg_sys::IndexAmRoutine>::alloc_node(pg_sys::NodeTag::T_IndexAmRoutine) };

    amroutine.amstrategies = 0;
    amroutine.amsupport = 1; // M49: opclass support FUNCTION 1 returns the metric tag (ADR-1 — resolved at ambuild via index_getprocinfo). L2 (DEFAULT) carries no support proc → resolve_metric falls back to L2.
    amroutine.amcanorder = false;
    amroutine.amcanorderbyop = true; // enables the `ORDER BY embedding <-> $1 LIMIT k` pushdown (Phase 4)
    amroutine.amcanbackward = false;
    amroutine.amcanunique = false;
    // M90 (inline label filter, Approach A): the index MAY carry a 2nd `smallint[]` label column so the planner
    // pushes `labels && '{…}'` as a ScanKey the scan evaluates inline (Stage-1 prune). Single-column (vector-only)
    // indexes are unaffected — the 2nd column is optional.
    amroutine.amcanmulticol = true;
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
    amroutine.aminsert = Some(build::aminsert);
    amroutine.ambulkdelete = Some(ambulkdelete);
    amroutine.amvacuumcleanup = Some(amvacuumcleanup);
    amroutine.amcostestimate = Some(amcostestimate);
    amroutine.amoptions = Some(options::amoptions); // M34 — WITH (lists=N)
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

/// Cost (M48 T5.1 / D5 — honest): refuse when there is no order-by (this AM only serves `ORDER BY <-> LIMIT`).
/// Otherwise base the cost on `genericcostestimate` and scale the STARTUP by the fraction of index tuples an
/// ordered ANN scan actually visits (`cost::scan_visit_ratio`, the pgvector model): a large index visits a
/// tiny fraction → small startup → it wins the LIMIT; a tiny index visits nearly all → startup ≈ total →
/// seqscan+sort wins. The old body returned cost 0, so the index always won (it lied to the planner, G6).
// The 8-arg signature is dictated by Postgres's `amcostestimate_function` FFI contract — irreducible.
#[allow(clippy::too_many_arguments)]
#[pg_guard]
pub unsafe extern "C-unwind" fn amcostestimate(
    root: *mut pg_sys::PlannerInfo,
    path: *mut pg_sys::IndexPath,
    loop_count: f64,
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
    let mut costs: pg_sys::GenericCosts = std::mem::zeroed();
    pg_sys::genericcostestimate(root, path, loop_count, &mut costs);

    let indexinfo = (*path).indexinfo;
    let tuples = (*indexinfo).tuples;
    // Open NoLock: the planner already holds a lock on this index for the query being planned (pgvector
    // `hnsw.c` / `ivfflat.c` pattern). `scan_visit_ratio` is fail-safe — any unreadable meta degrades to 1.0.
    let rel = pg_sys::index_open((*indexinfo).indexoid, pg_sys::NoLock as pg_sys::LOCKMODE);
    let ratio = cost::scan_visit_ratio(rel, tuples);
    pg_sys::index_close(rel, pg_sys::NoLock as pg_sys::LOCKMODE);

    *index_startup_cost = costs.indexTotalCost * ratio;
    *index_total_cost = costs.indexTotalCost;
    *index_selectivity = costs.indexSelectivity;
    *index_correlation = costs.indexCorrelation;
    *index_pages = costs.numIndexPages;
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
        // M56: HNSW deletes are now in-place tombstones (no O(N) rebuild, no advisory EXCLUSIVE → no total
        // stall); the rare O(N) compaction fold runs inside this call only when tombstones pass the ratio GUC.
        // IVF/blob fall back to the O(N) rebuild internally.
        let live = build::vacuum_delete_inplace(indexrel, &mut dead);
        (*results).num_index_tuples = live as f64;
        results
    }
}

/// VACUUM cleanup (M26 Phase 5 + M48 T3.1): report the final page count, AND — when `ambulkdelete` did not run
/// (`stats == NULL` ⇒ zero dead tuples, i.e. an insert-only workload) — fold the pending region into the main
/// structure once it exceeds `theodb.vacuum_pending_threshold` pages, so the scan returns to O(structure)
/// instead of paying O(pending) forever (D3). The per-INSERT path stays O(1); the fold is the same crash-safe
/// `vacuum_rebuild` used by `ambulkdelete`, with nothing dead. NEVER aborts on an unreadable/legacy meta —
/// `pending_page_count` swallows the error to 0 (skip), so a routine VACUUM is fail-safe.
#[pg_guard]
pub extern "C-unwind" fn amvacuumcleanup(
    vinfo: *mut pg_sys::IndexVacuumInfo,
    stats: *mut pg_sys::IndexBulkDeleteResult,
) -> *mut pg_sys::IndexBulkDeleteResult {
    unsafe {
        if (*vinfo).analyze_only {
            return stats;
        }
        let indexrel = (*vinfo).index;
        if stats.is_null() {
            // No bulkdelete this pass — fold the pending region if it grew past the threshold (insert-only path).
            let pending = page::pending_page_count(indexrel);
            if pending > guc::vacuum_pending_threshold() {
                let mut none_dead = |_id: i64| -> bool { false };
                let live = build::vacuum_rebuild(indexrel, &mut none_dead);
                let out = PgBox::<pg_sys::IndexBulkDeleteResult>::alloc0().into_pg();
                (*out).num_index_tuples = live as f64;
                (*out).pages_deleted = pending; // the pending pages folded away
                (*out).num_pages =
                    pg_sys::RelationGetNumberOfBlocksInFork(indexrel, pg_sys::ForkNumber::MAIN_FORKNUM);
                return out;
            }
            return stats; // NULL — below threshold, nothing to do
        }
        (*stats).num_pages =
            pg_sys::RelationGetNumberOfBlocksInFork(indexrel, pg_sys::ForkNumber::MAIN_FORKNUM);
        stats
    }
}

/// M49 opclass support FUNCTION 1: return the metric tag (matching `Metric::tag()`). Bound as `FUNCTION 1` of
/// the cosine/ip opclasses; `resolve_metric` (build.rs) reads it via `index_getprocinfo(rel,1,1)` (ADR-1,
/// pgvector's `HnswInitSupport` mechanism — `hnswutils.c:154`). L2 is DEFAULT and carries NO support proc, so a
/// bare `USING theodb_hnsw (col)` resolves to L2 by fallback (InvalidOid). 0-arg because our distance kernels
/// live in Rust — the proc only names the metric.
#[pg_extern(immutable, parallel_safe)]
fn theodb_metric_ip() -> i32 {
    crate::ann::Metric::Ip.tag() as i32
}

#[pg_extern(immutable, parallel_safe)]
fn theodb_metric_cosine() -> i32 {
    crate::ann::Metric::Cosine.tag() as i32
}

/// M90 (inline label filter): the `&&` (array overlap) predicate for `smallint[]` label sets — own code (Rule 9;
/// pgvectorscale's `smallint_array_overlap` is study-of-design only). Returns true iff the two label arrays share
/// at least one element. Backs `OPERATOR 1 &&` of the label opclass, so the planner pushes `labels && '{…}'` as a
/// ScanKey the scan evaluates inline. Empty arrays never overlap. `create_or_replace` so re-install is idempotent.
#[pg_extern(immutable, parallel_safe, create_or_replace)]
fn theodb_smallint_array_overlap(left: Array<i16>, right: Array<i16>) -> bool {
    if left.is_empty() || right.is_empty() {
        return false;
    }
    // Small arrays: quadratic is cheaper than hashing (labels are typically 1-3 tags).
    if left.len() <= 8 && right.len() <= 8 {
        for a in left.iter().flatten() {
            for b in right.iter().flatten() {
                if a == b {
                    return true;
                }
            }
        }
        return false;
    }
    let set: std::collections::HashSet<i16> = left.into_iter().flatten().collect();
    right.into_iter().flatten().any(|b| set.contains(&b))
}

// The DEFAULT l2 operator class — the ORDER-BY operator binding `<->` to this AM (no support procs; L2 is the
// fallback metric). M49 adds the non-default cosine (`<=>`) / ip (`<#>`) opclasses below, each with a
// `FUNCTION 1` metric-tag support proc that `resolve_metric` reads at ambuild (ADR-1). Requires pgvector's
// `vector` type + the operators (present in the TheoDB image). `requires` the amhandler so the AM exists first.
extension_sql!(
    r#"
    CREATE OPERATOR CLASS theodb_ivfflat_l2_ops DEFAULT FOR TYPE vector USING theodb_ivfflat AS
        OPERATOR 1 <-> (vector, vector) FOR ORDER BY float_ops;
    "#,
    name = "theodb_ivfflat_opclasses",
    requires = [theodb_ivfflat_amhandler, "vector_type"],
);

// The DEFAULT l2 operator class for the HNSW AM (same shape; metric L2 baked into the persisted graph).
extension_sql!(
    r#"
    CREATE OPERATOR CLASS theodb_hnsw_l2_ops DEFAULT FOR TYPE vector USING theodb_hnsw AS
        OPERATOR 1 <-> (vector, vector) FOR ORDER BY float_ops;
    "#,
    name = "theodb_hnsw_opclasses",
    requires = [theodb_hnsw_amhandler, "vector_type"],
);

// M49: non-default cosine (`<=>`) + inner-product (`<#>`) opclasses for both AMs. Strategy is always 1
// (`FOR ORDER BY float_ops`); the metric is encoded in the operator + the `FUNCTION 1` metric-tag support proc
// (ADR-1). `<#>` is pgvector's negative inner product (smaller = closer). `<=>` is cosine distance.
extension_sql!(
    r#"
    CREATE OPERATOR CLASS theodb_ivfflat_cosine_ops FOR TYPE vector USING theodb_ivfflat AS
        OPERATOR 1 <=> (vector, vector) FOR ORDER BY float_ops,
        FUNCTION 1 theodb_metric_cosine();
    CREATE OPERATOR CLASS theodb_ivfflat_ip_ops FOR TYPE vector USING theodb_ivfflat AS
        OPERATOR 1 <#> (vector, vector) FOR ORDER BY float_ops,
        FUNCTION 1 theodb_metric_ip();
    CREATE OPERATOR CLASS theodb_hnsw_cosine_ops FOR TYPE vector USING theodb_hnsw AS
        OPERATOR 1 <=> (vector, vector) FOR ORDER BY float_ops,
        FUNCTION 1 theodb_metric_cosine();
    CREATE OPERATOR CLASS theodb_hnsw_ip_ops FOR TYPE vector USING theodb_hnsw AS
        OPERATOR 1 <#> (vector, vector) FOR ORDER BY float_ops,
        FUNCTION 1 theodb_metric_ip();
    "#,
    name = "theodb_cosine_ip_opclasses",
    requires = [
        "vector_type",
        theodb_ivfflat_amhandler,
        theodb_hnsw_amhandler,
        "theodb_ivfflat_opclasses",
        "theodb_hnsw_opclasses",
        theodb_metric_cosine,
        theodb_metric_ip,
    ],
);

// M90 (inline label filter, Approach A): a label opclass on `smallint[]` binding `OPERATOR 1 &&` (array overlap) so
// a multicolumn index `(embedding, labels)` lets the planner push `labels && '{…}'` as a ScanKey (Index Cond), which
// the scan evaluates inline (Stage-1 prune). Mirrors pgvectorscale's mechanism (own code, Rule 9). The `&&` operator
// for `smallint[]` is created guarded (the generic `anyarray &&` exists, but a type-specific entry backed by OUR
// procedure is what the opclass binds); `contsel`/`contjoinsel` give the planner a selectivity estimate.
extension_sql!(
    r#"
    DO $$
    BEGIN
        IF NOT EXISTS (
            SELECT 1 FROM pg_operator
            WHERE oprname = '&&' AND oprleft = 'smallint[]'::regtype AND oprright = 'smallint[]'::regtype
        ) THEN
            CREATE OPERATOR && (
                LEFTARG = smallint[], RIGHTARG = smallint[],
                PROCEDURE = theodb_smallint_array_overlap,
                COMMUTATOR = &&, RESTRICT = contsel, JOIN = contjoinsel
            );
        END IF;
    END;
    $$;
    CREATE OPERATOR CLASS theodb_ivfflat_label_ops DEFAULT FOR TYPE smallint[] USING theodb_ivfflat AS
        OPERATOR 1 && (smallint[], smallint[]);
    "#,
    name = "theodb_ivfflat_label_opclass",
    requires = [theodb_ivfflat_amhandler, theodb_smallint_array_overlap, "theodb_ivfflat_opclasses"],
);
