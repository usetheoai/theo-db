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
thread_local! {
    static SCAN_MEMBERSHIP: RefCell<Option<Rc<HashSet<i64>>>> = const { RefCell::new(None) };
}

/// Set the TID membership the next index scan in THIS backend must filter by (encoded `tid::encode` i64s).
/// `None` clears it. Called by the Custom Scan node's `BeginCustomScan` before driving the AM scan.
pub(crate) fn set_membership(m: Option<HashSet<i64>>) {
    SCAN_MEMBERSHIP.with(|c| *c.borrow_mut() = m.map(Rc::new));
}

/// Read the active membership (cheap Rc clone) for this backend, or `None`. Read by the AM Stage-1 scan.
pub(crate) fn membership() -> Option<Rc<HashSet<i64>>> {
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

/// The planner hook: for a base relation, add a pass-through CustomPath that wraps the cheapest total path.
/// Gated behind `theodb.enable_vecfilter` (default OFF) so it is inert in production until the spike is enabled.
#[pg_guard]
unsafe extern "C-unwind" fn pathlist_hook(
    root: *mut pg_sys::PlannerInfo,
    rel: *mut pg_sys::RelOptInfo,
    rti: pg_sys::Index,
    rte: *mut pg_sys::RangeTblEntry,
) {
    // Chain the previous hook first (never drop it).
    if let Some(prev) = PREV_HOOK {
        prev(root, rel, rti, rte);
    }
    if !crate::am::guc::vecfilter_enabled() {
        return;
    }
    // Spike v0: only a plain base relation is wrapped.
    let relref = &mut *rel;
    if relref.reloptkind != pg_sys::RelOptKind::RELOPT_BASEREL {
        return;
    }
    // NOTE: `cheapest_total_path` is NOT yet set at hook time — `set_cheapest(rel)` runs AFTER the hook in
    // `set_rel_pathlist()`. So pick the child from the already-generated `rel->pathlist` (the cheapest by
    // total_cost). Empty pathlist ⇒ nothing to wrap.
    let paths = PgList::<pg_sys::Path>::from_pg(relref.pathlist);
    let mut child: *mut pg_sys::Path = std::ptr::null_mut();
    let mut best = f64::INFINITY;
    for i in 0..paths.len() {
        if let Some(p) = paths.get_ptr(i) {
            if (*p).total_cost < best {
                best = (*p).total_cost;
                child = p;
            }
        }
    }
    if child.is_null() {
        return;
    }

    // Hand-roll `create_customscan_path`: alloc a CustomPath node and populate it by hand.
    let mut cpath = PgBox::<pg_sys::CustomPath>::alloc_node(pg_sys::NodeTag::T_CustomPath);
    let path = &mut cpath.path;
    path.pathtype = pg_sys::NodeTag::T_CustomScan;
    path.parent = rel;
    path.pathtarget = relref.reltarget;
    path.param_info = std::ptr::null_mut();
    path.rows = (*child).rows;
    // Cost must be MEANINGFULLY below the child — `add_path` uses `compare_path_costs_fuzzily` with a 1% fuzz
    // factor (STD_FUZZ_FACTOR), so a tiny epsilon delta reads as a tie and the new path is rejected in favour of
    // the existing seqscan. Spike v0 halves the cost to force selection and prove the lifecycle; the real feature
    // costs the filtered scan honestly.
    path.startup_cost = (*child).startup_cost * 0.5;
    path.total_cost = (*child).total_cost * 0.5;
    cpath.flags = 0;
    // Carry the child path so PlanCustomPath receives its planned form in `custom_plans`.
    let mut children = PgList::<pg_sys::Path>::new();
    children.push(child);
    cpath.custom_paths = children.into_pg();
    cpath.custom_private = std::ptr::null_mut();
    cpath.methods = &PATH_METHODS.0;

    pg_sys::add_path(rel, cpath.into_pg() as *mut pg_sys::Path);
}

/// Path -> Plan: hand-roll `make_custom_scan`. The child path was planned by the core and handed back in
/// `custom_plans`; wrap it in a CustomScan node.
#[pg_guard]
unsafe extern "C-unwind" fn plan_custom_path(
    _root: *mut pg_sys::PlannerInfo,
    rel: *mut pg_sys::RelOptInfo,
    _best_path: *mut pg_sys::CustomPath,
    tlist: *mut pg_sys::List,
    clauses: *mut pg_sys::List,
    custom_plans: *mut pg_sys::List,
) -> *mut pg_sys::Plan {
    let mut cscan = PgBox::<pg_sys::CustomScan>::alloc_node(pg_sys::NodeTag::T_CustomScan);
    let plan = &mut cscan.scan.plan;
    plan.targetlist = tlist;
    // `clauses` are RestrictInfo nodes — a plan's `qual` must hold BARE expression nodes, so extract them
    // (else the executor hits "unrecognized node type: T_RestrictInfo"). v0: the child already applies the
    // filter, so keeping them here is belt-and-suspenders (and the v1 recheck site).
    plan.qual = pg_sys::extract_actual_clauses(clauses, false);
    plan.lefttree = std::ptr::null_mut();
    plan.righttree = std::ptr::null_mut();
    cscan.scan.scanrelid = (*rel).relid;
    cscan.flags = 0;
    cscan.custom_plans = custom_plans; // the planned child(ren)
    cscan.custom_exprs = std::ptr::null_mut();
    cscan.custom_private = std::ptr::null_mut();
    cscan.custom_scan_tlist = std::ptr::null_mut();
    cscan.custom_relids = std::ptr::null_mut();
    cscan.methods = &SCAN_METHODS.0;
    cscan.into_pg() as *mut pg_sys::Plan
}

/// Plan -> ScanState: allocate the CustomScanState with our exec methods. v0 keeps state in `custom_ps`
/// (the child PlanState list), so no custom-struct embedding is needed yet.
#[pg_guard]
unsafe extern "C-unwind" fn create_custom_scan_state(cscan: *mut pg_sys::CustomScan) -> *mut pg_sys::Node {
    let mut css = PgBox::<pg_sys::CustomScanState>::alloc_node(pg_sys::NodeTag::T_CustomScanState);
    css.methods = &EXEC_METHODS.0;
    // Stash nothing else in v0; `custom_ps` is filled at BeginCustomScan from cscan.custom_plans.
    let _ = cscan;
    css.into_pg() as *mut pg_sys::Node
}

/// Exec init: initialize the child plan into a PlanState and stash it in `custom_ps`.
#[pg_guard]
unsafe extern "C-unwind" fn begin_custom_scan(
    node: *mut pg_sys::CustomScanState,
    estate: *mut pg_sys::EState,
    eflags: c_int,
) {
    let css = &mut *node;
    let cscan = css.ss.ps.plan as *mut pg_sys::CustomScan;
    let planned = PgList::<pg_sys::Plan>::from_pg((*cscan).custom_plans);
    let child_plan = match planned.get_ptr(0) {
        Some(p) => p,
        None => pg_sys::error!("theodb vecfilter: CustomScan has no child plan"),
    };
    let child_ps = pg_sys::ExecInitNode(child_plan, estate, eflags);
    let mut ps_list = PgList::<pg_sys::PlanState>::new();
    ps_list.push(child_ps);
    css.custom_ps = ps_list.into_pg();
}

/// Exec next tuple: pure pass-through — pull from the child and return its slot.
#[pg_guard]
unsafe extern "C-unwind" fn exec_custom_scan(node: *mut pg_sys::CustomScanState) -> *mut pg_sys::TupleTableSlot {
    let css = &mut *node;
    let ps_list = PgList::<pg_sys::PlanState>::from_pg(css.custom_ps);
    let child_ps = match ps_list.get_ptr(0) {
        Some(p) => p,
        None => return std::ptr::null_mut(),
    };
    pg_sys::ExecProcNode(child_ps)
}

/// Exec teardown: end the child PlanState.
#[pg_guard]
unsafe extern "C-unwind" fn end_custom_scan(node: *mut pg_sys::CustomScanState) {
    let css = &mut *node;
    let ps_list = PgList::<pg_sys::PlanState>::from_pg(css.custom_ps);
    if let Some(child_ps) = ps_list.get_ptr(0) {
        pg_sys::ExecEndNode(child_ps);
    }
}

/// ReScan: forward to the child.
#[pg_guard]
unsafe extern "C-unwind" fn rescan_custom_scan(node: *mut pg_sys::CustomScanState) {
    let css = &mut *node;
    let ps_list = PgList::<pg_sys::PlanState>::from_pg(css.custom_ps);
    if let Some(child_ps) = ps_list.get_ptr(0) {
        pg_sys::ExecReScan(child_ps);
    }
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    /// M92 spike v0 — the Custom Scan Provider lifecycle proof. With `theodb.enable_vecfilter=on`, a filtered
    /// vector query is intercepted by the pass-through Custom Scan node: EXPLAIN shows the node AND the result is
    /// byte-identical to the un-hooked plan (pass-through correctness). Guards the hand-rolled node construction
    /// (registration → pathlist hook → CustomPath → CustomScan → exec) end-to-end.
    #[pgrx::pg_test]
    fn m92_customscan_lifecycle_passthrough() {
        Spi::run("CREATE TABLE cs92 (id int PRIMARY KEY, cat int, e vector(4))").unwrap();
        for i in 1..=60i32 {
            let lit = format!("[{},{},{},{}]", i, i * 2, i * 3, i % 7);
            Spi::run(&format!("INSERT INTO cs92 VALUES ({i}, {}, '{lit}')", i % 5)).unwrap();
        }
        Spi::run("CREATE INDEX cs92_e ON cs92 USING theodb_ivfflat (e) WITH (lists=4, pq_subspaces=2, aq_threshold=2000, separate_storage=1)").unwrap();
        let q = "[10,20,30,3]";
        let filt = "cat = 2";
        // Baseline: hook OFF.
        let base: Vec<i32> = Spi::connect(|c| {
            c.select("SET theodb.enable_vecfilter=off", None, &[]).ok();
            c.select(&format!("SELECT id FROM cs92 WHERE {filt} ORDER BY e <-> '{q}'::vector LIMIT 5"), None, &[])
                .unwrap().filter_map(|r| r.get::<i32>(1).unwrap()).collect()
        });
        // Hook ON: the plan must contain the Custom Scan node AND return the identical result.
        let plan: String = Spi::connect(|c| {
            c.select("SET theodb.enable_vecfilter=on", None, &[]).ok();
            c.select(&format!("EXPLAIN (COSTS OFF) SELECT id FROM cs92 WHERE {filt} ORDER BY e <-> '{q}'::vector LIMIT 5"), None, &[])
                .unwrap().filter_map(|r| r.get::<String>(1).unwrap()).collect::<Vec<_>>().join("\n")
        });
        assert!(plan.contains("Custom Scan (theodb_vecfilter)"), "hook ON must inject the Custom Scan node (plan:\n{plan})");
        let hooked: Vec<i32> = Spi::connect(|c| {
            c.select("SET theodb.enable_vecfilter=on", None, &[]).ok();
            c.select(&format!("SELECT id FROM cs92 WHERE {filt} ORDER BY e <-> '{q}'::vector LIMIT 5"), None, &[])
                .unwrap().filter_map(|r| r.get::<i32>(1).unwrap()).collect()
        });
        assert_eq!(hooked, base, "the pass-through Custom Scan must return the identical result to the un-hooked plan");
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
        super::set_membership(Some(mset));
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
        super::set_membership(Some(std::collections::HashSet::from([1i64])));
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
