//! M100 Phase C — planner `CustomScan` integration for vectorized columnar aggregates.
//!
//! A simple analytical aggregate (`SELECT count(*), sum(<float8 col>) FROM <columnar table>`, NO GROUP BY / HAVING /
//! WHERE) is intercepted at the `UPPERREL_GROUP_AGG` stage and replaced by a `CustomScan` that runs the DataFusion
//! vectorized executor (`df_executor.rs`) in ONE plan — `EXPLAIN` shows the node, result-identical to a row-store.
//!
//! Slice 1 admits ONLY the cases where the Arrow result type matches the PG aggregate output type without a cast:
//! `count(*)` → `int8`, `sum(float8)` → `float8`. Anything else (GROUP BY, HAVING, WHERE, other aggs, `avg`,
//! `sum(int/numeric)`, joins) fails the admission guard → the native plan runs (fail-safe; a planner hook must never
//! error). Gated behind `theodb.enable_columnar_agg` (default OFF). Own-code glue (Rule 9), reusing the M92-95
//! `customscan.rs` machinery idioms.
#![allow(non_snake_case)]

use super::df_executor::{run_columnar_aggs, AggSpec};
use super::columnar_codec::MinMaxKind;
use super::zonemap::{ZoneOp, ZonePredicate};
use pgrx::datum::FromDatum;
use pgrx::{pg_guard, pg_sys, PgBox, PgList};
use pgrx::{GucContext, GucFlags, GucRegistry, GucSetting};
use std::ffi::{c_int, c_void, CStr};

/// `theodb.enable_columnar_agg` — default OFF (the vectorized aggregate path is opt-in until benchmarked).
pub(crate) static ENABLE_COLUMNAR_AGG: GucSetting<bool> = GucSetting::<bool>::new(false);

struct Methods<T>(T);
unsafe impl<T> Sync for Methods<T> {}

