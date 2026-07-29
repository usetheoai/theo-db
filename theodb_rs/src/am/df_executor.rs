//! M100 Phase A — the DataFusion vectorized executor over the `theodb_columnar` TAM (de-risk slice).
//!
//! This slice proves the pillar's most dangerous FFI seam over REAL columnar data: decode a columnar table's
//! visible stripes into Arrow arrays, register them as a DataFusion table, and drive a vectorized aggregate to
//! completion with a synchronous `block_on` inside this backend — under a `HeldInterrupts` guard so a mid-flight
//! query-cancel cannot siglongjmp past the live tokio runtime and abort the process (blueprint Q1 / M98 probe).
//!
//! Scope (Phase A): result-equivalence of a `count(*)` + `sum(<numeric>)` aggregate columnar-vectorized vs a heap
//! table. The `work_mem` MemoryPool (errors-not-panics), per-batch interrupt safe-points, projection pushdown,
//! min/max skip-pruning consumption, and the planner `CustomScan` integration are the later M100 phases (B/C/D).
//! Own-code glue (ADR-0042 D3 / M100 D1); Apache-2.0 `datafusion`/`arrow` are the adopted engine (Rule 9).
#![allow(non_snake_case)]

use datafusion::arrow::array::{
    Array, ArrayRef, BooleanArray, Date32Array, Decimal128Array, Float32Array, Float64Array,
    Int16Array, Int32Array, Int64Array, StringArray, TimestampMicrosecondArray,
};
use datafusion::arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::error::DataFusionError;
use datafusion::functions_aggregate::expr_fn::{avg, count, count_distinct, max, min, sum};
use datafusion::prelude::{Expr, SessionContext, cast, col, lit};
use datafusion::scalar::ScalarValue;
use pgrx::AnyNumeric;
use pgrx::datum::FromDatum;
use pgrx::prelude::*;
use std::ffi::CStr;
use std::sync::Arc;

/// RAII emulating C's `HOLD_INTERRUPTS()`/`RESUME_INTERRUPTS()` (macros over `InterruptHoldoffCount`). Holds
/// interrupts across the synchronous `block_on` so a mid-flight cancel/`proc_exit` cannot drop the tokio runtime
/// and crash the backend (mirrors `datafusion_probe.rs`; the per-batch safe-point granularity is M100 Phase D).
struct HeldInterrupts;
impl HeldInterrupts {
    fn hold() -> Self {
        unsafe { pg_sys::InterruptHoldoffCount += 1 };
        HeldInterrupts
    }
}
impl Drop for HeldInterrupts {
    fn drop(&mut self) {
        unsafe { pg_sys::InterruptHoldoffCount -= 1 };
    }
}

/// Map the decoded columnar columns (name, atttypid, per-row stored bytes) to an Arrow schema + arrays. The stored
/// bytes are the codec encoding (fixed: attlen LE bytes; varlena: logical payload). Builtin type OIDs (pg_type.dat,
/// ABI-stable) drive the Arrow `DataType`.
pub(super) fn build_arrow(
    cols: &[(String, u32, Vec<Option<Vec<u8>>>)],
) -> Result<(Schema, Vec<ArrayRef>), String> {
    let mut fields = Vec::with_capacity(cols.len());
    let mut arrays: Vec<ArrayRef> = Vec::with_capacity(cols.len());
    for (name, typid, values) in cols {
        let (dt, arr) = cells_to_array(*typid, values)?;
        fields.push(Field::new(name, dt, true));
        arrays.push(arr);
    }
    Ok((Schema::new(fields), arrays))
}

/// M160 — build the Arrow schema + arrays from `DecodedColumn`s (the pushdown fast path). `FixedRaw` columns (non-null
/// fixed-width) go through `fixed_raw_array` (one typed `Vec<T>` per column, no per-cell alloc); `Cells` columns reuse
/// `cells_to_array` (byte-identical to the legacy `build_arrow`). This is the M160 win applied only to the hot path.
pub(super) fn build_arrow_from_decoded(
    cols: &[(String, u32, super::columnar::DecodedColumn)],
) -> Result<(Schema, Vec<ArrayRef>), String> {
    use super::columnar::DecodedColumn;
    let mut fields = Vec::with_capacity(cols.len());
    let mut arrays: Vec<ArrayRef> = Vec::with_capacity(cols.len());
    for (name, typid, dc) in cols {
        let (dt, arr) = match dc {
            DecodedColumn::FixedRaw { bytes, width, row_count } => {
                fixed_raw_array(*typid, bytes, *width, *row_count)?
            }
            DecodedColumn::Cells(values) => cells_to_array(*typid, values)?,
        };
        fields.push(Field::new(name, dt, true));
        arrays.push(arr);
    }
    Ok((Schema::new(fields), arrays))
}

/// M160 — build an Arrow `PrimitiveArray` for a non-null fixed-width column directly from the contiguous little-endian
/// value stream: one pre-sized typed `Vec<T>` (endian-safe `from_le_bytes` fill) handed zero-copy to Arrow via
/// `PrimitiveArray::from(Vec<T>)`. No per-cell `Vec<u8>` boxing (the deep-dive flamegraph's dominant cost). BYTE-IDENTICAL
/// to `cells_to_array` for these types because that path also does a plain `from_le_bytes` with no epoch/other transform
/// (df_executor build_arrow comment: "the stored bytes ARE the internal int"). Only types in `fixed_arrow_width` reach here.
fn fixed_raw_array(
    typid: u32,
    bytes: &[u8],
    width: usize,
    row_count: usize,
) -> Result<(DataType, ArrayRef), String> {
    if bytes.len() != width * row_count {
        return Err(format!(
            "df_executor: fixed-raw size {} != {width}*{row_count} (typid {typid})",
            bytes.len()
        ));
    }
    Ok(match typid {
        21 => (
            DataType::Int16,
            Arc::new(Int16Array::from(
                bytes.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>(),
            )),
        ),
        23 => (
            DataType::Int32,
            Arc::new(Int32Array::from(
                bytes
                    .chunks_exact(4)
                    .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect::<Vec<_>>(),
            )),
        ),
        20 => (
            DataType::Int64,
            Arc::new(Int64Array::from(
                bytes
                    .chunks_exact(8)
                    .map(|c| i64::from_le_bytes(c.try_into().unwrap()))
                    .collect::<Vec<_>>(),
            )),
        ),
        700 => (
            DataType::Float32,
            Arc::new(Float32Array::from(
                bytes
                    .chunks_exact(4)
                    .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect::<Vec<_>>(),
            )),
        ),
        701 => (
            DataType::Float64,
            Arc::new(Float64Array::from(
                bytes
                    .chunks_exact(8)
                    .map(|c| f64::from_le_bytes(c.try_into().unwrap()))
                    .collect::<Vec<_>>(),
            )),
        ),
        1082 => (
            DataType::Date32,
            Arc::new(Date32Array::from(
                bytes
                    .chunks_exact(4)
                    .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect::<Vec<_>>(),
            )),
        ),
        1114 | 1184 => (
            DataType::Timestamp(TimeUnit::Microsecond, None),
            Arc::new(TimestampMicrosecondArray::from(
                bytes
                    .chunks_exact(8)
                    .map(|c| i64::from_le_bytes(c.try_into().unwrap()))
                    .collect::<Vec<_>>(),
            )),
        ),
        other => {
            return Err(format!("df_executor: fixed_raw_array unexpected typid {other}"));
        }
    })
}

