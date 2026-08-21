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

use crate::am::{cost, guc, page};
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
    // The ACTIVE slot the AM reads. M94: only ever non-None DURING a vecfilter node's pull window (swap-discipline)
    // — never across statements, so multiple nodes in one plan each see only their own set.
    static SCAN_MEMBERSHIP: RefCell<Option<Rc<Membership>>> = const { RefCell::new(None) };
    // M94 — per-node membership registry, keyed by the CustomScanState pointer. Rust-side storage (normal Drop) —
    // NEVER store an `Rc` inside the palloc0'd VecFilterState: Postgres frees that memory without running Drop, so
    // the (potentially tens-of-MB) HashSets would leak per query (ADR M94-2).
    static NODE_MEMBERSHIPS: RefCell<std::collections::HashMap<usize, Rc<Membership>>> =
        RefCell::new(std::collections::HashMap::new());
}

/// Set the ACTIVE membership slot directly. Test-only API (the node path goes through the registry + swap).
pub(crate) fn set_membership(m: Option<Membership>) {
    SCAN_MEMBERSHIP.with(|c| *c.borrow_mut() = m.map(Rc::new));
}

/// M94 — swap the ACTIVE slot, returning the previous value. The save/restore stack discipline around each child
/// pull: re-entrant by construction (a SubPlan inside the child's Filter that runs another vecfilter node nests
/// correctly — it saves ours, installs its own, restores ours).
pub(crate) fn swap_active(m: Option<Rc<Membership>>) -> Option<Rc<Membership>> {
    SCAN_MEMBERSHIP.with(|c| std::mem::replace(&mut *c.borrow_mut(), m))
}

/// Read the active membership (cheap Rc clone) for this backend, or `None`. Read by the AM Stage-1 scan — only ever
/// non-None inside the owning node's pull window (M94).
pub(crate) fn membership() -> Option<Rc<Membership>> {
    SCAN_MEMBERSHIP.with(|c| c.borrow().clone())
}

/// Whether a membership filter is active (used by `amrescan` to keep `xs_recheck` on).
pub(crate) fn has_membership() -> bool {
    SCAN_MEMBERSHIP.with(|c| c.borrow().is_some())
}

/// M94 — store a node's membership in the per-node registry (replacing any previous entry for the node).
fn registry_insert(node: usize, m: Membership) {
    NODE_MEMBERSHIPS.with(|r| {
        r.borrow_mut().insert(node, Rc::new(m));
    });
}

/// M94 — fetch a node's membership from the registry (cheap Rc clone).
fn registry_get(node: usize) -> Option<Rc<Membership>> {
    NODE_MEMBERSHIPS.with(|r| r.borrow().get(&node).cloned())
}

/// M94 — drop a node's registry entry (frees the sets via normal Rust Drop).
fn registry_remove(node: usize) {
    NODE_MEMBERSHIPS.with(|r| {
        r.borrow_mut().remove(&node);
    });
}

/// M94 — wholesale cleanup at transaction end: entries orphaned by an error-longjmp (EndCustomScan skipped) are
/// freed here; the ACTIVE slot is cleared too.
fn clear_all_memberships() {
    let _ = swap_active(None);
    NODE_MEMBERSHIPS.with(|r| r.borrow_mut().clear());
}

/// M94 review MEDIUM-1 — RAII guard making the pull-window swap locally unwind-safe: a PG ERROR inside the child
/// pull becomes (via pgrx's FFI guard) a Rust panic that unwinds through our frame — Drop restores the previous
/// ACTIVE value even then, demoting the xact/subxact abort callbacks to belt-and-braces.
struct ActiveGuard(Option<Rc<Membership>>);
impl ActiveGuard {
    fn install(mine: Option<Rc<Membership>>) -> Self {
        ActiveGuard(swap_active(mine))
    }
}
impl Drop for ActiveGuard {
    fn drop(&mut self) {
        let _ = swap_active(self.0.take());
    }
}

/// M135 — `MaxHeapTuplesPerPage` (`access/htup_details.h:629-631`). pgrx does not expose it: it is a computed C
/// macro, so it never reaches the bindings. **Measured**, not assumed — compiled a probe against the real PG18.4
/// headers and read the symbol size: `char probe[MaxHeapTuplesPerPage]` → 0x123 = 291, with `char blk[BLCKSZ]` →
/// 0x2000 confirming the 8 kB block the value depends on. The assert below pins that dependency (plan Q2), so a
/// build with a different `BLCKSZ` fails loudly instead of silently under-sizing the offset buffer.
const MAX_TUPLES_PER_PAGE: usize = 291;
const _: () = assert!(
    pg_sys::BLCKSZ == 8192,
    "MAX_TUPLES_PER_PAGE = 291 was measured for an 8 kB BLCKSZ; re-measure for this build"
);