static PATH_METHODS: Methods<pg_sys::CustomPathMethods> = Methods(pg_sys::CustomPathMethods {
    CustomName: c"theodb_columnar_agg".as_ptr(),
    PlanCustomPath: Some(plan_custom_path),
    ReparameterizeCustomPathByChild: None,
});
static SCAN_METHODS: Methods<pg_sys::CustomScanMethods> = Methods(pg_sys::CustomScanMethods {
    CustomName: c"theodb_columnar_agg".as_ptr(),
    CreateCustomScanState: Some(create_custom_scan_state),
});
static EXEC_METHODS: Methods<pg_sys::CustomExecMethods> = Methods(pg_sys::CustomExecMethods {
    CustomName: c"theodb_columnar_agg".as_ptr(),
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

/// Node exec state: the CustomScanState (first, C-struct inheritance) + the computed aggregate result (a leaked
/// `Box<Vec<(Datum, is_null)>>` freed in `end`) + an emitted flag.
#[repr(C)]
struct ColumnarAggState {
    css: pg_sys::CustomScanState,
    result: *mut Vec<(pg_sys::Datum, bool)>,
    done: bool,
}

static mut PREV_UPPER_HOOK: pg_sys::create_upper_paths_hook_type = None;

/// Cached OID of the `theodb_columnar` table AM (resolved once per backend).
fn columnar_amoid() -> pg_sys::Oid {
    use std::sync::OnceLock;
    static AMOID: OnceLock<u32> = OnceLock::new();
    let raw = *AMOID.get_or_init(|| unsafe {
        pg_sys::get_am_oid(c"theodb_columnar".as_ptr(), true).to_u32()
    });
    unsafe { pg_sys::Oid::from_u32_unchecked(raw) }
}

/// Register the upper-paths hook + the CustomScan methods + the GUC. Called once from `_PG_init`.
pub(crate) fn init() {
    GucRegistry::define_bool_guc(
        c"theodb.enable_columnar_agg",
        c"Route simple columnar aggregates through the DataFusion vectorized executor",
        c"When on, count(*)/sum(float8) over a theodb_columnar table (no GROUP BY/WHERE) runs via a CustomScan.",
        &ENABLE_COLUMNAR_AGG,
        GucContext::Userset,
        GucFlags::default(),
    );
    unsafe {
        pg_sys::RegisterCustomScanMethods(&SCAN_METHODS.0);
        PREV_UPPER_HOOK = pg_sys::create_upper_paths_hook;
        pg_sys::create_upper_paths_hook = Some(upper_paths_hook);
    }
}

/// The parsed, admissible aggregate: (kind, attno). kind 0 = count(*), 1 = sum(float8). attno is the 1-based column
/// for sum (0 for count).
struct ParsedAgg {
    kind: i32,
    attno: i32,
}

/// Admission guard: is this a simple `count(*)` / `sum(float8)` aggregate (no GROUP BY/HAVING/WHERE/DISTINCT/window)
/// over a single base relation that is EITHER a columnar table (mode 0 — decode stripes) OR a heap table with a
/// usable Arrow cache (mode 1 — M101 HTAP)? Returns (mode, base RTE index, parsed aggs), or None (→ native plan).
/// Commute a `ZoneOp` for a `Const OP Var` clause normalised to `Var OP' Const`.
fn flip_op(op: ZoneOp) -> ZoneOp {
    match op {
        ZoneOp::Lt => ZoneOp::Gt,
        ZoneOp::Le => ZoneOp::Ge,
        ZoneOp::Eq => ZoneOp::Eq,
        ZoneOp::Ge => ZoneOp::Le,
        ZoneOp::Gt => ZoneOp::Lt,
    }
}

/// Encode a `Const`'s Datum to `const_bits` in the column's `MinMaxKind` domain — MUST match `compute_minmax`
/// (ints as `i64 as u64`, floats as `f64::to_bits`). `None` on a domain mismatch (fail-safe → clause not pushed).
unsafe fn encode_const_bits(datum: pg_sys::Datum, kind: MinMaxKind) -> Option<u64> {
    Some(match kind {
        MinMaxKind::I2 => (i16::from_datum(datum, false)? as i64) as u64,
        MinMaxKind::I4 => (i32::from_datum(datum, false)? as i64) as u64,
        MinMaxKind::I8 => (i64::from_datum(datum, false)?) as u64,
        MinMaxKind::Bool => (bool::from_datum(datum, false)? as i64) as u64,
        MinMaxKind::F4 => (f32::from_datum(datum, false)? as f64).to_bits(),
        MinMaxKind::F8 => f64::from_datum(datum, false)?.to_bits(),
        MinMaxKind::None => return None,
    })
}

/// Extract a pushable zone-map predicate from a base-rel qual (ADR D2/D5): `Var(col) <op> Const` where the operator
/// is the column-type-NATIVE btree comparison (strategy 1-5, both input types == the column type) and the const is
/// the same type. Returns `None` for ANY other shape (function, OR, cross-type, two-Var, NULL const, non-min/max-able
/// column) → the caller MUST fall back to the native plan so the WHERE is applied correctly.
unsafe fn extract_zone_predicate(clause: *mut pg_sys::Node, relid: i32) -> Option<ZonePredicate> {
    if clause.is_null() || (*clause).type_ != pg_sys::NodeTag::T_OpExpr {
        return None;
    }
    let op = clause as *mut pg_sys::OpExpr;
    let args = PgList::<pg_sys::Node>::from_pg((*op).args);
    if args.len() != 2 {
        return None;
    }
    let (a0, a1) = (args.get_ptr(0)?, args.get_ptr(1)?);
    let (var, konst, flipped) = if (*a0).type_ == pg_sys::NodeTag::T_Var && (*a1).type_ == pg_sys::NodeTag::T_Const {
        (a0 as *mut pg_sys::Var, a1 as *mut pg_sys::Const, false)
    } else if (*a0).type_ == pg_sys::NodeTag::T_Const && (*a1).type_ == pg_sys::NodeTag::T_Var {
        (a1 as *mut pg_sys::Var, a0 as *mut pg_sys::Const, true)
    } else {
        return None; // two-Var / function / etc.
    };
    if (*var).varno as i32 != relid || (*konst).constisnull {
        return None;
    }
    let vartype = (*var).vartype;
    if (*konst).consttype != vartype {
        return None; // cross-type (D5): the const must be the column type
    }
    let kind = super::columnar::minmax_kind_of(vartype.to_u32());
    if kind == MinMaxKind::None {
        return None;
    }
    // The operator's btree strategy in the column type's default opfamily (D5 — no hardcoded OIDs).
    let opclass = pg_sys::GetDefaultOpClass(vartype, pg_sys::BTREE_AM_OID);
    if opclass == pg_sys::InvalidOid {
        return None;
    }
    let opfamily = pg_sys::get_opclass_family(opclass);
    if !pg_sys::op_in_opfamily((*op).opno, opfamily) {
        return None;
    }
    let (mut strategy, mut lt, mut rt): (c_int, pg_sys::Oid, pg_sys::Oid) =
        (0, pg_sys::InvalidOid, pg_sys::InvalidOid);
    pg_sys::get_op_opfamily_properties((*op).opno, opfamily, false, &mut strategy, &mut lt, &mut rt);
    if lt != vartype || rt != vartype {
        return None; // not the same-type native comparison (D5)
    }
    let base = match strategy {
        1 => ZoneOp::Lt,
        2 => ZoneOp::Le,
        3 => ZoneOp::Eq,
        4 => ZoneOp::Ge,
        5 => ZoneOp::Gt,
        _ => return None,
    };
    let col = ((*var).varattno as i32).checked_sub(1)?; // 1-based AttrNumber → 0-based col; system cols (≤0) rejected
    Some(ZonePredicate {
        col: col as usize,
        op: if flipped { flip_op(base) } else { base },
        const_bits: encode_const_bits((*konst).constvalue, kind)?,
    })
}

/// Extract ALL of the base rel's WHERE quals as pushable predicates. Returns `None` if ANY qual is NOT pushable —
/// the DataFusion filter can only represent `col <op> const`, so an un-pushable qual means the CustomScan cannot
/// apply the full WHERE and MUST decline (the native plan then applies it correctly).
unsafe fn extract_all_predicates(input_rel: *mut pg_sys::RelOptInfo, relid: i32) -> Option<Vec<ZonePredicate>> {
    let ris = PgList::<pg_sys::RestrictInfo>::from_pg((*input_rel).baserestrictinfo);
    let mut preds = Vec::with_capacity(ris.len());
    for i in 0..ris.len() {
        let ri = ris.get_ptr(i)?;
        preds.push(extract_zone_predicate((*ri).clause as *mut pg_sys::Node, relid)?);
    }
    Some(preds)
}

unsafe fn admit(
    root: *mut pg_sys::PlannerInfo,
    input_rel: *mut pg_sys::RelOptInfo,
    output_rel: *mut pg_sys::RelOptInfo,
) -> Option<(i32, i32, Vec<ParsedAgg>, Vec<ZonePredicate>)> {
    let parse = (*root).parse;
    if !(*parse).groupClause.is_null()
        || !(*parse).groupingSets.is_null()
        || !(*parse).havingQual.is_null()
        || !(*parse).distinctClause.is_null()
        || (*parse).hasWindowFuncs
    {
        return None; // GROUP BY / HAVING / DISTINCT / window → not slice 1 (WHERE is now handled via zone-map pushdown)
    }
    if (*input_rel).reloptkind != pg_sys::RelOptKind::RELOPT_BASEREL {
        return None;
    }
    let relid = (*input_rel).relid as i32;
    if relid <= 0 {
        return None;
    }
    let rte = *(*root).simple_rte_array.add(relid as usize);
    if rte.is_null() || (*rte).rtekind != pg_sys::RTEKind::RTE_RELATION {
        return None;
    }
    // Every output target must be a supported bare aggregate.
    let target = (*output_rel).reltarget;
    if target.is_null() {
        return None;
    }
    let exprs = PgList::<pg_sys::Node>::from_pg((*target).exprs);
    if exprs.is_empty() {
        return None;
    }
    let mut aggs = Vec::with_capacity(exprs.len());
    for i in 0..exprs.len() {
        let node = exprs.get_ptr(i)?;
        if (*node).type_ != pg_sys::NodeTag::T_Aggref {
            return None;
        }
        let agg = node as *mut pg_sys::Aggref;
        if !(*agg).aggfilter.is_null() || !(*agg).aggorder.is_null() || !(*agg).aggdistinct.is_null() {
            return None;
        }
        // Only a SIMPLE (non-split) aggregate has the FINAL result type (int8/float8). A partial/parallel split
        // (AGGSPLIT_INITIAL_SERIAL etc.) produces the aggregate's transtype (internal/bytea), which would NOT match
        // the int8/float8 Datum we emit → fail-safe to the native plan (council-rust-pgrx HIGH).
        if (*agg).aggsplit != pg_sys::AggSplit::AGGSPLIT_SIMPLE {
            return None;
        }
        let fname = pg_sys::get_func_name((*agg).aggfnoid);
        if fname.is_null() {
            return None;
        }
        let name = CStr::from_ptr(fname).to_string_lossy();
        if name == "count" && (*agg).aggstar {
            aggs.push(ParsedAgg { kind: 0, attno: 0 });
        } else if name == "sum" {
            // sum(<bare Var of type float8>) — resolve the column attno.
            let args = PgList::<pg_sys::TargetEntry>::from_pg((*agg).args);
            if args.len() != 1 {
                return None;
            }
            let te = args.get_ptr(0)?;
            let e = (*te).expr as *mut pg_sys::Node;
            if e.is_null() || (*e).type_ != pg_sys::NodeTag::T_Var {
                return None;
            }
            let var = e as *mut pg_sys::Var;
            if (*var).vartype != pg_sys::FLOAT8OID || (*var).varno as i32 != relid {
                return None;
            }
            aggs.push(ParsedAgg { kind: 1, attno: (*var).varattno as i32 });
        } else {
            return None;
        }
    }

    // Mode: a columnar table (decode stripes) vs a heap table with a usable Arrow cache (M101 HTAP).
    let amoid = columnar_amoid();
    let is_columnar = amoid != pg_sys::InvalidOid && pg_sys::get_rel_relam((*rte).relid) == amoid;
    if is_columnar {
        // WHERE → zone-map predicates. ALL quals must be pushable (`col <op> const`), else decline so the native
        // plan applies the WHERE correctly (the DataFusion filter can only represent pushable clauses).
        let preds = extract_all_predicates(input_rel, relid)?;
        return Some((0, relid, aggs, preds));
    }
    // Heap (M101 cache) path does NOT filter → decline any WHERE.
    if !(*input_rel).baserestrictinfo.is_null() {
        return None;
    }
    // Heap: admissible IFF this backend has a cache covering the summed columns (the exec-time get_or_build then
    // does the generation check + snapshot-correct rebuild). Resolve the sum column names via the syscache.
    let sum_names: Vec<String> = aggs
        .iter()
        .filter(|a| a.kind == 1)
        .filter_map(|a| {
            let n = pg_sys::get_attname((*rte).relid, a.attno as pg_sys::AttrNumber, true);
            if n.is_null() {
                None
            } else {
                Some(CStr::from_ptr(n).to_string_lossy().into_owned())
            }
        })
        .collect();
    if sum_names.len() == aggs.iter().filter(|a| a.kind == 1).count()
        && super::arrow_cache::has_cached_columns((*rte).relid.to_u32(), &sum_names)
    {
        return Some((1, relid, aggs, Vec::new()));
    }
    None
}

/// `create_upper_paths_hook` — intercept a simple columnar aggregate and add the vectorized `CustomPath`.
#[pg_guard]
unsafe extern "C-unwind" fn upper_paths_hook(
    root: *mut pg_sys::PlannerInfo,
    stage: pg_sys::UpperRelationKind::Type,
    input_rel: *mut pg_sys::RelOptInfo,
    output_rel: *mut pg_sys::RelOptInfo,
    extra: *mut c_void,
) {
    if let Some(prev) = PREV_UPPER_HOOK {
        prev(root, stage, input_rel, output_rel, extra);
    }
    if !ENABLE_COLUMNAR_AGG.get() || stage != pg_sys::UpperRelationKind::UPPERREL_GROUP_AGG {
        return;
    }
    let Some((mode, relid, aggs, preds)) = admit(root, input_rel, output_rel) else {
        return; // fail-safe: any unsupported shape → native plan
    };

    // Encode the plan in custom_private as an IntList:
    //   [mode, relid, nagg, (kind,attno)×nagg, npred, (col, op, cbits_hi, cbits_lo)×npred].
    // u64 const_bits is split into two i32 (List holds C `int`); reassembled at exec (begin_custom_scan).
    let mut priv_list: *mut pg_sys::List = pg_sys::lappend_int(std::ptr::null_mut(), mode);
    priv_list = pg_sys::lappend_int(priv_list, relid);
    priv_list = pg_sys::lappend_int(priv_list, aggs.len() as i32);
    for a in &aggs {
        priv_list = pg_sys::lappend_int(priv_list, a.kind);
        priv_list = pg_sys::lappend_int(priv_list, a.attno);
    }
    priv_list = pg_sys::lappend_int(priv_list, preds.len() as i32);
    for p in &preds {
        priv_list = pg_sys::lappend_int(priv_list, p.col as i32);
        priv_list = pg_sys::lappend_int(priv_list, p.op as i32);
        priv_list = pg_sys::lappend_int(priv_list, (p.const_bits >> 32) as i32);
        priv_list = pg_sys::lappend_int(priv_list, (p.const_bits & 0xFFFF_FFFF) as i32);
    }

    let mut cpath = PgBox::<pg_sys::CustomPath>::alloc_node(pg_sys::NodeTag::T_CustomPath);
    let path = &mut cpath.path;
    path.pathtype = pg_sys::NodeTag::T_CustomScan;
    path.parent = output_rel;
    path.pathtarget = (*output_rel).reltarget;
    path.param_info = std::ptr::null_mut();
    path.rows = 1.0;
    path.startup_cost = 0.0;
    path.total_cost = 1.0; // cheap → wins over the native Agg for the admitted shape (opt-in GUC gates it)
    cpath.flags = 0;
    cpath.custom_paths = std::ptr::null_mut();
    cpath.custom_private = priv_list;
    cpath.methods = &PATH_METHODS.0;
    pg_sys::add_path(output_rel, cpath.into_pg() as *mut pg_sys::Path);
}

/// Path → Plan: a `CustomScan` with `scanrelid = 0` (synthetic aggregate result). `custom_scan_tlist` carries the
/// SAME aggregate output tlist as `plan.targetlist` — so `setrefs.c` rewrites the node's targetlist into INDEX_VAR
/// Vars referencing the custom_scan_tlist (resolving them), and the executor derives the scan tupdesc from the
/// aggregate output types via `ExecTypeFromTL(custom_scan_tlist)` WITHOUT evaluating the `Aggref`s (we fill the
/// scan slot with the computed values at exec time). A copy is used so setrefs' in-place rewrite of the plan
/// targetlist cannot corrupt the scan tlist.
#[pg_guard]
unsafe extern "C-unwind" fn plan_custom_path(
    _root: *mut pg_sys::PlannerInfo,
    _rel: *mut pg_sys::RelOptInfo,
    best_path: *mut pg_sys::CustomPath,
    tlist: *mut pg_sys::List,
    _clauses: *mut pg_sys::List,
    _custom_plans: *mut pg_sys::List,
) -> *mut pg_sys::Plan {
    let scan_tlist = pg_sys::copyObjectImpl(tlist as *const std::ffi::c_void) as *mut pg_sys::List;
    let mut cscan = PgBox::<pg_sys::CustomScan>::alloc_node(pg_sys::NodeTag::T_CustomScan);
    let plan = &mut cscan.scan.plan;
    plan.targetlist = tlist;
    plan.qual = std::ptr::null_mut();
    plan.lefttree = std::ptr::null_mut();
    plan.righttree = std::ptr::null_mut();
    cscan.scan.scanrelid = 0;
    cscan.flags = 0;
    cscan.custom_plans = std::ptr::null_mut();
    cscan.custom_exprs = std::ptr::null_mut();
    cscan.custom_private = (*best_path).custom_private;
    cscan.custom_scan_tlist = scan_tlist;
    cscan.custom_relids = std::ptr::null_mut();
    cscan.methods = &SCAN_METHODS.0;
    cscan.into_pg() as *mut pg_sys::Plan
}

#[pg_guard]
unsafe extern "C-unwind" fn create_custom_scan_state(_cscan: *mut pg_sys::CustomScan) -> *mut pg_sys::Node {
    let ptr = pg_sys::palloc0(std::mem::size_of::<ColumnarAggState>()) as *mut ColumnarAggState;
    let st = &mut *ptr;
    st.css.ss.ps.type_ = pg_sys::NodeTag::T_CustomScanState;
    st.css.methods = &EXEC_METHODS.0;
    st.result = std::ptr::null_mut();
    st.done = false;
    ptr as *mut pg_sys::Node
}

#[pg_guard]
unsafe extern "C-unwind" fn begin_custom_scan(
    node: *mut pg_sys::CustomScanState,
    estate: *mut pg_sys::EState,
    eflags: c_int,
) {
    let st = &mut *(node as *mut ColumnarAggState);
    st.done = false;
    st.result = std::ptr::null_mut();
    if (eflags & pg_sys::EXEC_FLAG_EXPLAIN_ONLY as c_int) != 0 {
        return; // EXPLAIN without ANALYZE: show the node, do not execute
    }
    let cscan = st.css.ss.ps.plan as *mut pg_sys::CustomScan;
    let priv_list = (*cscan).custom_private;
    let n = pg_sys::list_length(priv_list);
    let mode = pg_sys::list_nth_int(priv_list, 0);
    let relidx = pg_sys::list_nth_int(priv_list, 1);
    let rte = pg_sys::list_nth((*estate).es_range_table, relidx - 1) as *mut pg_sys::RangeTblEntry;
    let relid = (*rte).relid;

    let res = (|| -> Result<Vec<(pg_sys::Datum, bool)>, String> {
        // Parse the IntList: [mode, relid, nagg, (kind,attno)×nagg, npred, (col,op,cbits_hi,cbits_lo)×npred].
        let nagg = pg_sys::list_nth_int(priv_list, 2) as usize;
        let mut specs = Vec::with_capacity(nagg);
        let mut i = 3;
        for _ in 0..nagg {
            let kind = pg_sys::list_nth_int(priv_list, i);
            let attno = pg_sys::list_nth_int(priv_list, i + 1);
            i += 2;
            match kind {
                0 => specs.push(AggSpec::CountStar),
                1 => {
                    let nm = pg_sys::get_attname(relid, attno as pg_sys::AttrNumber, false);
                    specs.push(AggSpec::SumFloat8(CStr::from_ptr(nm).to_string_lossy().into_owned()));
                }
                _ => return Err(format!("columnar_agg: bad agg kind {kind}")),
            }
        }
        let npred = if i < n { pg_sys::list_nth_int(priv_list, i) as usize } else { 0 };
        i += 1;
        let mut preds = Vec::with_capacity(npred);
        for _ in 0..npred {
            let col = pg_sys::list_nth_int(priv_list, i) as usize;
            let opn = pg_sys::list_nth_int(priv_list, i + 1);
            let hi = pg_sys::list_nth_int(priv_list, i + 2) as u32 as u64;
            let lo = pg_sys::list_nth_int(priv_list, i + 3) as u32 as u64;
            i += 4;
            let op = match opn {
                0 => ZoneOp::Lt,
                1 => ZoneOp::Le,
                2 => ZoneOp::Eq,
                3 => ZoneOp::Ge,
                4 => ZoneOp::Gt,
                _ => return Err(format!("columnar_agg: bad zone op {opn}")),
            };
            preds.push(ZonePredicate { col, op, const_bits: (hi << 32) | lo });
        }
        if mode == 1 {
            // M101 HTAP: aggregate the heap-authoritative Arrow cache (rebuilt snapshot-correctly if invalidated).
            super::arrow_cache::run_cache_aggs(relid, &specs)
        } else {
            // M100: decode the columnar table's stripes, with zone-map skip-pruning (predicates + GUC kill-switch).
            let rel = pg_sys::relation_open(relid, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            let r = run_columnar_aggs(rel, &specs, &preds, super::guc::columnar_zonemap_skip());
            pg_sys::relation_close(rel, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            r
        }
    })();
    match res {
        Ok(v) => st.result = Box::into_raw(Box::new(v)),
        Err(e) => pg_sys::error!("{e}"),
    }
}

#[pg_guard]
unsafe extern "C-unwind" fn exec_custom_scan(node: *mut pg_sys::CustomScanState) -> *mut pg_sys::TupleTableSlot {
    let st = &mut *(node as *mut ColumnarAggState);
    let slot = st.css.ss.ss_ScanTupleSlot;
    if st.done || st.result.is_null() {
        return pg_sys::ExecClearTuple(slot);
    }
    let vals = &*st.result;
    pg_sys::ExecClearTuple(slot);
    let natts = (*(*slot).tts_tupleDescriptor).natts as usize;
    let tts_values = std::slice::from_raw_parts_mut((*slot).tts_values, natts);
    let tts_isnull = std::slice::from_raw_parts_mut((*slot).tts_isnull, natts);
    for i in 0..natts.min(vals.len()) {
        tts_values[i] = vals[i].0;
        tts_isnull[i] = vals[i].1;
    }
    pg_sys::ExecStoreVirtualTuple(slot);
    st.done = true;
    slot
}

#[pg_guard]
unsafe extern "C-unwind" fn end_custom_scan(node: *mut pg_sys::CustomScanState) {
    let st = &mut *(node as *mut ColumnarAggState);
    if !st.result.is_null() {
        drop(Box::from_raw(st.result));
        st.result = std::ptr::null_mut();
    }
}

#[pg_guard]
unsafe extern "C-unwind" fn rescan_custom_scan(node: *mut pg_sys::CustomScanState) {
    // The aggregate is unparameterized (admission required no correlated Var / param_info), so its result is
    // invariant across rescans — just re-emit the cached result.
    let st = &mut *(node as *mut ColumnarAggState);
    st.done = false;
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    /// M100 Phase C — a simple `count(*)` / `sum(measure)` over a `theodb_columnar` table (GUC on) is planned as a
    /// `CustomScan` (EXPLAIN shows the node) and is result-identical to the same aggregate over a heap table.
    #[pg_test]
    fn m100_columnar_agg_customscan_matches_heap() {
        Spi::run("SET theodb.enable_columnar_agg = on").unwrap();
        Spi::run("CREATE TABLE m100_ca (id int, measure float8) USING theodb_columnar").unwrap();
        Spi::run("CREATE TABLE m100_ha (id int, measure float8)").unwrap();
        let gen_sql = "SELECT g, (g * 1.5)::float8 FROM generate_series(1, 40000) g";
        Spi::run(&format!("INSERT INTO m100_ca {gen_sql}")).unwrap();
        Spi::run(&format!("INSERT INTO m100_ha {gen_sql}")).unwrap();

        // EXPLAIN: the top node over the columnar table is our CustomScan.
        let top = Spi::get_one::<String>("EXPLAIN (COSTS OFF) SELECT count(*), sum(measure) FROM m100_ca")
            .unwrap()
            .unwrap();
        assert!(
            top.contains("Custom Scan") || top.contains("theodb_columnar_agg"),
            "the columnar aggregate must be a CustomScan node: {top}"
        );

        // Result-equivalence (each aggregate goes through the vectorized path).
        let cc = Spi::get_one::<i64>("SELECT count(*) FROM m100_ca").unwrap().unwrap();
        let hc = Spi::get_one::<i64>("SELECT count(*) FROM m100_ha").unwrap().unwrap();
        assert_eq!(cc, hc, "count(*) must match the heap");
        let cs = Spi::get_one::<f64>("SELECT sum(measure) FROM m100_ca").unwrap().unwrap();
        let hs = Spi::get_one::<f64>("SELECT sum(measure) FROM m100_ha").unwrap().unwrap();
        assert!((cs - hs).abs() < 1e-3, "sum(measure) must match the heap ({cs} vs {hs})");

        Spi::run("DROP TABLE m100_ca").unwrap();
        Spi::run("DROP TABLE m100_ha").unwrap();
    }
}
