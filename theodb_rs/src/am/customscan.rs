//! M92 spike v0 — Custom Scan Provider scaffold (pass-through).
//!
//! Goal of THIS spike (ADR M92-3, de-risk unknown #1): prove that a Postgres Custom Scan Provider can be
//! hand-rolled in pgrx 0.16.1 (pg17) WITHOUT the absent `create_customscan_path`/`make_custom_scan` inline
//! helpers — i.e. register the methods, install `set_rel_pathlist_hook`, add a `CustomPath` that wins, plan it
//! into a `CustomScan`, execute it, and have `EXPLAIN` show the node. v0 is a PURE PASS-THROUGH: it wraps the
//! rel's existing cheapest path as a child and just forwards its tuples — so a correct result proves the
//! lifecycle end-to-end. The bitmap membership + MVCC recheck (unknown #2, ADR M92-1) layer on top in v1.
//!
//! SAFETY POSTURE: the whole hook is gated behind the GUC `theodb.enable_vecfilter` (default OFF). A planner
//! hook that misbehaves breaks EVERY query on the instance, so the spike is inert until explicitly enabled.
//! Every callback is `extern "C-unwind"` and routes corrupt/unexpected state to `pg_sys::error!`, never a panic.

use pgrx::pg_sys;
use pgrx::prelude::*;
use pgrx::{PgBox, PgList};
use std::cell::RefCell;
use std::collections::HashSet;
use std::os::raw::c_int;
use std::rc::Rc;

// ---- M92 v1a — TID membership side channel (Custom Scan node → index AM) ----
//
// The bitmap from a native sub-plan is UNORDERED and cannot ride a ScanKey (a TIDBitmap is not a SQL operator),
// so the Custom Scan node hands the materialized membership set to the index AM through this backend-local channel
// (a Postgres backend serves one query at a time — no cross-backend sharing). The AM's `amrescan` reads it and the
// Stage-1 scan skips non-member candidates inline (the M90 mechanism generalized from label-overlap to TID
// membership). MVCC correctness still relies on the node re-running the original qual on the heap tuple (v1c):
// membership is an *admission* filter, never the final authority (lossy bitmap pages + the pending region can
// over-admit).
/// The admission set the AM Stage-1 filters by. `exact` holds encoded TIDs (`tid::encode` = `(block<<16)|offset`)
/// from non-lossy bitmap pages; `lossy` holds block numbers whose per-offset detail the bitmap dropped under memory
/// pressure (`ntuples < 0`). A candidate is ADMITTED when its exact TID is in `exact` OR its block is in `lossy` —
/// the lossy case over-admits every candidate on the block, which the Custom Scan node's `ExecQual` recheck then
/// filters (membership is an admission filter, never the final authority). Dropping lossy blocks would UNDER-admit
/// (silently miss valid rows) — the correctness bug ADR M93-2 guards against.
pub(crate) struct Membership {
    pub exact: HashSet<i64>,
    pub lossy: HashSet<u32>,
}

thread_local! {
    static SCAN_MEMBERSHIP: RefCell<Option<Rc<Membership>>> = const { RefCell::new(None) };
}

/// Set the TID membership the next index scan in THIS backend must filter by. `None` clears it. Called by the Custom
/// Scan node's `BeginCustomScan` before driving the AM scan.
pub(crate) fn set_membership(m: Option<Membership>) {
    SCAN_MEMBERSHIP.with(|c| *c.borrow_mut() = m.map(Rc::new));
}

/// Read the active membership (cheap Rc clone) for this backend, or `None`. Read by the AM Stage-1 scan.
pub(crate) fn membership() -> Option<Rc<Membership>> {
    SCAN_MEMBERSHIP.with(|c| c.borrow().clone())
}

/// Whether a membership filter is active (used by `amrescan` to keep `xs_recheck` on).
pub(crate) fn has_membership() -> bool {
    SCAN_MEMBERSHIP.with(|c| c.borrow().is_some())
}

