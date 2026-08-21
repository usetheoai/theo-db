//! M149 — projection pushdown for the `theodb_columnar` seqscan, via a scan-replacing `CustomScan`.
//!
//! **Why a CustomScan and not the TableAM (ADR-1, blueprint § Descoberta):** the PG18 TableAM `scan_begin`
//! contract (`access/tableam.h:334`) receives no column bitmap — a pure TableAM CANNOT know which columns the
//! query projects, so it must materialize all of them. Projection is decided ABOVE the scan node (the target
//! list / `ExecProject`). The only place a provider sees `plan->targetlist`/`plan->qual` is a `CustomScan`.
//! This is exactly how Citus columnar solves it (`ColumnarAttrNeeded`, `columnar_customscan.c:1814`) — Rule 9,
//! mirror the primary prior art, do not reinvent. The plain TableAM seqscan stays as the fail-safe all-columns
//! path (M148 measured it as the dominant cost: ~80% is materializing a 105-column heap tuple per row).
//!
//! **What this node does:** it IS the scan of the columnar table (`scanrelid = rel.relid`, no children — unlike
//! the vecfilter `CustomScan` which wraps children). At `begin` it derives `wanted = pull_var_clause(targetlist)
//! ∪ pull_var_clause(qual)` (ADR-2 — the UNION is load-bearing: `SELECT a WHERE b<>0` must still materialize `b`
//! or the filter breaks), opens a real `table_beginscan` over the columnar rel, and — during each `ExecScan`
//! pull — installs `wanted` in a backend-local side channel keyed BY THE SCAN DESCRIPTOR so that
//! `columnar_scan_getnextslot` materializes ONLY those columns (rest `isnull=true`), reusing the existing
//! stripe decoder. `ExecScan` then applies the qual + projection exactly as for a plain scan, so the result is
//! byte-identical to the heap (columns not in `wanted` are never read by any upper node).
//!
//! **Safety posture (mirrors `customscan.rs`):** every callback is `#[pg_guard] extern "C-unwind"`, corrupt
//! state routes to `pg_sys::error!` (never a raw panic across C), the side-channel install is an RAII
//! `ActiveGuard` (restored even on a mid-pull longjmp), and xact/subxact callbacks clear any orphaned state.
//! Gated behind `theodb.enable_projection` (default ON, but the whole path is additive — if the node is not
//! chosen OR `wanted` cannot be derived, the plain seqscan runs unchanged).
//!
//! **License:** own code (Rule 9). Citus columnar (AGPLv3) is studied as design literature only — no source
//! copied, no library linked (same posture as `columnar.rs`).
#![allow(non_snake_case)]

use pgrx::{GucContext, GucFlags, GucRegistry, GucSetting};
use pgrx::{PgBox, PgList, pg_guard, pg_sys};
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::os::raw::c_int;
use std::rc::Rc;

use super::columnar_agg::extract_zone_predicate;
use super::zonemap::ZonePredicate;

/// `theodb.enable_projection` — default ON. When off, the pathlist hook adds no projection path and every
/// columnar scan falls back to the plain all-columns TableAM seqscan (the exact pre-M149 behavior).
pub(crate) static ENABLE_PROJECTION: GucSetting<bool> = GucSetting::<bool>::new(true);

/// `theodb.enable_chunk_skip` (M150) — default ON. When off, `load_next_batch` passes `skip=false` so no
/// chunk group is pruned by the zone-map (the exact pre-M150 path), giving the A/B OFF-vs-ON ablation on the
/// SAME binary. Correctness never depends on it (skip is an admission filter; `ExecScan` re-checks the qual).
pub(crate) static ENABLE_CHUNK_SKIP: GucSetting<bool> = GucSetting::<bool>::new(true);

// ---- side channel: the wanted-columns set the columnar getnextslot materializes ----
//
// UNLIKE the vecfilter's `SCAN_MEMBERSHIP` (a bare `Option`), the ACTIVE slot here carries the owning SCAN
// DESCRIPTOR pointer alongside the wanted set, and `scan_projection()` only returns it when the CALLING
// scandesc matches. Rationale (correctness under nesting): during our `ExecScan` window the qual may evaluate
// a subplan that scans a DIFFERENT columnar table; that nested scan reaches `columnar_scan_getnextslot` while
// OUR slot is still active. Without the scandesc match it would project the nested table by OUR column indices
// — a silent wrong answer that the A/B gate (Rule 5) would catch. The scandesc key makes the projection apply
// to exactly the one scan it was derived for; every other columnar scan (nested subplan, unrelated plain
// seqscan in the same backend) sees `None` and decodes all columns.
thread_local! {
    // The ACTIVE slot the columnar getnextslot reads — `(scandesc_ptr, wanted)`. Only ever non-None DURING a
    // projection node's `ExecScan` pull window (swap-discipline via `ActiveGuard`).
    static SCAN_PROJECTION: RefCell<Option<(usize, Rc<Vec<usize>>)>> = const { RefCell::new(None) };
    // Per-node wanted registry, keyed by the `CustomScanState` pointer. Rust-side storage (normal Drop) — an
    // `Rc<Vec<usize>>` must NEVER live inside the palloc0'd `ProjScanState`, which Postgres frees without
    // running Drop (would leak the Vec per query). Inserted at `begin`, removed at `end`, cleared on abort.
    static NODE_PROJECTIONS: RefCell<HashMap<usize, Rc<Vec<usize>>>> = RefCell::new(HashMap::new());
    // M150 — the PARALLEL zone-map predicate channel (ADR-3). Same scandesc keying + same registry discipline as
    // the projection above, but a SEPARATE slot so the released M149 projection tuple stays byte-identical. The
    // two channels SHARE the cleanup path (`clear_all`, `subxact_clear`, `end_custom_scan`) — one function clears
    // both, so the correctness discipline (nested-scan isolation, ABA-after-subxact-abort) can never drift apart.
    static SCAN_PREDICATES: RefCell<Option<(usize, Rc<Vec<ZonePredicate>>)>> = const { RefCell::new(None) };
    static NODE_PREDICATES: RefCell<HashMap<usize, Rc<Vec<ZonePredicate>>>> = RefCell::new(HashMap::new());
}

/// Swap the ACTIVE predicate slot, returning the previous value (mirror of `swap_active`, for `PredGuard`).
fn swap_active_preds(
    v: Option<(usize, Rc<Vec<ZonePredicate>>)>,
) -> Option<(usize, Rc<Vec<ZonePredicate>>)> {
    SCAN_PREDICATES.with(|c| std::mem::replace(&mut *c.borrow_mut(), v))
}

/// The zone-map predicates pushed for the columnar scan identified by `scandesc`, or `None` (⇒ no chunk skip).
/// Read by `columnar::load_next_batch`. Returns `Some` ONLY when the active slot's scandesc equals the caller's —
/// the exact nested-scan isolation the projection channel uses (a nested/unrelated columnar scan sees `None`).
pub(crate) fn scan_predicates(scandesc: usize) -> Option<Rc<Vec<ZonePredicate>>> {
    SCAN_PREDICATES.with(|c| {
        c.borrow()
            .as_ref()
            .and_then(|(sd, p)| if *sd == scandesc { Some(Rc::clone(p)) } else { None })
    })
}