/// M92 v1b — materialize a native `TIDBitmap` (produced by a bitmap sub-plan's `MultiExecProcNode`) into the
/// membership representation the AM Stage-1 consumes: a set of EXACT encoded TIDs (`(block<<16)|offset`, matching
/// `tid::encode`) plus a set of LOSSY block numbers. A page goes lossy under memory pressure — its individual
/// offsets are forgotten, so only the block is known and every candidate on that block must be ADMITTED then
/// rechecked on the heap (the executor / Custom Scan node re-runs the real qual). The exact set is authoritative;
/// the lossy set over-admits (safe under recheck).
///
/// **M135 — ported to the PG18 iterator contract.** Four things changed at once, and only one of them is a
/// compile error, which is why this is the risky half of the migration:
///
/// 1. `tbm_begin_iterate` returns a `TBMIterator` **by value** and takes 3 args. `dsp` is the discriminator:
///    valid ⇒ shared (DSA) iterator, `0` ⇒ private. Ours is always private — the `TIDBitmap` comes from a local
///    `MultiExecProcNode`, never from parallel workers. `InvalidDsaPointer` is a C macro (`utils/dsa.h:78`,
///    `((dsa_pointer) 0)`) that pgrx does not bind, hence the literal `0`.
/// 2. `tbm_iterate` returns `bool` and fills a **caller-owned** result. The old "returns NULL when exhausted"
///    idiom inverts silently if ported carelessly.
/// 3. The `ntuples < 0` sentinel is **gone**, replaced by an explicit `bool lossy`. Treating a lossy page as
///    exact would drop rows with no error.
/// 4. Offsets left the result struct: `tbm_extract_page_tuple` fills a caller buffer. It returns the page's TOTAL
///    offset count **even when that exceeds the buffer**, so the return value MUST be clamped before slicing —
///    otherwise we read uninitialised memory. And it must never be called on a lossy result, whose
///    `internal_page` is NULL (heap guards this the same way, `heapam_handler.c:2506`).
pub(crate) unsafe fn materialize_bitmap(
    tbm: *mut pg_sys::TIDBitmap,
) -> (HashSet<i64>, HashSet<u32>) {
    let mut exact: HashSet<i64> = HashSet::new();
    let mut lossy: HashSet<u32> = HashSet::new();

    // `0` = InvalidDsaPointer ⇒ private iterator (see doc note 1).
    let mut iter: pg_sys::TBMIterator =
        unsafe { pg_sys::tbm_begin_iterate(tbm, std::ptr::null_mut(), 0) };
    let mut res = pg_sys::TBMIterateResult::default();
    let mut offsets = [0u16; MAX_TUPLES_PER_PAGE];

    while unsafe { pg_sys::tbm_iterate(&mut iter, &mut res) } {
        if res.lossy {
            lossy.insert(res.blockno); // offsets gone; admit-then-recheck by block
        } else {
            let reported = unsafe {
                pg_sys::tbm_extract_page_tuple(
                    &mut res,
                    offsets.as_mut_ptr(),
                    MAX_TUPLES_PER_PAGE as u32,
                )
            } as usize;
            // Clamp: `reported` is the page total, which may exceed the buffer (doc note 4).
            let n = reported.min(MAX_TUPLES_PER_PAGE);
            for &off in &offsets[..n] {
                exact.insert(((res.blockno as i64) << 16) | (off as i64));
            }
        }
    }

    unsafe { pg_sys::tbm_end_iterate(&mut iter) }; // exactly once, after the drain (double call trips an Assert)
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

/// Transaction-end cleanup (M94): on COMMIT and ABORT (+ parallel variants), clear the ACTIVE slot AND the whole
/// registry — entries orphaned by an error-longjmp (EndCustomScan skipped) are freed here, bounding any leak to
/// one transaction. On ABORT this is also the correctness guard: a `pg_sys::error!` longjmp mid-pull skips the
/// swap-restore, and this clear prevents the stale ACTIVE from filtering a later plain scan (council H1/MEDIUM-1).
#[pg_guard]
unsafe extern "C-unwind" fn xact_clear_membership(
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
        // PREPARE (review LOW-2): `PREPARE TRANSACTION` ends the local xact without a COMMIT/ABORT event —
        // clear here too so orphaned entries never survive into the backend's next transaction.
        clear_all_memberships();
    }
}

/// Subtransaction-abort cleanup (M94, ADR M94-3): a PL/pgSQL `EXCEPTION` handler aborts only the SUBxact — the
/// xact callback never fires — so a mid-pull longjmp caught by an EXCEPTION block would leave a stale ACTIVE slot.
/// Clear ONLY the ACTIVE slot here: registry entries of unrelated live scans (e.g. an open cursor over a filtered
/// vector scan) must survive an unrelated subxact abort — their next pull re-installs from the registry
/// (self-healing), and orphaned entries are freed at xact end.
#[pg_guard]
unsafe extern "C-unwind" fn subxact_clear_membership(
    event: pg_sys::SubXactEvent::Type,
    _my_subid: pg_sys::SubTransactionId,
    _parent_subid: pg_sys::SubTransactionId,
    _arg: *mut std::ffi::c_void,
) {
    if event == pg_sys::SubXactEvent::SUBXACT_EVENT_ABORT_SUB {
        let _ = swap_active(None);
    }
}

