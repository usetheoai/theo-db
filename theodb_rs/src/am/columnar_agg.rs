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

use super::columnar_codec::MinMaxKind;
use super::df_executor::{AggSpec, run_columnar_aggs, run_columnar_grouped_aggs};
use super::zonemap::{ZoneOp, ZonePredicate};
use pgrx::datum::FromDatum;
use pgrx::{GucContext, GucFlags, GucRegistry, GucSetting};
use pgrx::{PgBox, PgList, pg_guard, pg_sys};
use std::ffi::{CStr, c_int, c_void};

/// `theodb.enable_columnar_agg` — default OFF (the vectorized aggregate path is opt-in until benchmarked).
pub(crate) static ENABLE_COLUMNAR_AGG: GucSetting<bool> = GucSetting::<bool>::new(false);

struct Methods<T>(T);
unsafe impl<T> Sync for Methods<T> {}

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
static mut PREV_PLANNER_HOOK: pg_sys::planner_hook_type = None;

/// M115 — the Agg-swap rearchitecture. `admit` runs at `upper_paths_hook` (parse-tree stage, where it can inspect the
/// query cleanly) but does NOT add a CustomPath — instead it STASHES the admission result keyed by the base table's
/// OID. The `planner_hook` then lets `standard_planner` build a NORMAL `Agg` node (whose output the parent nodes
/// reference as plain Vars via OUTER_VAR — NO Aggref leaks), and POST-planning (after `set_plan_refs`) swaps that Agg
/// → our `CustomScan`. Because the swap replaces an already-wired Agg with a node of the same output shape and the
/// CustomScan's targetlist is plain typed Vars (no Aggref), the aggregate output is consumable in
/// subqueries/joins/agg-ORDER-BY (fixes `cache lookup failed for attribute N of relation 0`) with no re-fixing.
#[derive(Clone)]
struct StashedAdmit {
    table_oid: u32,
    adm: Admitted,
    consumed: bool,
}
thread_local! {
    static ADMIT_STASH: std::cell::RefCell<Vec<StashedAdmit>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Cached OID of the `theodb_columnar` table AM (resolved once per backend).
fn columnar_amoid() -> pg_sys::Oid {
    use std::sync::OnceLock;
    static AMOID: OnceLock<u32> = OnceLock::new();
    let raw = *AMOID
        .get_or_init(|| unsafe { pg_sys::get_am_oid(c"theodb_columnar".as_ptr(), true).to_u32() });
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
        PREV_PLANNER_HOOK = pg_sys::planner_hook;
        pg_sys::planner_hook = Some(planner_hook);
    }
}

/// The parsed, admissible aggregate: (kind, attno). kind 0 = count(*), 1 = sum(float8), 2 = sum(int)→int8,
/// 3 = avg(float8). attno is the 1-based column (0 for count).
#[derive(Clone)]
struct ParsedAgg {
    kind: i32,
    attno: i32,
}

/// Admission guard: is this a simple `count(*)` / `sum(float8)` aggregate (no GROUP BY/HAVING/WHERE/DISTINCT/window)
/// over a single base relation that is EITHER a columnar table (mode 0 — decode stripes) OR a heap table with a
/// usable Arrow cache (mode 1 — M101 HTAP)? Returns (mode, base RTE index, parsed aggs), or None (→ native plan).
/// Commute a `ZoneOp` for a `Const OP Var` clause normalised to `Var OP' Const`.
pub(crate) fn flip_op(op: ZoneOp) -> ZoneOp {
    match op {
        ZoneOp::Lt => ZoneOp::Gt,
        ZoneOp::Le => ZoneOp::Ge,
        ZoneOp::Eq => ZoneOp::Eq,
        ZoneOp::Ge => ZoneOp::Le,
        ZoneOp::Gt => ZoneOp::Lt,
        ZoneOp::Ne => ZoneOp::Ne, // M151 — `<>` is symmetric: `c <> col` ≡ `col <> c`
    }
}