/// RAII guard restoring the ACTIVE predicate slot even on a mid-pull longjmp (mirror of `ActiveGuard`).
struct PredGuard(Option<(usize, Rc<Vec<ZonePredicate>>)>);
impl PredGuard {
    fn install(v: Option<(usize, Rc<Vec<ZonePredicate>>)>) -> Self {
        PredGuard(swap_active_preds(v))
    }
}
impl Drop for PredGuard {
    fn drop(&mut self) {
        let _ = swap_active_preds(self.0.take());
    }
}

/// Register a node's pushed predicates. UNCONDITIONAL insert (even an empty Vec) — unlike the projection registry
/// (which uses remove-on-None as its decode-all signal), the predicate channel's "no skip" signal is an empty
/// Vec, so overwriting on every `begin` makes a reused address ABA-proof by construction (the vecfilter idiom).
fn preds_registry_insert(node: usize, preds: Vec<ZonePredicate>) {
    NODE_PREDICATES.with(|r| {
        r.borrow_mut().insert(node, Rc::new(preds));
    });
}
fn preds_registry_get(node: usize) -> Option<Rc<Vec<ZonePredicate>>> {
    NODE_PREDICATES.with(|r| r.borrow().get(&node).cloned())
}
fn preds_registry_remove(node: usize) {
    NODE_PREDICATES.with(|r| {
        r.borrow_mut().remove(&node);
    });
}

/// Swap the ACTIVE slot, returning the previous value (re-entrant save/restore discipline around each pull).
fn swap_active(v: Option<(usize, Rc<Vec<usize>>)>) -> Option<(usize, Rc<Vec<usize>>)> {
    SCAN_PROJECTION.with(|c| std::mem::replace(&mut *c.borrow_mut(), v))
}

/// The wanted columns for the columnar scan identified by `scandesc`, or `None` (⇒ decode ALL columns). Read by
/// `columnar::load_next_batch`. Returns `Some` ONLY when the active slot's scandesc equals the caller's — see
/// the module-level side-channel note (nested-scan correctness).
pub(crate) fn scan_projection(scandesc: usize) -> Option<Rc<Vec<usize>>> {
    SCAN_PROJECTION.with(|c| {
        c.borrow()
            .as_ref()
            .and_then(|(sd, w)| if *sd == scandesc { Some(Rc::clone(w)) } else { None })
    })
}

/// RAII guard making the pull-window swap unwind-safe: a PG ERROR inside `ExecScan` becomes a Rust unwind at the
/// `#[pg_guard]` boundary, and Drop restores the previous ACTIVE value even then — so a longjmp mid-pull can
/// never leave a stale projection installed for a later scan (belt-and-braces with the xact/subxact clears).
struct ActiveGuard(Option<(usize, Rc<Vec<usize>>)>);
impl ActiveGuard {
    fn install(v: Option<(usize, Rc<Vec<usize>>)>) -> Self {
        ActiveGuard(swap_active(v))
    }
}
impl Drop for ActiveGuard {
    fn drop(&mut self) {
        let _ = swap_active(self.0.take());
    }
}

fn registry_insert(node: usize, wanted: Vec<usize>) {
    NODE_PROJECTIONS.with(|r| {
        r.borrow_mut().insert(node, Rc::new(wanted));
    });
}
fn registry_get(node: usize) -> Option<Rc<Vec<usize>>> {
    NODE_PROJECTIONS.with(|r| r.borrow().get(&node).cloned())
}
fn registry_remove(node: usize) {
    NODE_PROJECTIONS.with(|r| {
        r.borrow_mut().remove(&node);
    });
}
/// Wholesale cleanup at (sub)transaction abort: entries orphaned by an error-longjmp that skipped `end` are
/// freed here; the ACTIVE slot is cleared too. Bounds any leak to one (sub)transaction and — crucially — closes
/// the ABA window where a new scandesc palloc'd at a freed scandesc's address could inherit a stale projection.
fn clear_all() {
    let _ = swap_active(None);
    NODE_PROJECTIONS.with(|r| r.borrow_mut().clear());
    // M150 — the predicate channel shares this cleanup so both channels are freed together (no drift).
    let _ = swap_active_preds(None);
    NODE_PREDICATES.with(|r| r.borrow_mut().clear());
}

// ---- static method tables (registered once; addresses stable for the postmaster lifetime) ----
struct Methods<T>(T);
unsafe impl<T> Sync for Methods<T> {}

static PATH_METHODS: Methods<pg_sys::CustomPathMethods> = Methods(pg_sys::CustomPathMethods {
    CustomName: c"theodb_columnar_project".as_ptr(),
    PlanCustomPath: Some(plan_custom_path),
    ReparameterizeCustomPathByChild: None,
});

static SCAN_METHODS: Methods<pg_sys::CustomScanMethods> = Methods(pg_sys::CustomScanMethods {
    CustomName: c"theodb_columnar_project".as_ptr(),
    CreateCustomScanState: Some(create_custom_scan_state),
});

static EXEC_METHODS: Methods<pg_sys::CustomExecMethods> = Methods(pg_sys::CustomExecMethods {
    CustomName: c"theodb_columnar_project".as_ptr(),
    BeginCustomScan: Some(begin_custom_scan),
    ExecCustomScan: Some(exec_custom_scan),
    EndCustomScan: Some(end_custom_scan),
    ReScanCustomScan: Some(rescan_custom_scan),
    MarkPosCustomScan: None,
    RestrPosCustomScan: None,
    EstimateDSMCustomScan: None,
    InitializeDSMCustomScan: None,
    ReInitializeDSMCustomScan: None,
    InitializeWorkerCustomScan: None,
    ShutdownCustomScan: None,
    ExplainCustomScan: None,
});

/// The previous `set_rel_pathlist_hook` in the chain (Postgres allows only one). We install a SECOND chained
/// hook rather than editing `customscan.rs`: each hook saves whatever was there when IT installed and calls it
/// first, so the vecfilter hook (installed by `customscan::init`) keeps running untouched. Init order in
/// `_PG_init` puts this after `customscan::init`, so `PREV_HOOK` here IS the vecfilter's `pathlist_hook`.
static mut PREV_HOOK: pg_sys::set_rel_pathlist_hook_type = None;

/// Cached OID of the `theodb_columnar` table AM (resolved once per backend). Same idiom as `columnar_agg`.
fn columnar_amoid() -> pg_sys::Oid {
    use std::sync::OnceLock;
    static AMOID: OnceLock<u32> = OnceLock::new();
    let raw = *AMOID
        .get_or_init(|| unsafe { pg_sys::get_am_oid(c"theodb_columnar".as_ptr(), true).to_u32() });
    unsafe { pg_sys::Oid::from_u32_unchecked(raw) }
}