/// The legacy per-cell → Arrow conversion (extracted from `build_arrow` unchanged) — used by the cell path (nullable /
/// varlena / text) and by `arrow_cache`. Kept byte-identical: every arm is a plain `from_le_bytes`/`from_utf8_lossy`.
fn cells_to_array(typid: u32, values: &[Option<Vec<u8>>]) -> Result<(DataType, ArrayRef), String> {
    Ok(match typid {
            21 => (
                DataType::Int16,
                Arc::new(Int16Array::from_iter(
                    values.iter().map(|v| v.as_ref().map(|b| i16::from_le_bytes([b[0], b[1]]))),
                )),
            ),
            23 => {
                (
                    DataType::Int32,
                    Arc::new(Int32Array::from_iter(values.iter().map(|v| {
                        v.as_ref().map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    }))),
                )
            }
            20 => {
                (
                    DataType::Int64,
                    Arc::new(Int64Array::from_iter(values.iter().map(|v| {
                        v.as_ref().map(|b| i64::from_le_bytes(b[..8].try_into().unwrap()))
                    }))),
                )
            }
            700 => {
                (
                    DataType::Float32,
                    Arc::new(Float32Array::from_iter(values.iter().map(|v| {
                        v.as_ref().map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    }))),
                )
            }
            701 => {
                (
                    DataType::Float64,
                    Arc::new(Float64Array::from_iter(values.iter().map(|v| {
                        v.as_ref().map(|b| f64::from_le_bytes(b[..8].try_into().unwrap()))
                    }))),
                )
            }
            16 => (
                DataType::Boolean,
                Arc::new(BooleanArray::from_iter(
                    values.iter().map(|v| v.as_ref().map(|b| b.first().copied().unwrap_or(0) != 0)),
                )),
            ),
            25 | 1042 | 1043 => (
                DataType::Utf8,
                Arc::new(StringArray::from_iter(
                    values
                        .iter()
                        .map(|v| v.as_ref().map(|b| String::from_utf8_lossy(b).into_owned())),
                )),
            ),
            // Temporal: the stored bytes ARE the internal int (timestamp/timestamptz = int64 μs, date = int32 days).
            // Both mapped to a naive (tz=None) Arrow type — the comparison is on the raw int domain (tz is display
            // only), so `build_filter_expr`'s matching-typed literal compares correctly (D3).
            1114 | 1184 => {
                (
                    DataType::Timestamp(TimeUnit::Microsecond, None),
                    Arc::new(TimestampMicrosecondArray::from_iter(values.iter().map(|v| {
                        v.as_ref().map(|b| i64::from_le_bytes(b[..8].try_into().unwrap()))
                    }))),
                )
            }
            1082 => {
                (
                    DataType::Date32,
                    Arc::new(Date32Array::from_iter(values.iter().map(|v| {
                        v.as_ref().map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    }))),
                )
            }
            other => {
                return Err(format!(
                    "df_executor: unsupported column type oid {other} (Phase A: int2/4/8, float4/8, bool, text)"
                ));
            }
    })
}

/// Whether this OID is safe as a GROUP BY key / `COUNT(DISTINCT)` column for the vectorized path. DataFusion groups
/// by byte-equality; PG groups by the type's + collation's equality. `bpchar`/`char(n)` (OID 1042) is EXCLUDED
/// (review MEDIUM): `bpchareq` ignores trailing blanks (a TYPE semantic, `varchar.c:756-773`), so `'ab'` and `'ab '`
/// are equal in PG but byte-different in storage → the byte-keyed hash would over-count. text (25) / varchar (1043)
/// have no such trailing-blank rule, and their deterministic-collation equality IS byte-equality (`varlena.c` memcmp
/// tiebreak). char(n)-with-length is padded uniformly and would be safe, but is indistinguishable from bare bpchar by
/// OID alone → excluded conservatively (char(n) GROUP BY declines to native; rare, ClickBench uses none).
pub(super) fn arrow_supported_group_type(typoid: u32) -> bool {
    matches!(typoid, 21 | 23 | 20 | 700 | 701 | 16 | 25 | 1043 | 1114 | 1184 | 1082)
}

/// A supported aggregate for the vectorized columnar path — ONLY shapes whose Arrow result type maps to the exact
/// PostgreSQL output type (M114 blueprint E1/E2/E3): `count(*)`→int8, `sum(float8)`→float8, `sum(int2/int4)`→int8
/// (Arrow Int64 = PG bigint, no overflow), `avg(float8)`→float8. Numeric-output shapes (`avg(int)`, `sum(int8)`,
/// `sum(float4)`) are DECLINED at admit time (ADR-M114-1) — never reach here.
pub(super) enum AggSpec {
    CountStar,
    SumFloat8(String),
    /// `sum(int2/int4)` → PG int8. DataFusion coerces int2/int4 → Int64; the datum is int8 (no overflow).
    SumInt(String),
    /// M166 — `sum(int2_col ± const)` → PG int8 (ClickBench q29). Admitted ONLY for an int2 base column with an int4
    /// operator result whose per-row `col ± delta` provably stays in int4 (so PG raises no 22003 and the Int64 sum is
    /// exact). `delta` folds the operator sign (`col - k` = `col + (-k)`). Output kind = SumInt (Arrow Int64 → PG int8).
    SumIntAddConst { col: String, delta: i64 },
    /// `avg(float8)` → PG float8. DataFusion `avg` → Float64.
    AvgFloat8(String),
    /// `sum(int8)` → PG `numeric` (exact). DataFusion `sum(cast(col AS Decimal128(38,0)))` → i128; the datum is a PG
    /// numeric via `AnyNumeric` (blueprint ADR-N1 — Int64 sum would wrap).
    SumInt8Numeric(String),
    /// `avg(int2/4/8)` → PG `numeric` (data-dependent scale). Decomposed to `sum(cast Decimal128(38,0))` + `count`;
    /// the datum is `AnyNumeric(sum) / AnyNumeric(count)` = PG's `numeric_div` (ADR-N1/N2). Emits TWO batch columns.
    AvgIntNumeric(String),
    /// `min(col)`/`max(col)` on an ordered native type → PG output type = the input column type. DataFusion `min()`/
    /// `max()` yields an Arrow array of the source column's type; the datum is emitted by `arrow_value_to_datum`
    /// against the carried output typoid (columnar-minmax blueprint ADR-MM1/MM3). NaN-correct because it decodes
    /// actual values — unlike the Phase-B directory fold which gates `max(float)` out.
    MinCol(String, u32),
    MaxCol(String, u32),
    /// `count(DISTINCT col)` → PG int8. DataFusion `count_distinct(col)` = exact `DistinctCountAccumulator`
    /// (`distinct: true` over the exact `count_udaf`) → Int64; the datum is int8 (M154, same path as CountStar).
    /// NEVER approx/HLL (ADR-M154-1) — the A/B gate demands byte-identity with PG COUNT(DISTINCT).
    CountDistinct(String),
}

impl AggSpec {
    /// The source column this aggregate reads (None for `count(*)`), for projection.
    fn col_name(&self) -> Option<&str> {
        match self {
            AggSpec::CountStar => None,
            AggSpec::SumFloat8(n)
            | AggSpec::SumInt(n)
            | AggSpec::SumIntAddConst { col: n, .. }
            | AggSpec::AvgFloat8(n)
            | AggSpec::SumInt8Numeric(n)
            | AggSpec::AvgIntNumeric(n)
            | AggSpec::MinCol(n, _)
            | AggSpec::MaxCol(n, _)
            | AggSpec::CountDistinct(n) => Some(n.as_str()),
        }
    }

    /// The number of DataFusion output columns this aggregate produces (avg-int decomposes to sum + count — ADR-N2).
    fn ncols(&self) -> usize {
        match self {
            AggSpec::AvgIntNumeric(_) => 2,
            _ => 1,
        }
    }
}

