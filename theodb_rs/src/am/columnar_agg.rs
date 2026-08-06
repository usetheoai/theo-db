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
use super::zonemap::{TextOp, TextPredicate, ZoneOp, ZonePredicate};
use pgrx::datum::FromDatum;
use pgrx::{GucContext, GucFlags, GucRegistry, GucSetting};
use pgrx::{PgBox, PgList, pg_guard, pg_sys};
use std::ffi::{CStr, c_int, c_void};
use std::sync::atomic::{AtomicU8, Ordering};

/// `theodb.enable_columnar_agg` — default OFF (the vectorized aggregate path is opt-in until benchmarked).
pub(crate) static ENABLE_COLUMNAR_AGG: GucSetting<bool> = GucSetting::<bool>::new(false);

/// `theodb.enable_columnar_fast_decode` (M160) — default ON. When on, the pushdown decode path (`decode_columns_v2`)
/// decodes non-null fixed-width columns as one contiguous typed buffer (zero-copy into Arrow); when off, every column
/// falls back to the legacy per-cell path — the toggle exists so a same-binary/same-box A/B can MEASURE the M160 win.
pub(crate) static ENABLE_FAST_DECODE: GucSetting<bool> = GucSetting::<bool>::new(true);

/// `theodb.enable_columnar_late_mat` (M158; default flipped ON in M167) — default ON. When on, `Limit(k) → Sort([key]) → columnar-project` is
/// swapped for a late-materialization top-k CustomScan (decode {key∪filter} for all rows, DataFusion filter+sort+limit,
/// materialize the full projection only for the k survivors — avoiding `form_row`/`palloc` for N−k rows, the M148 cost).
/// M167 — the default is ON: measured on ClickBench 1M, q23/q24 route byte-identically and a columnar table has no
/// btree on the sort column, so native's only plan is Sort-over-projected-rows (late-mat is structurally >= native
/// here). The O(N) decode this path pays is bounded by the plan-time guard in `try_swap_topk` (M167 ADR-4), which is
/// what replaced "default OFF" as the mitigation.
pub(crate) static ENABLE_COLUMNAR_LATE_MAT: GucSetting<bool> = GucSetting::<bool>::new(true);

/// M168 — stream the top-k's input one chunk-group at a time instead of decoding the whole relation into one
/// Arrow batch. Default ON: measured peak for ClickBench q23 drops 772.2 MiB → 17.9 MiB (43.2x).
///
/// The switch exists for two reasons, both load-bearing. It makes the throughput comparison PAIRED inside one
/// session and one binary — the alternative is a cross-run comparison, and this box drifts up to 1.88x between
/// runs (M167 § 6), which would swamp the signal. And it is the escape hatch if streaming ever misbehaves in a
/// shape the oracles do not cover.
pub(crate) static ENABLE_COLUMNAR_TOPK_STREAM: GucSetting<bool> = GucSetting::<bool>::new(true);

/// M169 — the same streaming source, applied to the AGGREGATE paths (scalar and grouped).
///
/// Why the aggregate needed its own switch instead of reusing the top-k's: the two answer different questions.
/// The top-k's switch guards a path whose retention grows with `k`; the aggregate's guards a path whose peak was
/// the DECODE itself. Sharing one GUC would make the top-k escape hatch also disable the aggregate fix (and vice
/// versa), and the M169 measurement needs the aggregate arms paired inside one session and one binary.
///
/// Default ON, matching the top-k's — and that default is what makes `stream=false` at T4.1 a real arm rather
/// than a no-op: omitting the SET leaves the fix ENABLED, so the "before" arm has to say `off` out loud.
///
/// MEDIDO (baseline 100M, 2026-07-31): das 15 falhas, 4 roteiam pelo caminho agregado — q20 (escalar,
/// `COUNT(*) … WHERE URL LIKE`), q33 e q34 (agrupadas por URL) com `byte array offset overflow`, e q32
/// (`GROUP BY WatchID, ClientIP`) com timeout. As três primeiras morrem no decode; a q32 morre no ESTADO da
/// tabela de hash, que o streaming não reduz — dizer que este switch a conserta seria vender o que não acontece.
pub(crate) static ENABLE_COLUMNAR_AGG_STREAM: GucSetting<bool> = GucSetting::<bool>::new(true);

/// M167 ADR-4 — safety factor for the top-k decode bound. `run_columnar_topk` decodes {projection ∪ keys ∪ filter}
/// for ALL rows into one Arrow batch BEFORE the bounded-heap TopK runs, so the path costs O(N) memory where the
/// native top-N heapsort costs O(k). With the GUC defaulting ON (M167), an unfiltered wide `SELECT * … ORDER BY k
/// LIMIT 10` would otherwise decode the whole relation and OOM the backend. HEURISTIC, not a measured optimum: it
/// says "a relation an order of magnitude beyond what this session already budgeted for a sort is not ours to
/// decode whole". Applied to `pg_class.relpages` (see `relation_physical_bytes`), NOT to `plan_rows`, which the
/// columnar TableAM leaves at the planner's rows=1 default.
const TOPK_DECODE_WORK_MEM_FACTOR: f64 = 8.0;

/// M167 ADR-3 — ceiling on ORDER BY keys the top-k node will carry. The int wire format grows 3 slots per key, and
/// a sort with more keys than this is not a shape this path was measured on; declining is free and correct.
const TOPK_MAX_SORT_KEYS: usize = 8;