/// Register the CustomScan methods + the GUC + the chained pathlist hook + the xact/subxact cleanup callbacks.
/// Called once from `_PG_init` (AFTER `customscan::init` so the chain wraps the vecfilter hook).
pub(crate) fn init() {
    GucRegistry::define_bool_guc(
        c"theodb.enable_projection",
        c"Push column projection into the theodb_columnar scan via a CustomScan",
        c"When on, a SELECT of a subset of columns from a theodb_columnar table materializes only the referenced columns (targetlist ∪ qual); off falls back to the all-columns seqscan.",
        &ENABLE_PROJECTION,
        GucContext::Userset,
        GucFlags::default(),
    );
    GucRegistry::define_bool_guc(
        c"theodb.enable_chunk_skip",
        c"Skip theodb_columnar chunk groups whose zone-map min/max proves no row matches the query predicate",
        c"When on (M150), a SELECT with a simple `col op const` WHERE over a theodb_columnar table skips whole chunk groups the min/max directory proves cannot match, without decompressing them; off decodes every chunk group (the pre-M150 path). Never affects correctness — the executor re-checks the full qual. Requires theodb.enable_projection = on (the skip rides the projection CustomScan that pushes the predicate); with projection off, this GUC is a silent no-op.",
        &ENABLE_CHUNK_SKIP,
        GucContext::Userset,
        GucFlags::default(),
    );
    unsafe {
        pg_sys::RegisterCustomScanMethods(&SCAN_METHODS.0);
        PREV_HOOK = pg_sys::set_rel_pathlist_hook;
        pg_sys::set_rel_pathlist_hook = Some(pathlist_hook);
        pg_sys::RegisterXactCallback(Some(xact_clear), std::ptr::null_mut());
        pg_sys::RegisterSubXactCallback(Some(subxact_clear), std::ptr::null_mut());
    }
}

#[pg_guard]
unsafe extern "C-unwind" fn xact_clear(
    event: pg_sys::XactEvent::Type,
    _arg: *mut std::ffi::c_void,
) {
    use pg_sys::XactEvent as XE;
    if event == XE::XACT_EVENT_ABORT
        || event == XE::XACT_EVENT_PARALLEL_ABORT
        || event == XE::XACT_EVENT_COMMIT
        || event == XE::XACT_EVENT_PARALLEL_COMMIT
        || event == XE::XACT_EVENT_PREPARE
    {
        clear_all();
    }
}

#[pg_guard]
unsafe extern "C-unwind" fn subxact_clear(
    event: pg_sys::SubXactEvent::Type,
    _my: pg_sys::SubTransactionId,
    _parent: pg_sys::SubTransactionId,
    _arg: *mut std::ffi::c_void,
) {
    if event == pg_sys::SubXactEvent::SUBXACT_EVENT_ABORT_SUB {
        // Clear the ACTIVE slot only — a stale projection after a caught PL/pgSQL EXCEPTION must not leak into
        // the next statement (mirrors the vecfilter subxact guard). Registry entries of unrelated live scans
        // are freed at xact end.
        let _ = swap_active(None);
        let _ = swap_active_preds(None); // M150 — same discipline for the predicate channel
    }
}

/// The chained planner hook: for a BASEREL over `theodb_columnar` in a NON-aggregate query, add a projection
/// `CustomPath` priced just under the plain seqscan so it wins when the seqscan would have. Fail-safe by
/// construction — a planner hook must never `error!`, and any condition we cannot satisfy simply skips the node
/// (the native seqscan then runs). Gated behind `theodb.enable_projection`.
#[pg_guard]
unsafe extern "C-unwind" fn pathlist_hook(
    root: *mut pg_sys::PlannerInfo,
    rel: *mut pg_sys::RelOptInfo,
    _rti: pg_sys::Index,
    rte: *mut pg_sys::RangeTblEntry,
) {
    if let Some(prev) = unsafe { PREV_HOOK } {
        unsafe { prev(root, rel, _rti, rte) }; // keep the vecfilter hook (and any earlier chain) running
    }
    if !ENABLE_PROJECTION.get() {
        return;
    }
    let relref = unsafe { &mut *rel };
    if relref.reloptkind != pg_sys::RelOptKind::RELOPT_BASEREL {
        return;
    }
    // Plain relation using the theodb_columnar AM (same detection idiom as columnar_agg: get_rel_relam + amoid).
    if rte.is_null() || unsafe { (*rte).rtekind } != pg_sys::RTEKind::RTE_RELATION {
        return;
    }
    let amoid = columnar_amoid();
    if amoid == pg_sys::InvalidOid || unsafe { pg_sys::get_rel_relam((*rte).relid) } != amoid {
        return;
    }
    // (c) NOT an aggregate query — leave those to the plain seqscan (which columnar_agg may then swap at
    // UPPERREL_GROUP_AGG). Replacing the seqscan under an Agg would break columnar_agg's `find_scan_relid`
    // (it matches a T_SeqScan, not our CustomScan) and lose the vectorized-aggregate path (plan R3).
    let parse = unsafe { (*root).parse };
    if !parse.is_null()
        && (unsafe { (*parse).hasAggs }
            || !unsafe { (*parse).groupClause.is_null() }
            || !unsafe { (*parse).groupingSets.is_null() }
            || unsafe { (*parse).hasWindowFuncs })
    {
        return;
    }

    // Cost: beat the plain seqscan, but never a genuinely cheaper index path. Base off the cheapest existing
    // T_SeqScan path (unparameterized); price at 0.9× — a modest planner NUDGE, NOT a performance claim (the
    // real number is the T3.2 benchmark). If there is no seqscan path, add nothing (fail-safe).
    let paths = unsafe { PgList::<pg_sys::Path>::from_pg(relref.pathlist) };
    let mut seq: *mut pg_sys::Path = std::ptr::null_mut();
    for i in 0..paths.len() {
        if let Some(p) = paths.get_ptr(i) {
            if unsafe { (*p).pathtype } == pg_sys::NodeTag::T_SeqScan
                && unsafe { (*p).param_info.is_null() }
                && (seq.is_null() || unsafe { (*p).total_cost } < unsafe { (*seq).total_cost })
            {
                seq = p;
            }
        }
    }
    if seq.is_null() {
        return;
    }
    let startup = unsafe { (*seq).startup_cost };
    let total = unsafe { (*seq).total_cost } * 0.9;
    let rows = unsafe { (*seq).rows };

    let mut cpath =
        unsafe { PgBox::<pg_sys::CustomPath>::alloc_node(pg_sys::NodeTag::T_CustomPath) };
    let path = &mut cpath.path;
    path.pathtype = pg_sys::NodeTag::T_CustomScan;
    path.parent = rel;
    path.pathtarget = relref.reltarget;
    path.param_info = std::ptr::null_mut();
    path.pathkeys = std::ptr::null_mut(); // no ordering guarantee (pure projection scan)
    path.rows = rows;
    path.startup_cost = startup;
    path.total_cost = total;
    // Inherit the seqscan's `disabled_nodes` (PG17+ planner-preference fidelity, review LOW): with
    // `enable_seqscan = off` the seqscan is marked disabled_nodes=1; without copying it our derived path would
    // stay 0 and dominate the very scan the user disabled. Correct results either way — this respects the GUC.
    path.disabled_nodes = unsafe { (*seq).disabled_nodes };
    cpath.flags = 0; // scan-type projection is handled by ExecScan's ps_ProjInfo — no PROJECTION flag needed
    cpath.custom_paths = std::ptr::null_mut(); // no children — this node IS the scan
    cpath.custom_private = std::ptr::null_mut(); // wanted is derived at begin from the plan
    cpath.custom_restrictinfo = std::ptr::null_mut(); // ⇒ create_customscan_plan hands us baserestrictinfo
    cpath.methods = &PATH_METHODS.0;
    unsafe { pg_sys::add_path(rel, cpath.into_pg() as *mut pg_sys::Path) };
}