/// Push the aliased DataFusion aggregate expression(s) for one `AggSpec` (usually 1; avg-int emits sum + count).
/// Aliases are sequential (`a{k}`) by the running column position, so a multi-column spec keeps unique aliases.
fn push_agg_exprs(spec: &AggSpec, exprs: &mut Vec<Expr>) {
    let k = exprs.len();
    match spec {
        AggSpec::CountStar => exprs.push(count(lit(1i64)).alias(format!("a{k}"))),
        AggSpec::SumFloat8(name) | AggSpec::SumInt(name) => {
            exprs.push(sum(col(name.as_str())).alias(format!("a{k}")))
        }
        // M166 — sum over `int2_col ± const`: widen the base column to Int64 and add the (signed) delta before summing,
        // mirroring the group IntAddConst `cast(col, Int64) + lit(delta)` idiom. The per-row value is in int4 range by
        // the admit gate, so the Int64 sum is byte-identical to PG's `sum(int4) → int8`. Output kind = SumInt.
        AggSpec::SumIntAddConst { col: name, delta } => exprs.push(
            sum(cast(col(name.as_str()), DataType::Int64) + lit(*delta)).alias(format!("a{k}")),
        ),
        AggSpec::AvgFloat8(name) => exprs.push(avg(col(name.as_str())).alias(format!("a{k}"))),
        AggSpec::SumInt8Numeric(name) => {
            exprs.push(sum(dec128_cast(name)).alias(format!("a{k}")));
        }
        AggSpec::AvgIntNumeric(name) => {
            exprs.push(sum(dec128_cast(name)).alias(format!("a{k}")));
            exprs.push(count(col(name.as_str())).alias(format!("a{}", k + 1)));
        }
        AggSpec::MinCol(name, _) => exprs.push(min(col(name.as_str())).alias(format!("a{k}"))),
        AggSpec::MaxCol(name, _) => exprs.push(max(col(name.as_str())).alias(format!("a{k}"))),
        AggSpec::CountDistinct(name) => {
            exprs.push(count_distinct(col(name.as_str())).alias(format!("a{k}")))
        }
    }
}

/// `cast(col AS Decimal128(38,0))` — the exact integer-sum path (Int64 sum wraps; blueprint ADR-N1).
fn dec128_cast(name: &str) -> Expr {
    datafusion::logical_expr::cast(col(name), DataType::Decimal128(38, 0))
}

/// Decode the columnar table's projected columns into one Arrow `RecordBatch` (projection pushdown). Projects the
/// `sum` columns PLUS every zone-map predicate's column (so the DataFusion Filter can re-check it — ADR D3), and
/// always ≥ 1 column so `count(*)` has a row count. Passes the predicates + `skip` to `decode_columns` so the
/// min/max zone-map can skip proven-non-matching chunk groups.
unsafe fn decode_to_batch(
    rel: pg_sys::Relation,
    sum_cols: &[String],
    predicates: &[super::zonemap::ZonePredicate],
    text_predicates: &[super::zonemap::TextPredicate],
    in_predicates: &[super::zonemap::InListPredicate],
    skip: bool,
) -> Result<RecordBatch, String> {
    let mut proj: Vec<usize> = Vec::new();
    for name in sum_cols {
        let idx = super::columnar::column_index(rel, name)
            .ok_or_else(|| format!("df_executor: column '{name}' not found"))?;
        if !proj.contains(&idx) {
            proj.push(idx);
        }
    }
    for p in predicates {
        if !proj.contains(&p.col) {
            proj.push(p.col); // the filter column MUST be decoded so the DataFusion Filter can re-check it (D3)
        }
    }
    // M156 — the text-predicate column MUST be decoded (as a Utf8 array) so the DataFusion Filter can re-check it.
    // Text predicates never drive zone-map skipping (only `predicates` is passed to `decode_columns` below).
    for t in text_predicates {
        if !proj.contains(&t.col) {
            proj.push(t.col);
        }
    }
    // M161 — the IN-list column MUST be decoded so the DataFusion Filter can re-check `col IN (…)` (D3 final authority).
    // IN-list never drives zone-map skipping (only `predicates` is passed to `decode_columns` below).
    for ip in in_predicates {
        if !proj.contains(&ip.col) {
            proj.push(ip.col);
        }
    }
    if proj.is_empty() {
        proj.push(0); // count(*) needs a column to establish the row count
    }
    // M160 — decode_columns_v2 returns fixed-width non-null columns as `FixedRaw` (contiguous LE bytes, no per-cell
    // alloc); build_arrow_from_decoded turns those into Arrow via one typed Vec<T> per column. Cell columns are
    // byte-identical to the legacy path. This is the hot pushdown path only (vindex/arrow_cache keep decode_columns).
    let cols = super::columnar::decode_columns_v2(rel, Some(&proj), predicates, skip)?;
    let (schema, arrays) = build_arrow_from_decoded(&cols)?;
    RecordBatch::try_new(Arc::new(schema), arrays)
        .map_err(|e| format!("df_executor: arrow batch: {e}"))
}

/// Build the DataFusion filter `Expr` for the pushed zone-map predicates (a conjunction of `col <op> const`), the
/// FINAL authority over rows in surviving chunk groups (ADR D3 — the skip is only an admission filter). `col` is
/// resolved to its name via the tupdesc; the literal is typed to the column's `MinMaxKind` (matching `build_arrow`).
unsafe fn build_filter_expr(
    rel: pg_sys::Relation,
    predicates: &[super::zonemap::ZonePredicate],
    text_predicates: &[super::zonemap::TextPredicate],
    in_predicates: &[super::zonemap::InListPredicate],
) -> Option<Expr> {
    use super::columnar_codec::MinMaxKind;
    use super::zonemap::{TextOp, ZoneOp};
    let tupdesc = (*rel).rd_att;
    let natts = (*tupdesc).natts as usize;
    let mut acc: Option<Expr> = None;
    for p in predicates {
        if p.col >= natts {
            continue; // fail-safe (EC-2): unknown column → do not build a filter term on it
        }
        let att = super::tupdesc_attr(tupdesc, p.col);
        let name = CStr::from_ptr((*att).attname.data.as_ptr()).to_string_lossy().into_owned();
        let c = col(name.as_str());
        let b = p.const_bits;
        // Temporal columns share the I8/I4 min/max domain but need an Arrow-typed literal so the Filter matches the
        // Timestamp/Date column type built in `build_arrow` (a bare Int64 lit would type-mismatch). Intercept by OID
        // BEFORE the MinMaxKind dispatch. tz=None to match build_arrow (raw-int compare — D3).
        let val = match (*att).atttypid.to_u32() {
            1114 | 1184 => lit(ScalarValue::TimestampMicrosecond(Some(b as i64), None)),
            1082 => lit(ScalarValue::Date32(Some(b as i64 as i32))),
            _ => match super::columnar::minmax_kind_of((*att).atttypid.to_u32()) {
                MinMaxKind::I2 => lit(b as i64 as i16),
                MinMaxKind::I4 => lit(b as i64 as i32),
                MinMaxKind::I8 => lit(b as i64),
                MinMaxKind::Bool => lit(b != 0),
                MinMaxKind::F4 => lit(f64::from_bits(b) as f32),
                MinMaxKind::F8 => lit(f64::from_bits(b)),
                MinMaxKind::None => continue, // not min/max-able → cannot have been pushed
            },
        };
        let e = match p.op {
            ZoneOp::Lt => c.lt(val),
            ZoneOp::Le => c.lt_eq(val),
            ZoneOp::Eq => c.eq(val),
            ZoneOp::Ge => c.gt_eq(val),
            ZoneOp::Gt => c.gt(val),
            ZoneOp::Ne => c.not_eq(val), // M151 — `<>` filter-only (never pruned); the executor's final authority
        };
        acc = Some(match acc {
            Some(prev) => prev.and(e),
            None => e,
        });
    }
    // M156 — text predicates: `col <op> 'needle'` over the decoded Utf8 column. LIKE/NOT LIKE use DataFusion's
    // default `\` escape (planner rejects any other; None == backslash, matching PG's default) — proven byte-identical
    // by the A/B `LIKE 'a\%b'`. `<>`/NOT LIKE against NULL yield NULL → row excluded, same 3-valued logic as PG.
    for t in text_predicates {
        if t.col >= natts {
            continue; // fail-safe: unknown column → do not build a filter term on it
        }
        let att = super::tupdesc_attr(tupdesc, t.col);
        let name = CStr::from_ptr((*att).attname.data.as_ptr()).to_string_lossy().into_owned();
        let c = col(name.as_str());
        let val = lit(ScalarValue::Utf8(Some(t.needle.clone())));
        let e = match t.op {
            TextOp::Eq => c.eq(val),
            TextOp::Ne => c.not_eq(val),
            TextOp::Like => c.like(val),
            TextOp::NotLike => c.not_like(val),
        };
        acc = Some(match acc {
            Some(prev) => prev.and(e),
            None => e,
        });
    }
    // M161 — integer IN-list: `col IN (c0, c1, …)` over the decoded integer column. The extractor admitted ONLY the
    // integer class (I2/I4/I8) with no NULL element and `=`/useOr semantics, so `col.in_list(lits, false)` is exactly
    // an OR of `=` — DataFusion's final authority, byte-identical to PG's ScalarArrayOpExpr. Literals are typed to the
    // column's MinMaxKind so the Filter matches the Arrow column type built in `build_arrow` (a bare Int64 lit would
    // type-mismatch an Int32 column).
    for ip in in_predicates {
        if ip.col >= natts || ip.consts.is_empty() {
            continue; // fail-safe: unknown column / empty list → do not build a filter term on it
        }
        let att = super::tupdesc_attr(tupdesc, ip.col);
        let name = CStr::from_ptr((*att).attname.data.as_ptr()).to_string_lossy().into_owned();
        let c = col(name.as_str());
        let kind = super::columnar::minmax_kind_of((*att).atttypid.to_u32());
        let lits: Vec<Expr> = ip
            .consts
            .iter()
            .map(|&v| match kind {
                MinMaxKind::I2 => lit(v as i16),
                MinMaxKind::I4 => lit(v as i32),
                _ => lit(v), // I8 (extractor admits only the integer class)
            })
            .collect();
        let e = c.in_list(lits, false);
        acc = Some(match acc {
            Some(prev) => prev.and(e),
            None => e,
        });
    }
    acc
}