/// Register the Custom Scan methods + install the pathlist hook + the xact/subxact membership guards. From `_PG_init`.
pub fn init() {
    unsafe {
        pg_sys::RegisterCustomScanMethods(&SCAN_METHODS.0);
        PREV_HOOK = pg_sys::set_rel_pathlist_hook;
        pg_sys::set_rel_pathlist_hook = Some(pathlist_hook);
        pg_sys::RegisterXactCallback(Some(xact_clear_membership), std::ptr::null_mut());
        pg_sys::RegisterSubXactCallback(Some(subxact_clear_membership), std::ptr::null_mut());
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
    if let Some(prev) = unsafe { PREV_HOOK } {
        unsafe { prev(root, rel, rti, rte) };
    }
    if !crate::am::guc::vecfilter_enabled() {
        return;
    }
    let relref = unsafe { &mut *rel };
    if relref.reloptkind != pg_sys::RelOptKind::RELOPT_BASEREL {
        return;
    }
    // Find the vector-ordered path (the only base path with pathkeys — our index's distance ORDER BY) + a
    // BitmapHeapPath (the scalar filter). Both must exist for this to be a filtered vector query worth intercepting.
    let paths = unsafe { PgList::<pg_sys::Path>::from_pg(relref.pathlist) };
    let mut vector_path: *mut pg_sys::Path = std::ptr::null_mut();
    let mut bitmap_path: *mut pg_sys::Path = std::ptr::null_mut();
    for i in 0..paths.len() {
        if let Some(p) = paths.get_ptr(i) {
            // The vector-ordered path is the base IndexScan that carries distance pathkeys (only an amcanorderby
            // index — ours — produces base-rel pathkeys from `ORDER BY e <-> q`). Require T_IndexScan so an
            // unrelated ordered path can't be hijacked (council H3). Require UNPARAMETERIZED children (M94
            // hardening): our CustomPath declares `param_info = NULL`, so wrapping a parameterized child (e.g. a
            // LATERAL `cat = outer.col` bitmap) would break the planner's param contract — such queries fall back
            // to the native plans (correct, just not our fast path; honest boundary).
            if !unsafe { (*p).pathkeys.is_null() }
                && unsafe { (*p).pathtype } == pg_sys::NodeTag::T_IndexScan
                && unsafe { (*p).param_info.is_null() }
                && vector_path.is_null()
            {
                vector_path = p;
            }
            if unsafe { (*p).pathtype } == pg_sys::NodeTag::T_BitmapHeapScan
                && unsafe { (*p).param_info.is_null() }
                && bitmap_path.is_null()
            {
                bitmap_path = p;
            }
        }
    }
    if vector_path.is_null() || bitmap_path.is_null() {
        return;
    }

    // M95 — HONEST cost (replaces the spike's forced `min_cost * 0.1`). cost = term_B (produce the membership
    // bitmap) + term_V (the vector scan that filters by it). The planner then picks the node ONLY where it wins;
    // pathkeys (below) credit us the Sort the BitmapHeapScan+Sort competitor must pay. Fail-safe (EC-3): any
    // unreadable input degrades to NOT adding the node (native plan wins) — a planner hook must never error.
    let (startup_cost, total_cost) = if guc::vecfilter_force() {
        // Explicit user override (theodb.vecfilter_force): price below the cheapest base path so the planner picks
        // the node deterministically — for a selective filter where the node wins on RECALL but the probe-blind
        // native post-filter is under-priced (R4). Same rationale as an `enable_*` knob.
        let mut min_cost = unsafe { (*vector_path).total_cost };
        for i in 0..paths.len() {
            if let Some(p) = paths.get_ptr(i) {
                if unsafe { (*p).total_cost } < min_cost {
                    min_cost = unsafe { (*p).total_cost };
                }
            }
        }
        (0.0, min_cost * 0.1)
    } else {
        match unsafe { vecfilter_honest_cost(rel, vector_path, bitmap_path) } {
            Some(c) => c,
            None => return, // fail-safe: degenerate meta / null bitmapqual → skip the node, native plan is used
        }
    };
    // Hand-roll `create_customscan_path` — a 2-child CustomPath. Preserve the vector path's pathkeys so the node
    // reports distance-ordered output (no redundant Sort above it) and the planner credits us against a
    // Sort-paying competitor.
    let mut cpath =
        unsafe { PgBox::<pg_sys::CustomPath>::alloc_node(pg_sys::NodeTag::T_CustomPath) };
    let path = &mut cpath.path;
    path.pathtype = pg_sys::NodeTag::T_CustomScan;
    path.parent = rel;
    path.pathtarget = relref.reltarget;
    path.param_info = std::ptr::null_mut();
    path.pathkeys = unsafe { (*vector_path).pathkeys };
    path.rows = unsafe { (*vector_path).rows };
    path.startup_cost = startup_cost;
    path.total_cost = total_cost;
    cpath.flags = 0;
    let mut children = PgList::<pg_sys::Path>::new();
    children.push(vector_path); // [0] — ordered + carries the scalar Filter (the recheck)
    children.push(bitmap_path); // [1] — the whole BitmapHeapPath; its planned BitmapHeapScan.lefttree is the
    //        bitmap-producing sub-plan (BitmapIndexScan/BitmapAnd) we MultiExec
    cpath.custom_paths = children.into_pg();
    cpath.custom_private = std::ptr::null_mut();
    cpath.methods = &PATH_METHODS.0;
    unsafe { pg_sys::add_path(rel, cpath.into_pg() as *mut pg_sys::Path) };
}

/// M95 — the honest cost of the vecfilter node: `Some((startup, total))` or `None` when any input is unreadable
/// (EC-3 fail-safe — the caller then does NOT add the node, so the native plan wins; a planner hook must never
/// `error!`). `term_B` = the bitmap sub-plan's OWN cost (`bitmapqual.total_cost`, NOT the parent BitmapHeapPath
/// total — that would double-count the heap fetch we never perform). `term_V` = the vector scan re-priced for the
/// M91 adaptive probing derived from the bitmap selectivity (`cost::effective_probes` / `vecfilter_scan_cost`).
unsafe fn vecfilter_honest_cost(
    rel: *mut pg_sys::RelOptInfo,
    vector_path: *mut pg_sys::Path,
    bitmap_path: *mut pg_sys::Path,
) -> Option<(pg_sys::Cost, pg_sys::Cost)> {
    // term_B — the membership-PRODUCTION cost of the bitmap sub-plan (guard the null deref). Mirror
    // `cost_bitmap_tree_node` (costsize.c:1113-1136, Rule 9), NOT a blanket `.total_cost` read (M95 review HIGH-1):
    // for a plain `IndexPath` (the common single-predicate `cat=K` case) `.total_cost` INCLUDES the heap-tuple
    // fetch run cost — the fetch we never perform — so use `indextotalcost` + the bitmap-manipulation charge
    // instead; for `BitmapAndPath`/`BitmapOrPath` the node's own `.total_cost` is already the produce-only cost.
    let bpath = bitmap_path as *mut pg_sys::BitmapHeapPath;
    let bitmapqual = unsafe { (*bpath).bitmapqual };
    if bitmapqual.is_null() {
        return None;
    }
    let term_b = if unsafe { (*bitmapqual).type_ } == pg_sys::NodeTag::T_IndexPath {
        let ip = bitmapqual as *mut pg_sys::IndexPath;
        (unsafe { (*ip).indextotalcost })
            + 0.1 * unsafe { pg_sys::cpu_operator_cost } * unsafe { (*bitmapqual).rows }
    } else {
        (unsafe { (*bitmapqual).total_cost }) // BitmapAnd/BitmapOr — already produce-only
    };

    // Selectivity `s` = matching-tuple fraction = bitmap rows / rel tuples (the planner's own estimates).
    let rel_tuples = unsafe { (*rel).tuples };
    if rel_tuples <= 0.0 {
        return None; // never-ANALYZEd / empty rel — fall back to the native plan
    }
    let s = (unsafe { (*bitmap_path).rows } / rel_tuples).clamp(1e-9, 1.0);

    // Index geometry from the vector IndexPath: page count + tuple count + oid for a fail-safe meta read.
    let ipath = vector_path as *mut pg_sys::IndexPath;
    let ii = unsafe { (*ipath).indexinfo };
    if ii.is_null() {
        return None;
    }
    let index_pages = unsafe { (*ii).pages } as f64;
    let idx_tuples =
        if unsafe { (*ii).tuples } > 0.0 { unsafe { (*ii).tuples } } else { rel_tuples };

    // `lists` from the IVF meta — fail-safe like `cost::scan_visit_ratio` (any unreadable/torn meta → skip node).
    let irel = unsafe { pg_sys::index_open((*ii).indexoid, pg_sys::NoLock as pg_sys::LOCKMODE) };
    let lists = unsafe { page::read_ivf_aq_meta_split(irel) }
        .map(|m| m.dir.len())
        .or_else(|_| unsafe { page::read_ivf_aq_meta(irel).map(|m| m.dir.len()) })
        .or_else(|_| unsafe { page::read_ivf_meta(irel).map(|m| m.dir.len()) })
        .unwrap_or(0);
    unsafe { pg_sys::index_close(irel, pg_sys::NoLock as pg_sys::LOCKMODE) };
    if lists == 0 {
        return None; // not an IVF index we can cost honestly (e.g. HNSW) → let the native plan win
    }

    let avg_list_size = idx_tuples / lists as f64;
    let pages_per_list = (index_pages / lists as f64).max(1.0);
    let rerank_pool = 64.0 * guc::over_fetch() as f64;

    let eff = cost::effective_probes(s, lists, avg_list_size, rerank_pool, guc::probes());
    if eff <= 0.0 {
        return None; // fail-safe sentinel from a degenerate input
    }

    // Page/CPU cost globals (same source `cost_index` uses; per-tablespace random/seq).
    let mut spc_random: f64 = 0.0;
    let mut spc_seq: f64 = 0.0;
    unsafe {
        pg_sys::get_tablespace_page_costs((*ii).reltablespace, &mut spc_random, &mut spc_seq)
    };
    if spc_random <= 0.0 {
        spc_random = unsafe { pg_sys::random_page_cost };
    }
    let term_v = cost::vecfilter_scan_cost(
        eff,
        pages_per_list,
        avg_list_size,
        rerank_pool,
        lists,
        spc_random,
        unsafe { pg_sys::cpu_operator_cost },
    );

    // The ANN scan is not incremental: it gathers Stage-0/1 candidates before emitting the first ordered tuple, so
    // the whole term_V is front-loaded into startup (a LIMIT cannot skip it), plus term_B (the bitmap is
    // MultiExec'd eagerly in begin). total = the same (the scan then just pops the heap).
    let startup = term_b + term_v;
    let total = term_b + term_v;
    Some((startup, total))
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
    let mut cscan =
        unsafe { PgBox::<pg_sys::CustomScan>::alloc_node(pg_sys::NodeTag::T_CustomScan) };
    let plan = &mut cscan.scan.plan;
    plan.targetlist = tlist;
    plan.qual = std::ptr::null_mut(); // recheck lives on the vector child's Filter, not on the node
    plan.lefttree = std::ptr::null_mut();
    plan.righttree = std::ptr::null_mut();
    cscan.scan.scanrelid = unsafe { (*rel).relid };
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
unsafe extern "C-unwind" fn create_custom_scan_state(
    _cscan: *mut pg_sys::CustomScan,
) -> *mut pg_sys::Node {
    let ptr =
        unsafe { pg_sys::palloc0(std::mem::size_of::<VecFilterState>()) } as *mut VecFilterState;
    let st = unsafe { &mut *ptr };
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
    // M94: no concurrency guard needed anymore — each node stores its membership in the per-node REGISTRY and
    // installs it only during its own pull windows (swap-discipline in exec/rescan), so multiple vecfilter nodes in
    // one plan (UNION / self-join / partitioned Append) each see only their own set.
    let st = unsafe { &mut *(node as *mut VecFilterState) };
    let cscan = st.css.ss.ps.plan as *mut pg_sys::CustomScan;
    let planned = unsafe { PgList::<pg_sys::Plan>::from_pg((*cscan).custom_plans) };
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
    // IndexScan, which MultiExecProcNode rejects as "unrecognized node type: T_IndexScanState".) Tag-check first
    // (council M2 — defense in depth before dereferencing `.lefttree`).
    if unsafe { (*bplan).type_ } != pg_sys::NodeTag::T_BitmapHeapScan {
        pg_sys::error!("theodb vecfilter: bitmap child is not a BitmapHeapScan");
    }
    let bitmap_subplan = unsafe { (*bplan).lefttree };
    if bitmap_subplan.is_null() {
        pg_sys::error!("theodb vecfilter: BitmapHeapScan has no bitmap sub-plan");
    }
    st.vector_child = unsafe { pg_sys::ExecInitNode(vplan, estate, eflags) };
    st.bitmap_child = unsafe { pg_sys::ExecInitNode(bitmap_subplan, estate, eflags) };
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
    let res = unsafe { pg_sys::MultiExecProcNode(st.bitmap_child) };
    if res.is_null()
        || unsafe { (*(res as *mut pg_sys::Node)).type_ } != pg_sys::NodeTag::T_TIDBitmap
    {
        pg_sys::error!("theodb vecfilter: bitmap subplan did not return a TIDBitmap");
    }
    let (exact, lossy) = unsafe { materialize_bitmap(res as *mut pg_sys::TIDBitmap) };
    // Free the bitmap NOW (review MEDIUM-2): `MultiExecBitmapIndexScan` tbm_create's a FRESH bitmap per call and
    // `ExecEndBitmapIndexScan` does NOT free it (only core's BitmapHeapScan consumer does) — without this, every
    // begin/rescan leaks a work_mem-sized bitmap until the query context resets. We copied the TIDs out into owned
    // Rust sets above (copy-out-before-release), so the free is safe.
    unsafe { pg_sys::tbm_free(res as *mut pg_sys::TIDBitmap) };
    registry_insert(node as usize, Membership { exact, lossy });
    st.membership_active = true;
}

/// Exec next tuple (M94 swap-discipline): install THIS node's membership from the registry for the duration of the
/// synchronous child pull (all AM work — amrescan / amgettuple / iterative re-search — runs inside it), restore the
/// previous value after (RAII — restored even if the pull errors/unwinds). The child's own qpqual Filter has already
/// rechecked the scalar `WHERE` on the heap tuple (removing lossy/pending over-admits), so the node forwards the slot.
#[pg_guard]
unsafe extern "C-unwind" fn exec_custom_scan(
    node: *mut pg_sys::CustomScanState,
) -> *mut pg_sys::TupleTableSlot {
    let st = unsafe { &mut *(node as *mut VecFilterState) };
    if st.vector_child.is_null() {
        return std::ptr::null_mut();
    }
    let mine = registry_get(node as usize);
    if mine.is_none() && st.membership_active {
        // The node materialized a membership but the registry entry is gone (corrupt lifecycle) — fail loud,
        // never run the filtered scan unfiltered (Rule 8).
        pg_sys::error!("theodb vecfilter: membership missing for this scan");
    }
    let _guard = ActiveGuard::install(mine);
    unsafe { pg_sys::ExecProcNode(st.vector_child) }
}

/// Exec teardown: drop this node's registry entry (frees the sets) and end both children. The TIDBitmap was already
/// freed at materialization time (begin/rescan) — see the MEDIUM-2 note there.
#[pg_guard]
unsafe extern "C-unwind" fn end_custom_scan(node: *mut pg_sys::CustomScanState) {
    let st = unsafe { &mut *(node as *mut VecFilterState) };
    if st.membership_active {
        registry_remove(node as usize);
        st.membership_active = false;
    }
    if !st.vector_child.is_null() {
        unsafe { pg_sys::ExecEndNode(st.vector_child) };
    }
    if !st.bitmap_child.is_null() {
        unsafe { pg_sys::ExecEndNode(st.bitmap_child) };
    }
}

/// ReScan: re-derive this node's membership (the bitmap child may depend on changed state) into the registry, and
/// rescan both children — the vector child inside the swap window (its amrescan runs the Stage-1 scan synchronously,
/// which must see THIS node's membership).
#[pg_guard]
unsafe extern "C-unwind" fn rescan_custom_scan(node: *mut pg_sys::CustomScanState) {
    let st = unsafe { &mut *(node as *mut VecFilterState) };
    if !st.bitmap_child.is_null() {
        // Replace-then-set (mirror begin): re-derive from the rescanned bitmap child; fail loud on a non-TIDBitmap
        // (a rescanned bitmap child MUST produce one — Rule 8).
        unsafe { pg_sys::ExecReScan(st.bitmap_child) };
        let res = unsafe { pg_sys::MultiExecProcNode(st.bitmap_child) };
        if res.is_null()
            || unsafe { (*(res as *mut pg_sys::Node)).type_ } != pg_sys::NodeTag::T_TIDBitmap
        {
            pg_sys::error!("theodb vecfilter: rescanned bitmap subplan did not return a TIDBitmap");
        }
        let (exact, lossy) = unsafe { materialize_bitmap(res as *mut pg_sys::TIDBitmap) };
        // Free the fresh bitmap now (review MEDIUM-2) — nothing downstream frees it; the sets were copied out.
        unsafe { pg_sys::tbm_free(res as *mut pg_sys::TIDBitmap) };
        registry_insert(node as usize, Membership { exact, lossy });
        st.membership_active = true;
    }
    if !st.vector_child.is_null() {
        let mine = registry_get(node as usize);
        if mine.is_none() && st.membership_active {
            // Symmetry with exec (review LOW-3): a filtered scan must never rescan unfiltered.
            pg_sys::error!("theodb vecfilter: membership missing for this rescan");
        }
        let _guard = ActiveGuard::install(mine);
        unsafe { pg_sys::ExecReScan(st.vector_child) };
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
            c.select(
                "SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on",
                None,
                &[],
            )
            .ok();
            c.select(
                &format!("SELECT id FROM cs92 WHERE {filt} ORDER BY e <-> '{q}'::vector LIMIT 5"),
                None,
                &[],
            )
            .unwrap()
            .filter_map(|r| r.get::<i32>(1).unwrap())
            .collect()
        });
        // Hook ON — reset every enable_* GUC the exact block flipped (they persist in-session), then force the
        // bitmap path (enable_seqscan off so the planner keeps the BitmapHeapPath the hook needs).
        let hooked_setup = "SET theodb.enable_vecfilter=on; SET theodb.vecfilter_force=on; SET enable_indexscan=on; SET enable_bitmapscan=on; SET enable_seqscan=off; SET theodb_ivfflat.probes=8";
        let plan: String = Spi::connect(|c| {
            c.select(hooked_setup, None, &[]).ok();
            c.select(&format!("EXPLAIN (COSTS OFF) SELECT id FROM cs92 WHERE {filt} ORDER BY e <-> '{q}'::vector LIMIT 5"), None, &[])
                .unwrap().filter_map(|r| r.get::<String>(1).unwrap()).collect::<Vec<_>>().join("\n")
        });
        assert!(
            plan.contains("Custom Scan (theodb_vecfilter)"),
            "hook ON must inject the Custom Scan node (plan:\n{plan})"
        );
        let hooked: HashSet<i32> = Spi::connect(|c| {
            c.select(hooked_setup, None, &[]).ok();
            c.select(
                &format!("SELECT id FROM cs92 WHERE {filt} ORDER BY e <-> '{q}'::vector LIMIT 5"),
                None,
                &[],
            )
            .unwrap()
            .filter_map(|r| r.get::<i32>(1).unwrap())
            .collect()
        });
        assert_eq!(
            hooked, exact,
            "the 2-child Custom Scan filtered result must equal the exact seqscan-filtered top-5 (hooked={hooked:?} exact={exact:?})"
        );
    }

    /// M94 T1.1 — two vecfilter nodes in one plan (a UNION ALL of two filtered vector queries) each see their OWN
    /// membership (the pull-window swap-discipline). The branches filter on DISJOINT cat values, so any
    /// cross-contamination (the M93 BLOCKER this replaces) shows as a wrong id in one branch. Result must equal the
    /// union of the two exact seqscan-filtered top-3 sets.
    #[pgrx::pg_test]
    fn m94_union_two_filtered_scans_correct() {
        use std::collections::HashSet;
        Spi::run("CREATE TABLE cs94u (id int PRIMARY KEY, cat int, e vector(4))").unwrap();
        Spi::run("INSERT INTO cs94u SELECT g, g%100, ('['||g||','||(g*2)||',0,0]')::vector FROM generate_series(1,1000) g").unwrap();
        Spi::run("CREATE INDEX cs94u_e ON cs94u USING theodb_ivfflat (e) WITH (lists=8, pq_subspaces=2, aq_threshold=2000, separate_storage=1)").unwrap();
        Spi::run("CREATE INDEX cs94u_cat ON cs94u (cat)").unwrap();
        Spi::run("ANALYZE cs94u").unwrap();
        let q = "[0,0,0,0]";
        // Exact per-branch ground truth via seqscan.
        let exact = |cat: i32| -> HashSet<i32> {
            Spi::connect(|c| {
                c.select("SET theodb.enable_vecfilter=off; SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on", None, &[]).ok();
                c.select(
                    &format!(
                        "SELECT id FROM cs94u WHERE cat={cat} ORDER BY e <-> '{q}'::vector LIMIT 3"
                    ),
                    None,
                    &[],
                )
                .unwrap()
                .filter_map(|r| r.get::<i32>(1).unwrap())
                .collect()
            })
        };
        let want: HashSet<i32> = exact(1).union(&exact(2)).copied().collect();
        let hooked_setup = "SET theodb.enable_vecfilter=on; SET theodb.vecfilter_force=on; SET enable_indexscan=on; SET enable_bitmapscan=on; SET enable_seqscan=off; SET theodb_ivfflat.probes=8";
        // Both branches must get the node (2 occurrences) — proves TWO nodes coexist in the plan.
        let plan: String = Spi::connect(|c| {
            c.select(hooked_setup, None, &[]).ok();
            c.select(&format!("EXPLAIN (COSTS OFF) (SELECT id FROM cs94u WHERE cat=1 ORDER BY e <-> '{q}'::vector LIMIT 3) UNION ALL (SELECT id FROM cs94u WHERE cat=2 ORDER BY e <-> '{q}'::vector LIMIT 3)"), None, &[])
                .unwrap().filter_map(|r| r.get::<String>(1).unwrap()).collect::<Vec<_>>().join("\n")
        });
        let nodes = plan.matches("Custom Scan (theodb_vecfilter)").count();
        assert_eq!(
            nodes, 2,
            "both UNION branches must be intercepted (got {nodes} nodes; plan:\n{plan})"
        );
        let got: HashSet<i32> = Spi::connect(|c| {
            c.select(hooked_setup, None, &[]).ok();
            c.select(&format!("(SELECT id FROM cs94u WHERE cat=1 ORDER BY e <-> '{q}'::vector LIMIT 3) UNION ALL (SELECT id FROM cs94u WHERE cat=2 ORDER BY e <-> '{q}'::vector LIMIT 3)"), None, &[])
                .unwrap().filter_map(|r| r.get::<i32>(1).unwrap()).collect()
        });
        assert_eq!(
            got, want,
            "each UNION branch must be filtered by ITS OWN membership (got {got:?} want {want:?})"
        );
    }

    /// M94 T1.1 — a rescanned vecfilter node (nested-loop inner, materialization off) re-derives its membership and
    /// stays correct across repeated executions: each outer row's inner result equals the exact seqscan-filtered set.
    #[pgrx::pg_test]
    fn m94_rescan_reuses_membership_correct() {
        use std::collections::HashSet;
        Spi::run("CREATE TABLE cs94r (id int PRIMARY KEY, cat int, e vector(4))").unwrap();
        Spi::run("INSERT INTO cs94r SELECT g, g%100, ('['||g||','||(g*2)||',0,0]')::vector FROM generate_series(1,1000) g").unwrap();
        Spi::run("CREATE INDEX cs94r_e ON cs94r USING theodb_ivfflat (e) WITH (lists=8, pq_subspaces=2, aq_threshold=2000, separate_storage=1)").unwrap();
        Spi::run("CREATE INDEX cs94r_cat ON cs94r (cat)").unwrap();
        Spi::run("ANALYZE cs94r").unwrap();
        let q = "[0,0,0,0]";
        let exact: HashSet<i32> = Spi::connect(|c| {
            c.select("SET theodb.enable_vecfilter=off; SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on", None, &[]).ok();
            c.select(
                &format!("SELECT id FROM cs94r WHERE cat=7 ORDER BY e <-> '{q}'::vector LIMIT 3"),
                None,
                &[],
            )
            .unwrap()
            .filter_map(|r| r.get::<i32>(1).unwrap())
            .collect()
        });
        // A 2-row outer CROSS JOIN LATERAL over a constant filtered inner + materialization off → the inner vecfilter
        // node is re-executed (rescan) per outer row; both inner results must equal the exact set.
        let rows: Vec<(i32, i32)> = Spi::connect(|c| {
            c.select("SET theodb.enable_vecfilter=on; SET theodb.vecfilter_force=on; SET enable_indexscan=on; SET enable_bitmapscan=on; SET enable_seqscan=off; SET enable_material=off; SET theodb_ivfflat.probes=8", None, &[]).ok();
            c.select(&format!("SELECT v.o, s.id FROM (VALUES (1),(2)) v(o) CROSS JOIN LATERAL (SELECT id FROM cs94r WHERE cat=7 ORDER BY e <-> '{q}'::vector LIMIT 3) s ORDER BY v.o"), None, &[])
                .unwrap()
                .filter_map(|r| Some((r.get::<i32>(1).unwrap()?, r.get::<i32>(2).unwrap()?)))
                .collect()
        });
        for o in [1, 2] {
            let inner: HashSet<i32> =
                rows.iter().filter(|(oo, _)| *oo == o).map(|(_, id)| *id).collect();
            assert_eq!(
                inner, exact,
                "outer row {o}: the rescanned inner must equal the exact filtered set (got {inner:?})"
            );
        }
    }

    /// M94 ADR M94-3 — a PL/pgSQL EXCEPTION handler (subxact abort) clears a stale ACTIVE membership: after the
    /// abort, `has_membership()` is false, so a later plain scan cannot be silently starved by the dead filter.
    #[pgrx::pg_test]
    fn m94_subxact_abort_clears_membership() {
        // Simulate a mid-pull longjmp leaving the ACTIVE slot set (the swap-restore was skipped).
        super::set_membership(Some(super::Membership {
            exact: std::collections::HashSet::from([1i64]),
            lossy: std::collections::HashSet::new(),
        }));
        assert!(super::has_membership(), "precondition: ACTIVE slot set");
        // A plpgsql EXCEPTION block aborts a SUBtransaction — the subxact callback must clear the ACTIVE slot.
        Spi::run("DO $$ BEGIN RAISE EXCEPTION 'boom'; EXCEPTION WHEN OTHERS THEN NULL; END $$")
            .unwrap();
        assert!(!super::has_membership(), "subxact abort must clear the stale ACTIVE membership");
    }

    /// M95 — the honest cost is NOT the forced `min_cost * 0.1` hack anymore. At LOOSE selectivity (a predicate
    /// matching ~all rows) the bitmap-production cost (term_B over the whole table) makes the node lose to the
    /// native vector IndexScan+Filter → the node is NOT chosen. Before M95 the forced cost made the node ALWAYS
    /// appear regardless of selectivity; this test is the direct proof the hack is gone AND the cost discriminates.
    #[pgrx::pg_test]
    fn m95_loose_selectivity_not_chosen() {
        Spi::run("CREATE TABLE cs95l (id int PRIMARY KEY, cat int, e vector(4))").unwrap();
        Spi::run("INSERT INTO cs95l SELECT g, 1, ('['||g||','||(g*2)||',0,0]')::vector FROM generate_series(1,2000) g").unwrap();
        Spi::run("CREATE INDEX cs95l_e ON cs95l USING theodb_ivfflat (e) WITH (lists=8, pq_subspaces=2, aq_threshold=1000, separate_storage=1)").unwrap();
        Spi::run("CREATE INDEX cs95l_cat ON cs95l (cat)").unwrap();
        Spi::run("ANALYZE cs95l").unwrap();
        let plan: String = Spi::connect(|c| {
            c.select("SET theodb.enable_vecfilter=on; SET enable_seqscan=off; SET theodb_ivfflat.probes=8", None, &[]).ok();
            // cat=1 matches ALL 2000 rows (loose) — the bitmap covers the whole table, term_B is high.
            c.select("EXPLAIN (COSTS OFF) SELECT id FROM cs95l WHERE cat=1 ORDER BY e <-> '[0,0,0,0]'::vector LIMIT 10", None, &[])
                .unwrap().filter_map(|r| r.get::<String>(1).unwrap()).collect::<Vec<_>>().join("\n")
        });
        assert!(
            !plan.contains("theodb_vecfilter"),
            "loose selectivity ⇒ honest cost must NOT pick the node (plan:\n{plan})"
        );
    }

    /// M95 review HIGH-1 regression — a multi-predicate filter (two indexed columns) whose `bitmapqual` the planner
    /// may build as a `BitmapAndPath` exercises the term_B else-branch; with the honest cost active (force off) the
    /// query must PLAN and RUN without error (the term_B nodeTag branch + the fail-safe meta read are on the
    /// planning path of every filtered vector query, chosen or not). Forcing the node then proves correctness.
    #[pgrx::pg_test]
    fn m95_multi_predicate_filter_correct() {
        Spi::run("CREATE TABLE cs95m (id int PRIMARY KEY, cat int, grp int, e vector(4))").unwrap();
        Spi::run("INSERT INTO cs95m SELECT g, g%100, g%50, ('['||g||','||(g*2)||',0,0]')::vector FROM generate_series(1,1000) g").unwrap();
        Spi::run("CREATE INDEX cs95m_e ON cs95m USING theodb_ivfflat (e) WITH (lists=8, pq_subspaces=2, aq_threshold=2000, separate_storage=1)").unwrap();
        Spi::run("CREATE INDEX cs95m_cat ON cs95m (cat)").unwrap();
        Spi::run("CREATE INDEX cs95m_grp ON cs95m (grp)").unwrap();
        Spi::run("ANALYZE cs95m").unwrap();
        // Honest cost active (force OFF): planning executes vecfilter_honest_cost incl. the term_B branch — must not error.
        let _plan: String = Spi::connect(|c| {
            c.select("SET theodb.enable_vecfilter=on; SET theodb.vecfilter_force=off; SET enable_seqscan=off; SET theodb_ivfflat.probes=8", None, &[]).ok();
            c.select("EXPLAIN (COSTS OFF) SELECT id FROM cs95m WHERE cat=7 AND grp=7 ORDER BY e <-> '[0,0,0,0]'::vector LIMIT 5", None, &[])
                .unwrap().filter_map(|r| r.get::<String>(1).unwrap()).collect::<Vec<_>>().join("\n")
        });
        // Forced node on the multi-predicate filter: result == exact seqscan-filtered.
        let want: Vec<i32> = Spi::connect(|c| {
            c.select("SET theodb.enable_vecfilter=off; SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on", None, &[]).ok();
            c.select("SELECT id FROM cs95m WHERE cat=7 AND grp=7 ORDER BY e <-> '[0,0,0,0]'::vector LIMIT 5", None, &[])
                .unwrap().filter_map(|r| r.get::<i32>(1).unwrap()).collect()
        });
        let got: Vec<i32> = Spi::connect(|c| {
            c.select("SET theodb.enable_vecfilter=on; SET theodb.vecfilter_force=on; SET enable_seqscan=off; SET enable_indexscan=on; SET enable_bitmapscan=on; SET theodb_ivfflat.probes=8", None, &[]).ok();
            c.select("SELECT id FROM cs95m WHERE cat=7 AND grp=7 ORDER BY e <-> '[0,0,0,0]'::vector LIMIT 5", None, &[])
                .unwrap().filter_map(|r| r.get::<i32>(1).unwrap()).collect()
        });
        assert_eq!(
            got, want,
            "multi-predicate forced node must match exact seqscan (got {got:?} want {want:?})"
        );
    }

    // M95 auto-selection at TIGHT selectivity is measured on SIFT1M (T3.1 sweep, `wiki/benchmarks/m95-cost-model`),
    // NOT asserted here: with M48 unchanged the native post-filter competitor is probe-blind (under-priced), so
    // whether the planner AUTO-picks the node at tight is a measured, honest question — the blueprint's R4. The
    // node's honest cost correctness is proven by the `cost::` unit tests; the integration proof that the forced
    // `min_cost * 0.1` hack is GONE (no over-selection) is `m95_loose_selectivity_not_chosen` above.

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
            c.select("SET theodb.enable_vecfilter=on; SET theodb.vecfilter_force=on; SET enable_seqscan=off; SET theodb_ivfflat.probes=8", None, &[]).ok();
            c.select("EXPLAIN (COSTS OFF) SELECT id FROM cs92p WHERE cat=7 ORDER BY e <-> '[0,0,0,0]'::vector LIMIT 10", None, &[])
                .unwrap().filter_map(|r| r.get::<String>(1).unwrap()).collect::<Vec<_>>().join("\n")
        });
        assert!(
            plan.contains("Custom Scan (theodb_vecfilter)"),
            "the node must fire so its recheck is exercised (plan:\n{plan})"
        );
        let ids: Vec<i32> = Spi::connect(|c| {
            c.select("SET theodb.enable_vecfilter=on; SET theodb.vecfilter_force=on; SET enable_seqscan=off; SET theodb_ivfflat.probes=8", None, &[]).ok();
            c.select(
                "SELECT id FROM cs92p WHERE cat=7 ORDER BY e <-> '[0,0,0,0]'::vector LIMIT 10",
                None,
                &[],
            )
            .unwrap()
            .filter_map(|r| r.get::<i32>(1).unwrap())
            .collect()
        });
        assert!(
            !ids.contains(&9999),
            "the non-matching pending row (cat=99) must be rechecked out of a cat=7 filter (got {ids:?})"
        );
    }

    /// The hook is inert when the GUC is off (default): no Custom Scan node appears — production queries untouched.
    #[pgrx::pg_test]
    fn m92_customscan_inert_when_disabled() {
        Spi::run("CREATE TABLE cs92b (id int PRIMARY KEY, cat int, e vector(4))").unwrap();
        for i in 1..=20i32 {
            Spi::run(&format!("INSERT INTO cs92b VALUES ({i}, {}, '[{i},{},0,0]')", i % 3, i * 2))
                .unwrap();
        }
        Spi::run("CREATE INDEX cs92b_e ON cs92b USING theodb_ivfflat (e) WITH (lists=2, pq_subspaces=2, aq_threshold=2000, separate_storage=1)").unwrap();
        let plan: String = Spi::connect(|c| {
            c.select("SET theodb.enable_vecfilter=off", None, &[]).ok();
            c.select("EXPLAIN (COSTS OFF) SELECT id FROM cs92b WHERE cat=1 ORDER BY e <-> '[5,10,2,0]'::vector LIMIT 3", None, &[])
                .unwrap().filter_map(|r| r.get::<String>(1).unwrap()).collect::<Vec<_>>().join("\n")
        });
        assert!(
            !plan.contains("theodb_vecfilter"),
            "GUC off ⇒ no Custom Scan node (plan:\n{plan})"
        );
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
            Spi::run(&format!(
                "INSERT INTO m92m VALUES ({i}, '{{1}}', '[{i},{},{},{}]')",
                i * 2,
                i * 3,
                i % 7
            ))
            .unwrap();
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
            c.select(
                "SET enable_seqscan=off; SET enable_indexscan=on; SET theodb_ivfflat.probes=4",
                None,
                &[],
            )
            .ok();
            c.select(
                "SELECT id FROM m92m ORDER BY e <-> '[10,20,30,3]'::vector LIMIT 20",
                None,
                &[],
            )
            .unwrap()
            .filter_map(|r| r.get::<i32>(1).unwrap())
            .collect()
        });
        super::set_membership(None); // CRITICAL: clear so the filter does not leak to other queries in this backend.
        assert!(
            !got.is_empty(),
            "membership scan returned nothing — the skip filtered everything (plumbing broken)"
        );
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
            Spi::run(&format!("INSERT INTO m92c VALUES ({i}, '{{1}}', '[{i},{},0,0]')", i * 2))
                .unwrap();
        }
        Spi::run("CREATE INDEX m92c_e ON m92c USING theodb_ivfflat (e, lbl) WITH (lists=2, pq_subspaces=2, aq_threshold=2000, separate_storage=1)").unwrap();
        super::set_membership(Some(super::Membership {
            exact: std::collections::HashSet::from([1i64]),
            lossy: std::collections::HashSet::new(),
        }));
        super::set_membership(None);
        let got: Vec<i32> = Spi::connect(|c| {
            c.select(
                "SET enable_seqscan=off; SET enable_indexscan=on; SET theodb_ivfflat.probes=2",
                None,
                &[],
            )
            .ok();
            c.select("SELECT id FROM m92c ORDER BY e <-> '[5,10,0,0]'::vector LIMIT 10", None, &[])
                .unwrap()
                .filter_map(|r| r.get::<i32>(1).unwrap())
                .collect()
        });
        assert!(
            got.len() >= 5,
            "cleared membership ⇒ a normal scan returns many rows (got {got:?})"
        );
    }

    /// M93 T1.1 — the membership admits a candidate on a LOSSY-bitmap block (over-admit → the node recheck filters),
    /// never under-admits. Proof: a membership with only a lossy block admits that block's rows; an empty membership
    /// (no exact, no lossy) admits nothing. Guards ADR M93-2 (dropping lossy pages would silently miss valid rows).
    #[pgrx::pg_test]
    fn m93_t1_membership_admits_lossy_block() {
        use std::collections::HashSet;
        Spi::run("CREATE TABLE m92l (id int PRIMARY KEY, lbl smallint[], e vector(4))").unwrap();
        for i in 1..=40i32 {
            Spi::run(&format!("INSERT INTO m92l VALUES ({i}, '{{1}}', '[{i},{},0,0]')", i * 2))
                .unwrap();
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
                c.select(
                    "SET enable_seqscan=off; SET enable_indexscan=on; SET theodb_ivfflat.probes=2",
                    None,
                    &[],
                )
                .ok();
                c.select(
                    "SELECT id FROM m92l ORDER BY e <-> '[5,10,0,0]'::vector LIMIT 20",
                    None,
                    &[],
                )
                .unwrap()
                .filter_map(|r| r.get::<i32>(1).unwrap())
                .collect()
            })
        };
        super::set_membership(Some(super::Membership {
            exact: HashSet::new(),
            lossy: HashSet::from([block0]),
        }));
        let via_lossy = run();
        super::set_membership(Some(super::Membership {
            exact: HashSet::new(),
            lossy: HashSet::new(),
        }));
        let via_empty = run();
        super::set_membership(None);
        assert!(
            !via_lossy.is_empty(),
            "a lossy-member block must admit its rows (got {via_lossy:?})"
        );
        assert!(
            via_empty.is_empty(),
            "empty membership (no exact, no lossy) admits nothing (got {via_empty:?})"
        );
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
                ip_blkid: pgrx::pg_sys::BlockIdData {
                    bi_hi: (b >> 16) as u16,
                    bi_lo: (b & 0xffff) as u16,
                },
                ip_posid: o,
            };
            // sorted (block, offset): (0,5) (0,12) (2,3)
            let mut tids = [mk(0, 5), mk(0, 12), mk(2, 3)];
            pgrx::pg_sys::tbm_add_tuples(tbm, tids.as_mut_ptr(), 3, false);
            let (exact, lossy) = super::materialize_bitmap(tbm);
            pgrx::pg_sys::tbm_free(tbm);
            assert!(lossy.is_empty(), "a 3-tuple bitmap must not go lossy (got {lossy:?})");
            let expect: HashSet<i64> =
                [(0i64 << 16) | 5, (0i64 << 16) | 12, (2i64 << 16) | 3].into_iter().collect();
            assert_eq!(exact, expect, "materialized exact TIDs must match the added tuples");
        }
    }
}