/// Path → Plan (hand-rolled `make_custom_scan`). This node IS the table scan: `scanrelid = rel.relid`, no
/// children, no `custom_scan_tlist` (⇒ the executor uses `RelationGetDescr` + the columnar slot callbacks). The
/// WHERE clauses become the node's `qual`, so `ExecScan` re-checks the filter over the (projected) scan slot —
/// the same semantics as a plain seqscan (M149 leaves qual to the executor; the zone-map filter push-down is
/// the separate M150 milestone). NOTE: `create_customscan_plan` hands `clauses` as RestrictInfo nodes (unlike
/// the built-in scan planners it does NOT extract them for us), so we MUST `extract_actual_clauses` to bare
/// `Expr`s before assigning to `plan.qual` — `ExecInitQual` would choke on a RestrictInfo.
#[pg_guard]
unsafe extern "C-unwind" fn plan_custom_path(
    _root: *mut pg_sys::PlannerInfo,
    rel: *mut pg_sys::RelOptInfo,
    _best_path: *mut pg_sys::CustomPath,
    tlist: *mut pg_sys::List,
    clauses: *mut pg_sys::List,
    _custom_plans: *mut pg_sys::List,
) -> *mut pg_sys::Plan {
    let mut cscan =
        unsafe { PgBox::<pg_sys::CustomScan>::alloc_node(pg_sys::NodeTag::T_CustomScan) };
    {
        let plan = &mut cscan.scan.plan;
        plan.targetlist = tlist;
        // RestrictInfo list → bare AND-expr list (pseudoconstant=false, same call the built-in scan planners make).
        plan.qual = unsafe { pg_sys::extract_actual_clauses(clauses, false) };
        plan.lefttree = std::ptr::null_mut();
        plan.righttree = std::ptr::null_mut();
    }
    cscan.scan.scanrelid = unsafe { (*rel).relid };
    cscan.flags = 0;
    cscan.custom_plans = std::ptr::null_mut();
    cscan.custom_exprs = std::ptr::null_mut();
    cscan.custom_private = std::ptr::null_mut();
    cscan.custom_scan_tlist = std::ptr::null_mut(); // scanrelid > 0 ⇒ scan tuple desc = RelationGetDescr(rel)
    cscan.custom_relids = std::ptr::null_mut();
    cscan.methods = &SCAN_METHODS.0;
    cscan.into_pg() as *mut pg_sys::Plan
}

/// Exec state — `CustomScanState` FIRST (C-struct inheritance), plus the open columnar `TableScanDesc`. NO
/// `Rc` here (palloc0'd memory is freed without Drop) — the wanted set lives in `NODE_PROJECTIONS`.
#[repr(C)]
struct ProjScanState {
    css: pg_sys::CustomScanState,
    scandesc: pg_sys::TableScanDesc,
}

#[pg_guard]
unsafe extern "C-unwind" fn create_custom_scan_state(
    _cscan: *mut pg_sys::CustomScan,
) -> *mut pg_sys::Node {
    let ptr =
        unsafe { pg_sys::palloc0(std::mem::size_of::<ProjScanState>()) } as *mut ProjScanState;
    let st = unsafe { &mut *ptr };
    st.css.ss.ps.type_ = pg_sys::NodeTag::T_CustomScanState;
    st.css.methods = &EXEC_METHODS.0;
    st.scandesc = std::ptr::null_mut();
    ptr as *mut pg_sys::Node
}

/// Derive `wanted = pull_var_clause(targetlist) ∪ pull_var_clause(qual)` as 0-based attnos (ADR-2). `None` ⇒
/// fail-safe decode-ALL: any whole-row Var (`varattno == 0`), any system column (`varattno < 0`), any Var not
/// referencing this scan rel, or any non-Var node the walker surfaces. `RECURSE_*` flags mean the walker
/// descends into aggregate/window/placeholder sub-expressions and yields their leaf Vars (defense in depth —
/// a scan node's own tlist/qual should carry none). Mirrors Citus `ColumnarAttrNeeded`.
unsafe fn columns_needed(
    plan: *mut pg_sys::Plan,
    scanrelid: u32,
    natts: usize,
) -> Option<Vec<usize>> {
    let flags = (pg_sys::PVC_RECURSE_AGGREGATES
        | pg_sys::PVC_RECURSE_WINDOWFUNCS
        | pg_sys::PVC_RECURSE_PLACEHOLDERS) as c_int;
    let mut set: BTreeSet<usize> = BTreeSet::new();
    for &list in &[unsafe { (*plan).targetlist }, unsafe { (*plan).qual }] {
        if list.is_null() {
            continue;
        }
        let vars = unsafe { pg_sys::pull_var_clause(list as *mut pg_sys::Node, flags) };
        let vl = unsafe { PgList::<pg_sys::Node>::from_pg(vars) };
        for i in 0..vl.len() {
            let node = vl.get_ptr(i)?;
            if unsafe { (*node).type_ } != pg_sys::NodeTag::T_Var {
                return None; // e.g. a surviving PlaceHolderVar — decode all (correctness over cleverness)
            }
            let var = node as *mut pg_sys::Var;
            if unsafe { (*var).varno } as u32 != scanrelid {
                return None; // a Var from another rel / special varno (OUTER/INNER/INDEX) → decode all
            }
            let att = unsafe { (*var).varattno } as i32;
            if att <= 0 {
                return None; // whole-row (0) or system column (< 0) → decode all
            }
            let idx = (att - 1) as usize;
            if idx >= natts {
                return None; // out of range vs the live tupdesc → decode all
            }
            set.insert(idx);
        }
    }
    Some(set.into_iter().collect())
}

/// Exec init: derive `wanted` from THIS node's plan and stash it in the registry, then open the columnar scan.
/// Under `EXPLAIN` (without `ANALYZE`) the scan is NOT opened (EXEC_FLAG_EXPLAIN_ONLY) — the node just shows its
/// shape. `ss_currentRelation` was already opened by `ExecInitCustomScan` (scanrelid > 0) before this callback.
#[pg_guard]
unsafe extern "C-unwind" fn begin_custom_scan(
    node: *mut pg_sys::CustomScanState,
    estate: *mut pg_sys::EState,
    eflags: c_int,
) {
    let st = unsafe { &mut *(node as *mut ProjScanState) };
    if (eflags & pg_sys::EXEC_FLAG_EXPLAIN_ONLY as c_int) != 0 {
        st.scandesc = std::ptr::null_mut();
        return;
    }
    let rel = st.css.ss.ss_currentRelation;
    if rel.is_null() {
        pg_sys::error!("theodb columnar_project: scan relation not opened");
    }
    let natts = unsafe { (*(*rel).rd_att).natts } as usize;
    let cscan = st.css.ss.ps.plan as *mut pg_sys::CustomScan;
    let scanrelid = unsafe { (*cscan).scan.scanrelid };
    // Derive wanted from targetlist ∪ qual, and SYNCHRONIZE the registry on BOTH branches (review HIGH — ABA
    // hazard). A `ProjScanState` can be palloc'd at the address of a PRIOR node whose `end_custom_scan` was
    // longjmp'd over by a caught subxact abort (its `{addr → wanted}` entry stays orphaned — `subxact_clear`
    // evicts only the ACTIVE slot, not the registry, and `clear_all` runs only at top-xact end). If this node
    // takes the `None` fallback and did NOT evict, `exec` would `registry_get(addr)` the STALE mask and decode
    // only the dead node's columns — a silent wrong answer (the rest come back NULL). Removing on `None` makes
    // a reused address deterministic, mirroring the vecfilter's unconditional insert (`customscan.rs:572`).
    match unsafe { columns_needed(st.css.ss.ps.plan, scanrelid, natts) } {
        Some(wanted) => registry_insert(node as usize, wanted),
        None => registry_remove(node as usize),
    }
    // M150 — extract the pushable zone-map predicates from this node's qual, BEST-EFFORT (Citus
    // `ExtractPushdownClause`): keep the `col op const` clauses `extract_zone_predicate` accepts and DROP the rest
    // (function / OR / cross-type / two-Var) — the `ExecScan` re-checks the FULL qual, so an incomplete push is
    // still correct (ADR-2). UNCONDITIONAL insert (even empty) makes a reused address ABA-proof for this channel.
    let preds: Vec<ZonePredicate> = unsafe { predicates_needed(st.css.ss.ps.plan, scanrelid) };
    preds_registry_insert(node as usize, preds);
    // Open a real table scan over the columnar rel under the query snapshot (routes to columnar_scan_begin).
    st.scandesc =
        unsafe { pg_sys::table_beginscan(rel, (*estate).es_snapshot, 0, std::ptr::null_mut()) };
    st.css.ss.ss_currentScanDesc = st.scandesc;
}