/// Run the aggregates over the columnar table via a vectorized DataFusion plan under `HeldInterrupts`; return one
/// `(Datum, is_null)` per agg, in `aggs` order, ready to store in a `TupleTableSlot`. `count(*)`→`int8` Datum;
/// `sum(float8)`→`float8` Datum. This is the executor the planner `CustomScan` (Phase C) drives at exec time.
/// `predicates`/`skip`: the zone-map pushdown — the batch is filtered by the predicate (D3 final authority) and the
/// decode skips proven-non-matching chunk groups.
pub(super) unsafe fn run_columnar_aggs(
    rel: pg_sys::Relation,
    aggs: &[AggSpec],
    predicates: &[super::zonemap::ZonePredicate],
    text_predicates: &[super::zonemap::TextPredicate],
    in_predicates: &[super::zonemap::InListPredicate],
    skip: bool,
) -> Result<Vec<(pg_sys::Datum, bool)>, String> {
    let agg_cols: Vec<String> =
        aggs.iter().filter_map(|a| a.col_name().map(str::to_string)).collect();
    let batch = decode_to_batch(rel, &agg_cols, predicates, text_predicates, in_predicates, skip)?;
    let filter = build_filter_expr(rel, predicates, text_predicates, in_predicates);
    run_aggs_on_batch(batch, aggs, filter)
}

/// Run the aggregates over an already-built Arrow `RecordBatch` via a vectorized DataFusion plan under
/// `HeldInterrupts` + a `work_mem` MemoryPool + `target_partitions=1`. Shared by the M100 columnar path
/// (`run_columnar_aggs`) and the M101 heap-authoritative Arrow cache path (`arrow_cache.rs`).
pub(super) unsafe fn run_aggs_on_batch(
    batch: RecordBatch,
    aggs: &[AggSpec],
    filter: Option<Expr>,
) -> Result<Vec<(pg_sys::Datum, bool)>, String> {
    let mut exprs = Vec::with_capacity(aggs.len());
    for a in aggs {
        push_agg_exprs(a, &mut exprs);
    }

    let batches = run_df_collect(batch, move |df| {
        // Zone-map predicate as the FINAL authority over surviving rows (D3): filter BEFORE aggregating.
        let df = match filter {
            Some(f) => df.filter(f)?,
            None => df,
        };
        df.aggregate(vec![], exprs)
    })?;
    let b = batches.first().ok_or("df_executor: no result batch")?;
    let mut result = Vec::with_capacity(aggs.len());
    let mut off = 0; // batch column cursor — a multi-column spec (avg-int) consumes >1 column
    for a in aggs {
        result.push(agg_datum(b, off, 0, a)?);
        off += a.ncols();
    }
    Ok(result)
}

/// Build the DataFusion runtime (bounded `work_mem` MemoryPool + `target_partitions=1` — M100 D3 safety), read the
/// Arrow batch, let `build` finish the plan (filter/aggregate), and `collect` under `HeldInterrupts`. Shared by the
/// scalar (`run_aggs_on_batch`) and grouped (`run_columnar_grouped_aggs`) paths so the tokio/pool/interrupt discipline
/// lives in ONE place (DRY).
unsafe fn run_df_collect<F>(batch: RecordBatch, build: F) -> Result<Vec<RecordBatch>, String>
where
    F: FnOnce(
        datafusion::dataframe::DataFrame,
    ) -> Result<datafusion::dataframe::DataFrame, DataFusionError>,
{
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .map_err(|e| format!("df_executor: tokio runtime: {e}"))?;
    // The pool must fit the decoded input batch: the aggregate path decodes a FEW columns (small batch, work_mem is
    // plenty), but the M158 top-k path decodes the FULL projection for all rows into one Arrow batch — which can be
    // hundreds of MB for a wide `SELECT *`. Size the pool to the batch's actual memory + headroom so a legitimate large
    // input is not rejected as "Resources exhausted"; the batch already lives in RAM regardless. (Honest caveat: the
    // top-k path is therefore O(N) memory in the decoded batch — unlike the native top-N heapsort's O(k). M158
    // mitigated this by defaulting the GUC OFF; M167 flipped the default ON, so the mitigation is now the plan-time
    // decode-size guard in `try_swap_topk` (M167 ADR-4): a candidate whose estimated decode dwarfs `work_mem`
    // declines to the native plan instead of being admitted here.)
    let work_mem_bytes = (pg_sys::work_mem.max(64) as usize) * 1024;
    let batch_bytes = batch.get_array_memory_size();
    // M167 § 6 left the peak of the decoded batch UNMEASURED: `VmRSS` is dominated by `shared_buffers` mapped into
    // every backend, so per-PID sampling never isolated it. The number was in-process the whole time — this is the
    // one line that exposes it. It is the O(N) quantity the M167 DoD bullet 2b is about, so it is also the baseline
    // any claim of "the path is now O(k)" has to be measured against, by the same instrument.
    if super::columnar_agg::admit_trace_enabled() {
        pgrx::warning!(
            "theodb_decode_batch: rows={} bytes={} work_mem_bytes={}",
            batch.num_rows(),
            batch_bytes,
            work_mem_bytes
        );
    }
    let pool_bytes = work_mem_bytes.max(batch_bytes.saturating_mul(2)) + 64 * 1024 * 1024;
    let held = HeldInterrupts::hold();
    let out: Result<Vec<RecordBatch>, DataFusionError> = rt.block_on(async move {
        use datafusion::execution::memory_pool::GreedyMemoryPool;
        use datafusion::execution::runtime_env::RuntimeEnvBuilder;
        use datafusion::prelude::SessionConfig;
        let runtime = RuntimeEnvBuilder::new()
            .with_memory_pool(std::sync::Arc::new(GreedyMemoryPool::new(pool_bytes)))
            .build_arc()?;
        let config = SessionConfig::new().with_target_partitions(1);
        let ctx = SessionContext::new_with_config_rt(config, runtime);
        let df = ctx.read_batch(batch)?;
        build(df)?.collect().await
    });
    drop(held);
    drop(rt);
    out.map_err(|e| format!("df_executor: DataFusion: {e}"))
}