/// M92 v1b — materialize a native `TIDBitmap` (produced by a bitmap sub-plan's `MultiExecProcNode`) into the
/// membership representation the AM Stage-1 consumes: a set of EXACT encoded TIDs (`(block<<16)|offset`, matching
/// `tid::encode`) plus a set of LOSSY block numbers. A page goes lossy under memory pressure (`ntuples < 0`,
/// `tidbitmap.h`) — its individual offsets are forgotten, so only the block is known and every candidate on that
/// block must be ADMITTED then rechecked on the heap (the executor / Custom Scan node re-runs the real qual). The
/// exact set is authoritative; the lossy set over-admits (safe under recheck).
pub(crate) unsafe fn materialize_bitmap(tbm: *mut pg_sys::TIDBitmap) -> (HashSet<i64>, HashSet<u32>) {
    let mut exact: HashSet<i64> = HashSet::new();
    let mut lossy: HashSet<u32> = HashSet::new();
    let iter = pg_sys::tbm_begin_iterate(tbm);
    loop {
        let res = pg_sys::tbm_iterate(iter);
        if res.is_null() {
            break;
        }
        let r = &*res;
        if r.ntuples < 0 {
            lossy.insert(r.blockno); // lossy page — offsets gone; admit-then-recheck by block
        } else {
            let offs = r.offsets.as_slice(r.ntuples as usize);
            for &off in offs {
                exact.insert(((r.blockno as i64) << 16) | (off as i64));
            }
        }
    }
    pg_sys::tbm_end_iterate(iter);
    (exact, lossy)
}

// ---- static method tables (registered once; addresses are stable for the postmaster lifetime) ----
//
// The method tables hold a `*const c_char` (CustomName) + raw fn pointers, so they are not `Sync` and cannot be
// plain statics. They are immutable and read only by Postgres (single-threaded per backend), so a newtype with a
// hand-asserted `Sync` is the standard, safe wrapper.
struct Methods<T>(T);
unsafe impl<T> Sync for Methods<T> {}

static PATH_METHODS: Methods<pg_sys::CustomPathMethods> = Methods(pg_sys::CustomPathMethods {
    CustomName: c"theodb_vecfilter".as_ptr(),
    PlanCustomPath: Some(plan_custom_path),
    ReparameterizeCustomPathByChild: None,
});

static SCAN_METHODS: Methods<pg_sys::CustomScanMethods> = Methods(pg_sys::CustomScanMethods {
    CustomName: c"theodb_vecfilter".as_ptr(),
    CreateCustomScanState: Some(create_custom_scan_state),
});