/// Encode a `Const`'s Datum to `const_bits` in the column's `MinMaxKind` domain — MUST match `compute_minmax`
/// (ints as `i64 as u64`, floats as `f64::to_bits`). `None` on a domain mismatch (fail-safe → clause not pushed).
pub(crate) unsafe fn encode_const_bits(datum: pg_sys::Datum, kind: MinMaxKind) -> Option<u64> {
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
pub(crate) unsafe fn extract_zone_predicate(clause: *mut pg_sys::Node, relid: i32) -> Option<ZonePredicate> {
    if clause.is_null() || (*clause).type_ != pg_sys::NodeTag::T_OpExpr {
        return None;
    }
    let op = clause as *mut pg_sys::OpExpr;
    let args = PgList::<pg_sys::Node>::from_pg((*op).args);
    if args.len() != 2 {
        return None;
    }
    let (a0, a1) = (args.get_ptr(0)?, args.get_ptr(1)?);
    let (var, konst, flipped) =
        if (*a0).type_ == pg_sys::NodeTag::T_Var && (*a1).type_ == pg_sys::NodeTag::T_Const {
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
    let kind = super::columnar::minmax_kind_of(vartype.to_u32());
    if kind == MinMaxKind::None {
        return None;
    }
    // M151 — the const need NOT be the exact column type. The ClickBench `<>`/`=` predicates are CROSS-TYPE
    // (`AdvEngineID int2 <> 0 int4`): the literal is int4, the column int2. `encode_const_coerced` (below) reads
    // the const in ITS type and casts to the column's min/max domain with a RANGE CHECK (out-of-range → None →
    // clause not pushed, always safe). The A/B gate (diverged=0) proves correctness.
    let consttype = (*konst).consttype.to_u32();
    // The operator's btree strategy in the column type's default opfamily (D5 — no hardcoded OIDs).
    let opclass = pg_sys::GetDefaultOpClass(vartype, pg_sys::BTREE_AM_OID);
    if opclass == pg_sys::InvalidOid {
        return None;
    }
    let opfamily = pg_sys::get_opclass_family(opclass);
    // M151 — `<>` is NOT a btree strategy (btree defines only 1-5: `<,<=,=,>=,>`). It is detected as the NEGATOR
    // of the btree `=` (strategy 3): if the op is not itself in the family but its negator is the family's `=`,
    // this is `col <> const`. `<>` never prunes (`chunk_can_match(Ne)=true`) — it rides the predicate list only to
    // reach the DataFusion `Filter` (`build_filter_expr → not_eq`, the final authority). A/B gate proves it.
    let (probe_op, forced_ne) = if pg_sys::op_in_opfamily((*op).opno, opfamily) {
        ((*op).opno, false)
    } else {
        let neg = pg_sys::get_negator((*op).opno);
        if neg == pg_sys::InvalidOid || !pg_sys::op_in_opfamily(neg, opfamily) {
            return None; // neither a native btree op nor the negator of one
        }
        (neg, true) // probe the `=` negator for its strategy/types; force op to Ne below
    };
    let (mut strategy, mut lt, mut rt): (c_int, pg_sys::Oid, pg_sys::Oid) =
        (0, pg_sys::InvalidOid, pg_sys::InvalidOid);
    pg_sys::get_op_opfamily_properties(probe_op, opfamily, false, &mut strategy, &mut lt, &mut rt);
    // M151 — only the VAR's side of the operator must equal the column type; the CONST side may differ (cross-type,
    // coerced below). `flipped` means the Var is the RIGHT operand (`const <op> col`), so its type is `rt`.
    let var_side = if flipped { rt } else { lt };
    if var_side != vartype {
        return None; // the operator does not apply to this column type
    }
    if forced_ne && strategy != 3 {
        return None; // the negator must be exactly `=` (strategy 3) for this to be `<>`
    }
    let base = if forced_ne {
        ZoneOp::Ne
    } else {
        match strategy {
            1 => ZoneOp::Lt,
            2 => ZoneOp::Le,
            3 => ZoneOp::Eq,
            4 => ZoneOp::Ge,
            5 => ZoneOp::Gt,
            _ => return None,
        }
    };
    let col = ((*var).varattno as i32).checked_sub(1)?; // 1-based AttrNumber → 0-based col; system cols (≤0) rejected
    Some(ZonePredicate {
        col: col as usize,
        op: if flipped { flip_op(base) } else { base },
        const_bits: encode_const_coerced((*konst).constvalue, consttype, kind)?,
    })
}

/// M151 — coerce a `Const` (in `consttype`) into `const_bits` in the COLUMN's `target` MinMaxKind domain, for the
/// cross-type ClickBench pattern (`col int2 <> 0 int4`). Reads the const in ITS own type, then numerically casts
/// to the column domain with a RANGE CHECK: an out-of-range cast (e.g. `int2col = 40000`) returns `None` → the
/// clause is not pushed and the native plan handles it (ALWAYS SAFE — for `=`/`<>` the out-of-range value can
/// never match/exclude a real int2 row; for `<`/`>` an out-of-range bound makes the predicate trivially
/// true/false, which the native plan evaluates correctly). Same-type consts fall through to `encode_const_bits`.
/// The result MUST agree with `compute_minmax` (ints as `i64 as u64`, floats as `f64::to_bits`).
unsafe fn encode_const_coerced(datum: pg_sys::Datum, consttype: u32, target: MinMaxKind) -> Option<u64> {
    // Read the const value in its OWN type. Integers/temporal → i128 (wide enough for any range check); floats → f64.
    enum V {
        I(i128),
        F(f64),
        B(bool),
    }
    let v = match consttype {
        21 => V::I(i16::from_datum(datum, false)? as i128),
        23 => V::I(i32::from_datum(datum, false)? as i128),
        20 => V::I(i64::from_datum(datum, false)? as i128),
        16 => V::B(bool::from_datum(datum, false)?),
        700 => V::F(f32::from_datum(datum, false)? as f64),
        701 => V::F(f64::from_datum(datum, false)?),
        1114 | 1184 => V::I(i64::from_datum(datum, false)? as i128), // timestamp/tz μs
        1082 => V::I(i32::from_datum(datum, false)? as i128),        // date days
        _ => return None,                                            // non-numeric const → cannot coerce
    };
    // Cast into the column's min/max domain, RANGE-CHECKED (i128 → i16/i32/i64 via try_from).
    Some(match (target, v) {
        (MinMaxKind::I2, V::I(x)) => (i16::try_from(x).ok()? as i64) as u64,
        (MinMaxKind::I4, V::I(x)) => (i32::try_from(x).ok()? as i64) as u64,
        (MinMaxKind::I8, V::I(x)) => (i64::try_from(x).ok()? as i64) as u64,
        (MinMaxKind::Bool, V::B(b)) => (b as i64) as u64,
        (MinMaxKind::F4, V::F(f)) => (f as f32 as f64).to_bits(),
        (MinMaxKind::F8, V::F(f)) => f.to_bits(),
        // an integer literal compared against a float column (`col float8 <> 5`) — widen to float.
        (MinMaxKind::F4, V::I(x)) => (x as f32 as f64).to_bits(),
        (MinMaxKind::F8, V::I(x)) => (x as f64).to_bits(),
        _ => return None, // incompatible (e.g. a float literal into an int column — narrowing loses info; decline)
    })
}

/// Extract ALL of the base rel's WHERE quals as pushable predicates. Returns `None` if ANY qual is NOT pushable —
/// the DataFusion filter can only represent `col <op> const`, so an un-pushable qual means the CustomScan cannot
/// apply the full WHERE and MUST decline (the native plan then applies it correctly).
unsafe fn extract_all_predicates(
    input_rel: *mut pg_sys::RelOptInfo,
    relid: i32,
) -> Option<Vec<ZonePredicate>> {
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
#[derive(Clone)]
struct Admitted {
    mode: i32,
    relid: i32,
    aggs: Vec<ParsedAgg>,
    preds: Vec<ZonePredicate>,
    group_cols: Vec<(i32, u32)>,
    layout: Vec<(u8, usize)>,
}

impl Admitted {
    /// The number of output columns this admission produces — the layout length when grouped, else one per aggregate
    /// (the scalar path leaves `layout` empty). Used by the M115 swap's shape guard (review B3).
    fn expected_arity(&self) -> usize {
        if self.layout.is_empty() { self.aggs.len() } else { self.layout.len() }
    }
}

// M145 T1.4: `admit` (CC 59 → ≤ 25 por lizard) decomposto por Extract Function preservando o BYTE-IDÊNTICO do
// Agg-swap M115 — a ORDEM de decisão (preamble → walk → grouped-empty → mode) e TODO ponto de `None`/`?` são
// idênticos. `classify_target_node` retorna `None` exatamente para os mesmos nós que os ramos inline rejeitavam;
// os `layout.push((_, len()))` ficam no `main` (ordem/índices preservados). O check `grouped && group_cols.is_empty()`
// permanece no `main`, DEPOIS do walk. Nenhuma decisão cruza a fronteira do loop/modo.

/// Um slot de output classificado: uma group key (attno, vartype) ou um agregado parseado. `main` empurra em
/// `layout`/`group_cols`/`aggs` na ORDEM do target (índices dependem do comprimento no push — preservados).
enum TargetSlot {
    Group(i32, u32),
    Agg(ParsedAgg),
}

/// sum/avg/min/max × tipo de input → código `kind` (M114 blueprint E1/E2/E3 + numeric-output ADR-N1 + minmax
/// ADR-MM1), ou `None` para declinar (→ native plan). sum(float8)→1, sum(int2/4)→2, avg(float8)→3, sum(int8)→4,
/// avg(int2/4/8)→5, min→6, max→7. DECLINADO: sum(float4), avg(float4), min/max em tipo não-ordenado.
fn parse_agg_kind(name: &str, vartype: pg_sys::Oid) -> Option<i32> {
    let kind = if name == "sum" {
        if vartype == pg_sys::FLOAT8OID {
            1 // sum(float8)→float8
        } else if vartype == pg_sys::INT2OID || vartype == pg_sys::INT4OID {
            2 // sum(int2/4)→int8 (Arrow Int64, no overflow)
        } else if vartype == pg_sys::INT8OID {
            4 // sum(int8)→numeric (exact Decimal128 → AnyNumeric — numeric-output blueprint ADR-N1)
        } else {
            return None; // sum(float4)→real, sum(numeric): decline
        }
    } else if name == "avg" {
        if vartype == pg_sys::FLOAT8OID {
            3 // avg(float8)→float8
        } else if vartype == pg_sys::INT2OID
            || vartype == pg_sys::INT4OID
            || vartype == pg_sys::INT8OID
        {
            5 // avg(int2/4/8)→numeric (AnyNumeric division = PG numeric_div — ADR-N1)
        } else {
            return None; // avg(float4)→float8-ULP, avg(numeric): decline
        }
    } else {
        // min/max: any ordered native type (same set the zone-map supports) → output = input type.
        if super::columnar::minmax_kind_of(vartype.to_u32()) == MinMaxKind::None {
            return None; // unordered type (text/numeric/…) → native plan
        }
        if name == "min" { 6 } else { 7 }
    };
    Some(kind)
}

/// Classifica UM nó do output target: uma group `Var` (só quando há GROUP BY) ou um `Aggref` suportado. Retorna
/// `None` (→ decline) exatamente para os mesmos nós que os ramos inline do `admit` original rejeitavam — o
/// fail-safe `aggsplit != AGGSPLIT_SIMPLE` incluso (council-rust-pgrx HIGH).
unsafe fn classify_target_node(
    node: *mut pg_sys::Node,
    relid: i32,
    grouped: bool,
) -> Option<TargetSlot> {
    if (*node).type_ == pg_sys::NodeTag::T_Var {
        // A bare column reference in the target of a GROUP BY query is a grouping key. Only when GROUP BY is
        // present, and only for a base-rel column of a `build_arrow`-supported type.
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
        Some(TargetSlot::Group(attno, (*var).vartype.to_u32()))
    } else if (*node).type_ == pg_sys::NodeTag::T_Aggref {
        let agg = node as *mut pg_sys::Aggref;
        if !(*agg).aggfilter.is_null()
            || !(*agg).aggorder.is_null()
            || !(*agg).aggdistinct.is_null()
        {
            return None;
        }
        // Only a SIMPLE (non-split) aggregate has the FINAL result type. A partial/parallel split produces the
        // transtype (internal/bytea) → fail-safe to the native plan (council-rust-pgrx HIGH).
        if (*agg).aggsplit != pg_sys::AggSplit::AGGSPLIT_SIMPLE {
            return None;
        }
        let fname = pg_sys::get_func_name((*agg).aggfnoid);
        if fname.is_null() {
            return None;
        }
        let name = CStr::from_ptr(fname).to_string_lossy();
        if name == "count" && (*agg).aggstar {
            Some(TargetSlot::Agg(ParsedAgg { kind: 0, attno: 0 }))
        } else if name == "sum" || name == "avg" || name == "min" || name == "max" {
            let args = PgList::<pg_sys::TargetEntry>::from_pg((*agg).args);
            if args.len() != 1 {
                return None;
            }
            let te = args.get_ptr(0)?;
            let e = (*te).expr as *mut pg_sys::Node;
            if e.is_null() || (*e).type_ != pg_sys::NodeTag::T_Var {
                return None; // bare column Var only — reject min(col+1) / cast (directory is pre-projection)
            }
            let var = e as *mut pg_sys::Var;
            if (*var).varno as i32 != relid {
                return None;
            }
            let kind = parse_agg_kind(&name, (*var).vartype)?;
            Some(TargetSlot::Agg(ParsedAgg { kind, attno: (*var).varattno as i32 }))
        } else {
            None
        }
    } else {
        None // grouping expression (date_trunc(...)) / anything else → decline
    }
}

/// Decide o MODO (columnar-decode vs heap-cache M101) e monta o `Admitted` a partir de um walk já validado.
/// Consome `aggs`/`group_cols`/`layout`. columnar-antes-de-heap e todo `?`/`None` na ordem do `admit` original.
unsafe fn build_admission(
    rte: *mut pg_sys::RangeTblEntry,
    input_rel: *mut pg_sys::RelOptInfo,
    relid: i32,
    grouped: bool,
    aggs: Vec<ParsedAgg>,
    group_cols: Vec<(i32, u32)>,
    layout: Vec<(u8, usize)>,
) -> Option<Admitted> {
    // Mode: a columnar table (decode stripes) vs a heap table with a usable Arrow cache (M101 HTAP).
    let amoid = columnar_amoid();
    let is_columnar = amoid != pg_sys::InvalidOid && pg_sys::get_rel_relam((*rte).relid) == amoid;
    if is_columnar {
        if grouped {
            // GROUP BY + WHERE combined (M114): un-pushable qual → `extract_all_predicates` None → decline.
            let preds = extract_all_predicates(input_rel, relid)?;
            return Some(Admitted { mode: 0, relid, aggs, preds, group_cols, layout });
        }
        // Non-grouped: ALL quals must be pushable (`col <op> const`), else decline.
        let preds = extract_all_predicates(input_rel, relid)?;
        return Some(Admitted {
            mode: 0,
            relid,
            aggs,
            preds,
            group_cols: Vec::new(),
            layout: Vec::new(),
        });
    }
    // Heap (M101 cache): non-grouped only in this slice, and does NOT filter → decline GROUP BY or any WHERE.
    if grouped || !(*input_rel).baserestrictinfo.is_null() {
        return None;
    }
    // Admissible IFF this backend has a cache covering the source column of EVERY column-bearing aggregate
    // (kind != 0). If ANY attno is unresolved OR uncached, decline so the native plan runs (M114 regression fix).
    let col_aggs: Vec<&ParsedAgg> = aggs.iter().filter(|a| a.kind != 0).collect();
    let col_names: Vec<String> = col_aggs
        .iter()
        .filter_map(|a| {
            let n = pg_sys::get_attname((*rte).relid, a.attno as pg_sys::AttrNumber, true);
            if n.is_null() { None } else { Some(CStr::from_ptr(n).to_string_lossy().into_owned()) }
        })
        .collect();
    if col_names.len() == col_aggs.len()
        && super::arrow_cache::has_cached_columns((*rte).relid.to_u32(), &col_names)
    {
        return Some(Admitted {
            mode: 1,
            relid,
            aggs,
            preds: Vec::new(),
            group_cols: Vec::new(),
            layout: Vec::new(),
        });
    }
    None
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
        match classify_target_node(node, relid, grouped)? {
            TargetSlot::Group(attno, vartype) => {
                layout.push((0, group_cols.len()));
                group_cols.push((attno, vartype));
            }
            TargetSlot::Agg(parsed) => {
                layout.push((1, aggs.len()));
                aggs.push(parsed);
            }
        }
    }
    if grouped && group_cols.is_empty() {
        return None; // GROUP BY with no bare-column key (e.g. GROUP BY on an expression only) → native plan
    }
    build_admission(rte, input_rel, relid, grouped, aggs, group_cols, layout)
}

/// `create_upper_paths_hook` — run `admit` and STASH the result keyed by the base table's OID (M115). Does NOT add a
/// CustomPath: `standard_planner` then builds a normal `Agg`, and `planner_hook` swaps it post-planning. Stashing at
/// this stage reuses `admit`'s clean parse-tree analysis (aggs, group cols, pushable WHERE) instead of re-deriving it
/// from planned nodes.
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
    // Resolve the base table's stable pg_class OID (the swap matches the planned Agg's child scan by OID).
    let rte = *(*root).simple_rte_array.add(adm.relid as usize);
    if rte.is_null() {
        return;
    }
    let table_oid = (*rte).relid.to_u32();
    ADMIT_STASH.with(|s| s.borrow_mut().push(StashedAdmit { table_oid, adm, consumed: false }));
}

/// `planner_hook` — run the standard planner (which builds a normal `Agg` for each admitted columnar aggregate), then
/// swap those Aggs → our `CustomScan` post-`set_plan_refs` (M115 Agg-swap). The stash is per-planning-run.
#[pg_guard]
unsafe extern "C-unwind" fn planner_hook(
    parse: *mut pg_sys::Query,
    query_string: *const std::os::raw::c_char,
    cursor_options: c_int,
    bound_params: pg_sys::ParamListInfo,
) -> *mut pg_sys::PlannedStmt {
    // Save the enclosing run's stash and restore it on scope exit — INCLUDING a planner longjmp/ereport (pgrx
    // converts the C longjmp to a Rust unwind at the `#[pg_guard]` boundary, so `Drop` runs). Without this a planning
    // ERROR would leave a stale inner-run stash that a later query could mis-consume (review H1).
    struct StashGuard(Vec<StashedAdmit>);
    impl Drop for StashGuard {
        fn drop(&mut self) {
            ADMIT_STASH.with(|s| *s.borrow_mut() = std::mem::take(&mut self.0));
        }
    }
    let _guard =
        StashGuard(ADMIT_STASH.with(|s| std::mem::replace(&mut *s.borrow_mut(), Vec::new())));
    let stmt = match PREV_PLANNER_HOOK {
        Some(prev) => prev(parse, query_string, cursor_options, bound_params),
        None => pg_sys::standard_planner(parse, query_string, cursor_options, bound_params),
    };
    let have_stash = ADMIT_STASH.with(|s| !s.borrow().is_empty());
    if ENABLE_COLUMNAR_AGG.get() && !stmt.is_null() && have_stash {
        swap_walk(&mut (*stmt).planTree, (*stmt).rtable);
        let subplans = (*stmt).subplans;
        if !subplans.is_null() {
            let n = (*subplans).length;
            for i in 0..n {
                let cell = (*subplans).elements.add(i as usize);
                swap_walk(
                    &mut (*cell).ptr_value as *mut _ as *mut *mut pg_sys::Plan,
                    (*stmt).rtable,
                );
            }
        }
    }
    stmt // `_guard` restores the enclosing run's stash on drop (incl. unwind)
}

/// Find the base-relation scanrelid of the Agg's DIRECT input scan: a `SeqScan`, optionally under a `Sort` (the
/// GroupAgg input sort). STOPS at anything else (`Agg`, `SubqueryScan`, join, …) → `None`, so a nested aggregation is
/// NOT mistaken for the current Agg's columnar scan (else an outer `sum(s) FROM (grouped)` would match the inner
/// table and be swapped wrongly).
unsafe fn find_scan_relid(plan: *mut pg_sys::Plan) -> Option<u32> {
    if plan.is_null() {
        return None;
    }
    match (*plan).type_ {
        pg_sys::NodeTag::T_SeqScan => {
            let rid = (*(plan as *mut pg_sys::SeqScan)).scan.scanrelid;
            if rid > 0 { Some(rid) } else { None }
        }
        pg_sys::NodeTag::T_Sort => find_scan_relid((*plan).lefttree),
        _ => None,
    }
}

/// Build the plain-typed-`Var(INDEX_VAR, resno)` targetlist matching `tlist` positionally — NO `Aggref` (M115). The
/// exec callback fills the scan slot; the node never evaluates an aggregate, so no `Var` can escape into an upper node.
unsafe fn plain_var_tlist(tlist: *mut pg_sys::List) -> *mut pg_sys::List {
    let src = PgList::<pg_sys::TargetEntry>::from_pg(tlist);
    let mut out: *mut pg_sys::List = std::ptr::null_mut();
    for i in 0..src.len() {
        let te = src.get_ptr(i).expect("tlist entry");
        let e = (*te).expr as *mut pg_sys::Node;
        let var = pg_sys::makeVar(
            pg_sys::INDEX_VAR as i32,
            (i + 1) as pg_sys::AttrNumber,
            pg_sys::exprType(e),
            pg_sys::exprTypmod(e),
            pg_sys::exprCollation(e),
            0,
        );
        let nte = pg_sys::makeTargetEntry(
            var as *mut pg_sys::Expr,
            (i + 1) as pg_sys::AttrNumber,
            (*te).resname,
            (*te).resjunk,
        );
        out = pg_sys::lappend(out, nte as *mut c_void);
    }
    out
}

/// Build the DEPARSE-SAFE `custom_scan_tlist` (M131 — fix #135). For a `scanrelid = 0` CustomScan, ruleutils resolves
/// an upper node's Var down through `custom_scan_tlist` (`resolve_special_varno`), which recurses while the resolved
/// expr is itself a `Var`, and stops on any non-`Var` node. `plain_var_tlist` produced `Var(INDEX_VAR, i)` entries, so
/// resolving pointed right back at the same entry — an INFINITE recursion that hung EXPLAIN whenever a `Sort` above
/// this node had a key referencing the aggregate output (`ORDER BY count(*)`; ClickBench Q16/Q33). Live gdb backtrace
/// in `knowledge-base/discoveries/blueprints/columnar-agg-planner-hang-blueprint.md`.
///
/// Every entry here is therefore a NON-special expression:
/// - a group key → a base-rel `Var` (real `varno` = the scanned rel; its RTE survives dropping the child plan node),
/// - an aggregate → its `Aggref` (a non-`Var` node, so resolution stops at it) with any argument `Var` rebuilt as a
///   base-rel `Var` — the post-`set_plan_refs` argument is `OUTER_VAR` into the subtree we dropped, which would
///   re-introduce the same hazard when ruleutils deparses the aggregate's arguments.
///
/// `plan.targetlist` is deliberately NOT changed (it stays `plain_var_tlist`): the executed output is untouched and
/// the M115 invariant "no `Aggref` in the executed tlist" holds. This node is inserted post-`set_plan_refs`, so
/// `set_customscan_references` never re-processes `custom_scan_tlist`.
///
/// # DESCRIPTOR-EQUALITY INVARIANT (do not break — `custom_scan_tlist` is NOT just deparse metadata)
///
/// EXPLAIN deparse is NOT the only consumer: for `scanrelid = 0`, `ExecInitCustomScan` builds the node's RUNTIME
/// scan `TupleDesc` from this list —
/// `nodeCustom.c: if (cscan->custom_scan_tlist != NIL || scan_rel == NULL) scan_tupdesc = ExecTypeFromTL(cscan->custom_scan_tlist);`
/// Execution therefore stays byte-identical ONLY because this list is descriptor-equal to the `plain_var_tlist` it
/// replaces: same length, same per-entry `exprType`/`exprTypmod`/`exprCollation`, same `resname`, same `resjunk`.
/// That holds by construction — `admit` accepts a group key only as a BARE base-rel `Var` (so
/// `adm.group_cols[idx].1 == exprType(e)` and the typmod/collation taken from `e` are that same Var's), and an
/// aggregate entry is a COPY of the very same `Aggref` (identical type triple). Any future edit that changes an
/// entry's type, typmod, collation, or the list length silently changes the runtime tuple shape — with no test
/// signal beyond a result diff. Keep the invariant, or change `plan.targetlist` in lockstep.
///
/// Fail-closed: any inconsistency between `tlist` and the admission returns NIL, and the caller then declines the
/// swap (native plan) rather than emitting a SHORT tlist — a short descriptor would drop a column at runtime and let
/// `plan.targetlist`'s `Var(INDEX_VAR, k)` read past the end of the scan slot (council-rust-pgrx MEDIUM-1).
unsafe fn deparse_safe_tlist(
    tlist: *mut pg_sys::List,
    adm: &Admitted,
    scanrelid: u32,
) -> *mut pg_sys::List {
    let src = PgList::<pg_sys::TargetEntry>::from_pg(tlist);
    let mut out: *mut pg_sys::List = std::ptr::null_mut();
    for i in 0..src.len() {
        // Fail-closed on ANY inconsistency (see the descriptor-equality invariant above): NIL → caller declines.
        let Some(te) = src.get_ptr(i) else { return std::ptr::null_mut() };
        let e = (*te).expr as *mut pg_sys::Node;
        if e.is_null() {
            return std::ptr::null_mut();
        }
        // What does this output column represent? Grouped plans carry an explicit layout; the scalar path is
        // one aggregate per output column, in order.
        let (is_group, idx) = if adm.layout.is_empty() {
            (false, i)
        } else {
            match adm.layout.get(i) {
                Some(&(tag, k)) => (tag == 0, k),
                None => return std::ptr::null_mut(),
            }
        };
        let expr: *mut pg_sys::Expr = if is_group {
            match adm.group_cols.get(idx) {
                Some(&(attno, typoid)) => pg_sys::makeVar(
                    scanrelid as i32,
                    attno as pg_sys::AttrNumber,
                    pg_sys::Oid::from(typoid),
                    pg_sys::exprTypmod(e),
                    pg_sys::exprCollation(e),
                    0,
                ) as *mut pg_sys::Expr,
                None => return std::ptr::null_mut(),
            }
        } else {
            // Copy the Aggref so the original (shared) plan nodes are never mutated, then rebuild its argument Var
            // against the base rel so deparsing the arguments never follows OUTER_VAR into the dropped subtree.
            let copied = pg_sys::copyObjectImpl(e as *const c_void) as *mut pg_sys::Node;
            if copied.is_null() {
                return std::ptr::null_mut(); // never place a NULL expr — ExecTypeFromTL would deref it
            }
            if (*copied).type_ == pg_sys::NodeTag::T_Aggref {
                let aggref = copied as *mut pg_sys::Aggref;
                let Some(pa) = adm.aggs.get(idx) else { return std::ptr::null_mut() };
                let attno = pa.attno;
                if attno > 0 {
                    let args = PgList::<pg_sys::TargetEntry>::from_pg((*aggref).args);
                    for j in 0..args.len() {
                        let Some(ate) = args.get_ptr(j) else { return std::ptr::null_mut() };
                        let av = (*ate).expr as *mut pg_sys::Node;
                        if !av.is_null() && (*av).type_ == pg_sys::NodeTag::T_Var {
                            let v = av as *mut pg_sys::Var;
                            // makeVar sets the *syn fields consistently — safer than poking them field by field.
                            (*ate).expr = pg_sys::makeVar(
                                scanrelid as i32,
                                attno as pg_sys::AttrNumber,
                                (*v).vartype,
                                (*v).vartypmod,
                                (*v).varcollid,
                                0,
                            ) as *mut pg_sys::Expr;
                        }
                    }
                }
            }
            copied as *mut pg_sys::Expr
        };
        let nte = pg_sys::makeTargetEntry(
            expr,
            (i + 1) as pg_sys::AttrNumber,
            (*te).resname,
            (*te).resjunk,
        );
        out = pg_sys::lappend(out, nte as *mut c_void);
    }
    out
}

/// Encode a stashed admission as the CustomScan's `custom_private` IntList (M115 layout, table OID first):
/// `[table_oid, mode, nagg, (kind,attno)×nagg, npred, (col,op,hi,lo)×npred, ngroup, (attno,typoid)×ngroup,
///  noutput, (kind,idx)×noutput]`.
unsafe fn encode_private(adm: &Admitted, table_oid: u32) -> *mut pg_sys::List {
    let mut pl = pg_sys::lappend_int(std::ptr::null_mut(), table_oid as i32);
    pl = pg_sys::lappend_int(pl, adm.mode);
    pl = pg_sys::lappend_int(pl, adm.aggs.len() as i32);
    for a in &adm.aggs {
        pl = pg_sys::lappend_int(pl, a.kind);
        pl = pg_sys::lappend_int(pl, a.attno);
    }
    pl = pg_sys::lappend_int(pl, adm.preds.len() as i32);
    for p in &adm.preds {
        pl = pg_sys::lappend_int(pl, p.col as i32);
        pl = pg_sys::lappend_int(pl, p.op as i32);
        pl = pg_sys::lappend_int(pl, (p.const_bits >> 32) as i32);
        pl = pg_sys::lappend_int(pl, (p.const_bits & 0xFFFF_FFFF) as i32);
    }
    pl = pg_sys::lappend_int(pl, adm.group_cols.len() as i32);
    for &(attno, typoid) in &adm.group_cols {
        pl = pg_sys::lappend_int(pl, attno);
        pl = pg_sys::lappend_int(pl, typoid as i32);
    }
    pl = pg_sys::lappend_int(pl, adm.layout.len() as i32);
    for &(kind, idx) in &adm.layout {
        pl = pg_sys::lappend_int(pl, kind as i32);
        pl = pg_sys::lappend_int(pl, idx as i32);
    }
    pl
}

/// If `plan` is an `Agg` over a columnar table matching an unconsumed stash entry, build the replacement `CustomScan`
/// (plain-Var tlist, scanrelid=0, custom_private from the stash) with the same output shape; else `None`.
unsafe fn try_swap_agg(
    plan: *mut pg_sys::Plan,
    rtable: *mut pg_sys::List,
) -> Option<*mut pg_sys::Plan> {
    let agg = plan as *mut pg_sys::Agg;
    // B1 (review): only a SIMPLE (non-split) aggregate carries the FINAL result. A parallel plan splits into
    // Finalize(SIMPLE)→Gather→Partial(INITIAL_SERIAL)→ParallelSeqScan; swapping the Partial would emit the FINAL value
    // where a partial transvalue is expected → wrong result. Decline any non-SIMPLE split.
    if (*agg).aggsplit != pg_sys::AggSplit::AGGSPLIT_SIMPLE {
        return None;
    }
    // MIXED (grouping sets) is out of scope. PLAIN (scalar) and HASHED (unordered — any ORDER BY has an explicit Sort
    // ABOVE) swap freely. SORTED (GroupAgg) relies on its INPUT sort for output order; our exec re-imposes an ASC
    // nulls-last sort on the group keys — so a SORTED node is admitted ONLY when its input Sort is exactly ASC
    // nulls-last on numeric/temporal keys (checked below); DESC / nulls-first / text → decline (review B2).
    let strat = (*agg).aggstrategy;
    if strat != pg_sys::AggStrategy::AGG_PLAIN
        && strat != pg_sys::AggStrategy::AGG_HASHED
        && strat != pg_sys::AggStrategy::AGG_SORTED
    {
        return None;
    }
    let scanrelid = find_scan_relid((*agg).plan.lefttree)?;
    let scan_rte = pg_sys::list_nth(rtable, (scanrelid - 1) as i32) as *mut pg_sys::RangeTblEntry;
    if scan_rte.is_null() {
        return None;
    }
    let table_oid = (*scan_rte).relid.to_u32();
    let out_arity = pg_sys::list_length((*agg).plan.targetlist) as usize;
    let numcols = (*agg).numCols as usize;
    // B3 (review): match the first unconsumed stash entry for this OID WHOSE SHAPE matches the planned Agg — same
    // group-key count and output arity — so a scalar Agg cannot bind a grouped `Admitted` (or vice-versa) on the same
    // table, which would emit the wrong row shape.
    let adm = ADMIT_STASH.with(|s| {
        let mut v = s.borrow_mut();
        v.iter_mut()
            .find(|e| {
                !e.consumed
                    && e.table_oid == table_oid
                    && e.adm.group_cols.len() == numcols
                    && e.adm.expected_arity() == out_arity
            })
            .map(|e| {
                e.consumed = true;
                e.adm.clone()
            })
    })?;
    // B2 (review): a SORTED GroupAgg is only swappable when our ASC-nulls-last group sort reproduces its output order.
    if strat == pg_sys::AggStrategy::AGG_SORTED {
        // Text keys: PG collation order ≠ byte-wise sort → decline.
        if adm.group_cols.iter().any(|&(_, t)| matches!(t, 25 | 1042 | 1043)) {
            return None;
        }
        // The input Sort must be exactly ASC nulls-last (else the plan's output order isn't our ASC order).
        let child = (*agg).plan.lefttree;
        if child.is_null() || (*child).type_ != pg_sys::NodeTag::T_Sort {
            return None;
        }
        let s = child as *mut pg_sys::Sort;
        for i in 0..(*s).numCols as usize {
            if *(*s).nullsFirst.add(i) {
                return None; // nulls-first ≠ our nulls-last
            }
            let opno = *(*s).sortOperators.add(i);
            // M135: PG18 generalized the btree strategy number into an AM-agnostic `CompareType`, so the last
            // out-param changed TYPE. The VALUE did not change — `access/cmptype.h:34` defines `COMPARE_LT = 1`,
            // i.e. exactly `BTLessStrategyNumber` — so this is a type port, not a semantic one. (An earlier
            // version of this comment claimed the old constant "would silently accept the wrong ordering";
            // council-index-storage caught that as false, and a wrong rationale would mislead anyone auditing
            // the project's other strategy-number sites into expecting value drift that does not exist.)
            //
            // Seeded with `COMPARE_INVALID`, not `COMPARE_LT`: `lsyscache.c:275` overwrites it unconditionally
            // today, but seeding the ACCEPT value means a future PG that returns early without writing the
            // out-param would flip this gate from fail-closed to fail-open — wrong ordering accepted, wrong
            // results, no error. Seeding the reject value costs nothing.
            let (mut opfamily, mut opcintype, mut cmptype) =
                (pg_sys::InvalidOid, pg_sys::InvalidOid, pg_sys::CompareType::COMPARE_INVALID);
            pg_sys::get_ordering_op_properties(opno, &mut opfamily, &mut opcintype, &mut cmptype);
            if cmptype != pg_sys::CompareType::COMPARE_LT {
                return None; // DESC (or non-btree) ≠ our ascending
            }
        }
    }

    let tlist = (*agg).plan.targetlist;
    let mut cscan = PgBox::<pg_sys::CustomScan>::alloc_node(pg_sys::NodeTag::T_CustomScan);
    {
        let plan_out = &mut cscan.scan.plan;
        plan_out.targetlist = plain_var_tlist(tlist);
        plan_out.qual = std::ptr::null_mut();
        plan_out.lefttree = std::ptr::null_mut(); // drop the Agg's child subtree — the CustomScan scans itself
        plan_out.righttree = std::ptr::null_mut();
        plan_out.plan_node_id = (*agg).plan.plan_node_id;
        plan_out.startup_cost = (*agg).plan.startup_cost;
        plan_out.total_cost = (*agg).plan.total_cost;
        plan_out.plan_rows = (*agg).plan.plan_rows;
        plan_out.plan_width = (*agg).plan.plan_width;
        plan_out.parallel_aware = false;
        plan_out.parallel_safe = (*agg).plan.parallel_safe;
    }
    cscan.scan.scanrelid = 0;
    cscan.flags = 0;
    cscan.custom_plans = std::ptr::null_mut();
    cscan.custom_exprs = std::ptr::null_mut();
    cscan.custom_private = encode_private(&adm, table_oid);
    // M131 (#135): NOT `plain_var_tlist` — a self-referential INDEX_VAR here makes ruleutils' `resolve_special_varno`
    // recurse forever when a Sort above this node has a key on the aggregate output, hanging EXPLAIN. This list also
    // becomes the node's RUNTIME scan TupleDesc (`ExecTypeFromTL`), so it must stay descriptor-equal to
    // `plan.targetlist` — see `deparse_safe_tlist`. NIL means it could not be built consistently → decline the swap
    // and let the native plan run (fail-closed; never ship a short descriptor).
    let safe_tlist = deparse_safe_tlist(tlist, &adm, scanrelid);
    if safe_tlist.is_null() || pg_sys::list_length(safe_tlist) as usize != out_arity {
        return None;
    }
    cscan.custom_scan_tlist = safe_tlist;
    cscan.custom_relids = std::ptr::null_mut();
    cscan.methods = &SCAN_METHODS.0;
    Some(cscan.into_pg() as *mut pg_sys::Plan)
}

/// Walk the plan tree via a mutable node slot, swapping matching `Agg` nodes → our `CustomScan` in place.
unsafe fn swap_walk(slot: *mut *mut pg_sys::Plan, rtable: *mut pg_sys::List) {
    let plan = *slot;
    if plan.is_null() {
        return;
    }
    if (*plan).type_ == pg_sys::NodeTag::T_Agg {
        if let Some(newnode) = try_swap_agg(plan, rtable) {
            *slot = newnode;
            return; // replaced — the Agg's child subtree is dropped
        }
    }
    swap_walk(&mut (*plan).lefttree, rtable);
    swap_walk(&mut (*plan).righttree, rtable);
    match (*plan).type_ {
        pg_sys::NodeTag::T_Append => {
            swap_walk_list((*(plan as *mut pg_sys::Append)).appendplans, rtable)
        }
        pg_sys::NodeTag::T_MergeAppend => {
            swap_walk_list((*(plan as *mut pg_sys::MergeAppend)).mergeplans, rtable)
        }
        pg_sys::NodeTag::T_SubqueryScan => {
            swap_walk(&mut (*(plan as *mut pg_sys::SubqueryScan)).subplan, rtable)
        }
        _ => {}
    }
}

/// Walk a List of child plans with mutable slots (Append/MergeAppend members).
unsafe fn swap_walk_list(list: *mut pg_sys::List, rtable: *mut pg_sys::List) {
    if list.is_null() {
        return;
    }
    let n = (*list).length;
    for i in 0..n {
        let cell = (*list).elements.add(i as usize);
        swap_walk(&mut (*cell).ptr_value as *mut _ as *mut *mut pg_sys::Plan, rtable);
    }
}

#[pg_guard]
unsafe extern "C-unwind" fn create_custom_scan_state(
    _cscan: *mut pg_sys::CustomScan,
) -> *mut pg_sys::Node {
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
    // M115 layout: [table_oid, mode, nagg, ...]. The base table is resolved by its stable pg_class OID (the Agg-swap
    // dropped the child scan, so there is no scanrelid to index es_range_table).
    let relid = pg_sys::Oid::from_u32_unchecked(pg_sys::list_nth_int(priv_list, 0) as u32);
    let mode = pg_sys::list_nth_int(priv_list, 1);

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
            // min/max output type = the input column type; recover its OID from the relation attribute (kinds 6/7).
            let col_typoid = |ano: i32| -> u32 {
                pg_sys::get_atttype(relid, ano as pg_sys::AttrNumber).to_u32()
            };
            match kind {
                0 => specs.push(AggSpec::CountStar),
                1 => specs.push(AggSpec::SumFloat8(col_name(attno))),
                2 => specs.push(AggSpec::SumInt(col_name(attno))),
                3 => specs.push(AggSpec::AvgFloat8(col_name(attno))),
                4 => specs.push(AggSpec::SumInt8Numeric(col_name(attno))),
                5 => specs.push(AggSpec::AvgIntNumeric(col_name(attno))),
                6 => specs.push(AggSpec::MinCol(col_name(attno), col_typoid(attno))),
                7 => specs.push(AggSpec::MaxCol(col_name(attno), col_typoid(attno))),
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
                5 => ZoneOp::Ne, // M151 — `<>` round-trips through custom_private (encode writes `p.op as i32` = 5)
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
            let r = run_columnar_grouped_aggs(
                rel,
                &group_cols,
                &specs,
                &layout,
                &preds,
                super::guc::columnar_zonemap_skip(),
            );
            pg_sys::relation_close(rel, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            r
        } else if mode == 1 {
            // M101 HTAP: aggregate the heap-authoritative Arrow cache. Single row → wrap.
            super::arrow_cache::run_cache_aggs(relid, &specs).map(|row| vec![row])
        } else {
            // M100 scalar: decode the columnar stripes with zone-map skip-pruning. Single row → wrap.
            let rel = pg_sys::relation_open(relid, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            // Phase B (columnar-minmax): a scalar all-min/max output with NO predicate can be answered from the
            // zone-map directory (+ pending) WITHOUT decoding any column chunk. Try every agg; if all fold, emit that
            // row; if any gates out (unordered, max-float, has_minmax=false group, all-NaN pending), fall back to the
            // full Phase-A scan below. Byte-identical either way (blueprint ADR-MM1).
            let all_minmax = !specs.is_empty()
                && specs.iter().all(|s| matches!(s, AggSpec::MinCol(..) | AggSpec::MaxCol(..)));
            let fast: Option<Result<Vec<Vec<(pg_sys::Datum, bool)>>, String>> =
                if preds.is_empty() && all_minmax {
                    let mut row = Vec::with_capacity(specs.len());
                    let mut ok = true;
                    let mut err = None;
                    for s in &specs {
                        let (name, typoid, want_max) = match s {
                            AggSpec::MinCol(n, t) => (n.as_str(), *t, false),
                            AggSpec::MaxCol(n, t) => (n.as_str(), *t, true),
                            _ => unreachable!(),
                        };
                        match super::columnar::directory_minmax(rel, name, typoid, want_max) {
                            Ok(Some(cell)) => row.push(cell),
                            Ok(None) => {
                                ok = false;
                                break;
                            }
                            Err(e) => {
                                err = Some(e);
                                break;
                            }
                        }
                    }
                    match (ok, err) {
                        (_, Some(e)) => Some(Err(e)),
                        (true, None) => Some(Ok(vec![row])),
                        (false, None) => None, // gated out → Phase A
                    }
                } else {
                    None
                };
            // Honest path signal for the A/B (opt-in): which path answered this scalar min/max.
            if all_minmax && std::env::var("THEODB_SCAN_PROFILE").is_ok_and(|v| v == "1") {
                let taken = matches!(fast, Some(Ok(_)));
                pgrx::notice!(
                    "theodb_columnar minmax path={}",
                    if taken { "fastpath" } else { "scan" }
                );
            }
            let r = match fast {
                Some(res) => res,
                None => run_columnar_aggs(rel, &specs, &preds, super::guc::columnar_zonemap_skip())
                    .map(|row| vec![row]),
            };
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
unsafe extern "C-unwind" fn exec_custom_scan(
    node: *mut pg_sys::CustomScanState,
) -> *mut pg_sys::TupleTableSlot {
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
        let top = Spi::get_one::<String>(
            "EXPLAIN (COSTS OFF) SELECT count(*), sum(measure) FROM m100_ca",
        )
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
        let plan =
            Spi::get_one::<String>("EXPLAIN (COSTS OFF) SELECT k, sum(x) FROM gb_c GROUP BY k")
                .unwrap()
                .unwrap();
        assert!(
            plan.contains("Custom Scan") || plan.contains("theodb_columnar_agg"),
            "GROUP BY must be a CustomScan: {plan}"
        );

        // Fetch a grouped result set at TOP LEVEL (only bare Var/Aggref in the target so the CustomScan is admitted)
        // and compare the row lists. `q` is `(int_key, sum, count)` sorted by key.
        // Bare Var/Aggref target only (so the CustomScan is admitted); round sum(x) in Rust for a stable compare.
        let fetch = |t: &str| -> Vec<(i32, i64, i64)> {
            Spi::connect(|c| {
                let rows = c
                    .select(
                        &format!("SELECT k, sum(x), count(*) FROM {t} GROUP BY k ORDER BY k"),
                        None,
                        &[],
                    )
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
        assert_eq!(
            fetch("gb_c"),
            fetch("gb_h"),
            "int GROUP BY (key, sum, count) result set must match the heap"
        );

        // ADR-2: agg-BEFORE-key column order maps correctly (sum first, key second).
        let fetch_ab = |t: &str| -> Vec<(i64, i32)> {
            Spi::connect(|c| {
                let rows = c
                    .select(&format!("SELECT sum(x), k FROM {t} GROUP BY k ORDER BY k"), None, &[])
                    .unwrap();
                rows.map(|r| {
                    (
                        r.get::<f64>(1).unwrap().unwrap().round() as i64,
                        r.get::<i32>(2).unwrap().unwrap(),
                    )
                })
                .collect::<Vec<_>>()
            })
        };
        assert_eq!(
            fetch_ab("gb_c"),
            fetch_ab("gb_h"),
            "agg-before-key (ADR-2) result set must match the heap"
        );

        // ADR-3: a text key (palloc'd varlena) grouped over multiple emitted rows, incl. the NULL group.
        let fetch_txt = |t: &str| -> Vec<(Option<String>, i64)> {
            Spi::connect(|c| {
                let rows = c
                    .select(
                        &format!(
                            "SELECT lbl, count(*) FROM {t} GROUP BY lbl ORDER BY lbl NULLS FIRST"
                        ),
                        None,
                        &[],
                    )
                    .unwrap();
                rows.map(|r| (r.get::<String>(1).unwrap(), r.get::<i64>(2).unwrap().unwrap()))
                    .collect::<Vec<_>>()
            })
        };
        assert_eq!(
            fetch_txt("gb_c"),
            fetch_txt("gb_h"),
            "text GROUP BY (incl NULL group) must match the heap"
        );

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
            Spi::get_one::<String>(&format!("EXPLAIN (COSTS OFF) {sql}"))
                .unwrap()
                .unwrap()
                .contains("theodb_columnar_agg")
        };

        // ADMITTED (M114):
        assert!(
            is_cs("SELECT k, sum(x) FROM m114c WHERE k>=0 GROUP BY k"),
            "GROUP BY + pushable WHERE must be a CustomScan"
        );
        assert!(is_cs("SELECT avg(x) FROM m114c"), "avg(float8) must be a CustomScan");
        assert!(is_cs("SELECT sum(i4) FROM m114c"), "sum(int4) must be a CustomScan");
        assert!(is_cs("SELECT sum(i2) FROM m114c"), "sum(int2) must be a CustomScan");
        // ADMITTED (numeric-output slice — byte-identical via AnyNumeric = PG numeric_div):
        assert!(
            is_cs("SELECT avg(i4) FROM m114c"),
            "avg(int4)→numeric must be a CustomScan (numeric-output slice)"
        );
        assert!(
            is_cs("SELECT sum(b) FROM m114c"),
            "sum(int8)→numeric must be a CustomScan (numeric-output slice)"
        );
        // DECLINED (real output / numeric-column input still out of scope):
        assert!(!is_cs("SELECT sum(f4) FROM m114c"), "sum(real) must decline");
        assert!(
            !is_cs("SELECT date_trunc('day',ts), sum(x) FROM m114c GROUP BY date_trunc('day',ts)"),
            "grouping expr must decline"
        );
        // B1 regression (M114 heap M101-cache path): sum(int4) over a heap table with NO cache covering the column
        // must DECLINE — before the fix the empty kind==1 name-set made has_cached_columns([]) admit mode 1 wrongly.
        assert!(
            !is_cs("SELECT sum(i4) FROM m114h"),
            "heap sum(int4) with no cache must decline (B1 regression)"
        );

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

    /// Numeric-output slice: sum(int8)→numeric and avg(int2/4/8)→numeric are byte-identical to the heap,
    /// including PG's DATA-DEPENDENT avg scale (16 sig-digits for small sums, shrinking as the sum grows) and
    /// i128 exactness for a sum exceeding i64. Compared as TEXT so any scale/rounding drift fails the assert.
    #[pg_test]
    fn test_numeric_output_aggregates_byte_identical() {
        Spi::run("SET theodb.enable_columnar_agg = on").unwrap();
        Spi::run(
            "CREATE TABLE numc (g int, s2 int2, s4 int4, s8 int8, big int8) USING theodb_columnar",
        )
        .unwrap();
        Spi::run("CREATE TABLE numh (g int, s2 int2, s4 int4, s8 int8, big int8)").unwrap();
        // g%4 groups; small values (avg scale 16), ~1e9 in s8 (avg scale shrinks), and `big`=2e15 whose sum over 5000
        // rows (1e19) EXCEEDS i64 max (9.2e18) — a wrapping Int64 sum would go negative, so an identical numeric result
        // proves the exact Decimal128/i128 path is load-bearing.
        let gen_sql = "SELECT g%4, (g%100)::int2, g, (g::int8*1000000), 2000000000000000::int8 \
                   FROM generate_series(1,5000) g";
        Spi::run(&format!("INSERT INTO numc {gen_sql}")).unwrap();
        Spi::run(&format!("INSERT INTO numh {gen_sql}")).unwrap();

        let is_cs = |sql: &str| -> bool {
            Spi::get_one::<String>(&format!("EXPLAIN (COSTS OFF) {sql}"))
                .unwrap()
                .unwrap()
                .contains("theodb_columnar_agg")
        };
        // Compare scalar numeric aggregate output rendered as TEXT (captures scale exactly).
        let eq_text = |agg: &str| {
            assert!(
                is_cs(&format!("SELECT {agg} FROM numc")),
                "{agg} over columnar must be a CustomScan"
            );
            let c = Spi::get_one::<String>(&format!("SELECT ({agg})::text FROM numc"))
                .unwrap()
                .unwrap();
            let h = Spi::get_one::<String>(&format!("SELECT ({agg})::text FROM numh"))
                .unwrap()
                .unwrap();
            assert_eq!(c, h, "{agg} must be byte-identical (as text) to the heap");
        };

        // sum(int8): exact scale-0 numeric, and `big`'s sum (5000 × 4e9 = 2e13) fits i64 but sum(s8) over 5000 rows
        // of up to 5e9 exceeds i64 — proves the Decimal128 i128 path (not the wrapping Int64 path).
        eq_text("sum(s8)");
        eq_text("sum(big)");
        // avg(int2/4/8): PG numeric_div with data-dependent scale — small (scale 16) and large (shrinking) sums.
        eq_text("avg(s2)");
        eq_text("avg(s4)");
        eq_text("avg(s8)");

        // Empty group → NULL (zero-count guard), matching PG's finalfn. Spi::get_one already returns Ok(None) on NULL.
        let cnull = Spi::get_one::<String>("SELECT (avg(s4))::text FROM numc WHERE g < 0").unwrap();
        assert_eq!(cnull, None, "avg over an empty set must be SQL NULL");

        Spi::run("DROP TABLE numc").unwrap();
        Spi::run("DROP TABLE numh").unwrap();
    }

    /// columnar-minmax Phase A: min(col)/max(col) on ordered native types byte-identical via the DataFusion scan path
    /// (output type = input column type), incl. max(float)-with-NaN (returns NaN because it decodes actual values).
    /// The bare-Var gate declines min(col+1); unordered types decline.
    #[pg_test]
    fn test_columnar_minmax_phase_a_byte_identical() {
        Spi::run("SET theodb.enable_columnar_agg = on").unwrap();
        Spi::run("CREATE TABLE mmc (i4 int4, i2 int2, i8 int8, f8 float8, b bool, ts timestamptz) USING theodb_columnar").unwrap();
        Spi::run("CREATE TABLE mmh (i4 int4, i2 int2, i8 int8, f8 float8, b bool, ts timestamptz)")
            .unwrap();
        let g = "SELECT g-5000, (g%200-100)::int2, g::int8*1000, (g*1.5-7500)::float8, (g%2=0), timestamptz '2020-01-01'+(g*interval '1 min') FROM generate_series(1,10000) g";
        Spi::run(&format!("INSERT INTO mmc {g}")).unwrap();
        Spi::run(&format!("INSERT INTO mmh {g}")).unwrap();
        let is_cs = |sql: &str| {
            Spi::get_one::<String>(&format!("EXPLAIN (COSTS OFF) {sql}"))
                .unwrap()
                .unwrap()
                .contains("theodb_columnar_agg")
        };
        // WHERE forces Phase A (npred>0) — still byte-identical; assert every ordered type min+max.
        // NOTE: PG has no min/max aggregate for bool (only bool_and/bool_or), so `b` is not aggregated here.
        for (c, w) in [
            ("i4", "WHERE i4 > -999999"),
            ("i2", "WHERE i4 > -999999"),
            ("i8", "WHERE i4 > -999999"),
            ("f8", "WHERE i4 > -999999"),
            ("ts", "WHERE i4 > -999999"),
        ] {
            for agg in ["min", "max"] {
                let sql = format!("SELECT {agg}({c}) FROM mmc {w}");
                assert!(is_cs(&sql), "{agg}({c}) with WHERE must be a CustomScan");
                let vc = Spi::get_one::<String>(&format!("SELECT ({agg}({c}))::text FROM mmc {w}"))
                    .unwrap();
                let vh = Spi::get_one::<String>(&format!("SELECT ({agg}({c}))::text FROM mmh {w}"))
                    .unwrap();
                assert_eq!(vc, vh, "{agg}({c}) must be byte-identical to the heap");
            }
        }
        // Bare-Var gate: min(col+1) must decline the CustomScan.
        assert!(!is_cs("SELECT min(i4+1) FROM mmc"), "min(col+1) must decline (not a bare Var)");
        // max(float) with a NaN row → NaN (Phase A decodes actual values).
        Spi::run("INSERT INTO mmc (i4, f8) VALUES (1, 'NaN')").unwrap();
        Spi::run("INSERT INTO mmh (i4, f8) VALUES (1, 'NaN')").unwrap();
        let mc = Spi::get_one::<String>("SELECT (max(f8))::text FROM mmc").unwrap();
        let mh = Spi::get_one::<String>("SELECT (max(f8))::text FROM mmh").unwrap();
        assert_eq!(mc, mh, "max(float) with NaN must equal the heap (NaN)");
        Spi::run("DROP TABLE mmc").unwrap();
        Spi::run("DROP TABLE mmh").unwrap();
    }

    /// columnar-minmax Phase B: the zone-map directory fast-path answers scalar min/max (no WHERE) byte-identically,
    /// folds same-xact pending rows, and correctly falls back for max(float-with-NaN) and all-NULL.
    #[pg_test]
    fn test_columnar_minmax_fast_path() {
        Spi::run("SET theodb.enable_columnar_agg = on").unwrap();
        Spi::run("CREATE TABLE fpc (v int4) USING theodb_columnar").unwrap();
        Spi::run("CREATE TABLE fph (v int4)").unwrap();
        let g = "SELECT g-5000 FROM generate_series(1,20000) g";
        Spi::run(&format!("INSERT INTO fpc {g}")).unwrap();
        Spi::run(&format!("INSERT INTO fph {g}")).unwrap();
        // (a) clean scalar int min/max byte-identical (fast-path path; correctness is the assertion here).
        for agg in ["min", "max"] {
            let vc = Spi::get_one::<String>(&format!("SELECT ({agg}(v))::text FROM fpc")).unwrap();
            let vh = Spi::get_one::<String>(&format!("SELECT ({agg}(v))::text FROM fph")).unwrap();
            assert_eq!(vc, vh, "scalar {agg}(int4) must be byte-identical (fast-path)");
        }
        // (b) same-xact pending fold: this INSERT is uncommitted (pg_test runs in one xact) → max must see it.
        Spi::run("INSERT INTO fpc VALUES (999999)").unwrap();
        let pc = Spi::get_one::<i32>("SELECT max(v) FROM fpc").unwrap();
        assert_eq!(pc, Some(999999), "max must fold the same-xact pending row");
        // (c) all-NULL column → NULL.
        Spi::run("CREATE TABLE fpn (v int4) USING theodb_columnar").unwrap();
        Spi::run("INSERT INTO fpn SELECT NULL FROM generate_series(1,1000)").unwrap();
        assert_eq!(
            Spi::get_one::<i32>("SELECT max(v) FROM fpn").unwrap(),
            None,
            "all-NULL max → NULL"
        );
        // (d) max(float) with NaN falls back and returns NaN.
        Spi::run("CREATE TABLE fpf (v float8) USING theodb_columnar").unwrap();
        Spi::run("INSERT INTO fpf SELECT CASE WHEN g=1 THEN 'NaN'::float8 ELSE g::float8 END FROM generate_series(1,1000) g").unwrap();
        let f = Spi::get_one::<String>("SELECT (max(v))::text FROM fpf").unwrap();
        assert_eq!(f.as_deref(), Some("NaN"), "max(float) with NaN must return NaN (fallback)");
        Spi::run("DROP TABLE fpc").unwrap();
        Spi::run("DROP TABLE fph").unwrap();
        Spi::run("DROP TABLE fpn").unwrap();
        Spi::run("DROP TABLE fpf").unwrap();
    }

    /// M115 composability: the columnar-aggregate output is consumable inside an enclosing expression (subquery over a
    /// grouped agg, scalar `s+1`, JOIN on the grouped output) — byte-identical to the heap — instead of raising
    /// `cache lookup failed for attribute N of relation 0`. Also asserts the top-level GROUP BY is unchanged.
    #[pg_test]
    fn test_m115_columnar_aggregate_output_is_composable() {
        Spi::run("SET theodb.enable_columnar_agg = on").unwrap();
        Spi::run("CREATE TABLE m115c (k int, x float8) USING theodb_columnar").unwrap();
        Spi::run("CREATE TABLE m115h (k int, x float8)").unwrap();
        let gen_sql = "SELECT g%10, g::float8 FROM generate_series(1,5000) g";
        Spi::run(&format!("INSERT INTO m115c {gen_sql}")).unwrap();
        Spi::run(&format!("INSERT INTO m115h {gen_sql}")).unwrap();

        // Subquery over a grouped columnar aggregate (the shape that used to error). Byte-identical to the heap.
        let sub = |t: &str| {
            Spi::get_one::<f64>(&format!(
                "SELECT sum(s) FROM (SELECT k, sum(x) s FROM {t} GROUP BY k) q"
            ))
            .unwrap()
            .unwrap()
        };
        assert_eq!(sub("m115c"), sub("m115h"), "subquery over grouped agg must be byte-identical");

        // Scalar aggregate consumed in an outer expression.
        let scal = |t: &str| {
            Spi::get_one::<f64>(&format!("SELECT s+1 FROM (SELECT sum(x) s FROM {t}) q"))
                .unwrap()
                .unwrap()
        };
        assert_eq!(scal("m115c"), scal("m115h"), "scalar s+1 over subquery must be byte-identical");

        // JOIN on the grouped output — 10 matching groups.
        let jc = Spi::get_one::<i64>("SELECT count(*) FROM (SELECT k, sum(x) s FROM m115c GROUP BY k) a JOIN (SELECT k, sum(x) s FROM m115h GROUP BY k) b USING(k)").unwrap().unwrap();
        assert_eq!(jc, 10, "join on grouped output must match all 10 groups");

        // Top-level GROUP BY still a CustomScan (no regression).
        let plan =
            Spi::get_one::<String>("EXPLAIN (COSTS OFF) SELECT k, sum(x) FROM m115c GROUP BY k")
                .unwrap()
                .unwrap();
        assert!(
            plan.contains("Custom Scan") || plan.contains("theodb_columnar_agg"),
            "top-level GROUP BY must stay a CustomScan: {plan}"
        );

        Spi::run("DROP TABLE m115c").unwrap();
        Spi::run("DROP TABLE m115h").unwrap();
    }

    /// M131 regression for #135. Before the fix, `custom_scan_tlist` held self-referential `Var(INDEX_VAR, i)`
    /// entries, so ruleutils' `resolve_special_varno` recursed forever whenever a `Sort` ABOVE this CustomScan had a
    /// key on the aggregate output — EXPLAIN never returned (uninterruptible; ClickBench Q16/Q33). The trigger is
    /// `ORDER BY <aggregate>` + EXPLAIN — NOT table width or column types (ADR M131-2), so this test asserts exactly
    /// that shape. `FORMAT JSON` renders `Sort Key`, i.e. it exercises the same deparse path that hung.
    #[pg_test]
    fn test_m131_explain_orderby_aggregate_deparses() {
        Spi::run("SET theodb.enable_columnar_agg = on").unwrap();
        Spi::run("CREATE TABLE m131c (k int4, v int4) USING theodb_columnar").unwrap();
        Spi::run("INSERT INTO m131c SELECT g % 50, g FROM generate_series(1,5000) g").unwrap();

        // (a) single group key + ORDER BY the aggregate + LIMIT (the ClickBench Q16 shape).
        let j = Spi::get_one::<pgrx::Json>(
            "EXPLAIN (FORMAT JSON) SELECT k, count(*) FROM m131c GROUP BY k ORDER BY count(*) DESC LIMIT 10",
        )
        .unwrap()
        .expect("EXPLAIN must return a plan — it hung forever before the #135 fix");
        let s = serde_json::to_string(&j.0).unwrap();
        assert!(
            s.contains("theodb_columnar_agg"),
            "the columnar-agg CustomScan must engage AND deparse under ORDER BY <aggregate>: {s}"
        );

        // (b) multi group key + ORDER BY an aliased aggregate (the ClickBench Q33 shape).
        let j2 = Spi::get_one::<pgrx::Json>(
            "EXPLAIN (FORMAT JSON) SELECT k, v, count(*) AS c FROM m131c GROUP BY k, v ORDER BY c DESC",
        )
        .unwrap()
        .expect("multi-key EXPLAIN with ORDER BY aggregate must return a plan");
        assert!(
            !serde_json::to_string(&j2.0).unwrap().is_empty(),
            "multi-key ORDER BY aggregate must deparse"
        );

        // (c) the results themselves stay correct with the pushdown engaged (deparse fix must not change execution).
        Spi::run("CREATE TABLE m131h (k int4, v int4)").unwrap();
        Spi::run("INSERT INTO m131h SELECT g % 50, g FROM generate_series(1,5000) g").unwrap();
        let top = |t: &str| {
            Spi::get_one::<i64>(&format!(
                "SELECT count(*) FROM (SELECT k, count(*) c FROM {t} GROUP BY k ORDER BY c DESC LIMIT 10) q"
            ))
            .unwrap()
            .unwrap()
        };
        assert_eq!(top("m131c"), top("m131h"), "ORDER BY aggregate result must match heap");

        Spi::run("DROP TABLE m131c").unwrap();
        Spi::run("DROP TABLE m131h").unwrap();
    }
}