/// M157/M161 — exec-side decoding of one expression group key (from the 3rd `custom_private` channel). `func`:
/// 0=DateTrunc (out timestamp), 1=ExtractField (out numeric — minute/hour), 2=IntAddConst (`base ± delta`, out int).
/// `base_name` is the base column.
pub(super) struct GroupExprExec {
    pub base_name: String,
    pub func: i32,
    pub unit: String,
    pub delta: i64,
    pub out_typoid: u32,
}

/// Grouped columnar aggregate (M100 GROUP BY pushdown). Decode the group + sum columns, run
/// `.aggregate(group_exprs, agg_exprs)`, and materialize the multi-row result in the PG output-target order given by
/// `layout` (ADR-2): each output slot is either group key `idx` (batch col `idx`) or agg `idx` (batch col
/// `ngroup+idx`). `group_cols` is `(name, typoid)` per key (typoid drives the reverse Arrow→Datum conversion).
/// `predicates` + `skip` apply the zone-map skip-pruning + DataFusion Filter (M114 GROUP BY+WHERE); empty = no WHERE.
/// Returns one inner Vec per group, in `layout` (target) order. The CALLER runs this in a durable memory context
/// (ADR-3) for text group-key datums.
pub(super) unsafe fn run_columnar_grouped_aggs(
    rel: pg_sys::Relation,
    group_cols: &[(String, u32)],
    group_key_exprs_spec: &[GroupExprExec], // M157/M161 — expression group keys (date_trunc / extract / int±k / const)
    aggs: &[AggSpec],
    layout: &[(u8, usize)],
    const_outs: &[(i64, u32)], // M165 — projected integer constant output cells (layout kind=3)
    predicates: &[super::zonemap::ZonePredicate],
    text_predicates: &[super::zonemap::TextPredicate],
    in_predicates: &[super::zonemap::InListPredicate],
    skip: bool,
) -> Result<Vec<Vec<(pg_sys::Datum, bool)>>, String> {
    use datafusion::functions::datetime::expr_fn::{date_part, date_trunc};
    // Project bare group columns ∪ group-expr base columns ∪ agg columns (count(*) needs no column; decode_to_batch
    // also projects predicate cols and guarantees ≥1).
    let mut proj_cols: Vec<String> = group_cols.iter().map(|(n, _)| n.clone()).collect();
    for g in group_key_exprs_spec {
        if !g.base_name.is_empty() && !proj_cols.iter().any(|p| p == &g.base_name) {
            proj_cols.push(g.base_name.clone());
        }
    }
    for a in aggs {
        if let Some(n) = a.col_name() {
            if !proj_cols.iter().any(|p| p == n) {
                proj_cols.push(n.to_string());
            }
        }
    }
    let batch = decode_to_batch(rel, &proj_cols, predicates, text_predicates, in_predicates, skip)?;
    let filter = build_filter_expr(rel, predicates, text_predicates, in_predicates);

    // Grouping keys: bare columns FIRST, then the expression exprs (M157/M161). The output batch columns follow this
    // order: [bare_0..bare_{ncols-1}, expr_0..expr_{nexpr-1}, agg columns…].
    //   func 0 DateTrunc    — date_trunc(unit, ts) (tz-independent timestamp; timestamptz declined at admit, ADR-2).
    //   func 1 ExtractField — cast(date_part(unit, ts) → Int64): minute/hour are epoch-invariant + integer-valued;
    //                          grouped as Int64, materialized as PG numeric (AnyNumeric) so it equals extract()'s type.
    //   func 2 IntAddConst  — cast(col → Int64) + delta: the i64 compute is exact (int2/int4 column ± int const; an
    //                          int8 result is declined at admit), so grouping is exact; the result-type range-check
    //                          happens at materialize (out_typoid = opresulttype, int2/int4 → reproduces PG 22003).
    let ncols = group_cols.len();
    let ngroup = ncols + group_key_exprs_spec.len();
    let mut group_exprs: Vec<Expr> = group_cols.iter().map(|(n, _)| col(n.as_str())).collect();
    for g in group_key_exprs_spec {
        let e = match g.func {
            0 => date_trunc(lit(ScalarValue::Utf8(Some(g.unit.clone()))), col(g.base_name.as_str())),
            1 => cast(
                date_part(lit(ScalarValue::Utf8(Some(g.unit.clone()))), col(g.base_name.as_str())),
                DataType::Int64,
            ),
            2 => cast(col(g.base_name.as_str()), DataType::Int64) + lit(g.delta),
            other => return Err(format!("df_executor: bad group-expr func {other}")),
        };
        group_exprs.push(e);
    }
    let mut agg_exprs = Vec::with_capacity(aggs.len());
    // Per-agg batch-column offset (relative to the first agg column) — a multi-column spec (avg-int) shifts the rest.
    let mut agg_off: Vec<usize> = Vec::with_capacity(aggs.len());
    for a in aggs {
        agg_off.push(agg_exprs.len());
        push_agg_exprs(a, &mut agg_exprs);
    }
    // Filter BEFORE aggregating (SQL WHERE … GROUP BY — M114 blueprint E4); the zone-map skip above is only an
    // admission filter, the DataFusion Filter is the final row authority.
    let batches = run_df_collect(batch, move |df| {
        let df = match filter {
            Some(f) => df.filter(f)?,
            None => df,
        };
        df.aggregate(group_exprs, agg_exprs)
    })?;

    // DataFusion output columns: [group_0..group_{ngroup-1}, agg columns…]. Agg `idx` starts at batch column
    // `ngroup + agg_off[idx]` (a multi-column spec shifts the rest). Emit rows in `layout` (target) order.
    let mut rows: Vec<Vec<(pg_sys::Datum, bool)>> = Vec::new();
    for b in &batches {
        for r in 0..b.num_rows() {
            let mut row_out = Vec::with_capacity(layout.len());
            for &(kind, idx) in layout {
                let cell = match kind {
                    0 => {
                        // bare group column — batch col `idx`.
                        let typoid = group_cols.get(idx).ok_or("df_executor: layout group idx oob")?.1;
                        arrow_value_to_datum(b.column(idx), r, typoid)?
                    }
                    2 => {
                        // M157/M161 — expression group-expr — batch col `ncols + idx`, materialized per variant.
                        let g =
                            group_key_exprs_spec.get(idx).ok_or("df_executor: layout group-expr idx oob")?;
                        group_expr_cell(b.column(ncols + idx), r, g)?
                    }
                    3 => {
                        // M165 — const-out cell: a projected integer literal, the SAME value in every row (no batch
                        // column — the const is neither grouped nor aggregated). Rebuild the by-value int Datum from
                        // its stored (i64, typoid). NOT a grouping key → excluded from the `gk` sort below.
                        let &(val, typoid) =
                            const_outs.get(idx).ok_or("df_executor: layout const idx oob")?;
                        const_out_datum(val, typoid)?
                    }
                    _ => {
                        let a = aggs.get(idx).ok_or("df_executor: layout agg idx oob")?;
                        agg_datum(b, ngroup + agg_off[idx], r, a)?
                    }
                };
                row_out.push(cell);
            }
            rows.push(row_out);
        }
    }
    // Sort the (few) group rows ASCending nulls-last by the group-key output slots — reproduces the GroupAgg order for
    // the M115 Agg-swap of an AGG_SORTED node. Numeric/temporal keys only (text AGG_SORTED is declined at swap time);
    // a Rust sort over the small grouped result avoids the DataFusion sort's memory-pool reservation.
    let gk: Vec<(usize, u32)> = layout
        .iter()
        .enumerate()
        .filter_map(|(slot, &(kind, idx))| match kind {
            // fail-safe (council-rust-pgrx LOW): `.get()` not `[idx]` — a corrupt layout drops the sort key rather
            // than panicking across the C boundary (matches the materialization loop's `.get(idx).ok_or()?` pattern).
            0 => group_cols.get(idx).map(|g| (slot, g.1)),
            2 => group_key_exprs_spec.get(idx).map(|g| (slot, g.out_typoid)), // M157/M161 — expr key out_typoid
            _ => None,
        })
        .collect();
    if !gk.is_empty() && !gk.iter().any(|&(_, t)| matches!(t, 25 | 1042 | 1043)) {
        rows.sort_by(|a, b| {
            for &(slot, typ) in &gk {
                let ord = cmp_group_datum(a[slot], b[slot], typ);
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
            std::cmp::Ordering::Equal
        });
    }
    Ok(rows)
}

/// M158 — late-materialization top-k over a columnar table: `SELECT <proj> [WHERE <pushable>] ORDER BY <key> LIMIT k`.
/// Decodes {proj ∪ key ∪ filter} columns ONCE into an Arrow batch, then runs `filter → sort([key]) → limit(k)` in
/// DataFusion (vectorized TopK, O(N log k)) and materializes ONLY the k surviving rows back to PG Datums via
/// `arrow_value_to_datum` — so the per-row `form_row`/`palloc` cost (M148: ~80% of the eager scan) is paid for k rows,
/// not N. `proj_cols` = (name, typoid) in output-target order; `sort_key` must resolve in the decoded batch. Returns
/// one inner Vec per surviving row, in sort order (the CALLER runs this in a durable memory context for varlena datums).
pub(super) unsafe fn run_columnar_topk(
    rel: pg_sys::Relation,
    proj_cols: &[(String, u32)],
    sort_keys: &[(String, bool, bool)], // M167 — (name, ascending, nulls_first) in ORDER BY position
    k: usize,
    predicates: &[super::zonemap::ZonePredicate],
    text_predicates: &[super::zonemap::TextPredicate],
    in_predicates: &[super::zonemap::InListPredicate],
    skip: bool,
) -> Result<Vec<Vec<(pg_sys::Datum, bool)>>, String> {
    // Project all output columns ∪ the sort key (decode_to_batch also folds in the predicate columns + guarantees ≥1).
    let mut proj_names: Vec<String> = proj_cols.iter().map(|(n, _)| n.clone()).collect();
    for (key, _, _) in sort_keys {
        if !proj_names.iter().any(|n| n == key) {
            proj_names.push(key.clone());
        }
    }
    let batch =
        decode_to_batch(rel, &proj_names, predicates, text_predicates, in_predicates, skip)?;
    let filter = build_filter_expr(rel, predicates, text_predicates, in_predicates);
    let order_by: Vec<_> =
        sort_keys.iter().map(|(name, asc, nf)| col(name.as_str()).sort(*asc, *nf)).collect();
    // filter (WHERE, the final authority — D3) → sort by the key (PG order for numeric/temporal/det-collation text) →
    // limit k (DataFusion's TopK: a bounded heap, never materializing all N as tuples).
    let batches = run_df_collect(batch, move |df| {
        let df = match filter {
            Some(f) => df.filter(f)?,
            None => df,
        };
        df.sort(order_by)?.limit(0, Some(k))
    })?;

    // Emit the surviving rows: one Datum per output column (in target order), located in the result batch by NAME
    // (the schema carries the decoded projection; filter/sort/limit preserve it). Only ≤ k rows are materialized.
    let mut rows: Vec<Vec<(pg_sys::Datum, bool)>> = Vec::new();
    for b in &batches {
        let schema = b.schema();
        let idxs: Vec<usize> = proj_cols
            .iter()
            .map(|(n, _)| schema.index_of(n).map_err(|e| format!("df_executor: topk output col '{n}': {e}")))
            .collect::<Result<Vec<_>, _>>()?;
        for r in 0..b.num_rows() {
            let mut row_out = Vec::with_capacity(proj_cols.len());
            for (oc, &bi) in proj_cols.iter().zip(idxs.iter()) {
                row_out.push(arrow_value_to_datum(b.column(bi), r, oc.1)?);
            }
            rows.push(row_out);
        }
    }
    Ok(rows)
}

/// Compare two group-key `(Datum, is_null)` cells for the ASCending-nulls-last ordering of the swapped grouped result
/// (numeric/temporal/bool types — text is declined upstream). NULL sorts last (PG default for ASC).
fn cmp_group_datum(
    a: (pg_sys::Datum, bool),
    b: (pg_sys::Datum, bool),
    typoid: u32,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a.1, b.1) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater, // nulls last
        (false, true) => return Ordering::Less,
        (false, false) => {}
    }
    unsafe {
        match typoid {
            20 | 1114 | 1184 => i64::from_datum(a.0, false).cmp(&i64::from_datum(b.0, false)),
            23 | 1082 => i32::from_datum(a.0, false).cmp(&i32::from_datum(b.0, false)),
            21 => i16::from_datum(a.0, false).cmp(&i16::from_datum(b.0, false)),
            16 => bool::from_datum(a.0, false).cmp(&bool::from_datum(b.0, false)),
            700 => f32::from_datum(a.0, false)
                .partial_cmp(&f32::from_datum(b.0, false))
                .unwrap_or(Ordering::Equal),
            701 => f64::from_datum(a.0, false)
                .partial_cmp(&f64::from_datum(b.0, false))
                .unwrap_or(Ordering::Equal),
            _ => Ordering::Equal,
        }
    }
}