static EXEC_METHODS: Methods<pg_sys::CustomExecMethods> = Methods(pg_sys::CustomExecMethods {
    CustomName: c"theodb_vecfilter".as_ptr(),
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

// The previous `set_rel_pathlist_hook` in the chain (Postgres allows only one; we must call it).
static mut PREV_HOOK: pg_sys::set_rel_pathlist_hook_type = None;

/// Register the Custom Scan methods + install the pathlist hook. Called from `_PG_init`.
pub fn init() {
    unsafe {
        pg_sys::RegisterCustomScanMethods(&SCAN_METHODS.0);
        PREV_HOOK = pg_sys::set_rel_pathlist_hook;
        pg_sys::set_rel_pathlist_hook = Some(pathlist_hook);
    }
}

/// M93 — the node's exec-time state. Embeds `CustomScanState` as the FIRST field (nodeCustom.c:34-42 sanctions the
/// provider allocating a larger struct), so `*mut VecFilterState` and `*mut CustomScanState` are interchangeable.
/// Holds the two child PlanStates: the vector-ordered index scan (which carries the scalar `WHERE` as its own qpqual
/// Filter — that Filter IS the MVCC recheck of the lossy/pending over-admits) and the bitmap sub-plan (MultiExec'd to
/// a TIDBitmap for the membership).
#[repr(C)]
struct VecFilterState {
    css: pg_sys::CustomScanState,
    vector_child: *mut pg_sys::PlanState,
    bitmap_child: *mut pg_sys::PlanState,
    membership_active: bool,
}

/// The planner hook: intercept a filtered vector query — a base rel that has BOTH a vector-ordered path (an
/// `IndexPath` with distance `pathkeys`, from `ORDER BY e <-> q`) AND a `BitmapHeapPath` (the planner's native bitmap
/// over the scalar `WHERE`, Rule 9 — reuse its BitmapAnd/Or composition). Build a 2-child `CustomPath`
/// `[vector_ordered, bitmapqual]`. Anything else (no order, or no indexable filter) is left untouched. Gated behind
/// `theodb.enable_vecfilter` (default OFF).
#[pg_guard]
unsafe extern "C-unwind" fn pathlist_hook(
    root: *mut pg_sys::PlannerInfo,
    rel: *mut pg_sys::RelOptInfo,
    rti: pg_sys::Index,
    rte: *mut pg_sys::RangeTblEntry,
) {
    if let Some(prev) = PREV_HOOK {
        prev(root, rel, rti, rte);
    }
    if !crate::am::guc::vecfilter_enabled() {
        return;
    }
    let relref = &mut *rel;
    if relref.reloptkind != pg_sys::RelOptKind::RELOPT_BASEREL {
        return;
    }
    // Find the vector-ordered path (the only base path with pathkeys — our index's distance ORDER BY) + a
    // BitmapHeapPath (the scalar filter). Both must exist for this to be a filtered vector query worth intercepting.
    let paths = PgList::<pg_sys::Path>::from_pg(relref.pathlist);
    let mut vector_path: *mut pg_sys::Path = std::ptr::null_mut();
    let mut bitmap_path: *mut pg_sys::Path = std::ptr::null_mut();
    for i in 0..paths.len() {
        if let Some(p) = paths.get_ptr(i) {
            if !(*p).pathkeys.is_null() && vector_path.is_null() {
                vector_path = p;
            }
            if (*p).pathtype == pg_sys::NodeTag::T_BitmapHeapScan && bitmap_path.is_null() {
                bitmap_path = p;
            }
        }
    }
    if vector_path.is_null() || bitmap_path.is_null() {
        return;
    }

    // Hand-roll `create_customscan_path` — a 2-child CustomPath. Preserve the vector path's pathkeys so the node
    // reports distance-ordered output (no redundant Sort above it). Cost below the vector path so the planner picks
    // it (past the 1% add_path fuzz factor — spike posture; the real cost model is a follow-up).
    let mut cpath = PgBox::<pg_sys::CustomPath>::alloc_node(pg_sys::NodeTag::T_CustomPath);
    let path = &mut cpath.path;
    path.pathtype = pg_sys::NodeTag::T_CustomScan;
    path.parent = rel;
    path.pathtarget = relref.reltarget;
    path.param_info = std::ptr::null_mut();
    path.pathkeys = (*vector_path).pathkeys;
    path.rows = (*vector_path).rows;
    // Cost below the CHEAPEST base path so the ordered node beats even a selective bitmap+Sort (which is very cheap
    // at high selectivity). Spike posture: force selection to prove correctness; an honest cost model (membership-
    // reduced candidate count) is a follow-up. `startup=0` so a LIMIT prefers it.
    let mut min_cost = (*vector_path).total_cost;
    for i in 0..paths.len() {
        if let Some(p) = paths.get_ptr(i) {
            if (*p).total_cost < min_cost {
                min_cost = (*p).total_cost;
            }
        }
    }
    path.startup_cost = 0.0;
    path.total_cost = min_cost * 0.1;
    cpath.flags = 0;
    let mut children = PgList::<pg_sys::Path>::new();
    children.push(vector_path); // [0] — ordered + carries the scalar Filter (the recheck)
    children.push(bitmap_path); // [1] — the whole BitmapHeapPath; its planned BitmapHeapScan.lefttree is the
                                //        bitmap-producing sub-plan (BitmapIndexScan/BitmapAnd) we MultiExec
    cpath.custom_paths = children.into_pg();
    cpath.custom_private = std::ptr::null_mut();
    cpath.methods = &PATH_METHODS.0;
    pg_sys::add_path(rel, cpath.into_pg() as *mut pg_sys::Path);
}

/// Path -> Plan: hand-roll `make_custom_scan`. `custom_plans` holds the planned [vector, bitmap] children. The
/// node's own `qual` stays empty — the vector child's own qpqual Filter is the recheck (the scalar `WHERE` was
/// attached to the vector IndexScan by the planner), so the node does not re-filter.
#[pg_guard]
unsafe extern "C-unwind" fn plan_custom_path(
    _root: *mut pg_sys::PlannerInfo,
    rel: *mut pg_sys::RelOptInfo,
    _best_path: *mut pg_sys::CustomPath,
    tlist: *mut pg_sys::List,
    _clauses: *mut pg_sys::List,
    custom_plans: *mut pg_sys::List,
) -> *mut pg_sys::Plan {
    let mut cscan = PgBox::<pg_sys::CustomScan>::alloc_node(pg_sys::NodeTag::T_CustomScan);
    let plan = &mut cscan.scan.plan;
    plan.targetlist = tlist;
    plan.qual = std::ptr::null_mut(); // recheck lives on the vector child's Filter, not on the node
    plan.lefttree = std::ptr::null_mut();
    plan.righttree = std::ptr::null_mut();
    cscan.scan.scanrelid = (*rel).relid;
    cscan.flags = 0;
    cscan.custom_plans = custom_plans; // [vector_plan, bitmap_plan]
    cscan.custom_exprs = std::ptr::null_mut();
    cscan.custom_private = std::ptr::null_mut();
    cscan.custom_scan_tlist = std::ptr::null_mut();
    cscan.custom_relids = std::ptr::null_mut();
    cscan.methods = &SCAN_METHODS.0;
    cscan.into_pg() as *mut pg_sys::Plan
}

/// Plan -> ScanState: allocate the `VecFilterState` (embeds CustomScanState first) via `palloc0` and set the node
/// tag + exec methods by hand (`PgBox::alloc_node` only knows the base struct).
#[pg_guard]
unsafe extern "C-unwind" fn create_custom_scan_state(_cscan: *mut pg_sys::CustomScan) -> *mut pg_sys::Node {
    let ptr = pg_sys::palloc0(std::mem::size_of::<VecFilterState>()) as *mut VecFilterState;
    let st = &mut *ptr;
    st.css.ss.ps.type_ = pg_sys::NodeTag::T_CustomScanState;
    st.css.methods = &EXEC_METHODS.0;
    ptr as *mut pg_sys::Node
}

/// Exec init: init both children; MultiExec the bitmap child → TIDBitmap → materialize → `set_membership`. The
/// membership is set BEFORE the vector child is first pulled (in `exec`), so the AM Stage-1 pre-filters by it.
#[pg_guard]
unsafe extern "C-unwind" fn begin_custom_scan(
    node: *mut pg_sys::CustomScanState,
    estate: *mut pg_sys::EState,
    eflags: c_int,
) {
    // Defensive: clear any membership a prior scan in this backend left behind on an `error!` longjmp (anti-leak).
    set_membership(None);
    let st = &mut *(node as *mut VecFilterState);
    let cscan = st.css.ss.ps.plan as *mut pg_sys::CustomScan;
    let planned = PgList::<pg_sys::Plan>::from_pg((*cscan).custom_plans);
    let vplan = match planned.get_ptr(0) {
        Some(p) => p,
        None => pg_sys::error!("theodb vecfilter: missing vector child plan"),
    };
    let bplan = match planned.get_ptr(1) {
        Some(p) => p,
        None => pg_sys::error!("theodb vecfilter: missing bitmap child plan"),
    };
    // `bplan` is the planned BitmapHeapScan; its `lefttree` is the bitmap-PRODUCING sub-plan (BitmapIndexScan or
    // BitmapAnd) — the node MultiExecs THAT, not the heap scan. (`create_plan(bitmapqual)` alone would make a plain
    // IndexScan, which MultiExecProcNode rejects as "unrecognized node type: T_IndexScanState".)
    let bitmap_subplan = (*bplan).lefttree;
    if bitmap_subplan.is_null() {
        pg_sys::error!("theodb vecfilter: BitmapHeapScan has no bitmap sub-plan");
    }
    st.vector_child = pg_sys::ExecInitNode(vplan, estate, eflags);
    st.bitmap_child = pg_sys::ExecInitNode(bitmap_subplan, estate, eflags);
    // Show the vector child under the node in EXPLAIN.
    let mut ps_list = PgList::<pg_sys::PlanState>::new();
    ps_list.push(st.vector_child);
    st.css.custom_ps = ps_list.into_pg();
    // Under `EXPLAIN` without `ANALYZE` (EXEC_FLAG_EXPLAIN_ONLY) the children are init'd for DISPLAY only, not for
    // execution — `MultiExecProcNode` on an un-run bitmap child would crash. Skip the bitmap materialization; the
    // node just shows its shape.
    if (eflags & pg_sys::EXEC_FLAG_EXPLAIN_ONLY as c_int) != 0 {
        return;
    }
    // MultiExec the bitmap → TIDBitmap (nodeBitmapHeapscan.c:110 pattern; tag-check, not the absent `IsA`).
    let res = pg_sys::MultiExecProcNode(st.bitmap_child);
    if res.is_null() || (*(res as *mut pg_sys::Node)).type_ != pg_sys::NodeTag::T_TIDBitmap {
        pg_sys::error!("theodb vecfilter: bitmap subplan did not return a TIDBitmap");
    }
    let (exact, lossy) = materialize_bitmap(res as *mut pg_sys::TIDBitmap);
    set_membership(Some(Membership { exact, lossy }));
    st.membership_active = true;
}

/// Exec next tuple: pull the next distance-ordered, membership-surviving tuple from the vector child. The child's own
/// qpqual Filter has already rechecked the scalar `WHERE` on the heap tuple (removing lossy/pending over-admits), so
/// the node just forwards it.
#[pg_guard]
unsafe extern "C-unwind" fn exec_custom_scan(node: *mut pg_sys::CustomScanState) -> *mut pg_sys::TupleTableSlot {
    let st = &mut *(node as *mut VecFilterState);
    if st.vector_child.is_null() {
        return std::ptr::null_mut();
    }
    pg_sys::ExecProcNode(st.vector_child)
}

/// Exec teardown: clear the membership (anti-leak contract) and end both children. The TIDBitmap is owned by the
/// bitmap child's context — do NOT `tbm_free` it (double-free); `ExecEndNode(bitmap_child)` frees it.
#[pg_guard]
unsafe extern "C-unwind" fn end_custom_scan(node: *mut pg_sys::CustomScanState) {
    let st = &mut *(node as *mut VecFilterState);
    if st.membership_active {
        set_membership(None);
        st.membership_active = false;
    }
    if !st.vector_child.is_null() {
        pg_sys::ExecEndNode(st.vector_child);
    }
    if !st.bitmap_child.is_null() {
        pg_sys::ExecEndNode(st.bitmap_child);
    }
}

/// ReScan: re-derive the membership (the bitmap child may depend on changed params) and rescan both children.
#[pg_guard]
unsafe extern "C-unwind" fn rescan_custom_scan(node: *mut pg_sys::CustomScanState) {
    let st = &mut *(node as *mut VecFilterState);
    if !st.bitmap_child.is_null() {
        pg_sys::ExecReScan(st.bitmap_child);
        let res = pg_sys::MultiExecProcNode(st.bitmap_child);
        if !res.is_null() && (*(res as *mut pg_sys::Node)).type_ == pg_sys::NodeTag::T_TIDBitmap {
            let (exact, lossy) = materialize_bitmap(res as *mut pg_sys::TIDBitmap);
            set_membership(Some(Membership { exact, lossy }));
            st.membership_active = true;
        }
    }
    if !st.vector_child.is_null() {
        pg_sys::ExecReScan(st.vector_child);
    }
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    /// M93 T2.1 — arbitrary-`WHERE` filtered vector search via the 2-child Custom Scan node equals the exact
    /// seqscan-filtered result, on a NON-label scalar column (`cat`, btree-indexed → a bitmap sub-plan). The hook
    /// injects the node (EXPLAIN shows `Custom Scan (theodb_vecfilter)` with 2 children); the node MultiExecs the
    /// bitmap → membership → drives the vector-ordered child (whose own qpqual Filter is the MVCC recheck). The
    /// filtered id set MUST equal the exact seqscan-filtered top-k.
    #[pgrx::pg_test]
    fn m93_t2_customscan_arbitrary_where_equals_seqscan() {
        use std::collections::HashSet;
        Spi::run("CREATE TABLE cs92 (id int PRIMARY KEY, cat int, e vector(4))").unwrap();
        // 1000 rows, cat = id%100 (1% per value → the planner generates a BitmapHeapPath for the filter).
        Spi::run("INSERT INTO cs92 SELECT g, g%100, ('['||g||','||(g*2)||','||(g*3)||','||(g%7)||']')::vector FROM generate_series(1,1000) g").unwrap();
        Spi::run("CREATE INDEX cs92_e ON cs92 USING theodb_ivfflat (e) WITH (lists=8, pq_subspaces=2, aq_threshold=2000, separate_storage=1)").unwrap();
        Spi::run("CREATE INDEX cs92_cat ON cs92 (cat)").unwrap();
        Spi::run("ANALYZE cs92").unwrap();
        let q = "[10,20,30,3]";
        let filt = "cat = 7";
        // Exact ground truth via seqscan.
        let exact: HashSet<i32> = Spi::connect(|c| {
            c.select("SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on", None, &[]).ok();
            c.select(&format!("SELECT id FROM cs92 WHERE {filt} ORDER BY e <-> '{q}'::vector LIMIT 5"), None, &[])
                .unwrap().filter_map(|r| r.get::<i32>(1).unwrap()).collect()
        });
        // Hook ON — reset every enable_* GUC the exact block flipped (they persist in-session), then force the
        // bitmap path (enable_seqscan off so the planner keeps the BitmapHeapPath the hook needs).
        let hooked_setup = "SET theodb.enable_vecfilter=on; SET enable_indexscan=on; SET enable_bitmapscan=on; SET enable_seqscan=off; SET theodb_ivfflat.probes=8";
        let plan: String = Spi::connect(|c| {
            c.select(hooked_setup, None, &[]).ok();
            c.select(&format!("EXPLAIN (COSTS OFF) SELECT id FROM cs92 WHERE {filt} ORDER BY e <-> '{q}'::vector LIMIT 5"), None, &[])
                .unwrap().filter_map(|r| r.get::<String>(1).unwrap()).collect::<Vec<_>>().join("\n")
        });
        assert!(plan.contains("Custom Scan (theodb_vecfilter)"), "hook ON must inject the Custom Scan node (plan:\n{plan})");
        let hooked: HashSet<i32> = Spi::connect(|c| {
            c.select(hooked_setup, None, &[]).ok();
            c.select(&format!("SELECT id FROM cs92 WHERE {filt} ORDER BY e <-> '{q}'::vector LIMIT 5"), None, &[])
                .unwrap().filter_map(|r| r.get::<i32>(1).unwrap()).collect()
        });
        assert_eq!(hooked, exact, "the 2-child Custom Scan filtered result must equal the exact seqscan-filtered top-5 (hooked={hooked:?} exact={exact:?})");
    }

    /// M93 T2.1 — a post-build INSERT with a NON-matching `cat` (pending region, no stored membership) must NOT
    /// appear in a filtered result: the vector child's qpqual Filter rechecks `cat` on the heap tuple and rejects it.
    #[pgrx::pg_test]
    fn m93_t2_customscan_pending_rechecked() {
        Spi::run("CREATE TABLE cs92p (id int PRIMARY KEY, cat int, e vector(4))").unwrap();
        Spi::run("INSERT INTO cs92p SELECT g, g%100, ('['||g||','||(g*2)||',0,0]')::vector FROM generate_series(1,1000) g").unwrap();
        Spi::run("CREATE INDEX cs92p_e ON cs92p USING theodb_ivfflat (e) WITH (lists=8, pq_subspaces=2, aq_threshold=2000, separate_storage=1)").unwrap();
        Spi::run("CREATE INDEX cs92p_cat ON cs92p (cat)").unwrap();
        // Post-build INSERT → PENDING region, cat=99 (NON-matching the cat=7 filter), vector right at the query.
        Spi::run("INSERT INTO cs92p VALUES (9999, 99, '[0,0,0,0]')").unwrap();
        Spi::run("ANALYZE cs92p").unwrap();
        let plan: String = Spi::connect(|c| {
            c.select("SET theodb.enable_vecfilter=on; SET enable_seqscan=off; SET theodb_ivfflat.probes=8", None, &[]).ok();
            c.select("EXPLAIN (COSTS OFF) SELECT id FROM cs92p WHERE cat=7 ORDER BY e <-> '[0,0,0,0]'::vector LIMIT 10", None, &[])
                .unwrap().filter_map(|r| r.get::<String>(1).unwrap()).collect::<Vec<_>>().join("\n")
        });
        assert!(plan.contains("Custom Scan (theodb_vecfilter)"), "the node must fire so its recheck is exercised (plan:\n{plan})");
        let ids: Vec<i32> = Spi::connect(|c| {
            c.select("SET theodb.enable_vecfilter=on; SET enable_seqscan=off; SET theodb_ivfflat.probes=8", None, &[]).ok();
            c.select("SELECT id FROM cs92p WHERE cat=7 ORDER BY e <-> '[0,0,0,0]'::vector LIMIT 10", None, &[])
                .unwrap().filter_map(|r| r.get::<i32>(1).unwrap()).collect()
        });
        assert!(!ids.contains(&9999), "the non-matching pending row (cat=99) must be rechecked out of a cat=7 filter (got {ids:?})");
    }

    /// The hook is inert when the GUC is off (default): no Custom Scan node appears — production queries untouched.
    #[pgrx::pg_test]
    fn m92_customscan_inert_when_disabled() {
        Spi::run("CREATE TABLE cs92b (id int PRIMARY KEY, cat int, e vector(4))").unwrap();
        for i in 1..=20i32 {
            Spi::run(&format!("INSERT INTO cs92b VALUES ({i}, {}, '[{i},{},0,0]')", i % 3, i * 2)).unwrap();
        }
        Spi::run("CREATE INDEX cs92b_e ON cs92b USING theodb_ivfflat (e) WITH (lists=2, pq_subspaces=2, aq_threshold=2000, separate_storage=1)").unwrap();
        let plan: String = Spi::connect(|c| {
            c.select("SET theodb.enable_vecfilter=off", None, &[]).ok();
            c.select("EXPLAIN (COSTS OFF) SELECT id FROM cs92b WHERE cat=1 ORDER BY e <-> '[5,10,2,0]'::vector LIMIT 3", None, &[])
                .unwrap().filter_map(|r| r.get::<String>(1).unwrap()).collect::<Vec<_>>().join("\n")
        });
        assert!(!plan.contains("theodb_vecfilter"), "GUC off ⇒ no Custom Scan node (plan:\n{plan})");
    }

    /// M92 v1a — the AM-side TID membership primitive (the correctness core of unknown #2). A membership set is
    /// pushed via the backend-local side channel; a plain vector index scan (NO WHERE) must then return ONLY rows
    /// whose heap TID is a member — proving the bitmap-membership inline skip reaches Stage-1 and filters correctly.
    /// This is the mechanism the Custom Scan node will drive in v1b (with a real TIDBitmap).
    #[pgrx::pg_test]
    fn m92_v1a_membership_skip_returns_only_members() {
        use std::collections::HashSet;
        Spi::run("CREATE TABLE m92m (id int PRIMARY KEY, lbl smallint[], e vector(4))").unwrap();
        for i in 1..=60i32 {
            Spi::run(&format!("INSERT INTO m92m VALUES ({i}, '{{1}}', '[{i},{},{},{}]')", i * 2, i * 3, i % 7)).unwrap();
        }
        Spi::run("CREATE INDEX m92m_e ON m92m USING theodb_ivfflat (e, lbl) WITH (lists=4, pq_subspaces=2, aq_threshold=2000, separate_storage=1)").unwrap();
        // Encode the heap ctids of a chosen id subset into the membership i64s (tid::encode = (block<<16)|offset).
        let member_ids: HashSet<i32> = [5, 12, 20, 33, 41].into_iter().collect();
        let mset: HashSet<i64> = Spi::connect(|c| {
            c.select("SELECT ctid::text FROM m92m WHERE id IN (5,12,20,33,41)", None, &[])
                .unwrap()
                .filter_map(|r| r.get::<String>(1).unwrap())
                .map(|s| {
                    let inner = s.trim_matches(|ch| ch == '(' || ch == ')');
                    let mut it = inner.split(',');
                    let b: i64 = it.next().unwrap().parse().unwrap();
                    let o: i64 = it.next().unwrap().parse().unwrap();
                    (b << 16) | o
                })
                .collect()
        });
        assert_eq!(mset.len(), 5, "resolved 5 member ctids");
        super::set_membership(Some(super::Membership { exact: mset, lossy: HashSet::new() }));
        // Plain vector scan, NO WHERE → the membership is the ONLY filter. LIMIT high enough to pull all members.
        let got: Vec<i32> = Spi::connect(|c| {
            c.select("SET enable_seqscan=off; SET enable_indexscan=on; SET theodb_ivfflat.probes=4", None, &[]).ok();
            c.select("SELECT id FROM m92m ORDER BY e <-> '[10,20,30,3]'::vector LIMIT 20", None, &[])
                .unwrap()
                .filter_map(|r| r.get::<i32>(1).unwrap())
                .collect()
        });
        super::set_membership(None); // CRITICAL: clear so the filter does not leak to other queries in this backend.
        assert!(!got.is_empty(), "membership scan returned nothing — the skip filtered everything (plumbing broken)");
        assert!(
            got.iter().all(|id| member_ids.contains(id)),
            "every returned id must be a member (got {got:?}, members {member_ids:?})"
        );
    }

    /// M92 v1a — membership is inert once cleared: after `set_membership(None)` a plain scan returns non-members
    /// again (proves the side channel does not leak across queries — the EndCustomScan clear contract).
    #[pgrx::pg_test]
    fn m92_v1a_membership_cleared_is_inert() {
        Spi::run("CREATE TABLE m92c (id int PRIMARY KEY, lbl smallint[], e vector(4))").unwrap();
        for i in 1..=30i32 {
            Spi::run(&format!("INSERT INTO m92c VALUES ({i}, '{{1}}', '[{i},{},0,0]')", i * 2)).unwrap();
        }
        Spi::run("CREATE INDEX m92c_e ON m92c USING theodb_ivfflat (e, lbl) WITH (lists=2, pq_subspaces=2, aq_threshold=2000, separate_storage=1)").unwrap();
        super::set_membership(Some(super::Membership {
            exact: std::collections::HashSet::from([1i64]),
            lossy: std::collections::HashSet::new(),
        }));
        super::set_membership(None);
        let got: Vec<i32> = Spi::connect(|c| {
            c.select("SET enable_seqscan=off; SET enable_indexscan=on; SET theodb_ivfflat.probes=2", None, &[]).ok();
            c.select("SELECT id FROM m92c ORDER BY e <-> '[5,10,0,0]'::vector LIMIT 10", None, &[])
                .unwrap()
                .filter_map(|r| r.get::<i32>(1).unwrap())
                .collect()
        });
        assert!(got.len() >= 5, "cleared membership ⇒ a normal scan returns many rows (got {got:?})");
    }

    /// M93 T1.1 — the membership admits a candidate on a LOSSY-bitmap block (over-admit → the node recheck filters),
    /// never under-admits. Proof: a membership with only a lossy block admits that block's rows; an empty membership
    /// (no exact, no lossy) admits nothing. Guards ADR M93-2 (dropping lossy pages would silently miss valid rows).
    #[pgrx::pg_test]
    fn m93_t1_membership_admits_lossy_block() {
        use std::collections::HashSet;
        Spi::run("CREATE TABLE m92l (id int PRIMARY KEY, lbl smallint[], e vector(4))").unwrap();
        for i in 1..=40i32 {
            Spi::run(&format!("INSERT INTO m92l VALUES ({i}, '{{1}}', '[{i},{},0,0]')", i * 2)).unwrap();
        }
        Spi::run("CREATE INDEX m92l_e ON m92l USING theodb_ivfflat (e, lbl) WITH (lists=2, pq_subspaces=2, aq_threshold=2000, separate_storage=1)").unwrap();
        let block0: u32 = Spi::connect(|c| {
            c.select("SELECT ctid::text FROM m92l WHERE id=1", None, &[])
                .unwrap()
                .filter_map(|r| r.get::<String>(1).unwrap())
                .next()
                .map(|s| {
                    let inner = s.trim_matches(|ch| ch == '(' || ch == ')');
                    inner.split(',').next().unwrap().parse().unwrap()
                })
                .unwrap()
        });
        let run = || -> Vec<i32> {
            Spi::connect(|c| {
                c.select("SET enable_seqscan=off; SET enable_indexscan=on; SET theodb_ivfflat.probes=2", None, &[]).ok();
                c.select("SELECT id FROM m92l ORDER BY e <-> '[5,10,0,0]'::vector LIMIT 20", None, &[])
                    .unwrap()
                    .filter_map(|r| r.get::<i32>(1).unwrap())
                    .collect()
            })
        };
        super::set_membership(Some(super::Membership { exact: HashSet::new(), lossy: HashSet::from([block0]) }));
        let via_lossy = run();
        super::set_membership(Some(super::Membership { exact: HashSet::new(), lossy: HashSet::new() }));
        let via_empty = run();
        super::set_membership(None);
        assert!(!via_lossy.is_empty(), "a lossy-member block must admit its rows (got {via_lossy:?})");
        assert!(via_empty.is_empty(), "empty membership (no exact, no lossy) admits nothing (got {via_empty:?})");
    }

    /// M92 v1b — the bitmap-materialization step: iterate a native `TIDBitmap` into the exact-TID + lossy-block
    /// membership sets. Builds a bitmap by hand (`tbm_create` + `tbm_add_tuples`) so the iteration + encoding are
    /// tested in isolation, before wiring the node to `MultiExecProcNode` a real sub-plan.
    #[pgrx::pg_test]
    fn m92_v1b_materialize_bitmap_exact() {
        use std::collections::HashSet;
        unsafe {
            let tbm = pgrx::pg_sys::tbm_create(1024 * 1024, std::ptr::null_mut());
            let mk = |b: u32, o: u16| pgrx::pg_sys::ItemPointerData {
                ip_blkid: pgrx::pg_sys::BlockIdData { bi_hi: (b >> 16) as u16, bi_lo: (b & 0xffff) as u16 },
                ip_posid: o,
            };
            // sorted (block, offset): (0,5) (0,12) (2,3)
            let mut tids = [mk(0, 5), mk(0, 12), mk(2, 3)];
            pgrx::pg_sys::tbm_add_tuples(tbm, tids.as_mut_ptr(), 3, false);
            let (exact, lossy) = super::materialize_bitmap(tbm);
            pgrx::pg_sys::tbm_free(tbm);
            assert!(lossy.is_empty(), "a 3-tuple bitmap must not go lossy (got {lossy:?})");
            let expect: HashSet<i64> = [(0i64 << 16) | 5, (0i64 << 16) | 12, (2i64 << 16) | 3].into_iter().collect();
            assert_eq!(exact, expect, "materialized exact TIDs must match the added tuples");
        }
    }
}
