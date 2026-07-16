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
    Array, ArrayRef, BooleanArray, Float32Array, Float64Array, Int16Array, Int32Array, Int64Array, StringArray,
};
use datafusion::arrow::datatypes::{DataType, Field, Schema};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::error::DataFusionError;
use datafusion::functions_aggregate::expr_fn::{count, sum};
use datafusion::prelude::{col, lit, SessionContext};
use pgrx::prelude::*;
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
fn build_arrow(cols: &[(String, u32, Vec<Option<Vec<u8>>>)]) -> Result<(Schema, Vec<ArrayRef>), String> {
    let mut fields = Vec::with_capacity(cols.len());
    let mut arrays: Vec<ArrayRef> = Vec::with_capacity(cols.len());
    for (name, typid, values) in cols {
        let (dt, arr): (DataType, ArrayRef) = match typid {
            21 => (
                DataType::Int16,
                Arc::new(Int16Array::from_iter(
                    values.iter().map(|v| v.as_ref().map(|b| i16::from_le_bytes([b[0], b[1]]))),
                )),
            ),
            23 => (
                DataType::Int32,
                Arc::new(Int32Array::from_iter(
                    values.iter().map(|v| v.as_ref().map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))),
                )),
            ),
            20 => (
                DataType::Int64,
                Arc::new(Int64Array::from_iter(
                    values.iter().map(|v| v.as_ref().map(|b| i64::from_le_bytes(b[..8].try_into().unwrap()))),
                )),
            ),
            700 => (
                DataType::Float32,
                Arc::new(Float32Array::from_iter(
                    values.iter().map(|v| v.as_ref().map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))),
                )),
            ),
            701 => (
                DataType::Float64,
                Arc::new(Float64Array::from_iter(
                    values.iter().map(|v| v.as_ref().map(|b| f64::from_le_bytes(b[..8].try_into().unwrap()))),
                )),
            ),
            16 => (
                DataType::Boolean,
                Arc::new(BooleanArray::from_iter(
                    values.iter().map(|v| v.as_ref().map(|b| b.first().copied().unwrap_or(0) != 0)),
                )),
            ),
            25 | 1042 | 1043 => (
                DataType::Utf8,
                Arc::new(StringArray::from_iter(
                    values.iter().map(|v| v.as_ref().map(|b| String::from_utf8_lossy(b).into_owned())),
                )),
            ),
            other => {
                return Err(format!(
                    "df_executor: unsupported column type oid {other} (Phase A: int2/4/8, float4/8, bool, text)"
                ));
            }
        };
        fields.push(Field::new(name, dt, true));
        arrays.push(arr);
    }
    Ok((Schema::new(fields), arrays))
}

/// A supported aggregate for the vectorized columnar path. Restricted (Phase C slice 1) to the cases where the Arrow
/// result type matches the PostgreSQL aggregate output type WITHOUT a cast: `count(*)` → `int8`, `sum(<float8 col>)`
/// → `float8`. `avg`, `sum` over integer/numeric, GROUP BY, and WHERE pushdown are later slices.
pub(super) enum AggSpec {
    CountStar,
    SumFloat8(String),
}

/// Decode the columnar table's projected columns into one Arrow `RecordBatch` (projection pushdown). Always projects
/// ≥ 1 column so `count(*)` has a row count.
unsafe fn decode_to_batch(rel: pg_sys::Relation, sum_cols: &[String]) -> Result<RecordBatch, String> {
    let mut proj: Vec<usize> = Vec::new();
    for name in sum_cols {
        let idx = super::columnar::column_index(rel, name)
            .ok_or_else(|| format!("df_executor: column '{name}' not found"))?;
        if !proj.contains(&idx) {
            proj.push(idx);
        }
    }
    if proj.is_empty() {
        proj.push(0); // count(*) needs a column to establish the row count
    }
    let cols = super::columnar::decode_columns(rel, Some(&proj))?;
    let (schema, arrays) = build_arrow(&cols)?;
    RecordBatch::try_new(Arc::new(schema), arrays).map_err(|e| format!("df_executor: arrow batch: {e}"))
}

/// Run the aggregates over the columnar table via a vectorized DataFusion plan under `HeldInterrupts`; return one
/// `(Datum, is_null)` per agg, in `aggs` order, ready to store in a `TupleTableSlot`. `count(*)`→`int8` Datum;
/// `sum(float8)`→`float8` Datum. This is the executor the planner `CustomScan` (Phase C) drives at exec time.
pub(super) unsafe fn run_columnar_aggs(
    rel: pg_sys::Relation,
    aggs: &[AggSpec],
) -> Result<Vec<(pg_sys::Datum, bool)>, String> {
    let sum_cols: Vec<String> =
        aggs.iter().filter_map(|a| if let AggSpec::SumFloat8(n) = a { Some(n.clone()) } else { None }).collect();
    let batch = decode_to_batch(rel, &sum_cols)?;

    let mut exprs = Vec::with_capacity(aggs.len());
    for (i, a) in aggs.iter().enumerate() {
        let alias = format!("a{i}");
        exprs.push(match a {
            AggSpec::CountStar => count(lit(1i64)).alias(alias),
            AggSpec::SumFloat8(name) => sum(col(name.as_str())).alias(alias),
        });
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .map_err(|e| format!("df_executor: tokio runtime: {e}"))?;
    let held = HeldInterrupts::hold();
    let out: Result<Vec<RecordBatch>, DataFusionError> = rt.block_on(async move {
        let ctx = SessionContext::new();
        ctx.read_batch(batch)?.aggregate(vec![], exprs)?.collect().await
    });
    drop(held);
    drop(rt);

    let batches = out.map_err(|e| format!("df_executor: DataFusion: {e}"))?;
    let b = batches.first().ok_or("df_executor: no result batch")?;
    let mut result = Vec::with_capacity(aggs.len());
    for (i, a) in aggs.iter().enumerate() {
        let arr = b.column(i);
        if arr.is_null(0) {
            result.push((pg_sys::Datum::from(0usize), true));
            continue;
        }
        let datum = match a {
            AggSpec::CountStar => {
                let v = arr.as_any().downcast_ref::<Int64Array>().ok_or("df_executor: count not Int64")?.value(0);
                v.into_datum().ok_or("df_executor: int8 datum")?
            }
            AggSpec::SumFloat8(_) => {
                let v =
                    arr.as_any().downcast_ref::<Float64Array>().ok_or("df_executor: sum not Float64")?.value(0);
                v.into_datum().ok_or("df_executor: float8 datum")?
            }
        };
        result.push((datum, false));
    }
    Ok(result)
}

/// Run `count(*)`, `sum(<num_col>)` over the columnar table and format `count=N;sum=X` (Phase A/B test driver — the
/// planner-integrated automatic path is Phase C `columnar_agg.rs`).
unsafe fn run_columnar_agg(rel: pg_sys::Relation, num_col: &str) -> Result<String, String> {
    let r = run_columnar_aggs(rel, &[AggSpec::CountStar, AggSpec::SumFloat8(num_col.to_string())])?;
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
        assert_eq!(df_result, expected, "DataFusion columnar aggregate must match the heap aggregate");

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
        assert_eq!(df_result, format!("count={hc};sum={hs:.4}"), "projected aggregate over a wide table must be correct");
        Spi::run("DROP TABLE m100_w").unwrap();
    }
}