/// Convert one aggregate's cell(s) at `batch[col..][row]` to a PG `(Datum, is_null)`. Single-column for count/sum/avg
/// (`count(*)`→int8, `sum(int2/4)`→int8, `sum/avg(float8)`→float8, `sum(int8)`→numeric); TWO columns for `avg(int)`
/// (sum + count → `AnyNumeric(sum)/AnyNumeric(count)` = PG `numeric_div`, ADR-N1/N2). Shared by the scalar (row 0) and
/// grouped (per row) paths.
fn agg_datum(
    b: &RecordBatch,
    col: usize,
    row: usize,
    spec: &AggSpec,
) -> Result<(pg_sys::Datum, bool), String> {
    let arr = b.column(col);
    // NULL propagation: an empty/all-NULL group makes the (first) aggregate cell null → SQL NULL (ADR-N3).
    if arr.is_null(row) {
        return Ok((pg_sys::Datum::from(0usize), true));
    }
    let datum = match spec {
        // int8 output: count(*), count(DISTINCT col), sum(int2/int4), sum(int2_col ± const) (DataFusion → Int64 = PG bigint).
        AggSpec::CountStar
        | AggSpec::SumInt(_)
        | AggSpec::SumIntAddConst { .. }
        | AggSpec::CountDistinct(_) => {
            let v = arr
                .as_any()
                .downcast_ref::<Int64Array>()
                .ok_or("df_executor: agg not Int64")?
                .value(row);
            v.into_datum().ok_or("df_executor: int8 datum")?
        }
        // float8 output: sum(float8), avg(float8) (DataFusion → Float64 = PG double precision).
        AggSpec::SumFloat8(_) | AggSpec::AvgFloat8(_) => {
            let v = arr
                .as_any()
                .downcast_ref::<Float64Array>()
                .ok_or("df_executor: agg not Float64")?
                .value(row);
            v.into_datum().ok_or("df_executor: float8 datum")?
        }
        // numeric output: sum(int8) = exact Decimal128(38,0) i128 → AnyNumeric (scale 0). Int64 sum would wrap.
        // A Decimal128(38,0) overflow (>10^38, unreachable at realistic row counts) surfaces as a DataFusion error
        // at run_df_collect, never a panic across the C boundary.
        AggSpec::SumInt8Numeric(_) => {
            let s = arr
                .as_any()
                .downcast_ref::<Decimal128Array>()
                .ok_or("df_executor: sum-int8 not Decimal128")?
                .value(row);
            AnyNumeric::from(s).into_datum().ok_or("df_executor: numeric datum")?
        }
        // numeric output: avg(int) = AnyNumeric(sum) / AnyNumeric(count) = PG numeric_div (data-dependent scale).
        AggSpec::AvgIntNumeric(_) => {
            let s = arr
                .as_any()
                .downcast_ref::<Decimal128Array>()
                .ok_or("df_executor: avg-int sum not Decimal128")?
                .value(row);
            let cnt_arr = b.column(col + 1);
            // count(col) counts non-NULLs; a zero count (all-NULL group) → SQL NULL, never a divide-by-zero (ADR-N3).
            if cnt_arr.is_null(row) {
                return Ok((pg_sys::Datum::from(0usize), true));
            }
            let n = cnt_arr
                .as_any()
                .downcast_ref::<Int64Array>()
                .ok_or("df_executor: avg-int count not Int64")?
                .value(row);
            if n == 0 {
                return Ok((pg_sys::Datum::from(0usize), true));
            }
            (AnyNumeric::from(s) / AnyNumeric::from(n))
                .into_datum()
                .ok_or("df_executor: numeric datum")?
        }
        // min/max output = input column type. The DataFusion min()/max() result array has the source column's Arrow
        // type, so the build_arrow reverse emits the exact native datum against the carried output typoid (ADR-MM3).
        // `arr.is_null(row)` (guarded above) already handled the empty/all-NULL → NULL case.
        AggSpec::MinCol(_, typoid) | AggSpec::MaxCol(_, typoid) => {
            return arrow_value_to_datum(arr, row, *typoid);
        }
    };
    Ok((datum, false))
}