/// M167 ADR-4 — the physical size of `rel_oid` in bytes, from `pg_class.relpages`.
///
/// The obvious signal — the planner's `plan_rows × plan_width` — is INERT on a columnar table: the TableAM reports
/// no tuple count, so `reltuples` stays 0 and the planner estimates `rows=1` even for a 1M-row relation (measured:
/// `EXPLAIN` of `SELECT * FROM hits ORDER BY EventTime LIMIT 10` shows `rows=1 width=1604`). A bound built on that
/// never fires. `relpages` IS maintained (measured: 27863 pages for the same relation) and is the honest proxy for
/// how much this path has to decode.
///
/// Imprecision, stated rather than hidden — and note the direction is the DANGEROUS one: the on-disk bytes are
/// COMPRESSED, so the decoded Arrow batch is LARGER than this estimate. For an OOM bound, under-estimating causes
/// false ADMITS (the failure this guard exists to prevent), not merely false declines. Consequence to be explicit
/// about: with PostgreSQL's stock `work_mem` of 4 MB the budget is 32 MB, so a 1M-row ClickBench `hits`
/// (measured: 27863 pages = 228 MB) DECLINES — the routing win of this milestone needs a larger `work_mem`
/// (measured at 64 MB → 512 MB budget). The guard bills the whole relation, ignoring projection width and filter
/// selectivity, so it is a ceiling on catastrophe rather than a tight bound.
fn relation_physical_bytes(rel_oid: u32) -> f64 {
    unsafe {
        let rel_oid_t = pg_sys::Oid::from(rel_oid);
        // The syscache key is an OID datum — build it through `Oid` rather than from a bare u32.
        let tup = pg_sys::SearchSysCache1(
            pg_sys::SysCacheIdentifier::RELOID as i32,
            pg_sys::Datum::from(rel_oid_t),
        );
        if tup.is_null() {
            // Fail CLOSED: an unknown size must not be read as "small". Returning 0.0 here would admit, which is
            // the same direction as the original fail-open defect this function was written to fix.
            return f64::INFINITY;
        }
        // `SysCacheGetAttr` rather than the GETSTRUCT macro (a C macro, absent from the pgrx bindings) — same
        // pattern as `database_collate_is_byte_order`. `relpages` is a fixed-width int4, so no detoasting.
        let mut isnull = false;
        let d = pg_sys::SysCacheGetAttr(
            pg_sys::SysCacheIdentifier::RELOID as i32,
            tup,
            pg_sys::Anum_pg_class_relpages as pg_sys::AttrNumber,
            &mut isnull,
        );
        // Read the datum BEFORE releasing the tuple. Safe today only because `relpages` is int4/attbyval (the datum
        // is a copied value, not a pointer into the tuple) — reading after the release would become a use-after-free
        // the moment this is repointed at a by-reference attribute. Order it correctly rather than rely on the type.
        let pages = if isnull { 0 } else { d.value() as i32 };
        pg_sys::ReleaseSysCache(tup);
        if pages > 0 {
            return f64::from(pages) * f64::from(pg_sys::BLCKSZ);
        }
        // `relpages` is 0 until ANALYZE/VACUUM runs — and a big columnar relation is typically CREATE + bulk INSERT
        // with no ANALYZE yet, which is exactly when the bound matters most. Falling back to 0.0 here would make the
        // guard INERT in that window (measured: a 200k-row columnar table reports relpages=0 right after load and
        // routes even at work_mem=64kB). Ask the storage manager for the CURRENT physical size instead — it is always
        // right and never stale.
        let rel = pg_sys::relation_open(rel_oid_t, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
        let blocks = pg_sys::RelationGetNumberOfBlocksInFork(rel, pg_sys::ForkNumber::MAIN_FORKNUM);
        pg_sys::relation_close(rel, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
        f64::from(blocks) * f64::from(pg_sys::BLCKSZ)
    }
}

/// M167 ADR-2 — is `coll` a BYTE-ORDER (memcmp) collation, i.e. does DataFusion's Utf8 sort agree with PG's
/// `ORDER BY`? C (950) and POSIX (951) always are. The DEFAULT collation (100) delegates to the database, so it is
/// byte-order iff `pg_database.datcollate` is `C`/`POSIX` — which the M158 OID allowlist could not see, declining a
/// provably safe case on a C cluster (measured: ClickBench q25 on a `datcollate=C` database).
///
/// Determinism is NOT the property we need: it fixes equality (byte-equal ⟺ string-equal), not order — `en_US`
/// sorts 'a' < 'Z' while bytes sort 'Z'(0x5A) < 'a'(0x61).
///
/// Why the catalog and not a GUC: PG 18 has **no** `lc_collate` GUC (removed once per-database collation became
/// authoritative; `pg_settings` exposes only `lc_messages`/`monetary`/`numeric`/`time`). Why not
/// `pg_newlocale_from_collation()->collate_is_c`, which is PG's own predicate: pgrx 0.19 generates no binding for
/// it, so using it would mean a hand-written `extern "C"` plus an assumption about `pg_locale_struct`'s layout.
/// Fail-closed: any unreadable catalog answer returns false (decline to the native plan, correct for any input).
fn database_collate_is_byte_order() -> bool {
    // datcollate cannot change for a live database, so resolve once per backend.
    static CACHED: AtomicU8 = AtomicU8::new(0); // 0 = unknown, 1 = yes, 2 = no
    match CACHED.load(Ordering::Relaxed) {
        1 => return true,
        2 => return false,
        _ => {}
    }
    let is_c = unsafe {
        let tup = pg_sys::SearchSysCache1(
            pg_sys::SysCacheIdentifier::DATABASEOID as i32,
            pg_sys::Datum::from(pg_sys::MyDatabaseId),
        );
        if tup.is_null() {
            false
        } else {
            let mut isnull = false;
            let d = pg_sys::SysCacheGetAttr(
                pg_sys::SysCacheIdentifier::DATABASEOID as i32,
                tup,
                pg_sys::Anum_pg_database_datcollate as pg_sys::AttrNumber,
                &mut isnull,
            );
            // `datcollate` alone does NOT determine how the DEFAULT collation orders — `datlocprovider` does.
            // `CREATE DATABASE d LOCALE_PROVIDER icu ICU_LOCALE 'en-US' LOCALE 'C'` stores datcollate='C' while the
            // default collation orders by ICU en-US (pg_locale.c dispatches on datlocprovider, and dbcommands.c
            // writes the two fields independently). Trusting datcollate there would admit a text sort key whose
            // DataFusion byte order disagrees with PG — the exact wrong-rows class this guard exists to prevent.
            // So: require the libc provider ('c'). Anything else (ICU 'i', builtin 'b') declines — fail-closed.
            let mut prov_isnull = false;
            let prov = pg_sys::SysCacheGetAttr(
                pg_sys::SysCacheIdentifier::DATABASEOID as i32,
                tup,
                pg_sys::Anum_pg_database_datlocprovider as pg_sys::AttrNumber,
                &mut prov_isnull,
            );
            // pgrx exports the catalog constant; a local copy could drift from it silently (Rule 9).
            let provider_is_libc =
                !prov_isnull && (prov.value() as u8) == pg_sys::COLLPROVIDER_LIBC;
            let verdict = if isnull || !provider_is_libc {
                false
            } else {
                let cs = pg_sys::text_to_cstring(d.cast_mut_ptr());
                let s = CStr::from_ptr(cs).to_string_lossy().into_owned();
                pg_sys::pfree(cs.cast());
                s == "C" || s == "POSIX"
            };
            pg_sys::ReleaseSysCache(tup);
            verdict
        }
    };
    CACHED.store(if is_c { 1 } else { 2 }, Ordering::Relaxed);
    is_c
}

/// M167 ADR-2 — the guard a TEXT sort key must pass. Only ever ADDS provably-safe cases to the M158 allowlist.
fn sort_collation_is_byte_order(coll: u32) -> bool {
    const C_COLL: u32 = 950;
    const POSIX_COLL: u32 = 951; // pgrx exports C_COLLATION_OID but not the POSIX one; both are stable catalog OIDs
    const DEFAULT_COLL: u32 = 100;
    match coll {
        C_COLL | POSIX_COLL => true,
        DEFAULT_COLL => database_collate_is_byte_order(),
        _ => false,
    }
}

/// Is the decline trace on? Split out so callers can skip building an expensive message when it is off — the
/// `format!` would otherwise allocate on every decline even with tracing disabled (review finding L4).
///
/// `pub(super)` so the sibling executor can gate its own measurement trace on the same switch: one env var
/// controls all of this subsystem's diagnostics rather than each site inventing its own.
#[inline]
pub(super) fn admit_trace_enabled() -> bool {
    static TRACE_ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *TRACE_ON.get_or_init(|| std::env::var("THEODB_ADMIT_TRACE").as_deref() == Ok("1"))
}

/// M152 (spike) — behavior-NEUTRAL decline trace. When `THEODB_ADMIT_TRACE=1`, emit the reason the columnar-agg
/// path declined a candidate (the ground-truth the static SQL analysis can't give — plan-shape declines in
/// `try_swap_agg` are only visible at runtime). Off by default → zero emission, routing IDENTICAL to M151. Used
/// ONLY to build the M152 routing-map; carries no functional effect (mirrors `THEODB_SCAN_PROFILE` of M150).
#[inline]
fn admit_trace(reason: &str) {
    // Resolve the env var ONCE per backend. With M167 flipping the late-mat default ON, `swap_walk` now runs on
    // every planned statement, so this is called per Sort node per plan on the default path — a `std::env::var`
    // there is a syscall-free but still allocating lookup on the hot planning path (review finding F13).
    if admit_trace_enabled() {
        pgrx::warning!("theodb_admit_decline: {reason}");
    }
}

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
    GucRegistry::define_bool_guc(
        c"theodb.enable_columnar_fast_decode",
        c"Zero-copy fixed-width columnar decode into Arrow (M160)",
        c"When on, non-null fixed-width columns decode as one contiguous typed buffer (no per-cell alloc). Toggle for A/B.",
        &ENABLE_FAST_DECODE,
        GucContext::Userset,
        GucFlags::default(),
    );
    GucRegistry::define_bool_guc(
        c"theodb.enable_columnar_late_mat",
        c"Late-materialization top-k for columnar SELECT … ORDER BY key LIMIT k (M158)",
        c"When on, swap Limit→Sort→columnar-project for a top-k CustomScan that materializes only the k survivors.",
        &ENABLE_COLUMNAR_LATE_MAT,
        GucContext::Userset,
        GucFlags::default(),
    );
    GucRegistry::define_bool_guc(
        c"theodb.enable_columnar_topk_stream",
        c"Stream the columnar top-k input per chunk-group instead of one whole-relation batch (M168)",
        c"When on, the top-k decodes one chunk-group at a time so peak memory is a chunk-group + k, not O(N).",
        &ENABLE_COLUMNAR_TOPK_STREAM,
        GucContext::Userset,
        GucFlags::default(),
    );
    GucRegistry::define_bool_guc(
        c"theodb.enable_columnar_agg_stream",
        c"Stream the columnar aggregate input per chunk-group instead of one whole-relation batch (M169)",
        c"When on, scalar and grouped aggregates decode one chunk-group at a time, so a text column wider than \
          2 GiB no longer overflows the i32 offsets of a single Arrow array.",
        &ENABLE_COLUMNAR_AGG_STREAM,
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

/// The parsed, admissible aggregate: (kind, attno, delta). kind 0 = count(*), 1 = sum(float8), 2 = sum(int)→int8,
/// 3 = avg(float8), … 9 = sum(int2 ± const)→int8 (M166). attno is the 1-based column (0 for count). `delta` carries the
/// SumIntAddConst offset (kind 9, sign already folded); 0 for every other kind.
#[derive(Clone)]
struct ParsedAgg {
    kind: i32,
    attno: i32,
    delta: i64,
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

/// Extract a pushable zone-map predicate from a base-rel qual: `Var(col) <op> Const`. The operator is a btree
/// comparison of `col`'s type (strategy 1-5) OR its negator for `<>` (M151 — `<>` is not a btree strategy). The
/// const may be a DIFFERENT type within the INTEGER class {int2,int4,int8} (M151 cross-type, coerced with a range
/// check by `encode_const_coerced`); cross-type outside the integer class (temporal/float) is declined because raw
/// min/max coercion is not order-isomorphic there. Returns `None` for ANY other shape (function, OR, two-Var, NULL
/// const, non-min/max-able column, unsafe cross-type) → the caller MUST fall back to the native plan.
pub(crate) unsafe fn extract_zone_predicate(
    clause: *mut pg_sys::Node,
    relid: i32,
) -> Option<ZonePredicate> {
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
    // clause not pushed).
    let consttype = (*konst).consttype.to_u32();
    // M151 review HIGH — cross-type coercion is order-isomorphic in the RAW min/max domain ONLY within the integer
    // class {int2,int4,int8}. Two other cross-type classes live in a SINGLE btree opfamily and would be admitted by
    // `var_side == vartype`, but coercing their const by raw bits is WRONG (the zone prune AND `build_filter_expr`
    // compare raw bits, and PG promotes differently):
    //   • temporal (date=days I4 vs timestamp/timestamptz=μs I8; + timezone rotation under a non-UTC `TimeZone`)
    //   • float (`f4col = x::float8` — PG promotes the COLUMN to f8; rounding the const to f32 flips edge rows)
    // Decline cross-type outside the integer class → the native plan applies the real (promoted) comparison. Same
    // type (`consttype == vt`) always passes (temporal/float SAME-type is unaffected).
    let vt = vartype.to_u32();
    const INT_CLASS: [u32; 3] = [21, 23, 20]; // int2, int4, int8
    if consttype != vt && !(INT_CLASS.contains(&consttype) && INT_CLASS.contains(&vt)) {
        return None;
    }
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

/// M161 — extract a pushable INTEGER `IN`-list predicate `Var(int col) IN (int-const, …)` from a base-rel qual.
/// SAFE-ONLY (fail-closed to native plan): `ScalarArrayOpExpr` with `useOr=true` (an `IN`, not `ALL`/`NOT IN`), the
/// scalar operator is `=` (btree strategy 3) in the column's default opfamily, the column is int2/int4/int8, and every
/// array element is a non-NULL integer `Const` coercible into the column type (range-checked). Any NULL element, a
/// non-`Const` element, `NOT IN`/`<> ALL`, a non-integer column, or an empty list → `None` (decline). Rides only to
/// the DataFusion `Filter` (`build_filter_expr` → `col.in_list(lits)`, the D3 authority); never prunes chunk groups.
pub(crate) unsafe fn extract_inlist_predicate(
    clause: *mut pg_sys::Node,
    relid: i32,
) -> Option<super::zonemap::InListPredicate> {
    if clause.is_null() || (*clause).type_ != pg_sys::NodeTag::T_ScalarArrayOpExpr {
        return None;
    }
    let sa = clause as *mut pg_sys::ScalarArrayOpExpr;
    if !(*sa).useOr {
        return None; // `= ANY` (IN); `<> ALL` (NOT IN) is useOr=false → decline
    }
    let args = PgList::<pg_sys::Node>::from_pg((*sa).args);
    if args.len() != 2 {
        return None;
    }
    let (a0, a1) = (args.get_ptr(0)?, args.get_ptr(1)?);
    if (*a0).type_ != pg_sys::NodeTag::T_Var {
        return None; // only `col IN (...)`, not `expr IN (...)`
    }
    let var = a0 as *mut pg_sys::Var;
    if (*var).varno as i32 != relid {
        return None;
    }
    let vartype = (*var).vartype;
    let kind = super::columnar::minmax_kind_of(vartype.to_u32());
    // TRUE integer OIDs only (int2/int4/int8) — NOT `minmax_kind_of`, which folds temporal types into the integer
    // domain (timestamp/timestamptz→I8, date→I4). A temporal column would pass an I2/I4/I8 check yet the IN-list
    // filter emits bare `lit(i64)` (never a Timestamp/Date Arrow literal) → type-mismatch / wrong result. Gate on the
    // OID so temporal declines to the native plan (fail-closed; the M151 temporal cross-type class the A/B never exercises).
    if !matches!(vartype.to_u32(), 20 | 21 | 23) {
        return None;
    }
    let col = ((*var).varattno as i32).checked_sub(1)?; // 1-based → 0-based; system cols rejected
    // The scalar operator must be `=` (btree strategy 3) in the column type's default opfamily (no hardcoded OIDs).
    let opclass = pg_sys::GetDefaultOpClass(vartype, pg_sys::BTREE_AM_OID);
    if opclass == pg_sys::InvalidOid {
        return None;
    }
    let opfamily = pg_sys::get_opclass_family(opclass);
    if !pg_sys::op_in_opfamily((*sa).opno, opfamily) {
        return None;
    }
    let (mut strategy, mut lt, mut rt) = (0 as c_int, pg_sys::InvalidOid, pg_sys::InvalidOid);
    pg_sys::get_op_opfamily_properties(
        (*sa).opno,
        opfamily,
        false,
        &mut strategy,
        &mut lt,
        &mut rt,
    );
    if strategy != 3 || lt != vartype {
        return None; // must be `=` and apply to the column type on the Var (left) side
    }
    // Collect the array's integer elements, each coerced+range-checked into the column type (→ i64 for the filter).
    let coerce = |datum: pg_sys::Datum, ctype: u32| -> Option<i64> {
        let bits = encode_const_coerced(datum, ctype, kind)?;
        Some(match kind {
            MinMaxKind::I2 => bits as i64 as i16 as i64,
            MinMaxKind::I4 => bits as i64 as i32 as i64,
            _ => bits as i64,
        })
    };
    let mut consts: Vec<i64> = Vec::new();
    let arr = a1;
    if (*arr).type_ == pg_sys::NodeTag::T_ArrayExpr {
        // `IN (a, b, c)` before const-folding: an ArrayExpr of Const elements.
        let elems = PgList::<pg_sys::Node>::from_pg((*(arr as *mut pg_sys::ArrayExpr)).elements);
        let elemtype = (*(arr as *mut pg_sys::ArrayExpr)).element_typeid.to_u32();
        for i in 0..elems.len() {
            let e = elems.get_ptr(i)?;
            if (*e).type_ != pg_sys::NodeTag::T_Const {
                return None; // a non-literal element → decline
            }
            let k = e as *mut pg_sys::Const;
            if (*k).constisnull {
                return None; // IN (NULL, …) → 3-valued logic, decline
            }
            consts.push(coerce((*k).constvalue, elemtype)?);
        }
    } else if (*arr).type_ == pg_sys::NodeTag::T_Const {
        // Const-folded array literal: deconstruct the array datum.
        let k = arr as *mut pg_sys::Const;
        if (*k).constisnull {
            return None;
        }
        let arrtype = (*k).consttype;
        let elemtype = pg_sys::get_element_type(arrtype);
        if elemtype == pg_sys::InvalidOid {
            return None;
        }
        let (mut elemlen, mut elembyval, mut elemalign) = (0i16, false, 0i8);
        pg_sys::get_typlenbyvalalign(elemtype, &mut elemlen, &mut elembyval, &mut elemalign);
        let arrp = (*k).constvalue.cast_mut_ptr::<pg_sys::ArrayType>();
        let (mut elems_ptr, mut nulls_ptr, mut nelems) =
            (std::ptr::null_mut(), std::ptr::null_mut(), 0i32);
        pg_sys::deconstruct_array(
            arrp,
            elemtype,
            elemlen as c_int,
            elembyval,
            elemalign,
            &mut elems_ptr,
            &mut nulls_ptr,
            &mut nelems,
        );
        let et = elemtype.to_u32();
        for i in 0..nelems as usize {
            if !nulls_ptr.is_null() && *nulls_ptr.add(i) {
                return None; // a NULL element → decline
            }
            consts.push(coerce(*elems_ptr.add(i), et)?);
        }
    } else {
        return None; // an array subquery / expr → decline
    }
    if consts.is_empty() {
        return None; // empty IN () — decline (native plan handles the degenerate case)
    }
    Some(super::zonemap::InListPredicate { col: col as usize, consts })
}

/// M151 — coerce a `Const` (in `consttype`) into `const_bits` in the COLUMN's `target` MinMaxKind domain, for the
/// cross-type ClickBench pattern (`col int2 <> 0 int4`). Reads the const in ITS own type, then numerically casts
/// to the column domain with a RANGE CHECK: an out-of-range cast (e.g. `int2col = 40000`) returns `None` → the
/// clause is not pushed and the native plan handles it (ALWAYS SAFE — for `=`/`<>` the out-of-range value can
/// never match/exclude a real int2 row; for `<`/`>` an out-of-range bound makes the predicate trivially
/// true/false, which the native plan evaluates correctly). Same-type consts fall through to `encode_const_bits`.
/// The result MUST agree with `compute_minmax` (ints as `i64 as u64`, floats as `f64::to_bits`).
unsafe fn encode_const_coerced(
    datum: pg_sys::Datum,
    consttype: u32,
    target: MinMaxKind,
) -> Option<u64> {
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
        _ => return None, // non-numeric const → cannot coerce
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

/// M156 — classify a built-in text operator to its pushable `TextOp`, or `None` (decline). Only BUILT-IN operators
/// (OID < `FirstNormalObjectId`, the boundary postgres_fdw's `is_builtin` uses) are trusted — a user-defined
/// operator named `=` could carry arbitrary semantics. `=`/`<>`/`~~`(LIKE)/`!~~`(NOT LIKE) push; ILIKE (`~~*`,
/// `!~~*`, locale-aware) and regex (`~`/`!~`/`~*`/`!~*`, RE2/Rust ≠ PG POSIX ERE) DECLINE fail-closed (M152).
unsafe fn classify_text_op(opno: pg_sys::Oid) -> Option<TextOp> {
    if opno.to_u32() >= pg_sys::FirstNormalObjectId {
        return None; // user-defined operator — untrusted semantics
    }
    let namep = pg_sys::get_opname(opno);
    if namep.is_null() {
        return None;
    }
    match CStr::from_ptr(namep).to_str().ok()? {
        "=" => Some(TextOp::Eq),
        "<>" => Some(TextOp::Ne),
        "~~" => Some(TextOp::Like),
        "!~~" => Some(TextOp::NotLike),
        _ => {
            admit_trace("text_where_unsupported_operator"); // ILIKE / regex / other → native plan
            None
        }
    }
}

/// M156 — extract a pushable TEXT predicate `Var(col) <op> Const('needle')` from a base-rel qual. Guards (ADR-2 +
/// blueprint Corner 3/4): the Var is a text/varchar column (25/1043; bpchar 1042 EXCLUDED — `bpchareq` trims
/// trailing blanks, a TYPE semantic the byte-wise DataFusion compare would diverge on, M153); the Const is a
/// non-NULL text/varchar literal; the operator's `inputcollid` is a DETERMINISTIC collation (DataFusion matches
/// byte-wise — the `varlena.c` memcmp guarantee only under a deterministic collation, M153/M154); the operator is
/// one of `=`/`<>`/LIKE/NOT LIKE (`classify_text_op`). NOT symmetric — only `col <op> 'const'` (LIKE has no
/// commutator). Returns `None` for ANY other shape → the caller declines to the native plan.
unsafe fn extract_text_predicate(clause: *mut pg_sys::Node, relid: i32) -> Option<TextPredicate> {
    if clause.is_null() || (*clause).type_ != pg_sys::NodeTag::T_OpExpr {
        return None;
    }
    let op = clause as *mut pg_sys::OpExpr;
    let args = PgList::<pg_sys::Node>::from_pg((*op).args);
    if args.len() != 2 {
        return None;
    }
    let (a0, a1) = (args.get_ptr(0)?, args.get_ptr(1)?);
    // Only `Var <op> Const` (LIKE is not commutable, so a flipped `'const' <op> col` is never admitted).
    if (*a0).type_ != pg_sys::NodeTag::T_Var || (*a1).type_ != pg_sys::NodeTag::T_Const {
        return None;
    }
    let var = a0 as *mut pg_sys::Var;
    let konst = a1 as *mut pg_sys::Const;
    if (*var).varno as i32 != relid || (*konst).constisnull {
        return None; // wrong rel or NULL const (`col = NULL` → native plan evaluates the 3-valued logic)
    }
    let vartype = (*var).vartype.to_u32();
    if vartype != 25 && vartype != 1043 {
        return None; // text (25) / varchar (1043) only — bpchar (1042) excluded (M153)
    }
    let consttype = (*konst).consttype.to_u32();
    if consttype != 25 && consttype != 1043 {
        return None; // the literal must itself be a text-class value we can read as a UTF-8 string
    }
    // Collation guard (M153/M154): the operator compares under `inputcollid`; a byte-wise DataFusion compare only
    // matches PG under a DETERMINISTIC collation. `InvalidOid` = no collation (byte compare) → allowed.
    let collid = (*op).inputcollid;
    if collid != pg_sys::InvalidOid && !pg_sys::get_collation_isdeterministic(collid) {
        admit_trace("text_where_nondeterministic_collation");
        return None;
    }
    let text_op = classify_text_op((*op).opno)?;
    if (*var).varattno < 1 {
        return None; // system column / whole-row Var — never a real text column to push (explicit, not implied)
    }
    // Read the literal's payload WITHOUT pgrx's UTF-8-asserting conversion (council-rust-pgrx HIGH): `String::from_datum`
    // / `<&str>::from_datum` PANIC on a non-ASCII byte under a non-UTF-8 server encoding (LATIN1/WIN1252 → Ascii policy;
    // SQL_ASCII → strict UTF-8), turning a valid query into a planner ERROR inside the upper-paths hook. Go through
    // `text_to_cstring` (raw payload copy, no assertion) and DECLINE fail-closed when the bytes are not valid UTF-8 —
    // the DataFusion `Utf8` filter cannot represent non-UTF-8 bytes anyway, so the native plan must handle that case.
    let cstr = pg_sys::text_to_cstring((*konst).constvalue.cast_mut_ptr::<pg_sys::text>());
    let needle = CStr::from_ptr(cstr).to_str().ok()?.to_owned();
    // M156 (council-index-storage MEDIUM): a LIKE/NOT LIKE pattern ending in an ODD number of `\` has a dangling
    // escape → PG raises ERROR 22025 ("LIKE pattern must not end with escape character", like_match.c) while arrow's
    // kernel treats the trailing `\` as a literal and returns rows. Decline so the native plan applies the real
    // (error) semantics. `=`/`<>` (texteq/textne) have no escape rule and are unaffected.
    if matches!(text_op, TextOp::Like | TextOp::NotLike) {
        let trailing_backslashes = needle.bytes().rev().take_while(|&b| b == b'\\').count();
        if trailing_backslashes % 2 == 1 {
            admit_trace("text_where_like_dangling_escape");
            return None;
        }
    }
    let col = ((*var).varattno as i32) - 1; // 1-based AttrNumber → 0-based (varattno ≥ 1 checked above)
    Some(TextPredicate { col: col as usize, op: text_op, needle })
}

/// Extract ALL of the base rel's WHERE quals as pushable predicates. Returns `None` if ANY qual is NOT pushable
/// (neither a numeric zone predicate nor a text predicate) — the DataFusion filter can only represent
/// `col <op> const`, so an un-pushable qual means the CustomScan cannot apply the full WHERE and MUST decline (the
/// native plan then applies it correctly). M156 — a qual is pushable as a numeric zone predicate OR a text predicate.
unsafe fn extract_all_predicates(
    input_rel: *mut pg_sys::RelOptInfo,
    relid: i32,
) -> Option<(Vec<ZonePredicate>, Vec<TextPredicate>, Vec<super::zonemap::InListPredicate>)> {
    let ris = PgList::<pg_sys::RestrictInfo>::from_pg((*input_rel).baserestrictinfo);
    let mut zpreds = Vec::with_capacity(ris.len());
    let mut tpreds: Vec<TextPredicate> = Vec::new();
    let mut inpreds: Vec<super::zonemap::InListPredicate> = Vec::new();
    for i in 0..ris.len() {
        let ri = ris.get_ptr(i)?;
        let clause = (*ri).clause as *mut pg_sys::Node;
        if let Some(z) = extract_zone_predicate(clause, relid) {
            zpreds.push(z);
        } else if let Some(t) = extract_text_predicate(clause, relid) {
            tpreds.push(t);
        } else {
            // `?` em vez de `else { return None }`: qual nao-empurravel declina o pushdown, e o plano
            // nativo aplica o WHERE completo. Mesmo efeito, e o clippy::question_mark deixa de reprovar.
            inpreds.push(extract_inlist_predicate(clause, relid)?); // M161 — integer IN-list
        }
    }
    Some((zpreds, tpreds, inpreds))
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
    text_preds: Vec<TextPredicate>, // M156 — text WHERE predicates (filter-only, never prune)
    in_preds: Vec<super::zonemap::InListPredicate>, // M161 — integer IN-list WHERE (filter-only, never prune)
    group_cols: Vec<(i32, u32)>,
    group_exprs: Vec<GroupExprSpec>, // M157 — expression group keys (date_trunc), layout kind=2
    const_outs: Vec<(i64, u32)>, // M165 — projected integer constant output cells (SELECT 1, …), layout kind=3
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

/// M157/M161 — a função de uma chave de grupo por EXPRESSÃO. Discriminante explícito: serializado como `func as i32`
/// no 4º canal `custom_private` e decodificado por literal (o compilador não checa esse mapeamento — mesmo contrato do
/// `ZoneOp`/`TextOp`). M157 shipou `DateTrunc`; M161 adiciona a subclasse SEGURA de expressões: `ExtractField`
/// (epoch-invariante minute/hour), `IntAddConst` (`col ± k` int, com range-check no materialize) e `Const` (literal int).
#[derive(Clone, Copy, PartialEq, Debug)]
#[repr(i32)]
enum GroupFunc {
    DateTrunc = 0,    // date_trunc('unit', ts::timestamp) → timestamp
    ExtractField = 1, // M161 — date_part('unit', ts::timestamp) → numeric (só minute/hour: epoch-invariante + inteiro)
    IntAddConst = 2, // M161 — col ± k (int2/4/8) → mesmo tipo int (compute widened + range-check no materialize)
                     // NB: um GROUP BY por constante literal (`GROUP BY 1`) NÃO é uma variante aqui — o planner do PG ELIMINA chaves de
                     // grupo constantes antes do plano final (grouping por constante é redundante), então o admit contaria a chave mas o
                     // Agg do plano não a teria → mismatch no swap → declina. Medido no ClickBench q34 (M161 honest-negative).
}

/// M157/M161 — uma chave de grupo por EXPRESSÃO. `func` seleciona a computação:
/// - `DateTrunc` — `date_trunc(unit, col[base_attno]::timestamp)` → `out_typoid==1114`.
/// - `ExtractField` — `date_part(unit, col[base_attno]::timestamp)` → `out_typoid==1700` (numeric); `unit` ∈ {minute,hour}.
/// - `IntAddConst` — `col[base_attno] <op> delta` (op embutido no sinal de `delta`) → `out_typoid` = tipo int da coluna.
#[derive(Clone, Debug)]
struct GroupExprSpec {
    base_attno: i32,
    func: GroupFunc,
    unit: String,    // date_trunc/extract unit; "" no int±k
    delta: i64,      // IntAddConst: delta já com sinal aplicado; 0 nos demais
    out_typoid: u32, // tipo PG de saída (1114 date_trunc, 1700 extract, tipo int da coluna no int±k)
}

/// Um slot de output classificado: uma group key `Var` (attno, vartype), uma group key por EXPRESSÃO (M157) ou um
/// agregado parseado. `main` empurra em `layout`/`group_cols`/`group_exprs`/`aggs` na ORDEM do target (índices
/// dependem do comprimento no push — preservados).
enum TargetSlot {
    Group(i32, u32),
    GroupExpr(GroupExprSpec), // M157 — layout kind=2
    ConstOut(i64, u32), // M165 — projected integer constant (SELECT 1, …), layout kind=3; (value, typoid)
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
            admit_trace("agg_output_type_numeric");
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
            admit_trace("agg_output_type_numeric");
            return None; // avg(float4)→float8-ULP, avg(numeric): decline
        }
    } else {
        // min/max: any ordered native type (same set the zone-map supports) → output = input type.
        if super::columnar::minmax_kind_of(vartype.to_u32()) == MinMaxKind::None {
            admit_trace("minmax_over_unordered_text"); // M152
            return None; // unordered type (text/numeric/…) → native plan
        }
        if name == "min" { 6 } else { 7 }
    };
    Some(kind)
}

/// M166 — classify a `SUM(int2_col ± const)` aggregate argument (ClickBench q29) into the SAFE `SumIntAddConst` slot
/// (kind 9), or `None` (→ the caller declines fail-closed). The provably-byte-identical class is NARROWER than the
/// GROUP BY `IntAddConst` gate (line ~785): that path materializes each per-row value and reproduces PG's 22003 with a
/// range check, but a SUM only sums — it never forms the per-row int4 — so we admit ONLY inputs where the whole int2
/// domain ± delta provably stays inside int4 (then PG raises no 22003 and the Int64 sum is exact). Requirements, all
/// fail-closed: `Var(int2 base-rel col) <+/-> Const(int)` canonical shape (const on the right; sign folded into
/// `delta`), an int4 operator RESULT type (`int2 ± int4-const → int4`), and `±32768 + delta` both fit int4. An int4/int8
/// base column, an int2 or int8 result, a non-additive op, a float/numeric const, or an out-of-range delta all decline.
unsafe fn classify_sum_int_add_const(node: *mut pg_sys::Node, relid: i32) -> Option<TargetSlot> {
    let op = node as *mut pg_sys::OpExpr;
    // Only a builtin operator has trusted semantics — a user-defined `OPERATOR +(int2,int4)` placed ahead of pg_catalog
    // in search_path could run arbitrary code while DataFusion computes `col+delta` (silent divergence). Decline it, as
    // classify_text_op does (council-rust-pgrx MEDIUM; parity with the builtin-only gate at line ~443).
    if (*op).opno.to_u32() >= pg_sys::FirstNormalObjectId {
        return None;
    }
    let opname_ptr = pg_sys::get_opname((*op).opno);
    if opname_ptr.is_null() {
        return None;
    }
    let opname = CStr::from_ptr(opname_ptr).to_string_lossy().into_owned();
    if opname != "+" && opname != "-" {
        return None; // additive int arithmetic only
    }
    let args = PgList::<pg_sys::Node>::from_pg((*op).args);
    if args.len() != 2 {
        return None;
    }
    let (a0, a1) = (args.get_ptr(0)?, args.get_ptr(1)?);
    // Canonical `Var <op> Const` only (`k - col` is not `col - k`; decline the non-canonical form).
    if (*a0).type_ != pg_sys::NodeTag::T_Var || (*a1).type_ != pg_sys::NodeTag::T_Const {
        return None;
    }
    let var = a0 as *mut pg_sys::Var;
    let konst = a1 as *mut pg_sys::Const;
    if (*var).varno as i32 != relid || (*konst).constisnull {
        return None;
    }
    let attno = (*var).varattno as i32;
    if attno <= 0 {
        return None; // system / whole-row column
    }
    // int2 base column ONLY: `int4col ± k` can overflow int4 per row (PG raises 22003) and the widened Int64 sum would
    // silently succeed instead — not byte-identical. int8/float/temporal base also decline.
    if (*var).vartype.to_u32() != 21 {
        return None;
    }
    // Result type MUST be int4 (`int2 ± int4-const → int4`). An int2 result (`int2 ± int2-const`) declines — per-row
    // int2 overflow is reachable and un-reproduced by a widened sum; an int8 result declines likewise.
    if (*op).opresulttype.to_u32() != 23 {
        return None;
    }
    // Read the const in its own int type → i64. A non-integer const → decline.
    let k: i64 = match (*konst).consttype.to_u32() {
        21 => i16::from_datum((*konst).constvalue, false)? as i64,
        23 => i32::from_datum((*konst).constvalue, false)? as i64,
        20 => i64::from_datum((*konst).constvalue, false)?,
        _ => return None,
    };
    // Fold the operator sign into `delta`: `col - k` == `col + (-k)`. `-i64::MIN` overflow → decline.
    let delta = if opname == "-" { k.checked_neg()? } else { k };
    // PROVE no per-row int4 overflow over the WHOLE int2 domain ± delta: both extremes must fit int4. (`int2 ± huge
    // int4-const` CAN overflow int4 — e.g. 32767 + 2147483647; declining it keeps the "sum is exact" argument sound and
    // lets the native plan reproduce PG's 22003.) This is the check the GROUP BY IntAddConst gate does not need.
    i32::try_from(32767i64.checked_add(delta)?).ok()?;
    i32::try_from((-32768i64).checked_add(delta)?).ok()?;
    Some(TargetSlot::Agg(ParsedAgg { kind: 9, attno, delta }))
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
            admit_trace("group_key_type_unsupported"); // M152
            return None; // unsupported key type (numeric, etc.) → native plan
        }
        // M163 (found by the type-coverage A/B harness): a FLOAT group key diverges. DataFusion groups by IEEE
        // byte-value (−0.0 ≠ +0.0; distinct NaN bit-patterns) while PG's `float8eq` groups −0.0 WITH +0.0 and treats
        // all NaN as equal — so `GROUP BY float` splits the −0.0/+0.0 rows into separate groups where PG merges them
        // (measured diverged=2 on the −0.0 edge). Decline float group keys to the native plan — mirrors the M154
        // count(DISTINCT float) decline (same IEEE-vs-float8eq root cause), now for grouping.
        if matches!((*var).vartype, pg_sys::FLOAT4OID | pg_sys::FLOAT8OID) {
            admit_trace("group_key_float_ieee_semantics"); // M163
            return None;
        }
        // M153: DataFusion's byte-keyed hash groups by byte-equality; PG groups by the key's collation equality.
        // They coincide only under a DETERMINISTIC collation (deterministic ⟺ equality is byte-wise). A
        // non-deterministic collation (ICU case/accent-insensitive) would group byte-different-but-collation-equal
        // strings SEPARATELY here but TOGETHER in PG → wrong group counts. Decline (covers BOTH HASHED and SORTED).
        if (*var).varcollid != pg_sys::InvalidOid
            && !pg_sys::get_collation_isdeterministic((*var).varcollid)
        {
            admit_trace("group_key_nondeterministic_collation"); // M153
            return None;
        }
        Some(TargetSlot::Group(attno, (*var).vartype.to_u32()))
    } else if (*node).type_ == pg_sys::NodeTag::T_FuncExpr {
        // M157 — a group key that is `date_trunc('unit', ts::timestamp)`. Only when GROUP BY is present.
        if !grouped {
            return None;
        }
        let fe = node as *mut pg_sys::FuncExpr;
        // Function name via the catalog (no hardcoded OID — ADR-1 / D5). `date_trunc` (M157) OR `extract` (M161).
        let fnamep = pg_sys::get_func_name((*fe).funcid);
        if fnamep.is_null() {
            return None;
        }
        let fname = CStr::from_ptr(fnamep).to_string_lossy().into_owned();
        let is_extract = fname == "extract"; // M161 — EXTRACT(field FROM ts) → func `extract`, returns numeric (PG14+)
        if fname != "date_trunc" && !is_extract {
            admit_trace("group_expr_func_unsupported"); // other function → native
            return None;
        }
        let args = PgList::<pg_sys::Node>::from_pg((*fe).args);
        if args.len() != 2 {
            return None; // the 2-arg `date_trunc(unit, ts)` / `extract(unit, ts)` only (3-arg tz form out of scope)
        }
        let (a0, a1) = (args.get_ptr(0)?, args.get_ptr(1)?);
        // arg0 = a text Const granularity in the whitelist (the units where PG and Arrow agree — blueprint Corner 3).
        if (*a0).type_ != pg_sys::NodeTag::T_Const {
            return None;
        }
        let unit_const = a0 as *mut pg_sys::Const;
        let ut = (*unit_const).consttype.to_u32();
        if (*unit_const).constisnull || (ut != 25 && ut != 1043) {
            return None;
        }
        let unit_cstr =
            pg_sys::text_to_cstring((*unit_const).constvalue.cast_mut_ptr::<pg_sys::text>());
        let unit = CStr::from_ptr(unit_cstr).to_str().ok()?.to_ascii_lowercase();
        // EPOCH GUARD (council-index-storage CRITICAL): the columnar timestamp is stored as µs-since-2000 (PG epoch)
        // but decoded into an Arrow `Timestamp` that DataFusion reads as µs-since-1970. The PG↔Arrow offset is exactly
        // 10957 days — a whole multiple of day/hour/minute/second, so those granularities are epoch-INVARIANT (the
        // truncation commutes with adding a whole-unit offset) and match PG byte-for-byte. But 10957 days is NOT a
        // whole number of months/quarters/years, and the leap-day count differs between the [1970..] and [2000..]
        // windows, so date_trunc's CALENDAR truncation for month/quarter/year lands on the wrong absolute date
        // (±1 day, crossing the bucket boundary → wrong key AND wrong partition). `week` is ISO-week (PG≠Arrow).
        // Restrict to the epoch-invariant granularities; everything coarser declines to the native plan (fail-closed).
        // M161 EXTRACT epoch/value guard (STRICTER than date_trunc): `extract(u FROM ts)` reads the Arrow value
        // (= PG value + 10957 days). `minute`/`hour` are epoch-invariant (10957 days is a whole multiple of both) AND
        // integer-valued. `day` (day-of-month) shifts with the calendar offset → WRONG. `second` returns FRACTIONAL
        // seconds (numeric with µs) that DataFusion's integer date_part truncates → diverges. So extract is restricted
        // to {minute, hour}; date_trunc keeps its epoch-invariant {second, minute, hour, day} (truncation, not field).
        const DT_UNITS: [&str; 4] = ["second", "minute", "hour", "day"];
        const EX_UNITS: [&str; 2] = ["minute", "hour"];
        let unit_ok = if is_extract {
            EX_UNITS.contains(&unit.as_str())
        } else {
            DT_UNITS.contains(&unit.as_str())
        };
        if !unit_ok {
            admit_trace("group_expr_granularity_unsupported"); // week / month / quarter / year / day-extract / second-extract → native
            return None;
        }
        // arg1 = a base-rel Var of type `timestamp` (1114). `timestamptz` (1184) DIVERGES under `TimeZone≠UTC`
        // (PG uses session_timezone; DataFusion truncates in UTC) → decline unconditionally (ADR-2; same class the
        // M151 temporal cross-type review caught).
        if (*a1).type_ != pg_sys::NodeTag::T_Var {
            return None;
        }
        let var = a1 as *mut pg_sys::Var;
        if (*var).varno as i32 != relid {
            return None;
        }
        let base_attno = (*var).varattno as i32;
        if base_attno <= 0 {
            return None;
        }
        if (*var).vartype.to_u32() != 1114 {
            admit_trace("group_expr_date_trunc_timestamptz"); // 1184 timestamptz / non-timestamp → native
            return None;
        }
        Some(TargetSlot::GroupExpr(GroupExprSpec {
            base_attno,
            func: if is_extract { GroupFunc::ExtractField } else { GroupFunc::DateTrunc },
            unit,
            delta: 0,
            // date_trunc(timestamp) → timestamp (1114); extract(minute|hour FROM timestamp) → numeric (1700, PG14+).
            out_typoid: if is_extract { 1700 } else { 1114 },
        }))
    } else if (*node).type_ == pg_sys::NodeTag::T_OpExpr && grouped {
        // M161 — a group key `col ± const` (int2/4/8). PG evaluates in the column's int type and RAISES 22003 on
        // overflow; DataFusion (Int32/Int64) would wrap. We compute WIDENED to Int64 (never overflows for int2/4/8 ±
        // int const) so the grouping is exact, and RANGE-CHECK back to the base type at materialize — reproducing PG's
        // 22003 byte-for-byte when a value is out of range, and the exact value otherwise (ADR-2). Only `+`/`-` with a
        // `Var(int) <op> Const(int)` shape (constant on the right — canonical form after const-folding).
        let op = node as *mut pg_sys::OpExpr;
        // Builtin operator only — a user-defined `OPERATOR +` ahead of pg_catalog in search_path could run arbitrary
        // semantics while DataFusion computes `col+delta` (silent divergence). Parity with classify_text_op (~:443) and
        // the SUM(int2±const) gate (council-rust-pgrx MEDIUM — close the class, not just the instance).
        if (*op).opno.to_u32() >= pg_sys::FirstNormalObjectId {
            admit_trace("group_expr_op_unsupported");
            return None;
        }
        let opname_ptr = pg_sys::get_opname((*op).opno);
        if opname_ptr.is_null() {
            admit_trace("group_expr_op_unsupported");
            return None;
        }
        let opname = CStr::from_ptr(opname_ptr).to_string_lossy().into_owned();
        if opname != "+" && opname != "-" {
            admit_trace("group_expr_op_unsupported"); // only additive int arithmetic
            return None;
        }
        let args = PgList::<pg_sys::Node>::from_pg((*op).args);
        if args.len() != 2 {
            return None;
        }
        let (a0, a1) = (args.get_ptr(0)?, args.get_ptr(1)?);
        // Canonical `Var <op> Const` only (a flipped `k - col` is NOT `col - k`; decline the non-canonical form).
        if (*a0).type_ != pg_sys::NodeTag::T_Var || (*a1).type_ != pg_sys::NodeTag::T_Const {
            admit_trace("group_expr_int_arith_shape"); // k - col / col - col / etc → native
            return None;
        }
        let var = a0 as *mut pg_sys::Var;
        let konst = a1 as *mut pg_sys::Const;
        if (*var).varno as i32 != relid || (*konst).constisnull {
            return None;
        }
        let base_attno = (*var).varattno as i32;
        if base_attno <= 0 {
            return None;
        }
        let base_typoid = (*var).vartype.to_u32();
        // TRUE integer OIDs only — NOT `minmax_kind_of` (which folds date→I4, timestamp→I8): `date + int` (date_pli)
        // would pass an I2/I4/I8 check yet materialize with out_typoid=1082, hitting `group_expr_cell`'s int-only
        // range-check → admitted-then-errored instead of declining. Gate on the OID so temporal ± int → native plan.
        if !matches!(base_typoid, 20 | 21 | 23) {
            admit_trace("group_expr_int_arith_nonint"); // float/text/numeric/temporal arithmetic → native
            return None;
        }
        // Read the const in its own int type → i64 (int2/4/8 all fit). A non-integer const type → decline.
        let ct = (*konst).consttype.to_u32();
        let k: i64 = match ct {
            21 => i16::from_datum((*konst).constvalue, false)? as i64,
            23 => i32::from_datum((*konst).constvalue, false)? as i64,
            20 => i64::from_datum((*konst).constvalue, false)?,
            _ => {
                admit_trace("group_expr_int_arith_nonint_const");
                return None;
            }
        };
        // Fold the operator sign into `delta`: `col - k` == `col + (-k)`. `-i64::MIN` would overflow → decline (never a
        // real ClickBench constant; fail-closed keeps the widened-add correctness argument sound).
        let delta = if opname == "-" { k.checked_neg()? } else { k };
        // OUTPUT type = the OPERATOR's result type, NOT the column type. PG integer +/- are cross-type and WIDEN:
        // `int2±int4→int4`, `int4±int8→int8` (an undecorated literal is int4). Using the column type would fail
        // `i16::try_from` on a valid int4 result (`int2col+5` at 32767 → PG=int4 32772, ours would error). Decline any
        // int8 result (opresulttype==20): the widened Int64 compute itself can overflow for an int8 result → not
        // PG-22003-equivalent (fail-closed, avoids a wrong answer). int2/int4 results keep the exact i64 compute
        // (int2/int4 column ± int const fits i64 with huge margin) + range-check to the result type at materialize.
        let out_typoid = (*op).opresulttype.to_u32();
        if !matches!(out_typoid, 21 | 23) {
            admit_trace("group_expr_int_arith_wide_result"); // int8 (or wider) result → native plan
            return None;
        }
        Some(TargetSlot::GroupExpr(GroupExprSpec {
            base_attno,
            func: GroupFunc::IntAddConst,
            unit: String::new(),
            delta,
            out_typoid, // = (*op).opresulttype (int2/int4); int8 result declined above
        }))
    } else if (*node).type_ == pg_sys::NodeTag::T_Aggref {
        let agg = node as *mut pg_sys::Aggref;
        // aggfilter / aggorder are always declined; aggdistinct is declined here EXCEPT for the
        // `count(DISTINCT single-var)` shape handled below (M154 — exact `count_distinct` via DataFusion).
        if !(*agg).aggfilter.is_null() || !(*agg).aggorder.is_null() {
            admit_trace("agg_distinct_filter_order"); // M152
            return None;
        }
        let has_distinct = !(*agg).aggdistinct.is_null();
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
            // count(*) is never DISTINCT — kind 0.
            Some(TargetSlot::Agg(ParsedAgg { kind: 0, attno: 0, delta: 0 }))
        } else if name == "count" && has_distinct {
            // count(DISTINCT col) — M154 (kind 8). Exactly 1 base-rel Var of an Arrow-decodable type, and — for
            // collatable (text) columns — a DETERMINISTIC collation (ADR-M154-3 / edge EC-1): DataFusion's
            // count_distinct uses byte-wise equality, which matches PG only under deterministic collations.
            let args = PgList::<pg_sys::TargetEntry>::from_pg((*agg).args);
            if args.len() != 1 {
                admit_trace("count_distinct_multiarg"); // count(DISTINCT a,b) → decline (ADR-M154-2)
                return None;
            }
            let te = args.get_ptr(0)?;
            let e = (*te).expr as *mut pg_sys::Node;
            if e.is_null() || (*e).type_ != pg_sys::NodeTag::T_Var {
                admit_trace("agg_over_expression"); // count(DISTINCT col+1) → decline
                return None;
            }
            let var = e as *mut pg_sys::Var;
            if (*var).varno as i32 != relid {
                return None; // Var from another rel → decline
            }
            let attno = (*var).varattno as i32;
            if attno <= 0 {
                return None; // system / whole-row column → decline
            }
            if !super::df_executor::arrow_supported_group_type((*var).vartype.to_u32()) {
                admit_trace("count_distinct_unsupported_type"); // type not decodable to Arrow → decline
                return None;
            }
            // Float DISTINCT diverges (ADR-M154-4 / review HIGH): DataFusion's FloatDistinctCountAccumulator dedups
            // by IEEE total-order (-0.0 != +0.0; distinct NaN bit-patterns count separately), but PG's float8eq
            // treats 0.0 == -0.0 and all NaN as equal. Decline float to the native plan (provably-safe class only).
            let vt = (*var).vartype;
            if vt == pg_sys::FLOAT4OID || vt == pg_sys::FLOAT8OID {
                admit_trace("count_distinct_float_ieee_semantics");
                return None;
            }
            // Collation equality (ADR-M154-3 / EC-1): DataFusion count_distinct is byte-wise; PG uses the input
            // collation for the DISTINCT equality. Only deterministic collations coincide. Use `inputcollid` — the
            // exact collation PG drives the DISTINCT with (nodeAgg.c) — for precision + defense-in-depth.
            let coll = (*agg).inputcollid;
            if coll != pg_sys::InvalidOid && !pg_sys::get_collation_isdeterministic(coll) {
                admit_trace("count_distinct_nondeterministic_collation");
                return None;
            }
            Some(TargetSlot::Agg(ParsedAgg { kind: 8, attno, delta: 0 }))
        } else if !has_distinct
            && (name == "sum" || name == "avg" || name == "min" || name == "max")
        {
            let args = PgList::<pg_sys::TargetEntry>::from_pg((*agg).args);
            if args.len() != 1 {
                return None;
            }
            let te = args.get_ptr(0)?;
            let e = (*te).expr as *mut pg_sys::Node;
            if e.is_null() {
                return None;
            }
            // M166 — SUM(int2_col ± const) (ClickBench q29): the argument is an OpExpr, not a bare Var. Admit the
            // provably-byte-identical `int2 base + int4 result` class here; every other expr shape (min/avg/max of an
            // expression, or a SUM that fails the safe-class gate) declines to the native plan below (fail-closed).
            if name == "sum" && (*e).type_ == pg_sys::NodeTag::T_OpExpr {
                if let Some(slot) = classify_sum_int_add_const(e, relid) {
                    return Some(slot);
                }
                admit_trace("agg_sum_expr_unsupported"); // int4-col / int8 result / non-additive / out-of-range → native
                return None;
            }
            if (*e).type_ != pg_sys::NodeTag::T_Var {
                admit_trace("agg_over_expression"); // M152
                return None; // bare column Var only — reject min(col+1) / cast (directory is pre-projection)
            }
            let var = e as *mut pg_sys::Var;
            if (*var).varno as i32 != relid {
                return None;
            }
            let kind = parse_agg_kind(&name, (*var).vartype)?;
            Some(TargetSlot::Agg(ParsedAgg { kind, attno: (*var).varattno as i32, delta: 0 }))
        } else {
            // Includes sum/avg/min/max(DISTINCT ...) → declined (ADR-M154-2).
            admit_trace("unsupported_agg_func");
            None
        }
    } else if (*node).type_ == pg_sys::NodeTag::T_Const {
        // M165 — a bare integer literal projected in the output target (`SELECT 1, url, count(*) …`). PG's planner
        // ELIMINATES a constant group key from groupClause/numCols, so the effective grouping is single-key
        // (`GROUP BY url`) and only the projected constant column blocks routing (the M161 q34 honest-negative). Admit
        // it as a FIXED OUTPUT CELL — NOT a grouping key (it never counts toward numCols). FAIL-CLOSED to the integer
        // class {int2,int4,int8} + non-NULL: a float const would carry IEEE −0.0/NaN, a text const a collation, a
        // numeric const a scale — none can reach the output byte-identically (the same class the M163 float group-key
        // and M154 count(DISTINCT float) declines protect). Only when GROUP BY is present (a scalar `SELECT 1, count(*)`
        // keeps the layout-empty scalar-path invariant → decline).
        if !grouped {
            return None;
        }
        let konst = node as *mut pg_sys::Const;
        if (*konst).constisnull {
            admit_trace("const_out_null"); // NULL const → native plan (fail-closed)
            return None;
        }
        let ctype = (*konst).consttype.to_u32();
        let val: i64 = match ctype {
            21 => i16::from_datum((*konst).constvalue, false)? as i64,
            23 => i32::from_datum((*konst).constvalue, false)? as i64,
            20 => i64::from_datum((*konst).constvalue, false)?,
            _ => {
                admit_trace("const_out_type_unsupported"); // float/text/numeric/bool/other const → native plan
                return None;
            }
        };
        Some(TargetSlot::ConstOut(val, ctype))
    } else {
        admit_trace("target_grouping_expression_or_other"); // M152
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
    group_exprs: Vec<GroupExprSpec>, // M157
    const_outs: Vec<(i64, u32)>, // M165 — projected integer constant output cells (layout kind=3)
    layout: Vec<(u8, usize)>,
) -> Option<Admitted> {
    // Mode: a columnar table (decode stripes) vs a heap table with a usable Arrow cache (M101 HTAP).
    let amoid = columnar_amoid();
    let is_columnar = amoid != pg_sys::InvalidOid && pg_sys::get_rel_relam((*rte).relid) == amoid;
    if is_columnar {
        if grouped {
            // GROUP BY + WHERE combined (M114): un-pushable qual → `extract_all_predicates` None → decline.
            let (preds, text_preds, in_preds) = match extract_all_predicates(input_rel, relid) {
                Some(p) => p,
                None => {
                    admit_trace("unpushable_where_qual");
                    return None;
                } // M152
            };
            return Some(Admitted {
                mode: 0,
                relid,
                aggs,
                preds,
                text_preds,
                in_preds,
                group_cols,
                group_exprs,
                const_outs,
                layout,
            });
        }
        // Non-grouped: ALL quals must be pushable (`col <op> const`), else decline.
        let (preds, text_preds, in_preds) = match extract_all_predicates(input_rel, relid) {
            Some(p) => p,
            None => {
                admit_trace("unpushable_where_qual");
                return None;
            } // M152
        };
        return Some(Admitted {
            mode: 0,
            relid,
            aggs,
            preds,
            text_preds,
            in_preds,
            group_cols: Vec::new(),
            group_exprs: Vec::new(),
            const_outs: Vec::new(), // M165 — non-grouped path never admits a const-out (grouped-only)
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
            text_preds: Vec::new(), // M156 — heap-cache path declines any WHERE (guarded above), so never text preds
            in_preds: Vec::new(), // M161 — heap-cache path declines any WHERE, so never IN-list preds
            group_cols: Vec::new(),
            group_exprs: Vec::new(), // M157 — heap-cache path is non-grouped, so never group exprs
            const_outs: Vec::new(),  // M165 — heap-cache path is non-grouped, so never const-outs
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
        admit_trace("grouping_sets_having_distinct_window"); // M152
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
    let mut group_exprs: Vec<GroupExprSpec> = Vec::new(); // M157
    let mut const_outs: Vec<(i64, u32)> = Vec::new(); // M165 — projected integer constant output cells (kind=3)
    let mut layout: Vec<(u8, usize)> = Vec::with_capacity(exprs.len());
    for i in 0..exprs.len() {
        let node = exprs.get_ptr(i)?;
        match classify_target_node(node, relid, grouped)? {
            TargetSlot::Group(attno, vartype) => {
                layout.push((0, group_cols.len()));
                group_cols.push((attno, vartype));
            }
            TargetSlot::GroupExpr(spec) => {
                layout.push((2, group_exprs.len())); // M157 — kind=2 group-expr slot
                group_exprs.push(spec);
            }
            TargetSlot::ConstOut(val, typoid) => {
                layout.push((3, const_outs.len())); // M165 — kind=3 const-out slot (NOT a grouping key)
                const_outs.push((val, typoid));
            }
            TargetSlot::Agg(parsed) => {
                layout.push((1, aggs.len()));
                aggs.push(parsed);
            }
        }
    }
    if grouped && group_cols.is_empty() && group_exprs.is_empty() {
        return None; // GROUP BY with NO grouping key at all (a const-out alone is not a key) → native plan
    }
    build_admission(
        rte,
        input_rel,
        relid,
        grouped,
        aggs,
        group_cols,
        group_exprs,
        const_outs,
        layout,
    )
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
    // Run the walk when EITHER an admitted aggregate awaits its Agg-swap (M115 — needs the stash + agg GUC), OR the
    // M158 late-materialization GUC is on (a pure top-k query has no aggregate/stash). `try_swap_agg` declines
    // fail-closed without a matching stash entry; `try_swap_topk` carries its own GUC + shape guards — so running the
    // walk under the disjunction never mis-swaps.
    if !stmt.is_null()
        && ((ENABLE_COLUMNAR_AGG.get() && have_stash) || ENABLE_COLUMNAR_LATE_MAT.get())
    {
        swap_walk(&mut (*stmt).planTree, (*stmt).rtable, std::ptr::null_mut());
        let subplans = (*stmt).subplans;
        if !subplans.is_null() {
            let n = (*subplans).length;
            for i in 0..n {
                let cell = (*subplans).elements.add(i as usize);
                swap_walk(
                    &mut (*cell).ptr_value as *mut _ as *mut *mut pg_sys::Plan,
                    (*stmt).rtable,
                    std::ptr::null_mut(),
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
        // one aggregate per output column, in order. Layout tags: 0=group col, 1=agg, 2=group-expr (M157), 3=const-out (M165).
        let (tag, idx) = if adm.layout.is_empty() {
            (1u8, i)
        } else {
            match adm.layout.get(i) {
                Some(&(t, k)) => (t, k),
                None => return std::ptr::null_mut(),
            }
        };
        let expr: *mut pg_sys::Expr = if tag == 0 {
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
        } else if tag == 2 {
            // M157/M161 — a group-expr output column (date_trunc / extract / int±k). POST-PLANNING the Agg tlist entry
            // `e` for this slot references the child-computed group column (OUTER_VAR) or is the raw expression —
            // copying it would deparse into the dropped subtree (M131 #135). So build a FRESH plain
            // `Var(scanrelid, base_attno, out_typoid)` that is descriptor-equal (out_typoid == exprType(e)) and is ONLY
            // descriptor + deparse metadata (runtime tuples come from the materialized `run_columnar_grouped_aggs`
            // result; scanrelid=0, never a real scan). Deparse shows the base column for this slot — cosmetic; the value
            // is byte-identical.
            match adm.group_exprs.get(idx) {
                Some(g) => pg_sys::makeVar(
                    scanrelid as i32,
                    g.base_attno as pg_sys::AttrNumber,
                    pg_sys::Oid::from(g.out_typoid),
                    pg_sys::exprTypmod(e),
                    pg_sys::exprCollation(e),
                    0,
                ) as *mut pg_sys::Expr,
                None => return std::ptr::null_mut(),
            }
        } else if tag == 3 {
            // M165 — a const-out output column (`SELECT 1, …`). Copy the Const literal itself: it is descriptor-equal
            // (exprType/typmod/collation are the const's own) and deparse-safe — a Const is a non-Var leaf, so
            // ruleutils' resolve_special_varno stops at it (no INDEX_VAR self-recursion, the M131 #135 hang) and a
            // literal carries no OUTER_VAR into the dropped subtree. Runtime tuples come from the materialized
            // const_outs (scanrelid=0, never a real scan); this entry is descriptor + deparse metadata only.
            let copied = pg_sys::copyObjectImpl(e as *const c_void) as *mut pg_sys::Node;
            if copied.is_null() {
                return std::ptr::null_mut(); // never place a NULL expr — ExecTypeFromTL would deref it
            }
            copied as *mut pg_sys::Expr
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
                        } else if pa.kind == 9
                            && !av.is_null()
                            && (*av).type_ == pg_sys::NodeTag::T_OpExpr
                        {
                            // M166 — the SumIntAddConst argument is `OpExpr(Var(base int2), Const)`. Its nested Var is
                            // OUTER_VAR post-set_plan_refs (into the subtree we dropped); leaving it would make
                            // ruleutils' deparse of the aggregate argument follow OUTER_VAR through this scanrelid=0
                            // node (the M131 #135 EXPLAIN hazard). Replace the whole argument with a fresh base-rel Var
                            // of the base column's type — resolvable and descriptor-equal (exprType(Aggref)=int8 is
                            // unchanged); EXPLAIN then shows `sum(<col>)` (cosmetic — the `+ k` is folded into the
                            // executed sum, byte-identical, same convention as the group-expr slot's base-column deparse).
                            let inner = PgList::<pg_sys::Node>::from_pg(
                                (*(av as *mut pg_sys::OpExpr)).args,
                            );
                            let Some(iv) = inner.get_ptr(0) else { return std::ptr::null_mut() };
                            if (*iv).type_ != pg_sys::NodeTag::T_Var {
                                return std::ptr::null_mut(); // admit guaranteed a Var arg; otherwise decline the swap
                            }
                            let v = iv as *mut pg_sys::Var;
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
/// `[table_oid, mode, nagg, (kind,attno,delta_hi,delta_lo)×nagg, npred, (col,op,hi,lo)×npred, ngroup,
///  (attno,typoid)×ngroup, noutput, (kind,idx)×noutput]`. (M166 — `delta` is the SumIntAddConst offset, 0 otherwise.)
unsafe fn encode_private(adm: &Admitted, table_oid: u32) -> *mut pg_sys::List {
    let mut pl = pg_sys::lappend_int(std::ptr::null_mut(), table_oid as i32);
    pl = pg_sys::lappend_int(pl, adm.mode);
    pl = pg_sys::lappend_int(pl, adm.aggs.len() as i32);
    for a in &adm.aggs {
        pl = pg_sys::lappend_int(pl, a.kind);
        pl = pg_sys::lappend_int(pl, a.attno);
        // M166 — delta (SumIntAddConst offset, kind 9; 0 otherwise) split hi/lo i32 like the IN-list/const-out words.
        pl = pg_sys::lappend_int(pl, (a.delta >> 32) as i32);
        pl = pg_sys::lappend_int(pl, (a.delta & 0xFFFF_FFFF) as i32);
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
    // M165 — const-out block (layout kind=3): [nconst, (val_hi, val_lo, typoid)×nconst]. Rides the int channel (a
    // projected integer literal is varlena-free); the i64 value is split hi/lo i32 exactly like the IN-list/delta
    // encodings (a `List` Integer is i32). Appended LAST so a pre-M165 decoder that stops after the layout block
    // round-trips as zero const-outs (the exec-side `if i < n` guard treats absence as nconst 0).
    pl = pg_sys::lappend_int(pl, adm.const_outs.len() as i32);
    for &(val, typoid) in &adm.const_outs {
        pl = pg_sys::lappend_int(pl, (val >> 32) as i32);
        pl = pg_sys::lappend_int(pl, (val & 0xFFFF_FFFF) as i32);
        pl = pg_sys::lappend_int(pl, typoid as i32);
    }
    pl
}

/// M156 — encode the text predicates as the 2nd `custom_private` channel (ADR-1): a `List` whose members are each
/// `[Integer(col), Integer(op), String(needle)]` — all leaf `Value` nodes, so `copyObject`/out/read round-trip the
/// varlena-free way (the int-only channel cannot carry a string). Returns `NIL` when there are no text predicates
/// (decode treats NIL as zero). Returns `None` only if a needle carries an interior NUL (impossible for text values)
/// — the caller then declines the swap rather than shipping a filter that silently drops a predicate.
unsafe fn encode_text_preds(tpreds: &[TextPredicate]) -> Option<*mut pg_sys::List> {
    let mut outer: *mut pg_sys::List = std::ptr::null_mut();
    for t in tpreds {
        let cneedle = std::ffi::CString::new(t.needle.as_str()).ok()?; // interior NUL → decline (never for text)
        let pgstr = pg_sys::pstrdup(cneedle.as_ptr()); // copy into the current (planner) memory context
        let mut entry: *mut pg_sys::List = std::ptr::null_mut();
        entry = pg_sys::lappend(entry, pg_sys::makeInteger(t.col as i32) as *mut c_void);
        entry = pg_sys::lappend(entry, pg_sys::makeInteger(t.op as i32) as *mut c_void);
        entry = pg_sys::lappend(entry, pg_sys::makeString(pgstr) as *mut c_void);
        outer = pg_sys::lappend(outer, entry as *mut c_void);
    }
    Some(outer)
}

/// M157/M161 — encode the expression group keys as the 3rd `custom_private` channel (ADR-1, natural extension of the
/// M156 text channel): a `List` whose members are each
/// `[Integer(base_attno), Integer(func), String(unit), Integer(out_typoid), Integer(delta_hi), Integer(delta_lo)]`
/// — all leaf `Value` nodes (copy/out/read-safe). `delta` (IntAddConst offset) is split hi/lo i32 (a `Value` Integer is
/// i32) exactly like the IN-list channel. `NIL` when there are no group exprs (decode → zero).
unsafe fn encode_group_exprs(gexprs: &[GroupExprSpec]) -> Option<*mut pg_sys::List> {
    let mut outer: *mut pg_sys::List = std::ptr::null_mut();
    for g in gexprs {
        // `unit` is a lowercase ASCII granularity from a validated whitelist (or "" for int±k/const) — never contains
        // an interior NUL. Still, decline fail-closed on one (council-rust-pgrx LOW: symmetry with `encode_text_preds`).
        let cs = std::ffi::CString::new(g.unit.as_str()).ok()?;
        let pgstr = pg_sys::pstrdup(cs.as_ptr());
        let mut entry: *mut pg_sys::List = std::ptr::null_mut();
        entry = pg_sys::lappend(entry, pg_sys::makeInteger(g.base_attno) as *mut c_void);
        entry = pg_sys::lappend(entry, pg_sys::makeInteger(g.func as i32) as *mut c_void);
        entry = pg_sys::lappend(entry, pg_sys::makeString(pgstr) as *mut c_void);
        entry = pg_sys::lappend(entry, pg_sys::makeInteger(g.out_typoid as i32) as *mut c_void);
        entry = pg_sys::lappend(entry, pg_sys::makeInteger((g.delta >> 32) as i32) as *mut c_void);
        entry = pg_sys::lappend(
            entry,
            pg_sys::makeInteger((g.delta & 0xFFFF_FFFF) as i32) as *mut c_void,
        );
        outer = pg_sys::lappend(outer, entry as *mut c_void);
    }
    Some(outer)
}

/// M161 — encode the integer IN-list predicates as the 4th `custom_private` channel: a `List` whose members are each
/// `[Integer(col), Integer(n), Integer(c0_hi), Integer(c0_lo), …]` — the consts as hi/lo i32 halves (a `Value` Integer
/// is i32, so an int8 const needs two words, exactly like `encode_private`'s `const_bits`). All leaf `Value` nodes →
/// copy/out/read-safe (varlena-free), same contract as `encode_text_preds`. `NIL` when there are no IN-list predicates.
unsafe fn encode_in_preds(inpreds: &[super::zonemap::InListPredicate]) -> *mut pg_sys::List {
    let mut outer: *mut pg_sys::List = std::ptr::null_mut();
    for p in inpreds {
        let mut entry: *mut pg_sys::List = std::ptr::null_mut();
        entry = pg_sys::lappend(entry, pg_sys::makeInteger(p.col as i32) as *mut c_void);
        entry = pg_sys::lappend(entry, pg_sys::makeInteger(p.consts.len() as i32) as *mut c_void);
        for &c in &p.consts {
            entry = pg_sys::lappend(entry, pg_sys::makeInteger((c >> 32) as i32) as *mut c_void);
            entry = pg_sys::lappend(
                entry,
                pg_sys::makeInteger((c & 0xFFFF_FFFF) as i32) as *mut c_void,
            );
        }
        outer = pg_sys::lappend(outer, entry as *mut c_void);
    }
    outer
}

/// If `plan` is an `Agg` over a columnar table matching an unconsumed stash entry, build the replacement `CustomScan`
/// (plain-Var tlist, scanrelid=0, custom_private from the stash) with the same output shape; else `None`.
unsafe fn try_swap_agg(
    plan: *mut pg_sys::Plan,
    rtable: *mut pg_sys::List,
    parent: *mut pg_sys::Plan,
) -> Option<*mut pg_sys::Plan> {
    let agg = plan as *mut pg_sys::Agg;
    // B1 (review): only a SIMPLE (non-split) aggregate carries the FINAL result. A parallel plan splits into
    // Finalize(SIMPLE)→Gather→Partial(INITIAL_SERIAL)→ParallelSeqScan; swapping the Partial would emit the FINAL value
    // where a partial transvalue is expected → wrong result. Decline any non-SIMPLE split.
    if (*agg).aggsplit != pg_sys::AggSplit::AGGSPLIT_SIMPLE {
        admit_trace("swap_agg_split_nonsimple"); // M152
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
        admit_trace("swap_unsupported_agg_strategy"); // M152
        return None;
    }
    let scanrelid = find_scan_relid((*agg).plan.lefttree)?;
    let scan_rte = pg_sys::list_nth(rtable, (scanrelid - 1) as i32) as *mut pg_sys::RangeTblEntry;
    if scan_rte.is_null() {
        admit_trace("swap_scan_rte_null"); // M152
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
                    // M157 — total grouping keys = bare columns + expression keys (date_trunc) must match numCols.
                    && e.adm.group_cols.len() + e.adm.group_exprs.len() == numcols
                    && e.adm.expected_arity() == out_arity
            })
            .map(|e| {
                e.consumed = true;
                e.adm.clone()
            })
    })?;
    // B2 (review): a SORTED GroupAgg is only swappable when our ASC-nulls-last group sort reproduces its output order.
    if strat == pg_sys::AggStrategy::AGG_SORTED {
        // Text keys (M153): our executor emits groups in byte-wise ASC order, which ≠ PG's collation order — so we
        // CANNOT reproduce the AGG_SORTED promised (collation) output order for text. It is correct ONLY when a full
        // `Sort` above re-sorts the whole output (then our emit order is irrelevant). Grouping-equality correctness is
        // already guaranteed at admit time (deterministic-collation guard). So: text AGG_SORTED is swappable iff the
        // parent is a plain `Sort` (NOT IncrementalSort, which relies on input pre-sortedness). Else decline.
        if adm.group_cols.iter().any(|&(_, t)| matches!(t, 25 | 1043)) {
            // Text (text/varchar; bpchar excluded at admit — review MEDIUM). Safe ONLY when a full `Sort` above
            // re-sorts the output (group order then irrelevant). Fall
            // through to the swap WITHOUT the numeric ASC-nulls-last input-Sort check (we don't reproduce key order).
            if parent.is_null() || (*parent).type_ != pg_sys::NodeTag::T_Sort {
                admit_trace("swap_sorted_text_group_not_resorted"); // M153 — direct group-order consumption
                return None;
            }
        } else {
            // Numeric/temporal: reproduce the promised order exactly. The input Sort must be ASC nulls-last (else the
            // plan's output order isn't our ASC order).
            let child = (*agg).plan.lefttree;
            if child.is_null() || (*child).type_ != pg_sys::NodeTag::T_Sort {
                return None;
            }
            let s = child as *mut pg_sys::Sort;
            for i in 0..(*s).numCols as usize {
                if *(*s).nullsFirst.add(i) {
                    admit_trace("swap_agg_sorted_nulls_first"); // M152
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
                pg_sys::get_ordering_op_properties(
                    opno,
                    &mut opfamily,
                    &mut opcintype,
                    &mut cmptype,
                );
                if cmptype != pg_sys::CompareType::COMPARE_LT {
                    admit_trace("swap_agg_sorted_desc_or_nonbtree"); // M152
                    return None; // DESC (or non-btree) ≠ our ascending
                }
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
    // M156/M157/M161 — custom_private is a 4-channel outer List:
    //   [<numeric IntList>, <text-preds List>, <group-exprs List>, <in-list List>].
    // The int-only channel (M115 layout) cannot carry a varlena, so text predicates (M156), expression group keys
    // (M157), and integer IN-list predicates (M161, hi/lo i32 halves) ride parallel node channels (ADR-1). A text
    // needle with an interior NUL (impossible for a text value) → decline the swap rather than ship an incomplete filter.
    let int_channel = encode_private(&adm, table_oid);
    let text_channel = encode_text_preds(&adm.text_preds)?;
    let group_expr_channel = encode_group_exprs(&adm.group_exprs)?;
    let in_channel = encode_in_preds(&adm.in_preds); // M161 — 4th channel (integer IN-list)
    let mut outer = pg_sys::lappend(std::ptr::null_mut(), int_channel as *mut c_void);
    outer = pg_sys::lappend(outer, text_channel as *mut c_void);
    outer = pg_sys::lappend(outer, group_expr_channel as *mut c_void);
    outer = pg_sys::lappend(outer, in_channel as *mut c_void);
    cscan.custom_private = outer;
    // M131 (#135): NOT `plain_var_tlist` — a self-referential INDEX_VAR here makes ruleutils' `resolve_special_varno`
    // recurse forever when a Sort above this node has a key on the aggregate output, hanging EXPLAIN. This list also
    // becomes the node's RUNTIME scan TupleDesc (`ExecTypeFromTL`), so it must stay descriptor-equal to
    // `plan.targetlist` — see `deparse_safe_tlist`. NIL means it could not be built consistently → decline the swap
    // and let the native plan run (fail-closed; never ship a short descriptor).
    let safe_tlist = deparse_safe_tlist(tlist, &adm, scanrelid);
    if safe_tlist.is_null() || pg_sys::list_length(safe_tlist) as usize != out_arity {
        admit_trace("swap_deparse_safe_tlist_sort_on_agg"); // M152 — Sort/ORDER-BY on the agg output (M131 #135)
        return None;
    }
    cscan.custom_scan_tlist = safe_tlist;
    cscan.custom_relids = std::ptr::null_mut();
    cscan.methods = &SCAN_METHODS.0;
    Some(cscan.into_pg() as *mut pg_sys::Plan)
}

/// M158 — a projected output column: base attno + type OID. Both the columnar decoder (`build_arrow`) and the
/// materializer (`arrow_value_to_datum`) must support the type, else the top-k run would `error!` at runtime instead
/// of declining — so this guard is a swap-time fail-closed gate (the intersection of both supported sets).
fn topk_type_supported(typoid: u32) -> bool {
    matches!(typoid, 21 | 23 | 20 | 700 | 701 | 16 | 25 | 1042 | 1043 | 1114 | 1184 | 1082)
}

/// M158 — encode the top-k `custom_private` int channel (mode == 2, distinct from the agg layouts).
/// M167 generalized the single sort key to N keys:
/// `[relid, 2, k, nkeys, (attno,asc,nulls_first)×nkeys, nproj, (attno,typoid)×nproj, npred, (col,op,hi,lo)×npred]`.
/// Text predicates ride the shared 2nd channel (`encode_text_preds`); the 3rd channel is empty (NIL) for top-k.
unsafe fn encode_topk_private(
    table_oid: u32,
    k: usize,
    sort_keys: &[(i32, bool, bool)], // (attno, ascending, nulls_first) — in ORDER BY position
    proj_cols: &[(i32, u32)],
    preds: &[ZonePredicate],
) -> *mut pg_sys::List {
    let mut pl = pg_sys::lappend_int(std::ptr::null_mut(), table_oid as i32);
    pl = pg_sys::lappend_int(pl, 2); // mode = 2 (top-k)
    pl = pg_sys::lappend_int(pl, k as i32);
    pl = pg_sys::lappend_int(pl, sort_keys.len() as i32);
    for &(attno, asc, nf) in sort_keys {
        pl = pg_sys::lappend_int(pl, attno);
        pl = pg_sys::lappend_int(pl, asc as i32);
        pl = pg_sys::lappend_int(pl, nf as i32);
    }
    pl = pg_sys::lappend_int(pl, proj_cols.len() as i32);
    for &(attno, typoid) in proj_cols {
        pl = pg_sys::lappend_int(pl, attno);
        pl = pg_sys::lappend_int(pl, typoid as i32);
    }
    pl = pg_sys::lappend_int(pl, preds.len() as i32);
    for p in preds {
        pl = pg_sys::lappend_int(pl, p.col as i32);
        pl = pg_sys::lappend_int(pl, p.op as i32);
        pl = pg_sys::lappend_int(pl, (p.const_bits >> 32) as i32);
        pl = pg_sys::lappend_int(pl, (p.const_bits & 0xFFFF_FFFF) as i32);
    }
    pl
}

/// M158 — detect `Limit(k) → Sort([single key]) → CustomScan(theodb_columnar_project over a columnar rel)` and swap
/// the **Sort** for a late-materialization top-k CustomScan (the Limit above is preserved and re-applies k). The node
/// reuses the agg CustomScan framework (`SCAN_METHODS`, exec/end/rescan) via `mode == 2`: `begin_custom_scan` decodes
/// {key ∪ filter} for all rows, runs DataFusion `filter → sort → limit(k)`, and materializes the full projection only
/// for the k survivors — paying the M148 `form_row`/`palloc` cost for k rows, not N. Returns `None` (keep the native
/// plan) for ANY unsupported shape (fail-closed). Gated by `theodb.enable_columnar_late_mat` (default ON since M167).
unsafe fn try_swap_topk(
    plan: *mut pg_sys::Plan,
    rtable: *mut pg_sys::List,
    parent: *mut pg_sys::Plan,
) -> Option<*mut pg_sys::Plan> {
    if !ENABLE_COLUMNAR_LATE_MAT.get() {
        return None;
    }
    admit_trace("topk_entered_sort"); // M158 diag — reached a Sort under the late-mat GUC
    // Parent must be a plain LIMIT k with no OFFSET (OFFSET would need the top k+offset — out of scope).
    if parent.is_null() || (*parent).type_ != pg_sys::NodeTag::T_Limit {
        admit_trace("topk_parent_not_limit");
        return None;
    }
    let limit = parent as *mut pg_sys::Limit;
    if !(*limit).limitOffset.is_null() {
        return None;
    }
    let lc = (*limit).limitCount;
    if lc.is_null() || (*lc).type_ != pg_sys::NodeTag::T_Const {
        return None; // non-constant LIMIT (param/expr) → decline
    }
    let lc_const = lc as *mut pg_sys::Const;
    if (*lc_const).constisnull || (*lc_const).consttype.to_u32() != 20 {
        return None; // LIMIT ALL (NULL) or non-int8 → decline
    }
    let k_i64 = i64::from_datum((*lc_const).constvalue, false)?;
    // k must be positive AND fit i32: it is serialized into the int-only `custom_private` channel as one i32
    // (`lappend_int`), so a LIMIT ≥ 2^31 would truncate to a wrong/negative k → too-few rows the parent Limit cannot
    // add back (council-index-storage + council-rust-pgrx LOW). Such a limit is absurd (O(N) memory would OOM first);
    // decline to the native plan (correct for any k) rather than risk a silent short result.
    if k_i64 <= 0 || k_i64 > i32::MAX as i64 {
        return None;
    }
    let k = k_i64 as usize;

    // This node is the Sort. M167: the key count is validated per-key below (ADR-3) — M158's `numCols != 1` was a
    // scope limit, not a safety property.
    let sort = plan as *mut pg_sys::Sort;
    // Grandchild must be a theodb_columnar_project CustomScan (M149) over a columnar rel.
    let child = (*sort).plan.lefttree;
    if child.is_null() || (*child).type_ != pg_sys::NodeTag::T_CustomScan {
        return None;
    }
    let proj = child as *mut pg_sys::CustomScan;
    let mname = (*(*proj).methods).CustomName;
    if mname.is_null() || CStr::from_ptr(mname) != c"theodb_columnar_project" {
        return None;
    }
    let scanrelid = (*proj).scan.scanrelid;
    if scanrelid == 0 {
        return None;
    }
    // The projection = the project node's own targetlist (base-rel Vars). The top-k node (replacing the Sort) emits the
    // SAME columns in the SAME order (Sort only re-orders rows), so this is descriptor-equal to the Sort's output.
    let src = PgList::<pg_sys::TargetEntry>::from_pg((*proj).scan.plan.targetlist);
    if src.is_empty() {
        return None;
    }
    let mut proj_meta: Vec<(i32, u32)> = Vec::with_capacity(src.len());
    let mut cst: *mut pg_sys::List = std::ptr::null_mut(); // custom_scan_tlist (fresh base Vars)
    for i in 0..src.len() {
        let te = src.get_ptr(i)?;
        let e = (*te).expr as *mut pg_sys::Node;
        if e.is_null() || (*e).type_ != pg_sys::NodeTag::T_Var {
            return None; // a computed target expr → cannot materialize as a column (fail-closed)
        }
        let v = e as *mut pg_sys::Var;
        if (*v).varno as u32 != scanrelid {
            return None; // not a base column of the scanned rel
        }
        let attno = (*v).varattno as i32;
        if attno <= 0 {
            return None; // system / whole-row col → decline
        }
        let typoid = (*v).vartype.to_u32();
        if !topk_type_supported(typoid) {
            return None; // build_arrow / arrow_value_to_datum cannot handle it → decline
        }
        proj_meta.push((attno, typoid));
        let nv = pg_sys::makeVar(
            scanrelid as i32,
            attno as pg_sys::AttrNumber,
            (*v).vartype,
            (*v).vartypmod,
            (*v).varcollid,
            0,
        );
        let nte = pg_sys::makeTargetEntry(
            nv as *mut pg_sys::Expr,
            (i + 1) as pg_sys::AttrNumber,
            (*te).resname,
            (*te).resjunk,
        );
        cst = pg_sys::lappend(cst, nte as *mut c_void);
    }
    // Sort keys (M167 ADR-3 — generalized from M158's single key). `sortColIdx[i]` is a 1-based resno into the
    // Sort's tlist, positionally aligned with the project's tlist (Sort passes columns through unchanged). EVERY key
    // must pass EVERY guard: the checks below are per-key properties, not a scope limit, so a multi-key sort is
    // admissible exactly when each of its keys is. Fail-closed — one bad key declines the whole swap.
    let nkeys = (*sort).numCols as usize;
    if nkeys == 0 || nkeys > TOPK_MAX_SORT_KEYS {
        admit_trace("topk_sort_key_count_out_of_range");
        return None;
    }
    let mut sort_keys: Vec<(i32, bool, bool)> = Vec::with_capacity(nkeys);
    for ki in 0..nkeys {
        let key_pos = *(*sort).sortColIdx.add(ki) as usize;
        if key_pos == 0 || key_pos > proj_meta.len() {
            return None;
        }
        let (attno, key_type) = proj_meta[key_pos - 1];
        // Text sort key: DataFusion byte-sorts Utf8, which matches PG ORDER BY only under a BYTE-order collation.
        // A merely DETERMINISTIC collation (e.g. en_US.UTF-8) is NOT enough: determinism fixes equality (byte-equal
        // ⟺ string-equal, the M153/M154 grouping/LIKE guarantee) but NOT sort order — en_US sorts linguistically
        // ('a' < 'Z') while bytes sort 'Z'(0x5A) < 'a'(0x61) → a silently WRONG top-k row (council-index-storage
        // HIGH). Use the SORT's effective collation for THIS key (`sort.collations[ki]`), not the column's
        // varcollid — `ORDER BY s COLLATE "C"` overrides the column collation and the override lives on the Sort.
        // (Text FILTER predicates keep the determinism guard — filter equality, not order — unchanged.)
        if matches!(key_type, 25 | 1043) {
            let sort_coll = (*(*sort).collations.add(ki)).to_u32();
            if !sort_collation_is_byte_order(sort_coll) {
                admit_trace("topk_text_key_non_byte_collation");
                return None;
            }
        } else if key_type == 1042 {
            return None; // bpchar sort key → PG trims trailing blanks; DataFusion does not → decline
        }
        // Direction from this key's sort operator (M135: PG18 CompareType port; COMPARE_LT == ascending).
        let opno = *(*sort).sortOperators.add(ki);
        let (mut opfamily, mut opcintype, mut cmptype) =
            (pg_sys::InvalidOid, pg_sys::InvalidOid, pg_sys::CompareType::COMPARE_INVALID);
        pg_sys::get_ordering_op_properties(opno, &mut opfamily, &mut opcintype, &mut cmptype);
        let ascending = match cmptype {
            pg_sys::CompareType::COMPARE_LT => true,
            pg_sys::CompareType::COMPARE_GT => false,
            _ => return None, // non-btree ordering operator → decline
        };
        sort_keys.push((attno, ascending, *(*sort).nullsFirst.add(ki)));
    }
    // Predicates: the project node applies the WHERE via its own qual (a List of final clauses whose Vars are base
    // Vars with varno == scanrelid). Every clause MUST be pushable (zone or text) — else decline, because the top-k
    // node OWNS the filter (the project subtree is dropped).
    let qual = PgList::<pg_sys::Node>::from_pg((*proj).scan.plan.qual);
    let mut zpreds: Vec<ZonePredicate> = Vec::new();
    let mut tpreds: Vec<TextPredicate> = Vec::new();
    for i in 0..qual.len() {
        let clause = qual.get_ptr(i)?;
        if let Some(z) = extract_zone_predicate(clause, scanrelid as i32) {
            zpreds.push(z);
        } else {
            // idem ao pushdown de agregacao acima: `?` declina o pushdown sem mudar o efeito.
            tpreds.push(extract_text_predicate(clause, scanrelid as i32)?);
        }
    }
    // Resolve the table OID from the RTE (the project's scanrelid indexes the flat range table).
    let scan_rte = pg_sys::rt_fetch(scanrelid, rtable);
    if scan_rte.is_null() {
        return None;
    }
    let table_oid = (*scan_rte).relid.to_u32();
    // M167 ADR-4 / EC-1 — bound the O(N) decode. `run_columnar_topk` decodes {projection ∪ keys ∪ filter} for ALL
    // rows into one Arrow batch BEFORE the bounded-heap TopK, so this path costs O(N) memory where the native top-N
    // heapsort costs O(k). M158 mitigated that by defaulting the GUC OFF; M167 flipped it ON, so the bound lives
    // here. Fail-closed: declining falls back to the native plan, correct for any input.
    let est_decode_bytes = relation_physical_bytes(table_oid);
    let work_mem_bytes = f64::from(pg_sys::work_mem.max(64)) * 1024.0;
    if est_decode_bytes > work_mem_bytes * TOPK_DECODE_WORK_MEM_FACTOR {
        if admit_trace_enabled() {
            admit_trace(&format!(
                "topk_decode_estimate_too_large est_bytes={est_decode_bytes:.0} budget={:.0}",
                work_mem_bytes * TOPK_DECODE_WORK_MEM_FACTOR
            ));
        }
        return None;
    }

    // Build the replacement CustomScan (scanrelid = 0 — it scans the columnar rel by OID, like the agg node).
    let mut cscan = PgBox::<pg_sys::CustomScan>::alloc_node(pg_sys::NodeTag::T_CustomScan);
    {
        let plan_out = &mut cscan.scan.plan;
        plan_out.targetlist = plain_var_tlist((*proj).scan.plan.targetlist);
        plan_out.qual = std::ptr::null_mut();
        plan_out.lefttree = std::ptr::null_mut(); // drop the project subtree — the CustomScan scans itself
        plan_out.righttree = std::ptr::null_mut();
        plan_out.plan_node_id = (*sort).plan.plan_node_id;
        plan_out.startup_cost = (*sort).plan.startup_cost;
        plan_out.total_cost = (*sort).plan.total_cost;
        plan_out.plan_rows = k_i64 as f64;
        plan_out.plan_width = (*sort).plan.plan_width;
        plan_out.parallel_aware = false;
        plan_out.parallel_safe = (*sort).plan.parallel_safe;
    }
    cscan.scan.scanrelid = 0;
    cscan.flags = 0;
    cscan.custom_plans = std::ptr::null_mut();
    cscan.custom_exprs = std::ptr::null_mut();
    let int_channel = encode_topk_private(table_oid, k, &sort_keys, &proj_meta, &zpreds);
    let text_channel = encode_text_preds(&tpreds)?;
    let mut outer = pg_sys::lappend(std::ptr::null_mut(), int_channel as *mut c_void);
    outer = pg_sys::lappend(outer, text_channel as *mut c_void);
    outer = pg_sys::lappend(outer, std::ptr::null_mut()); // 3rd channel unused for top-k
    outer = pg_sys::lappend(outer, std::ptr::null_mut()); // 4th channel (IN-list) unused for top-k (M161)
    cscan.custom_private = outer;
    cscan.custom_scan_tlist = cst;
    cscan.custom_relids = std::ptr::null_mut();
    cscan.methods = &SCAN_METHODS.0;
    admit_trace("swap_topk_admitted"); // M158
    Some(cscan.into_pg() as *mut pg_sys::Plan)
}

/// Walk the plan tree via a mutable node slot, swapping matching `Agg` nodes → our `CustomScan` in place.
/// `parent` is the enclosing plan node (NULL at the root) — `try_swap_agg` uses it to check, for an AGG_SORTED
/// text GROUP BY, whether the output is re-sorted by a `Sort` above (M153).
unsafe fn swap_walk(
    slot: *mut *mut pg_sys::Plan,
    rtable: *mut pg_sys::List,
    parent: *mut pg_sys::Plan,
) {
    let plan = *slot;
    if plan.is_null() {
        return;
    }
    if (*plan).type_ == pg_sys::NodeTag::T_Agg {
        if let Some(newnode) = try_swap_agg(plan, rtable, parent) {
            *slot = newnode;
            return; // replaced — the Agg's child subtree is dropped
        }
    }
    // M158 — a `Sort` under a `Limit(k)` over the columnar-project scan → late-materialization top-k CustomScan.
    if (*plan).type_ == pg_sys::NodeTag::T_Sort {
        if let Some(newnode) = try_swap_topk(plan, rtable, parent) {
            *slot = newnode;
            return; // replaced — the Sort's project subtree is dropped; the Limit above re-applies k
        }
    }
    swap_walk(&mut (*plan).lefttree, rtable, plan);
    swap_walk(&mut (*plan).righttree, rtable, plan);
    match (*plan).type_ {
        pg_sys::NodeTag::T_Append => {
            swap_walk_list((*(plan as *mut pg_sys::Append)).appendplans, rtable, plan)
        }
        pg_sys::NodeTag::T_MergeAppend => {
            swap_walk_list((*(plan as *mut pg_sys::MergeAppend)).mergeplans, rtable, plan)
        }
        pg_sys::NodeTag::T_SubqueryScan => {
            swap_walk(&mut (*(plan as *mut pg_sys::SubqueryScan)).subplan, rtable, plan)
        }
        _ => {}
    }
}

/// Walk a List of child plans with mutable slots (Append/MergeAppend members).
unsafe fn swap_walk_list(
    list: *mut pg_sys::List,
    rtable: *mut pg_sys::List,
    parent: *mut pg_sys::Plan,
) {
    if list.is_null() {
        return;
    }
    let n = (*list).length;
    for i in 0..n {
        let cell = (*list).elements.add(i as usize);
        swap_walk(&mut (*cell).ptr_value as *mut _ as *mut *mut pg_sys::Plan, rtable, parent);
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
    // M156/M157 — custom_private is the 3-channel outer List `[int_channel, text_channel, group_expr_channel]` (a
    // T_List). A flat IntList (T_IntList) — a legacy/channel-less encode — is read directly with zero text preds /
    // group exprs (backward compatible). The 3rd channel is read via list_length (absent → NIL → zero group exprs).
    let raw_priv = (*cscan).custom_private;
    let (priv_list, text_list, group_expr_list, in_list): (
        *mut pg_sys::List,
        *mut pg_sys::List,
        *mut pg_sys::List,
        *mut pg_sys::List,
    ) = if !raw_priv.is_null()
        && (*(raw_priv as *mut pg_sys::Node)).type_ == pg_sys::NodeTag::T_List
    {
        let outer_len = pg_sys::list_length(raw_priv);
        (
            pg_sys::list_nth(raw_priv, 0) as *mut pg_sys::List,
            pg_sys::list_nth(raw_priv, 1) as *mut pg_sys::List,
            if outer_len >= 3 {
                pg_sys::list_nth(raw_priv, 2) as *mut pg_sys::List
            } else {
                std::ptr::null_mut()
            },
            if outer_len >= 4 {
                pg_sys::list_nth(raw_priv, 3) as *mut pg_sys::List // M161 — 4th channel (integer IN-list)
            } else {
                std::ptr::null_mut()
            },
        )
    } else {
        (raw_priv, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut())
    };
    let n = pg_sys::list_length(priv_list);
    // M115 layout: [table_oid, mode, nagg, ...]. The base table is resolved by its stable pg_class OID (the Agg-swap
    // dropped the child scan, so there is no scanrelid to index es_range_table).
    let relid = pg_sys::Oid::from_u32_unchecked(pg_sys::list_nth_int(priv_list, 0) as u32);
    let mode = pg_sys::list_nth_int(priv_list, 1);

    // Materialize the result rows in the durable per-query context so text/varlena GROUP BY key datums survive across
    // exec() calls (ADR-3). By-value datums (int8/float8/date/timestamptz) are context-independent.
    let oldcxt = pg_sys::MemoryContextSwitchTo((*estate).es_query_cxt);
    let res = (|| -> Result<Vec<Vec<(pg_sys::Datum, bool)>>, String> {
        // M158 — top-k late materialization (mode == 2). Distinct IntList layout keyed by mode:
        // [relid, 2, k, nkeys, (attno,asc,nf)×nkeys, nproj, (attno,typoid)×nproj, npred, (col,op,hi,lo)×npred].
        if mode == 2 {
            let k = pg_sys::list_nth_int(priv_list, 2) as usize;
            let nkeys = pg_sys::list_nth_int(priv_list, 3) as usize;
            let mut j = 4;
            let mut sort_keys: Vec<(String, bool, bool)> = Vec::with_capacity(nkeys);
            for _ in 0..nkeys {
                let attno = pg_sys::list_nth_int(priv_list, j);
                let asc = pg_sys::list_nth_int(priv_list, j + 1) != 0;
                let nf = pg_sys::list_nth_int(priv_list, j + 2) != 0;
                j += 3;
                let nm = pg_sys::get_attname(relid, attno as pg_sys::AttrNumber, false);
                sort_keys.push((CStr::from_ptr(nm).to_string_lossy().into_owned(), asc, nf));
            }
            let nproj = pg_sys::list_nth_int(priv_list, j) as usize;
            j += 1;
            let mut proj_cols: Vec<(String, u32)> = Vec::with_capacity(nproj);
            for _ in 0..nproj {
                let attno = pg_sys::list_nth_int(priv_list, j);
                let typoid = pg_sys::list_nth_int(priv_list, j + 1) as u32;
                j += 2;
                let nm = pg_sys::get_attname(relid, attno as pg_sys::AttrNumber, false);
                proj_cols.push((CStr::from_ptr(nm).to_string_lossy().into_owned(), typoid));
            }
            let np = if j < n { pg_sys::list_nth_int(priv_list, j) as usize } else { 0 };
            j += 1;
            let mut tk_preds = Vec::with_capacity(np);
            for _ in 0..np {
                let col = pg_sys::list_nth_int(priv_list, j) as usize;
                let opn = pg_sys::list_nth_int(priv_list, j + 1);
                let hi = pg_sys::list_nth_int(priv_list, j + 2) as u32 as u64;
                let lo = pg_sys::list_nth_int(priv_list, j + 3) as u32 as u64;
                j += 4;
                let op = match opn {
                    0 => ZoneOp::Lt,
                    1 => ZoneOp::Le,
                    2 => ZoneOp::Eq,
                    3 => ZoneOp::Ge,
                    4 => ZoneOp::Gt,
                    5 => ZoneOp::Ne,
                    _ => return Err(format!("columnar_topk: bad zone op {opn}")),
                };
                tk_preds.push(ZonePredicate { col, op, const_bits: (hi << 32) | lo });
            }
            // Text predicates from the shared 2nd channel (same leaf-Value layout as the agg path).
            let mut tk_text: Vec<TextPredicate> = Vec::new();
            let tn = pg_sys::list_length(text_list);
            for kk in 0..tn {
                let entry = pg_sys::list_nth(text_list, kk) as *mut pg_sys::List;
                let col = (*(pg_sys::list_nth(entry, 0) as *mut pg_sys::Integer)).ival as usize;
                let opn = (*(pg_sys::list_nth(entry, 1) as *mut pg_sys::Integer)).ival;
                let sval = (*(pg_sys::list_nth(entry, 2) as *mut pg_sys::String)).sval;
                let op = match opn {
                    0 => TextOp::Eq,
                    1 => TextOp::Ne,
                    2 => TextOp::Like,
                    3 => TextOp::NotLike,
                    _ => return Err(format!("columnar_topk: bad text op {opn}")),
                };
                tk_text.push(TextPredicate {
                    col,
                    op,
                    needle: CStr::from_ptr(sval).to_string_lossy().into_owned(),
                });
            }
            let rel = pg_sys::relation_open(relid, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            let r = super::df_executor::run_columnar_topk(
                rel,
                &proj_cols,
                &sort_keys,
                k,
                &tk_preds,
                &tk_text,
                &[], // M161 — top-k routing does not admit IN-list predicates
                super::guc::columnar_zonemap_skip(),
            );
            pg_sys::relation_close(rel, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            return r;
        }
        // IntList: [mode, relid, nagg, (kind,attno,delta_hi,delta_lo)×nagg, npred, (col,op,hi,lo)×npred,
        //           ngroup, (attno,typoid)×ngroup, noutput, (kind,idx)×noutput].
        let nagg = pg_sys::list_nth_int(priv_list, 2) as usize;
        let mut specs = Vec::with_capacity(nagg);
        let mut i = 3;
        for _ in 0..nagg {
            let kind = pg_sys::list_nth_int(priv_list, i);
            let attno = pg_sys::list_nth_int(priv_list, i + 1);
            // M166 — delta (SumIntAddConst offset, kind 9; 0 for every other kind), reconstructed from its hi/lo i32
            // pair (a `List` Integer is i32, so an i64 rides two words — same split as the IN-list/const-out channels).
            let dhi = pg_sys::list_nth_int(priv_list, i + 2);
            let dlo = pg_sys::list_nth_int(priv_list, i + 3);
            let delta = ((dhi as i64) << 32) | (dlo as u32 as i64);
            i += 4;
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
                8 => specs.push(AggSpec::CountDistinct(col_name(attno))), // M154
                9 => specs.push(AggSpec::SumIntAddConst { col: col_name(attno), delta }), // M166
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
        // M156 — decode the text predicates from the 2nd channel (`text_list`; NIL → none). Each entry is a
        // `[Integer(col), Integer(op), String(needle)]` list of leaf Value nodes.
        let mut text_preds: Vec<TextPredicate> = Vec::new();
        let tn = pg_sys::list_length(text_list);
        for k in 0..tn {
            let entry = pg_sys::list_nth(text_list, k) as *mut pg_sys::List;
            let col = (*(pg_sys::list_nth(entry, 0) as *mut pg_sys::Integer)).ival as usize;
            let opn = (*(pg_sys::list_nth(entry, 1) as *mut pg_sys::Integer)).ival;
            let sval = (*(pg_sys::list_nth(entry, 2) as *mut pg_sys::String)).sval;
            let op = match opn {
                0 => TextOp::Eq,
                1 => TextOp::Ne,
                2 => TextOp::Like,
                3 => TextOp::NotLike,
                _ => return Err(format!("columnar_agg: bad text op {opn}")),
            };
            let needle = CStr::from_ptr(sval).to_string_lossy().into_owned();
            text_preds.push(TextPredicate { col, op, needle });
        }
        // M161 — decode the integer IN-list predicates from the 4th channel (`in_list`; NIL → none). Each entry is a
        // `[Integer(col), Integer(n), Integer(c0_hi), Integer(c0_lo), …]` list of leaf Value nodes; each i64 const is
        // reconstructed from its hi/lo i32 pair (makeInteger is i32, so encode_in_preds split every const in two).
        let mut in_preds: Vec<super::zonemap::InListPredicate> = Vec::new();
        let inn = pg_sys::list_length(in_list);
        for k in 0..inn {
            let entry = pg_sys::list_nth(in_list, k) as *mut pg_sys::List;
            let col = (*(pg_sys::list_nth(entry, 0) as *mut pg_sys::Integer)).ival as usize;
            let n = (*(pg_sys::list_nth(entry, 1) as *mut pg_sys::Integer)).ival as usize;
            let mut consts = Vec::with_capacity(n);
            for j in 0..n {
                let hi =
                    (*(pg_sys::list_nth(entry, (2 + j * 2) as i32) as *mut pg_sys::Integer)).ival;
                let lo = (*(pg_sys::list_nth(entry, (2 + j * 2 + 1) as i32)
                    as *mut pg_sys::Integer))
                    .ival;
                consts.push(((hi as i64) << 32) | (lo as u32 as i64));
            }
            in_preds.push(super::zonemap::InListPredicate { col, consts });
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
        // M165 — const-out block (layout kind=3): [nconst, (val_hi, val_lo, typoid)×nconst], appended after the layout
        // by encode_private. Absent in a pre-M165 IntList (i == n → nconst 0), so backward compatible. Each i64 value
        // is reconstructed from its hi/lo i32 pair (lappend_int is i32).
        let nconst = if i < n { pg_sys::list_nth_int(priv_list, i) as usize } else { 0 };
        i += 1;
        let mut const_outs: Vec<(i64, u32)> = Vec::with_capacity(nconst);
        for _ in 0..nconst {
            let hi = pg_sys::list_nth_int(priv_list, i);
            let lo = pg_sys::list_nth_int(priv_list, i + 1);
            let typoid = pg_sys::list_nth_int(priv_list, i + 2) as u32;
            i += 3;
            let val = ((hi as i64) << 32) | (lo as u32 as i64);
            const_outs.push((val, typoid));
        }
        // M157/M161 — decode the expression group keys from the 3rd channel; each entry is
        // [Integer(base_attno), Integer(func), String(unit), Integer(out_typoid), Integer(delta_hi), Integer(delta_lo)].
        // Resolve base_attno → column name. NIL → empty.
        let mut group_exprs: Vec<super::df_executor::GroupExprExec> = Vec::new();
        let gn = pg_sys::list_length(group_expr_list);
        for k in 0..gn {
            let entry = pg_sys::list_nth(group_expr_list, k) as *mut pg_sys::List;
            let base_attno = (*(pg_sys::list_nth(entry, 0) as *mut pg_sys::Integer)).ival;
            let func = (*(pg_sys::list_nth(entry, 1) as *mut pg_sys::Integer)).ival;
            let unit = CStr::from_ptr((*(pg_sys::list_nth(entry, 2) as *mut pg_sys::String)).sval)
                .to_string_lossy()
                .into_owned();
            let out_typoid = (*(pg_sys::list_nth(entry, 3) as *mut pg_sys::Integer)).ival as u32;
            let dhi = (*(pg_sys::list_nth(entry, 4) as *mut pg_sys::Integer)).ival;
            let dlo = (*(pg_sys::list_nth(entry, 5) as *mut pg_sys::Integer)).ival;
            let delta = ((dhi as i64) << 32) | (dlo as u32 as i64);
            let nm = pg_sys::get_attname(relid, base_attno as pg_sys::AttrNumber, false);
            let base_name = CStr::from_ptr(nm).to_string_lossy().into_owned();
            group_exprs.push(super::df_executor::GroupExprExec {
                base_name,
                func,
                unit,
                delta,
                out_typoid,
            });
        }

        if ngroup > 0 || !group_exprs.is_empty() {
            // GROUP BY (columnar only — admit declined grouped heap / grouped+WHERE). Multi-row result. M157: a
            // grouped query may have ONLY expression keys (bare group_cols empty) — still the grouped path.
            let rel = pg_sys::relation_open(relid, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            let r = run_columnar_grouped_aggs(
                rel,
                &group_cols,
                &group_exprs,
                &specs,
                &layout,
                &const_outs,
                &preds,
                &text_preds,
                &in_preds,
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
                // M156 — a text predicate (like a numeric one) forbids the directory fast-path: the min/max answer
                // must be filtered, and the zone directory holds no text filter. Fall through to the full scan.
                if preds.is_empty() && text_preds.is_empty() && in_preds.is_empty() && all_minmax {
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
                None => run_columnar_aggs(
                    rel,
                    &specs,
                    &preds,
                    &text_preds,
                    &in_preds,
                    super::guc::columnar_zonemap_skip(),
                )
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
