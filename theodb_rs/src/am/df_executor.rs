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

/// Run `SELECT count(*), sum(<num_col>) FROM t` over the columnar table via DataFusion; return `count=N;sum=X`.
unsafe fn run_columnar_agg(rel: pg_sys::Relation, num_col: &str) -> Result<String, String> {
    let cols = super::columnar::decode_columns(rel)?;
    let (schema, arrays) = build_arrow(&cols)?;
    let batch =
        RecordBatch::try_new(Arc::new(schema), arrays).map_err(|e| format!("df_executor: arrow batch: {e}"))?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .map_err(|e| format!("df_executor: tokio runtime: {e}"))?;
    let held = HeldInterrupts::hold();
    // DataFrame aggregate API (no SQL parser needed): count(*) + sum(<num_col>).
    let num_col = num_col.to_string();
    let out: Result<Vec<RecordBatch>, DataFusionError> = rt.block_on(async move {
        let ctx = SessionContext::new();
        let df = ctx.read_batch(batch)?.aggregate(
            vec![],
            vec![count(lit(1i64)).alias("c"), sum(col(num_col.as_str())).alias("s")],
        )?;
        df.collect().await
    });
    drop(held);
    drop(rt);

    let batches = out.map_err(|e| format!("df_executor: DataFusion: {e}"))?;
    let b = batches.first().ok_or("df_executor: no result batch")?;
    let c = b
        .column(0)
        .as_any()
        .downcast_ref::<Int64Array>()
        .ok_or("df_executor: count not Int64")?
        .value(0);
    let s_col = b.column(1);
    let s = if s_col.is_null(0) {
        0.0
    } else {
        s_col.as_any().downcast_ref::<Float64Array>().ok_or("df_executor: sum not Float64")?.value(0)
    };
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
}