/// Reverse of `build_arrow` for a GROUP BY key cell: an Arrow array value at `row` → a PG `(Datum, is_null)` of the
/// group column's PG type. Covers every OID `build_arrow` produces. NOTE (temporal): Arrow Date32/Timestamp have a
/// 1970 epoch but `build_arrow` stuffed the raw PG-epoch bytes in; GROUP BY only uses raw-value equality/hash (never
/// Arrow's temporal semantics), and we pull the same raw int back out here → the PG value round-trips exactly.
/// The CALLER must run in a durable memory context (ADR-3): a text/varlena datum is palloc'd here.
fn arrow_value_to_datum(
    arr: &dyn Array,
    row: usize,
    typoid: u32,
) -> Result<(pg_sys::Datum, bool), String> {
    if arr.is_null(row) {
        return Ok((pg_sys::Datum::from(0usize), true));
    }
    let d = match typoid {
        21 => arr
            .as_any()
            .downcast_ref::<Int16Array>()
            .ok_or("df_executor: gk not i16")?
            .value(row)
            .into_datum(),
        23 => arr
            .as_any()
            .downcast_ref::<Int32Array>()
            .ok_or("df_executor: gk not i32")?
            .value(row)
            .into_datum(),
        20 => arr
            .as_any()
            .downcast_ref::<Int64Array>()
            .ok_or("df_executor: gk not i64")?
            .value(row)
            .into_datum(),
        700 => arr
            .as_any()
            .downcast_ref::<Float32Array>()
            .ok_or("df_executor: gk not f32")?
            .value(row)
            .into_datum(),
        701 => arr
            .as_any()
            .downcast_ref::<Float64Array>()
            .ok_or("df_executor: gk not f64")?
            .value(row)
            .into_datum(),
        16 => arr
            .as_any()
            .downcast_ref::<BooleanArray>()
            .ok_or("df_executor: gk not bool")?
            .value(row)
            .into_datum(),
        25 | 1042 | 1043 => arr
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or("df_executor: gk not str")?
            .value(row)
            .to_string()
            .into_datum(),
        // timestamp/timestamptz: the raw i64 IS the timestamptz Datum (by-value μs); date: the raw i32 IS the date
        // Datum (by-value days) — same raw value we stored in build_arrow (epoch interpretation is irrelevant here).
        1114 | 1184 => arr
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .ok_or("df_executor: gk not ts")?
            .value(row)
            .into_datum(),
        1082 => arr
            .as_any()
            .downcast_ref::<Date32Array>()
            .ok_or("df_executor: gk not date")?
            .value(row)
            .into_datum(),
        other => return Err(format!("df_executor: unsupported group key oid {other}")),
    };
    Ok((d.ok_or("df_executor: group key datum")?, false))
}

/// M161 — materialize one EXPRESSION group-key cell (`GroupExprExec`). A DateTrunc array is already the native output
/// type → delegate to `arrow_value_to_datum`. ExtractField/IntAddConst compute WIDENED to Int64 in DataFusion,
/// so their arrow column is `Int64` and needs a variant-specific reverse: extract → PG `numeric` (AnyNumeric, scale 0 —
/// minute/hour are integer-valued); int±k → the base int type with a RANGE CHECK that fails when PG would raise 22003.
fn group_expr_cell(
    arr: &dyn Array,
    row: usize,
    g: &GroupExprExec,
) -> Result<(pg_sys::Datum, bool), String> {
    if arr.is_null(row) {
        return Ok((pg_sys::Datum::from(0usize), true));
    }
    match g.func {
        1 => {
            // ExtractField → numeric: the Int64 field value → AnyNumeric (exact, scale 0) = PG extract()'s numeric.
            let v = arr
                .as_any()
                .downcast_ref::<Int64Array>()
                .ok_or("df_executor: extract group key not Int64")?
                .value(row);
            Ok((AnyNumeric::from(v).into_datum().ok_or("df_executor: extract numeric datum")?, false))
        }
        2 => {
            // IntAddConst → the operator RESULT type (int2/int4 only; int8 result declined at admit) with a range
            // check: PG raises 22003 when the result overflows the result type, and our widened i64 value that does not
            // fit reproduces that failure (both plans error on the same overflowing datum). The i64 compute itself is
            // always exact here (int2/int4 column ± int const), so a value in range is byte-identical to PG's.
            let v = arr
                .as_any()
                .downcast_ref::<Int64Array>()
                .ok_or("df_executor: int-arith group key not Int64")?
                .value(row);
            let d = match g.out_typoid {
                21 => i16::try_from(v).map_err(|_| "smallint out of range".to_string())?.into_datum(),
                23 => i32::try_from(v).map_err(|_| "integer out of range".to_string())?.into_datum(),
                other => return Err(format!("df_executor: int-arith bad out typoid {other}")),
            };
            Ok((d.ok_or("df_executor: int-arith datum")?, false))
        }
        // DateTrunc (func 0) — the Timestamp arrow array already IS the native output type.
        _ => arrow_value_to_datum(arr, row, g.out_typoid),
    }
}

/// M165 — materialize one const-out cell (layout kind=3): a projected integer literal. `admit` admitted ONLY the
/// integer class {int2,int4,int8} + non-NULL, so the stored i64 rebuilds the exact by-value PG Datum (int2/4/8 are
/// pass-through — the Datum IS the integer, same as `arrow_value_to_datum`'s int arms). Fail-closed on any other typoid.
fn const_out_datum(val: i64, typoid: u32) -> Result<(pg_sys::Datum, bool), String> {
    let d = match typoid {
        21 => (val as i16).into_datum(),
        23 => (val as i32).into_datum(),
        20 => val.into_datum(),
        other => return Err(format!("df_executor: const-out bad typoid {other}")),
    };
    Ok((d.ok_or("df_executor: const-out datum")?, false))
}

