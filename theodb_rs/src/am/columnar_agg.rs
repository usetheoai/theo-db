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

use super::df_executor::{run_columnar_aggs, run_columnar_grouped_aggs, AggSpec};
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

/// Node exec state: the CustomScanState (first, C-struct inheritance) + the computed result ROWS (a leaked
/// `Box<Vec<Vec<(Datum, is_null)>>>` freed in `end`; one inner Vec per output row — a single row for a scalar
/// aggregate, N rows for GROUP BY) + a `cursor` over those rows (one emitted per `exec` call).
#[repr(C)]
struct ColumnarAggState {
    css: pg_sys::CustomScanState,
    result: *mut Vec<Vec<(pg_sys::Datum, bool)>>,
    cursor: usize,
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

/// Admission result. `group_cols` = (attno, typoid) per GROUP BY key; `layout` maps each output-target slot to its
/// source (kind 0=group→`group_cols[idx]`, 1=agg→`aggs[idx]`) so exec emits rows in target order (ADR-2). Non-grouped
/// admissions leave `group_cols`/`layout` empty (the scalar path needs no layout — aggs are already in target order).
struct Admitted {
    mode: i32,
    relid: i32,
    aggs: Vec<ParsedAgg>,
    preds: Vec<ZonePredicate>,
    group_cols: Vec<(i32, u32)>,
    layout: Vec<(u8, usize)>,
}

unsafe fn admit(
    root: *mut pg_sys::PlannerInfo,
    input_rel: *mut pg_sys::RelOptInfo,
    output_rel: *mut pg_sys::RelOptInfo,
) -> Option<Admitted> {
    let parse = (*root).parse;
    // GROUP BY is now admissible; groupingSets / HAVING / DISTINCT / window are still out of scope → native plan.
    if !(*parse).groupingSets.is_null()
        || !(*parse).havingQual.is_null()
        || !(*parse).distinctClause.is_null()
        || (*parse).hasWindowFuncs
    {
        return None;
    }
    let grouped = !(*parse).groupClause.is_null();
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
    let target = (*output_rel).reltarget;
    if target.is_null() {
        return None;
    }
    let exprs = PgList::<pg_sys::Node>::from_pg((*target).exprs);
    if exprs.is_empty() {
        return None;
    }
    // Walk the output target: each expr is a bare group `Var` (only when GROUP BY is present) or a supported `Aggref`.
    // Build the aggs, the group keys, and the output layout (ADR-2) in one pass, in target order.
    let mut aggs: Vec<ParsedAgg> = Vec::with_capacity(exprs.len());
    let mut group_cols: Vec<(i32, u32)> = Vec::new();
    let mut layout: Vec<(u8, usize)> = Vec::with_capacity(exprs.len());
    for i in 0..exprs.len() {
        let node = exprs.get_ptr(i)?;
        if (*node).type_ == pg_sys::NodeTag::T_Var {
            // A bare column reference in the target of a GROUP BY query is a grouping key (PG guarantees this). Only
            // when GROUP BY is present, and only for a base-rel column of a `build_arrow`-supported type.
            if !grouped {
                return None;
            }
            let var = node as *mut pg_sys::Var;
            if (*var).varno as i32 != relid {
                return None; // a Var from another rel → not a bare base-rel key
            }
            let attno = (*var).varattno as i32;
            if attno <= 0 {
                return None; // system / whole-row column → decline
            }
            if !super::df_executor::arrow_supported_group_type((*var).vartype.to_u32()) {
                return None; // unsupported key type (numeric, etc.) → native plan
            }
            layout.push((0, group_cols.len()));
            group_cols.push((attno, (*var).vartype.to_u32()));
        } else if (*node).type_ == pg_sys::NodeTag::T_Aggref {
            let agg = node as *mut pg_sys::Aggref;
            if !(*agg).aggfilter.is_null() || !(*agg).aggorder.is_null() || !(*agg).aggdistinct.is_null() {
                return None;
            }
            // Only a SIMPLE (non-split) aggregate has the FINAL result type (int8/float8). A partial/parallel split
            // produces the transtype (internal/bytea) → fail-safe to the native plan (council-rust-pgrx HIGH).
            if (*agg).aggsplit != pg_sys::AggSplit::AGGSPLIT_SIMPLE {
                return None;
            }
            let fname = pg_sys::get_func_name((*agg).aggfnoid);
            if fname.is_null() {
                return None;
            }
            let name = CStr::from_ptr(fname).to_string_lossy();
            if name == "count" && (*agg).aggstar {
                layout.push((1, aggs.len()));
                aggs.push(ParsedAgg { kind: 0, attno: 0 });
            } else if name == "sum" || name == "avg" {
                // sum/avg of a bare base-rel Var. Only the shapes whose Arrow output maps to the EXACT PG output type
                // are admitted (M114 blueprint E1/E2/E3): sum(float8)→float8 (kind 1), sum(int2/int4)→int8 (kind 2,
                // Arrow Int64=bigint, no overflow), avg(float8)→float8 (kind 3). DECLINED (→ native plan, PG returns
                // the exact numeric): avg(int*), sum(int8), sum(float4), avg(float4) (ADR-M114-1).
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
                if (*var).varno as i32 != relid {
                    return None;
                }
                let vartype = (*var).vartype;
                let kind = if name == "sum" {
                    if vartype == pg_sys::FLOAT8OID {
                        1
                    } else if vartype == pg_sys::INT2OID || vartype == pg_sys::INT4OID {
                        2
                    } else {
                        return None; // sum(int8)→numeric, sum(float4)→real: decline
                    }
                } else if vartype == pg_sys::FLOAT8OID {
                    3 // avg(float8)→float8
                } else {
                    return None; // avg(int*)/avg(float4)→numeric/float8-ULP: decline
                };
                layout.push((1, aggs.len()));
                aggs.push(ParsedAgg { kind, attno: (*var).varattno as i32 });
            } else {
                return None;
            }
        } else {
            return None; // grouping expression (date_trunc(...)) / anything else → decline
        }
    }
    if grouped && group_cols.is_empty() {
        return None; // GROUP BY with no bare-column key (e.g. GROUP BY on an expression only) → native plan
    }

    // Mode: a columnar table (decode stripes) vs a heap table with a usable Arrow cache (M101 HTAP).
    let amoid = columnar_amoid();
    let is_columnar = amoid != pg_sys::InvalidOid && pg_sys::get_rel_relam((*rte).relid) == amoid;
    if is_columnar {
        if grouped {
            // GROUP BY + WHERE combined (M114): extract the zone-map predicates like the scalar path. If ANY qual is
            // un-pushable, `extract_all_predicates` returns None → decline so the native plan applies the WHERE (the
            // DataFusion Filter can only represent pushable clauses); the skip + Filter compose with the grouping.
            let preds = extract_all_predicates(input_rel, relid)?;
            return Some(Admitted { mode: 0, relid, aggs, preds, group_cols, layout });
        }
        // Non-grouped: WHERE → zone-map predicates. ALL quals must be pushable (`col <op> const`), else decline so
        // the native plan applies the WHERE correctly.
        let preds = extract_all_predicates(input_rel, relid)?;
        return Some(Admitted { mode: 0, relid, aggs, preds, group_cols: Vec::new(), layout: Vec::new() });
    }
    // Heap (M101 cache) path: non-grouped only in this slice, and does NOT filter → decline GROUP BY or any WHERE.
    if grouped || !(*input_rel).baserestrictinfo.is_null() {
        return None;
    }
    // Heap (M101 cache): admissible IFF this backend has a cache covering the source column of EVERY column-bearing
    // aggregate (kinds 1/2/3 = SumFloat8/SumInt/AvgFloat8; count(*)=kind 0 needs no column). Resolve the attnos via
    // the syscache; if ANY is unresolved OR uncached, decline so the native plan runs. (M114 regression fix: filtering
    // only kind==1 left sum(int)/avg(float8) with an EMPTY name set → has_cached_columns([])==true → mode-1 admitted
    // with the column NOT cached → a runtime error on a query the native plan runs correctly.)
    let col_aggs: Vec<&ParsedAgg> = aggs.iter().filter(|a| a.kind != 0).collect();
    let col_names: Vec<String> = col_aggs
        .iter()
        .filter_map(|a| {
            let n = pg_sys::get_attname((*rte).relid, a.attno as pg_sys::AttrNumber, true);
            if n.is_null() {
                None
            } else {
                Some(CStr::from_ptr(n).to_string_lossy().into_owned())
            }
        })
        .collect();
    if col_names.len() == col_aggs.len()
        && super::arrow_cache::has_cached_columns((*rte).relid.to_u32(), &col_names)
    {
        return Some(Admitted { mode: 1, relid, aggs, preds: Vec::new(), group_cols: Vec::new(), layout: Vec::new() });
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
    let Some(adm) = admit(root, input_rel, output_rel) else {
        return; // fail-safe: any unsupported shape → native plan
    };

    // Encode the plan in custom_private as an IntList:
    //   [mode, relid, nagg, (kind,attno)×nagg, npred, (col, op, cbits_hi, cbits_lo)×npred,
    //    ngroup, (attno, typoid)×ngroup, noutput, (kind, idx)×noutput].
    // u64 const_bits is split into two i32 (List holds C `int`); reassembled at exec (begin_custom_scan). The group
    // block is appended last so old-shape parsers (npred-terminated) stay backward compatible (ngroup defaults 0).
    let mut priv_list: *mut pg_sys::List = pg_sys::lappend_int(std::ptr::null_mut(), adm.mode);
    priv_list = pg_sys::lappend_int(priv_list, adm.relid);
    priv_list = pg_sys::lappend_int(priv_list, adm.aggs.len() as i32);
    for a in &adm.aggs {
        priv_list = pg_sys::lappend_int(priv_list, a.kind);
        priv_list = pg_sys::lappend_int(priv_list, a.attno);
    }
    priv_list = pg_sys::lappend_int(priv_list, adm.preds.len() as i32);
    for p in &adm.preds {
        priv_list = pg_sys::lappend_int(priv_list, p.col as i32);
        priv_list = pg_sys::lappend_int(priv_list, p.op as i32);
        priv_list = pg_sys::lappend_int(priv_list, (p.const_bits >> 32) as i32);
        priv_list = pg_sys::lappend_int(priv_list, (p.const_bits & 0xFFFF_FFFF) as i32);
    }
    priv_list = pg_sys::lappend_int(priv_list, adm.group_cols.len() as i32);
    for &(attno, typoid) in &adm.group_cols {
        priv_list = pg_sys::lappend_int(priv_list, attno);
        priv_list = pg_sys::lappend_int(priv_list, typoid as i32);
    }
    priv_list = pg_sys::lappend_int(priv_list, adm.layout.len() as i32);
    for &(kind, idx) in &adm.layout {
        priv_list = pg_sys::lappend_int(priv_list, kind as i32);
        priv_list = pg_sys::lappend_int(priv_list, idx as i32);
    }

    let mut cpath = PgBox::<pg_sys::CustomPath>::alloc_node(pg_sys::NodeTag::T_CustomPath);
    let path = &mut cpath.path;
    path.pathtype = pg_sys::NodeTag::T_CustomScan;
    path.parent = output_rel;
    path.pathtarget = (*output_rel).reltarget;
    path.param_info = std::ptr::null_mut();
    // The planner's row estimate for this upper rel (the group count; 1 for a scalar aggregate) — accurate rows keep
    // any ORDER BY / LIMIT above the GROUP BY costed sanely.
    path.rows = if (*output_rel).rows > 0.0 { (*output_rel).rows } else { 1.0 };
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
    st.cursor = 0;
    ptr as *mut pg_sys::Node
}

#[pg_guard]
unsafe extern "C-unwind" fn begin_custom_scan(
    node: *mut pg_sys::CustomScanState,
    estate: *mut pg_sys::EState,
    eflags: c_int,
) {
    let st = &mut *(node as *mut ColumnarAggState);
    st.cursor = 0;
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

    // Materialize the result rows in the durable per-query context so text/varlena GROUP BY key datums survive across
    // exec() calls (ADR-3). By-value datums (int8/float8/date/timestamptz) are context-independent.
    let oldcxt = pg_sys::MemoryContextSwitchTo((*estate).es_query_cxt);
    let res = (|| -> Result<Vec<Vec<(pg_sys::Datum, bool)>>, String> {
        // IntList: [mode, relid, nagg, (kind,attno)×nagg, npred, (col,op,hi,lo)×npred,
        //           ngroup, (attno,typoid)×ngroup, noutput, (kind,idx)×noutput].
        let nagg = pg_sys::list_nth_int(priv_list, 2) as usize;
        let mut specs = Vec::with_capacity(nagg);
        let mut i = 3;
        for _ in 0..nagg {
            let kind = pg_sys::list_nth_int(priv_list, i);
            let attno = pg_sys::list_nth_int(priv_list, i + 1);
            i += 2;
            let col_name = |ano: i32| -> String {
                let nm = pg_sys::get_attname(relid, ano as pg_sys::AttrNumber, false);
                CStr::from_ptr(nm).to_string_lossy().into_owned()
            };
            match kind {
                0 => specs.push(AggSpec::CountStar),
                1 => specs.push(AggSpec::SumFloat8(col_name(attno))),
                2 => specs.push(AggSpec::SumInt(col_name(attno))),
                3 => specs.push(AggSpec::AvgFloat8(col_name(attno))),
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
        // Group block (appended last; absent → ngroup 0 → scalar path, backward compatible).
        let ngroup = if i < n { pg_sys::list_nth_int(priv_list, i) as usize } else { 0 };
        i += 1;
        let mut group_cols: Vec<(String, u32)> = Vec::with_capacity(ngroup);
        for _ in 0..ngroup {
            let attno = pg_sys::list_nth_int(priv_list, i);
            let typoid = pg_sys::list_nth_int(priv_list, i + 1) as u32;
            i += 2;
            let nm = pg_sys::get_attname(relid, attno as pg_sys::AttrNumber, false);
            group_cols.push((CStr::from_ptr(nm).to_string_lossy().into_owned(), typoid));
        }
        let noutput = if i < n { pg_sys::list_nth_int(priv_list, i) as usize } else { 0 };
        i += 1;
        let mut layout: Vec<(u8, usize)> = Vec::with_capacity(noutput);
        for _ in 0..noutput {
            let kind = pg_sys::list_nth_int(priv_list, i) as u8;
            let idx = pg_sys::list_nth_int(priv_list, i + 1) as usize;
            i += 2;
            layout.push((kind, idx));
        }

        if ngroup > 0 {
            // GROUP BY (columnar only — admit declined grouped heap / grouped+WHERE). Multi-row result.
            let rel = pg_sys::relation_open(relid, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            let r =
                run_columnar_grouped_aggs(rel, &group_cols, &specs, &layout, &preds, super::guc::columnar_zonemap_skip());
            pg_sys::relation_close(rel, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            r
        } else if mode == 1 {
            // M101 HTAP: aggregate the heap-authoritative Arrow cache. Single row → wrap.
            super::arrow_cache::run_cache_aggs(relid, &specs).map(|row| vec![row])
        } else {
            // M100 scalar: decode the columnar stripes with zone-map skip-pruning. Single row → wrap.
            let rel = pg_sys::relation_open(relid, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            let r = run_columnar_aggs(rel, &specs, &preds, super::guc::columnar_zonemap_skip()).map(|row| vec![row]);
            pg_sys::relation_close(rel, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            r
        }
    })();
    pg_sys::MemoryContextSwitchTo(oldcxt);
    match res {
        Ok(v) => st.result = Box::into_raw(Box::new(v)),
        Err(e) => pg_sys::error!("{e}"),
    }
}

#[pg_guard]
unsafe extern "C-unwind" fn exec_custom_scan(node: *mut pg_sys::CustomScanState) -> *mut pg_sys::TupleTableSlot {
    let st = &mut *(node as *mut ColumnarAggState);
    let slot = st.css.ss.ss_ScanTupleSlot;
    if st.result.is_null() {
        return pg_sys::ExecClearTuple(slot);
    }
    let rows = &*st.result;
    if st.cursor >= rows.len() {
        return pg_sys::ExecClearTuple(slot); // all rows emitted (scalar: 1 row; GROUP BY: N rows; empty: 0)
    }
    let vals = &rows[st.cursor];
    pg_sys::ExecClearTuple(slot);
    let natts = (*(*slot).tts_tupleDescriptor).natts as usize;
    let tts_values = std::slice::from_raw_parts_mut((*slot).tts_values, natts);
    let tts_isnull = std::slice::from_raw_parts_mut((*slot).tts_isnull, natts);
    for i in 0..natts.min(vals.len()) {
        tts_values[i] = vals[i].0;
        tts_isnull[i] = vals[i].1;
    }
    pg_sys::ExecStoreVirtualTuple(slot);
    st.cursor += 1;
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
    // invariant across rescans — just rewind the cursor and re-emit the cached rows.
    let st = &mut *(node as *mut ColumnarAggState);
    st.cursor = 0;
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

    /// GROUP BY pushdown — a grouped aggregate over a columnar table is a CustomScan and produces a result set
    /// byte-identical to the heap plan (fetched at top level via Spi and compared row-by-row — including a `text` key
    /// exercising the ADR-3 datum lifetime across the multi-row emit, a NULL group, and the ADR-2 agg-before-key
    /// column order). The full 1M in-PG A/B (`benchmarks/columnar_groupby_ab.py`) is the broader correctness gate.
    #[pg_test]
    fn test_admit_groupby_single_key_is_customscan_and_matches_heap() {
        Spi::run("SET theodb.enable_columnar_agg = on").unwrap();
        Spi::run("CREATE TABLE gb_c (k int, lbl text, x float8) USING theodb_columnar").unwrap();
        Spi::run("CREATE TABLE gb_h (k int, lbl text, x float8)").unwrap();
        // 5 groups on k (g%5); a text key with a NULL every 7th row; x monotonic.
        let gen_sql = "SELECT (g%5), CASE WHEN g%7=0 THEN NULL ELSE ('t'||(g%3)) END, g::float8 FROM generate_series(1,10000) g";
        Spi::run(&format!("INSERT INTO gb_c {gen_sql}")).unwrap();
        Spi::run(&format!("INSERT INTO gb_h {gen_sql}")).unwrap();

        // CustomScan engaged for the grouped aggregate.
        let plan = Spi::get_one::<String>("EXPLAIN (COSTS OFF) SELECT k, sum(x) FROM gb_c GROUP BY k").unwrap().unwrap();
        assert!(plan.contains("Custom Scan") || plan.contains("theodb_columnar_agg"), "GROUP BY must be a CustomScan: {plan}");

        // Fetch a grouped result set at TOP LEVEL (only bare Var/Aggref in the target so the CustomScan is admitted)
        // and compare the row lists. `q` is `(int_key, sum, count)` sorted by key.
        // Bare Var/Aggref target only (so the CustomScan is admitted); round sum(x) in Rust for a stable compare.
        let fetch = |t: &str| -> Vec<(i32, i64, i64)> {
            Spi::connect(|c| {
                let rows = c
                    .select(&format!("SELECT k, sum(x), count(*) FROM {t} GROUP BY k ORDER BY k"), None, &[])
                    .unwrap();
                rows.map(|r| {
                    (
                        r.get::<i32>(1).unwrap().unwrap(),
                        r.get::<f64>(2).unwrap().unwrap().round() as i64,
                        r.get::<i64>(3).unwrap().unwrap(),
                    )
                })
                .collect::<Vec<_>>()
            })
        };
        assert_eq!(fetch("gb_c"), fetch("gb_h"), "int GROUP BY (key, sum, count) result set must match the heap");

        // ADR-2: agg-BEFORE-key column order maps correctly (sum first, key second).
        let fetch_ab = |t: &str| -> Vec<(i64, i32)> {
            Spi::connect(|c| {
                let rows = c
                    .select(&format!("SELECT sum(x), k FROM {t} GROUP BY k ORDER BY k"), None, &[])
                    .unwrap();
                rows.map(|r| (r.get::<f64>(1).unwrap().unwrap().round() as i64, r.get::<i32>(2).unwrap().unwrap())).collect::<Vec<_>>()
            })
        };
        assert_eq!(fetch_ab("gb_c"), fetch_ab("gb_h"), "agg-before-key (ADR-2) result set must match the heap");

        // ADR-3: a text key (palloc'd varlena) grouped over multiple emitted rows, incl. the NULL group.
        let fetch_txt = |t: &str| -> Vec<(Option<String>, i64)> {
            Spi::connect(|c| {
                let rows = c
                    .select(&format!("SELECT lbl, count(*) FROM {t} GROUP BY lbl ORDER BY lbl NULLS FIRST"), None, &[])
                    .unwrap();
                rows.map(|r| (r.get::<String>(1).unwrap(), r.get::<i64>(2).unwrap().unwrap())).collect::<Vec<_>>()
            })
        };
        assert_eq!(fetch_txt("gb_c"), fetch_txt("gb_h"), "text GROUP BY (incl NULL group) must match the heap");

        Spi::run("DROP TABLE gb_c").unwrap();
        Spi::run("DROP TABLE gb_h").unwrap();
    }

    /// M114 admission surface: GROUP BY + WHERE combined is NOW admitted (pushable qual); avg(float8) + sum(int2/int4)
    /// admitted (byte-identical output types); avg(int*), sum(int8), sum(float4), and a grouping expression DECLINE to
    /// the native plan (numeric/ULP output — ADR-M114-1). Includes a byte-identical scalar spot-check vs the heap.
    #[pg_test]
    fn test_m114_aggregate_admission_and_declines() {
        Spi::run("SET theodb.enable_columnar_agg = on").unwrap();
        Spi::run("CREATE TABLE m114c (k int, ts timestamptz, x float8, i4 int4, i2 int2, b int8, f4 real) USING theodb_columnar").unwrap();
        Spi::run("CREATE TABLE m114h (k int, ts timestamptz, x float8, i4 int4, i2 int2, b int8, f4 real)").unwrap();
        let gen_sql = "SELECT g%5, timestamptz '2020-01-01'+(g*interval '1 min'), g::float8, g, (g%100), g::int8, g::real FROM generate_series(1,5000) g";
        Spi::run(&format!("INSERT INTO m114c {gen_sql}")).unwrap();
        Spi::run(&format!("INSERT INTO m114h {gen_sql}")).unwrap();
        let is_cs = |sql: &str| -> bool {
            Spi::get_one::<String>(&format!("EXPLAIN (COSTS OFF) {sql}")).unwrap().unwrap().contains("theodb_columnar_agg")
        };

        // ADMITTED (M114):
        assert!(is_cs("SELECT k, sum(x) FROM m114c WHERE k>=0 GROUP BY k"), "GROUP BY + pushable WHERE must be a CustomScan");
        assert!(is_cs("SELECT avg(x) FROM m114c"), "avg(float8) must be a CustomScan");
        assert!(is_cs("SELECT sum(i4) FROM m114c"), "sum(int4) must be a CustomScan");
        assert!(is_cs("SELECT sum(i2) FROM m114c"), "sum(int2) must be a CustomScan");
        // DECLINED (numeric / ULP output → native plan):
        assert!(!is_cs("SELECT avg(i4) FROM m114c"), "avg(int4)→numeric must decline");
        assert!(!is_cs("SELECT sum(b) FROM m114c"), "sum(int8)→numeric must decline");
        assert!(!is_cs("SELECT sum(f4) FROM m114c"), "sum(real) must decline");
        assert!(!is_cs("SELECT date_trunc('day',ts), sum(x) FROM m114c GROUP BY date_trunc('day',ts)"), "grouping expr must decline");
        // B1 regression (M114 heap M101-cache path): sum(int4) over a heap table with NO cache covering the column
        // must DECLINE — before the fix the empty kind==1 name-set made has_cached_columns([]) admit mode 1 wrongly.
        assert!(!is_cs("SELECT sum(i4) FROM m114h"), "heap sum(int4) with no cache must decline (B1 regression)");

        // Byte-identical scalar spot-check (top-level single-row → Spi::get_one works despite the M100 composability limit).
        let cavg = Spi::get_one::<f64>("SELECT avg(x) FROM m114c").unwrap().unwrap();
        let havg = Spi::get_one::<f64>("SELECT avg(x) FROM m114h").unwrap().unwrap();
        assert_eq!(cavg, havg, "avg(float8) must be byte-identical to the heap");
        let csi = Spi::get_one::<i64>("SELECT sum(i4) FROM m114c").unwrap().unwrap();
        let hsi = Spi::get_one::<i64>("SELECT sum(i4) FROM m114h").unwrap().unwrap();
        assert_eq!(csi, hsi, "sum(int4) must be byte-identical to the heap");

        Spi::run("DROP TABLE m114c").unwrap();
        Spi::run("DROP TABLE m114h").unwrap();
    }
}