/// M150 — the pushable `col op const` predicates from this node's `plan.qual`, best-effort (un-pushable clauses
/// dropped; `ExecScan` re-checks the full qual). Empty ⇒ no chunk-group skip. Mirrors `columns_needed`'s walk of
/// `(*plan).qual` but keeps the whole clause (not its leaf Vars). Vars in the qual carry `varno == scanrelid`.
unsafe fn predicates_needed(plan: *mut pg_sys::Plan, scanrelid: u32) -> Vec<ZonePredicate> {
    if unsafe { (*plan).qual.is_null() } {
        return Vec::new();
    }
    let quals = unsafe { PgList::<pg_sys::Node>::from_pg((*plan).qual) };
    let mut preds = Vec::with_capacity(quals.len());
    for i in 0..quals.len() {
        if let Some(clause) = quals.get_ptr(i) {
            if let Some(p) = unsafe { extract_zone_predicate(clause, scanrelid as i32) } {
                preds.push(p);
            }
        }
    }
    preds
}

/// Access method for `ExecScan`: fetch the next tuple into the scan slot. `columnar_scan_getnextslot` reads the
/// active projection (keyed by this scandesc) and materializes only `wanted`. `node` aliases `css.ss` (offset 0
/// of `ProjScanState`), so it round-trips to the state.
#[pg_guard]
unsafe extern "C-unwind" fn proj_access(
    node: *mut pg_sys::ScanState,
) -> *mut pg_sys::TupleTableSlot {
    let st = node as *mut ProjScanState;
    let slot = unsafe { (*node).ss_ScanTupleSlot };
    if unsafe { (*st).scandesc.is_null() } {
        return unsafe { pg_sys::ExecClearTuple(slot) };
    }
    if unsafe {
        pg_sys::table_scan_getnextslot(
            (*st).scandesc,
            pg_sys::ScanDirection::ForwardScanDirection,
            slot,
        )
    } {
        slot
    } else {
        unsafe { pg_sys::ExecClearTuple(slot) }
    }
}

/// Recheck method for `ExecScan`: always true. `theodb_columnar` is append-only (UPDATE/DELETE are error stubs),
/// so no EPQ (EvalPlanQual) recheck can fire; and `ExecScan` re-applies the node's `qual` over the fetched slot
/// regardless. There is nothing to re-validate here.
#[pg_guard]
unsafe extern "C-unwind" fn proj_recheck(
    _node: *mut pg_sys::ScanState,
    _slot: *mut pg_sys::TupleTableSlot,
) -> bool {
    true
}

/// Exec next tuple: install THIS scan's `wanted` (keyed by scandesc) for the pull window, then drive `ExecScan`
/// (which calls `proj_access` → `table_scan_getnextslot` → `columnar_scan_getnextslot`, then applies qual +
/// projection). The `ActiveGuard` restores the previous active slot after — even on a longjmp (RAII).
#[pg_guard]
unsafe extern "C-unwind" fn exec_custom_scan(
    node: *mut pg_sys::CustomScanState,
) -> *mut pg_sys::TupleTableSlot {
    let st = unsafe { &mut *(node as *mut ProjScanState) };
    if st.scandesc.is_null() {
        return std::ptr::null_mut(); // EXPLAIN-only or torn down
    }
    // `mine` absent ⇒ fallback (decode all): install no active slot for this scandesc.
    let active = registry_get(node as usize).map(|w| (st.scandesc as usize, w));
    let _guard = ActiveGuard::install(active);
    // M150 — install this scan's predicates for the same pull window (empty ⇒ no active slot ⇒ no skip). Same
    // scandesc keying as the projection guard; `PredGuard` restores the previous slot on return or longjmp.
    let pactive = preds_registry_get(node as usize)
        .filter(|p| !p.is_empty())
        .map(|p| (st.scandesc as usize, p));
    let _pguard = PredGuard::install(pactive);
    unsafe { pg_sys::ExecScan(&mut st.css.ss, Some(proj_access), Some(proj_recheck)) }
}

/// Exec teardown: drop this node's registry entry (frees the Vec) and end the columnar scan.
#[pg_guard]
unsafe extern "C-unwind" fn end_custom_scan(node: *mut pg_sys::CustomScanState) {
    let st = unsafe { &mut *(node as *mut ProjScanState) };
    registry_remove(node as usize);
    preds_registry_remove(node as usize); // M150 — free this node's predicate entry too
    if !st.scandesc.is_null() {
        unsafe { pg_sys::table_endscan(st.scandesc) };
        st.scandesc = std::ptr::null_mut();
        st.css.ss.ss_currentScanDesc = std::ptr::null_mut();
    }
}