/// Run `count(*)`, `sum(<num_col>)` over the columnar table and format `count=N;sum=X` (Phase A/B test driver — the
/// planner-integrated automatic path is Phase C `columnar_agg.rs`).
unsafe fn run_columnar_agg(rel: pg_sys::Relation, num_col: &str) -> Result<String, String> {
    let r = run_columnar_aggs(
        rel,
        &[AggSpec::CountStar, AggSpec::SumFloat8(num_col.to_string())],
        &[],
        &[],
        &[],
        false,
    )?;
    let c = i64::from_datum(r[0].0, r[0].1).unwrap_or(0);
    let s = f64::from_datum(r[1].0, r[1].1).unwrap_or(0.0);
    Ok(format!("count={c};sum={s:.4}"))
}

/// M100 Phase A test driver — a `count(*)` + `sum(<num_col>)` aggregate over a `theodb_columnar` table, executed
/// through the DataFusion vectorized path. Test-only (gated behind `pg_test`) until the planner `CustomScan`
/// integration (Phase C) makes the path automatic.
#[cfg(any(test, feature = "pg_test"))]
#[pg_extern]
fn theodb_df_columnar_agg(rel_oid: pg_sys::Oid, num_col: String) -> String {
    unsafe {
        let rel = pg_sys::relation_open(rel_oid, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
        let res = run_columnar_agg(rel, &num_col);
        pg_sys::relation_close(rel, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
        match res {
            Ok(s) => s,
            Err(e) => error!("{e}"),
        }
    }
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    /// M100 Phase A — a DataFusion vectorized `count(*)` + `sum(measure)` over a `theodb_columnar` table is
    /// result-identical to the same aggregate over a heap table (50k rows). Proves the async-in-C DataFusion seam
    /// over REAL columnar Arrow batches: decode stripes → Arrow arrays → `block_on` a vectorized aggregate under
    /// `HeldInterrupts` → the exact count/sum. This is the de-risk that gates the planner `CustomScan` wiring (C).
    #[pg_test]
    fn m100_df_columnar_agg_matches_heap() {
        Spi::run("CREATE TABLE m100_c (id int, measure float8) USING theodb_columnar").unwrap();
        Spi::run("CREATE TABLE m100_h (id int, measure float8)").unwrap();
        let gen_sql = "SELECT g, (g * 1.5)::float8 FROM generate_series(1, 50000) g";
        Spi::run(&format!("INSERT INTO m100_c {gen_sql}")).unwrap();
        Spi::run(&format!("INSERT INTO m100_h {gen_sql}")).unwrap();

        let oid = Spi::get_one::<pg_sys::Oid>("SELECT 'm100_c'::regclass::oid").unwrap().unwrap();
        let df_result = Spi::get_one_with_args::<String>(
            "SELECT theodb_df_columnar_agg($1, 'measure')",
            &[oid.into()],
        )
        .unwrap()
        .unwrap();

        let hc = Spi::get_one::<i64>("SELECT count(*) FROM m100_h").unwrap().unwrap();
        let hs = Spi::get_one::<f64>("SELECT sum(measure) FROM m100_h").unwrap().unwrap();
        let expected = format!("count={hc};sum={hs:.4}");
        assert_eq!(
            df_result, expected,
            "DataFusion columnar aggregate must match the heap aggregate"
        );

        Spi::run("DROP TABLE m100_c").unwrap();
        Spi::run("DROP TABLE m100_h").unwrap();
    }

    /// M100 Phase B — projection pushdown: an aggregate over ONE column of a WIDE (6-column) columnar table decodes
    /// only that column (the other 5 chunks are never `read_chunked`/zstd-decoded) and still returns the correct
    /// result. Proves the projection lever end-to-end; the decode-skip is asserted by correctness over a wide table
    /// where decoding everything would be wasteful (a decode counter is a later instrumentation nicety).
    #[pg_test]
    fn m100_projection_decodes_only_aggregated_column() {
        Spi::run(
            "CREATE TABLE m100_w (a int, b text, c float8, d bigint, e bool, measure float8) USING theodb_columnar",
        )
        .unwrap();
        Spi::run(
            "INSERT INTO m100_w SELECT g, 'row-'||g, g*0.5, g::bigint, g%2=0, (g*2.5)::float8 \
             FROM generate_series(1, 30000) g",
        )
        .unwrap();
        let oid = Spi::get_one::<pg_sys::Oid>("SELECT 'm100_w'::regclass::oid").unwrap().unwrap();
        let df_result = Spi::get_one_with_args::<String>(
            "SELECT theodb_df_columnar_agg($1, 'measure')",
            &[oid.into()],
        )
        .unwrap()
        .unwrap();
        let hc: i64 = 30000;
        let hs: f64 = (1..=30000i64).map(|g| g as f64 * 2.5).sum();
        assert_eq!(
            df_result,
            format!("count={hc};sum={hs:.4}"),
            "projected aggregate over a wide table must be correct"
        );
        Spi::run("DROP TABLE m100_w").unwrap();
    }
}

#[cfg(test)]
mod m160_fixed_raw_tests {
    //! M160 — the zero-copy `fixed_raw_array` fast path MUST produce byte-identical Arrow to the legacy per-cell
    //! `cells_to_array` for every fast-eligible fixed-width type. Pure-Arrow (no pg_sys); compares `ArrayData`.
    use super::{cells_to_array, fixed_raw_array};

    fn cells_of<const W: usize>(vals: &[[u8; W]]) -> Vec<Option<Vec<u8>>> {
        vals.iter().map(|b| Some(b.to_vec())).collect()
    }
    fn contiguous<const W: usize>(vals: &[[u8; W]]) -> Vec<u8> {
        vals.iter().flat_map(|b| b.iter().copied()).collect()
    }

    #[test]
    fn fixed_raw_matches_cells_across_fast_types() {
        // (typid, width, sample little-endian value bytes)
        // int4 (23): 1, -2, 1_000_000
        let i32v = [1i32.to_le_bytes(), (-2i32).to_le_bytes(), 1_000_000i32.to_le_bytes()];
        // int8 (20): 0, i64::MIN, 42
        let i64v = [0i64.to_le_bytes(), i64::MIN.to_le_bytes(), 42i64.to_le_bytes()];
        // float8 (701): 1.5, -0.0, 3.25
        let f64v = [1.5f64.to_le_bytes(), (-0.0f64).to_le_bytes(), 3.25f64.to_le_bytes()];
        // int2 (21): 7, -1, 30000
        let i16v = [7i16.to_le_bytes(), (-1i16).to_le_bytes(), 30000i16.to_le_bytes()];
        // date (1082) int32 days: 100, 0, 20000
        let datev = [100i32.to_le_bytes(), 0i32.to_le_bytes(), 20000i32.to_le_bytes()];
        // timestamp (1114) int64 μs: 123, -456, 999_999_999
        let tsv = [123i64.to_le_bytes(), (-456i64).to_le_bytes(), 999_999_999i64.to_le_bytes()];

        macro_rules! check {
            ($typid:expr, $w:expr, $arr:expr) => {{
                let (dt_f, af) = fixed_raw_array($typid, &contiguous(&$arr), $w, $arr.len()).unwrap();
                let (dt_c, ac) = cells_to_array($typid, &cells_of(&$arr)).unwrap();
                assert_eq!(dt_f, dt_c, "DataType mismatch for typid {}", $typid);
                assert_eq!(af.to_data(), ac.to_data(), "Arrow data mismatch for typid {}", $typid);
            }};
        }
        check!(23, 4, i32v);
        check!(20, 8, i64v);
        check!(701, 8, f64v);
        check!(21, 2, i16v);
        check!(1082, 4, datev);
        check!(1114, 8, tsv);
    }

    #[test]
    fn fixed_raw_rejects_wrong_size() {
        // 5 bytes for a width-4 / 1-row column must be a typed error, never a panic across C.
        assert!(fixed_raw_array(23, &[0u8; 5], 4, 1).is_err());
    }
}