/// ReScan (nested-loop inner re-execution): restart the columnar scan from the first stripe. `wanted` does not
/// change across rescans (same plan), so the registry entry is kept; exec re-installs it each pull window.
#[pg_guard]
unsafe extern "C-unwind" fn rescan_custom_scan(node: *mut pg_sys::CustomScanState) {
    let st = unsafe { &mut *(node as *mut ProjScanState) };
    if !st.scandesc.is_null() {
        unsafe { pg_sys::table_rescan(st.scandesc, std::ptr::null_mut()) };
    }
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    /// Build a columnar table `t_col` and an identical heap twin `t_heap` with `n` rows. Column layout:
    /// `a int, b int, c text` — enough to exercise projection (narrow subset) vs the full row.
    fn seed(n: i32) {
        Spi::run("SET theodb.enable_projection = on").unwrap();
        Spi::run("SET theodb.enable_columnar_agg = off").unwrap();
        Spi::run("SET max_parallel_workers_per_gather = 0").unwrap();
        Spi::run("DROP TABLE IF EXISTS t_col").unwrap();
        Spi::run("DROP TABLE IF EXISTS t_heap").unwrap();
        Spi::run("CREATE TABLE t_col (a int, b int, c text) USING theodb_columnar").unwrap();
        Spi::run("CREATE TABLE t_heap (a int, b int, c text)").unwrap();
        let ins = format!(
            "INSERT INTO {{tbl}} SELECT g, (g % 7) - 3, 'row-'||g FROM generate_series(1,{n}) g"
        );
        Spi::run(&ins.replace("{tbl}", "t_col")).unwrap();
        Spi::run(&ins.replace("{tbl}", "t_heap")).unwrap();
    }

    /// T1.1 — the projection CustomScan wins over the plain SeqScan for a narrow projection over a columnar
    /// table. RED (before registering the node): the plan is a plain `Seq Scan`, not our Custom Scan.
    #[pgrx::pg_test]
    fn test_projection_customscan_wins() {
        seed(50);
        let plan: String = Spi::connect(|c| {
            c.select("EXPLAIN (COSTS OFF) SELECT b FROM t_col", None, &[])
                .unwrap()
                .filter_map(|r| r.get::<String>(1).unwrap())
                .collect::<Vec<_>>()
                .join("\n")
        });
        assert!(
            plan.contains("Custom Scan (theodb_columnar_project)"),
            "narrow projection over a columnar table must use the projection Custom Scan (plan:\n{plan})"
        );
    }

    /// T1.2 — `wanted` is the UNION of targetlist and qual: a query that PROJECTS `a` but FILTERS on `b` must
    /// still materialize `b`, or the filter would see NULLs and drop/keep the wrong rows. Behavioral A/B (the
    /// `wanted` bitmap is not directly observable): the columnar result MUST equal the heap twin. RED (only
    /// targetlist projected): `b` NULL ⇒ `b <> 0` filters everything ⇒ empty result ≠ heap.
    #[pgrx::pg_test]
    fn test_columns_needed_unions_targetlist_and_qual() {
        seed(200);
        let q = "SELECT a FROM {tbl} WHERE b <> 0 ORDER BY a";
        let col: Vec<i32> = Spi::connect(|c| {
            c.select(&q.replace("{tbl}", "t_col"), None, &[])
                .unwrap()
                .filter_map(|r| r.get::<i32>(1).unwrap())
                .collect()
        });
        let heap: Vec<i32> = Spi::connect(|c| {
            c.select(&q.replace("{tbl}", "t_heap"), None, &[])
                .unwrap()
                .filter_map(|r| r.get::<i32>(1).unwrap())
                .collect()
        });
        assert!(!col.is_empty(), "the qual must not filter everything (b was materialized)");
        assert_eq!(col, heap, "projected-with-filter result must equal the heap twin");
    }

    /// T2.1 — a projected multi-column result (`a, b`) is byte-identical to the heap twin: the columns in
    /// `wanted` carry their real values, the ones dropped are never read.
    #[pgrx::pg_test]
    fn test_projected_result_byte_identical() {
        seed(200);
        let q = "SELECT a, b FROM {tbl} ORDER BY a";
        let col: Vec<(i32, i32)> = Spi::connect(|c| {
            c.select(&q.replace("{tbl}", "t_col"), None, &[])
                .unwrap()
                .filter_map(|r| Some((r.get::<i32>(1).unwrap()?, r.get::<i32>(2).unwrap()?)))
                .collect()
        });
        let heap: Vec<(i32, i32)> = Spi::connect(|c| {
            c.select(&q.replace("{tbl}", "t_heap"), None, &[])
                .unwrap()
                .filter_map(|r| Some((r.get::<i32>(1).unwrap()?, r.get::<i32>(2).unwrap()?)))
                .collect()
        });
        assert_eq!(col, heap, "projected (a,b) must equal the heap twin");
    }

    /// T2.2 — every explicit column projected (`SELECT a, b, c`): `wanted = {0,1,2}` (all), result
    /// byte-identical to the heap twin. NOTE: this exercises the all-columns `Some` case (PG expands the list
    /// to per-column Vars), NOT the `None` decode-all fallback — that floor is covered by
    /// `test_fallback_system_column` below (review: this test was mislabeled "whole-row/SELECT *").
    #[pgrx::pg_test]
    fn test_fallback_whole_row() {
        seed(200);
        let q = "SELECT a, b, c FROM {tbl} ORDER BY a";
        let col: Vec<(i32, i32, String)> = Spi::connect(|c| {
            c.select(&q.replace("{tbl}", "t_col"), None, &[])
                .unwrap()
                .filter_map(|r| {
                    Some((
                        r.get::<i32>(1).unwrap()?,
                        r.get::<i32>(2).unwrap()?,
                        r.get::<String>(3).unwrap()?,
                    ))
                })
                .collect()
        });
        let heap: Vec<(i32, i32, String)> = Spi::connect(|c| {
            c.select(&q.replace("{tbl}", "t_heap"), None, &[])
                .unwrap()
                .filter_map(|r| {
                    Some((
                        r.get::<i32>(1).unwrap()?,
                        r.get::<i32>(2).unwrap()?,
                        r.get::<String>(3).unwrap()?,
                    ))
                })
                .collect()
        });
        assert_eq!(col, heap, "SELECT * (all columns) must equal the heap twin");
    }

    /// T2.2b — the REAL decode-all `None` fallback (ADR-2 correctness floor): a system column (`ctid`,
    /// `varattno < 0`) makes `columns_needed` return `None`, so `want_mask` is all-true and every column is
    /// materialized. The user columns projected alongside must still be byte-identical to the heap twin.
    /// This is the case `test_fallback_whole_row` claimed but never reached (review HIGH — tests).
    #[pgrx::pg_test]
    fn test_fallback_system_column() {
        seed(200);
        let q = "SELECT a, b FROM {tbl} WHERE ctid IS NOT NULL ORDER BY a";
        let col: Vec<(i32, i32)> = Spi::connect(|c| {
            c.select(&q.replace("{tbl}", "t_col"), None, &[])
                .unwrap()
                .filter_map(|r| Some((r.get::<i32>(1).unwrap()?, r.get::<i32>(2).unwrap()?)))
                .collect()
        });
        let heap: Vec<(i32, i32)> = Spi::connect(|c| {
            c.select(&q.replace("{tbl}", "t_heap"), None, &[])
                .unwrap()
                .filter_map(|r| Some((r.get::<i32>(1).unwrap()?, r.get::<i32>(2).unwrap()?)))
                .collect()
        });
        assert_eq!(
            col, heap,
            "system-column ctid forces None fallback (decode-all); result must equal heap"
        );
    }

    /// T2.3 — nested/self-join correctness: the scandesc-keyed side channel exists solely to stop a nested
    /// columnar scan from being projected by an outer scan's column indices. A self-join with DIFFERENT
    /// projections on each side (`x.a` vs `y.b`) drives two live columnar scans in one plan; drop the scandesc
    /// key and one side would materialize the wrong column → wrong join result. Must equal the heap twin.
    #[pgrx::pg_test]
    fn test_nested_self_join_projection() {
        seed(300);
        let q = "SELECT x.a, y.b FROM {tbl} x JOIN {tbl} y ON x.a = y.a WHERE x.b <> 0 ORDER BY x.a LIMIT 50";
        let col: Vec<(i32, i32)> = Spi::connect(|c| {
            c.select(&q.replace("{tbl}", "t_col"), None, &[])
                .unwrap()
                .filter_map(|r| Some((r.get::<i32>(1).unwrap()?, r.get::<i32>(2).unwrap()?)))
                .collect()
        });
        let heap: Vec<(i32, i32)> = Spi::connect(|c| {
            c.select(&q.replace("{tbl}", "t_heap"), None, &[])
                .unwrap()
                .filter_map(|r| Some((r.get::<i32>(1).unwrap()?, r.get::<i32>(2).unwrap()?)))
                .collect()
        });
        assert_eq!(
            col, heap,
            "self-join with per-side projections must equal the heap twin (scandesc keying)"
        );
    }

    /// T2.4 — regression for the ABA HIGH (stale registry after a caught subxact abort). A PL/pgSQL block
    /// runs a narrow-projection query, raises inside a subxact (aborting it and skipping `end_custom_scan`),
    /// the exception is caught, then a `None`-fallback query (`ctid`) runs in the SAME transaction. Before the
    /// fix (`registry_remove` on the `None` branch), a `ProjScanState` reusing the aborted node's address
    /// would inherit its stale mask and return NULLs; after the fix the fallback decodes all columns. Assert
    /// the post-abort query equals the heap twin (RED with the bug, GREEN with the fix).
    #[pgrx::pg_test]
    fn test_subxact_abort_no_stale_projection() {
        seed(200);
        // Q1 (narrow projection) inside a caught subxact that aborts mid-scan; then Q2 (None fallback) same txn.
        Spi::run(
            "DO $$ BEGIN \
               BEGIN \
                 PERFORM b FROM t_col WHERE b <> 0 AND (1.0/(a-a)) > 0; \
               EXCEPTION WHEN division_by_zero THEN NULL; END; \
             END $$",
        )
        .unwrap();
        // Q2: None fallback (ctid) — with the bug a reused node address would inherit Q1's mask.
        let col: Vec<(i32, i32, String)> = Spi::connect(|c| {
            c.select("SELECT a, b, c FROM t_col WHERE ctid IS NOT NULL ORDER BY a", None, &[])
                .unwrap()
                .filter_map(|r| {
                    Some((
                        r.get::<i32>(1).unwrap()?,
                        r.get::<i32>(2).unwrap()?,
                        r.get::<String>(3).unwrap()?,
                    ))
                })
                .collect()
        });
        let heap: Vec<(i32, i32, String)> = Spi::connect(|c| {
            c.select("SELECT a, b, c FROM t_heap WHERE ctid IS NOT NULL ORDER BY a", None, &[])
                .unwrap()
                .filter_map(|r| {
                    Some((
                        r.get::<i32>(1).unwrap()?,
                        r.get::<i32>(2).unwrap()?,
                        r.get::<String>(3).unwrap()?,
                    ))
                })
                .collect()
        });
        assert_eq!(
            col, heap,
            "post-subxact-abort fallback query must not inherit a stale projection mask"
        );
    }

    // ---- M150 — chunk-group skip (zone-map) tests ----

    /// Build a columnar table + heap twin with `n` rows where `a = g` is MONOTONE (so each 10k-row chunk group
    /// has a tight, non-overlapping min/max — the regime where zone-map skip actually prunes). `b = (g%7)-3`.
    fn seed_clustered(n: i32) {
        Spi::run("SET theodb.enable_projection = on").unwrap();
        Spi::run("SET theodb.enable_chunk_skip = on").unwrap();
        Spi::run("SET theodb.enable_columnar_agg = off").unwrap();
        Spi::run("SET max_parallel_workers_per_gather = 0").unwrap();
        // B-015 — WHY the three skip tests read `scanned 0`, and why the fix is here and not in the assertions.
        //
        // `#[pg_test]` runs each test inside ONE transaction (rolled back at the end). The columnar writer holds
        // rows in the pending set and only materializes a durable stripe when the buffer exceeds
        // `maintenance_work_mem`, or at pre-commit (M104 — see `columnar.rs` `PendingWrite`). Under the default
        // 64 MB, 50 000 narrow rows never reach that bound and the commit never arrives, so the scan reads the
        // pending set: there are no chunk groups in existence, hence `scanned=0`, hence `skipped=0`.
        //
        // Measured on the shipped binary, same session, same rows: inside a transaction → `skipped=0 scanned=0`;
        // after COMMIT → `skipped=4 scanned=5`; inside a transaction with a small `maintenance_work_mem` →
        // `skipped=30 scanned=31`. The zone map was pruning the whole time — the fixture just never produced
        // anything for it to prune.
        //
        // Lowering the bound makes the incremental flush the product already implements actually happen here, so
        // the test measures the thing it claims to measure. The DoD for B-015 explicitly forbids the other
        // route — relaxing the assertion — which would have hidden a working feature behind a green test.
        Spi::run("SET maintenance_work_mem = '64kB'").unwrap();
        Spi::run("DROP TABLE IF EXISTS t_col").unwrap();
        Spi::run("DROP TABLE IF EXISTS t_heap").unwrap();
        Spi::run("CREATE TABLE t_col (a int, b int, c text) USING theodb_columnar").unwrap();
        Spi::run("CREATE TABLE t_heap (a int, b int, c text)").unwrap();
        let ins = format!(
            "INSERT INTO {{tbl}} SELECT g, (g % 7) - 3, 'row-'||g FROM generate_series(1,{n}) g"
        );
        Spi::run(&ins.replace("{tbl}", "t_col")).unwrap();
        Spi::run(&ins.replace("{tbl}", "t_heap")).unwrap();
    }

    fn col_ints(sql: &str) -> Vec<i32> {
        Spi::connect(|c| {
            c.select(sql, None, &[]).unwrap().filter_map(|r| r.get::<i32>(1).unwrap()).collect()
        })
    }
    fn chunks_skipped() -> i64 {
        Spi::get_one::<i64>("SELECT theodb_columnar_chunks_skipped()").unwrap().unwrap()
    }
    fn chunks_scanned() -> i64 {
        Spi::get_one::<i64>("SELECT theodb_columnar_chunks_scanned()").unwrap().unwrap()
    }

    /// T3.1 — a highly selective `a = const` over a clustered columnar table PRUNES chunk groups (the min/max of
    /// every non-matching group excludes the const) AND the result is byte-identical to the heap twin. RED before
    /// the skip wiring: `chunks_skipped == 0` (decode_stripe decoded every group). GREEN: > 0 with the SAME rows.
    #[pgrx::pg_test]
    fn test_chunk_skip_prunes_and_ab_identical() {
        seed_clustered(50_000); // 5 chunk groups of 10k
        let col = col_ints("SELECT a FROM t_col WHERE a = 25000 ORDER BY a");
        let sk = chunks_skipped();
        let sc = chunks_scanned();
        let heap = col_ints("SELECT a FROM t_heap WHERE a = 25000 ORDER BY a");
        assert_eq!(
            col, heap,
            "chunk-skip result must equal the heap twin (skip is admission-only)"
        );
        assert_eq!(col, vec![25000], "the one matching row must be present");
        assert!(sc >= 2, "the table must span >= 2 chunk groups to prove pruning (scanned {sc})");
        assert!(sk > 0, "a selective `a = 25000` must prune >= 1 chunk group (skipped {sk}/{sc})");
    }

    /// T3.1 (correctness fail-safe) — the skip NEVER loses a row: a value that EXISTS lands inside its chunk's
    /// [min,max], so that chunk is NOT skipped and the row is returned; ranges spanning a boundary keep every
    /// matching row. A/B vs heap over several predicate shapes (Eq present, range, `>` tail, `<` head).
    #[pgrx::pg_test]
    fn test_skip_never_loses_row() {
        seed_clustered(50_000);
        for pred in
            ["a = 24999", "a BETWEEN 19998 AND 20003", "a > 49995", "a < 4", "a = 1", "a = 50000"]
        {
            let q = format!("SELECT a FROM {{tbl}} WHERE {pred} ORDER BY a");
            let col = col_ints(&q.replace("{tbl}", "t_col"));
            let heap = col_ints(&q.replace("{tbl}", "t_heap"));
            assert_eq!(
                col, heap,
                "predicate `{pred}` must not lose or gain a row vs the heap twin"
            );
        }
    }

    /// T4.1 — the `theodb.enable_chunk_skip` GUC gates the skip: OFF ⇒ `chunks_skipped == 0` (the pre-M150 path),
    /// ON ⇒ `> 0`, and the RESULT is identical either way (the A/B OFF-vs-ON ablation on the same binary).
    #[pgrx::pg_test]
    fn test_enable_chunk_skip_guc_off_disables_skip() {
        seed_clustered(50_000);
        Spi::run("SET theodb.enable_chunk_skip = off").unwrap();
        let col_off = col_ints("SELECT a FROM t_col WHERE a = 25000 ORDER BY a");
        let sk_off = chunks_skipped();
        Spi::run("SET theodb.enable_chunk_skip = on").unwrap();
        let col_on = col_ints("SELECT a FROM t_col WHERE a = 25000 ORDER BY a");
        let sk_on = chunks_skipped();
        assert_eq!(sk_off, 0, "with the GUC off no chunk group may be skipped (got {sk_off})");
        assert!(sk_on > 0, "with the GUC on the selective predicate must prune (got {sk_on})");
        assert_eq!(col_off, col_on, "the result must be identical with skip off vs on");
    }

    /// T2.2 — best-effort pushdown: an un-pushable qual (`OR`) pushes NOTHING (`chunks_skipped == 0`) but stays
    /// correct (ExecScan re-checks); a mixed `AND` still pushes the simple conjunct and prunes. A/B in both.
    #[pgrx::pg_test]
    fn test_predicate_pushdown_best_effort() {
        seed_clustered(50_000);
        // OR → not pushable → no skip, but the executor still applies it correctly.
        let col_or = col_ints("SELECT a FROM t_col WHERE a = 25000 OR a = 40000 ORDER BY a");
        let sk_or = chunks_skipped();
        let heap_or = col_ints("SELECT a FROM t_heap WHERE a = 25000 OR a = 40000 ORDER BY a");
        assert_eq!(col_or, heap_or, "un-pushable OR must still return the correct rows");
        assert_eq!(sk_or, 0, "an OR qual pushes no predicate → no chunk skip (got {sk_or})");
        // Mixed AND with a non-pushable function conjunct → the simple `a = 25000` still pushes and prunes.
        let col_and =
            col_ints("SELECT a FROM t_col WHERE a = 25000 AND lower(c) = 'row-25000' ORDER BY a");
        let sk_and = chunks_skipped();
        let heap_and =
            col_ints("SELECT a FROM t_heap WHERE a = 25000 AND lower(c) = 'row-25000' ORDER BY a");
        assert_eq!(col_and, heap_and, "mixed AND must return the correct rows");
        assert!(
            sk_and > 0,
            "the pushable `a = 25000` conjunct must prune despite the function (got {sk_and})"
        );
    }

    /// Review LOW/MEDIUM (rust-pgrx council) — predicate-channel isolation across TWO DIFFERENT columnar tables
    /// scanned in the same query, each with its OWN pushable predicate. If the scandesc keying leaked, one scan
    /// would skip by the OTHER's predicate and lose rows. A cross join `c1.a=K1 × c2.a=K2` (each selective) must
    /// equal the heap twin — proving `scan_predicates(scandesc)` returns each scan ONLY its own predicates.
    #[pgrx::pg_test]
    fn test_two_table_predicate_isolation() {
        seed_clustered(50_000); // t_col / t_heap
        Spi::run("DROP TABLE IF EXISTS t_col2").unwrap();
        Spi::run("DROP TABLE IF EXISTS t_heap2").unwrap();
        Spi::run("CREATE TABLE t_col2 (a int, b int, c text) USING theodb_columnar").unwrap();
        Spi::run("CREATE TABLE t_heap2 (a int, b int, c text)").unwrap();
        let ins = "INSERT INTO {tbl} SELECT g,(g%7)-3,'r2-'||g FROM generate_series(1,50000) g";
        Spi::run(&ins.replace("{tbl}", "t_col2")).unwrap();
        Spi::run(&ins.replace("{tbl}", "t_heap2")).unwrap();
        // Each side selective on a DIFFERENT value → each scan pushes a distinct predicate.
        let q =
            "SELECT x.a, y.a FROM {t1} x, {t2} y WHERE x.a = 25000 AND y.a = 35000 ORDER BY 1,2";
        let col: Vec<(i32, i32)> = Spi::connect(|c| {
            c.select(&q.replace("{t1}", "t_col").replace("{t2}", "t_col2"), None, &[])
                .unwrap()
                .filter_map(|r| Some((r.get::<i32>(1).unwrap()?, r.get::<i32>(2).unwrap()?)))
                .collect()
        });
        let heap: Vec<(i32, i32)> = Spi::connect(|c| {
            c.select(&q.replace("{t1}", "t_heap").replace("{t2}", "t_heap2"), None, &[])
                .unwrap()
                .filter_map(|r| Some((r.get::<i32>(1).unwrap()?, r.get::<i32>(2).unwrap()?)))
                .collect()
        });
        assert_eq!(
            col,
            vec![(25000, 35000)],
            "the one cross-join pair must survive both predicates"
        );
        assert_eq!(
            col, heap,
            "two-table cross join must equal the heap twin (predicate channels isolated)"
        );
    }

    /// T2.1 — ABA regression for the PREDICATE channel (mirror of `test_subxact_abort_no_stale_projection`). A
    /// PL/pgSQL block runs a selective query (installing predicates), raises inside a subxact (aborting it and
    /// skipping `end_custom_scan`), then a DIFFERENT-shape query reuses the address. The fallback must NOT inherit
    /// stale predicates and skip rows that its own (absent) predicate would keep — A/B vs the heap twin.
    #[pgrx::pg_test]
    fn test_subxact_abort_no_stale_predicate() {
        seed_clustered(50_000);
        Spi::run(
            "DO $$ BEGIN
                 BEGIN
                     PERFORM a FROM t_col WHERE a = 25000 AND (1.0/(a-a)) > 0;
                 EXCEPTION WHEN division_by_zero THEN NULL;
                 END;
             END $$",
        )
        .unwrap();
        // A whole-table query (no pushable predicate ⇒ preds empty ⇒ zero skip) must return ALL rows — if it
        // inherited the aborted subxact's `a = 25000` predicate it would prune chunk groups and lose rows.
        let col = col_ints("SELECT a FROM t_col ORDER BY a");
        let heap = col_ints("SELECT a FROM t_heap ORDER BY a");
        assert_eq!(col.len(), 50_000, "the fallback full scan must return every row");
        assert_eq!(
            col, heap,
            "post-subxact-abort full scan must not inherit a stale skip predicate"
        );
    }
}
