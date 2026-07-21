//! M98 — the DataFusion coexistence SMOKE probe (the pillar GATE, rung M-0).
//!
//! Proves that our LINKED Apache DataFusion (Apache-2.0, upstream `apache/datafusion`) executes a vectorized
//! `ExecutionPlan` INSIDE a real Postgres backend and returns a PG value — the runtime half of the coexistence
//! proof (the build/link + 277-test half is the pgrx-0.19 upgrade). This is deliberately minimal: a single
//! aggregate over a 3-row Arrow `RecordBatch`. The full planner-integrated `CustomScan` executor (planner hooks,
//! qual pushdown, batch materialization) is M100 — this only de-risks that by proving the crate can drive
//! DataFusion end-to-end from a backend without an arrow-version/ABI conflict.
//!
//! Safety (blueprint Q1 artifact, corrected per the M98 review H1): the synchronous `block_on` runs under a
//! `HeldInterrupts` guard that holds off a **query-cancel** (`ProcessInterrupts` → `ereport(ERROR)` → siglongjmp).
//! Without it, a cancel firing mid-`block_on` would longjmp straight PAST the live tokio runtime — never running
//! the Rust `Drop`s that quiesce it, leaking it / tearing PG state. (It does NOT — and cannot — guard a
//! SIGTERM/FATAL `proc_exit`; no holdoff count saves you from that.)
//!
//! M100 NOTE: holding across the WHOLE `block_on` is fine for a 3-row smoke, but the real CustomScan executor must
//! NOT hold across a full columnar scan (that would make the query uncancellable) — it must hold only around the
//! non-reentrant runtime hand-off and service interrupts BETWEEN batches.

use datafusion::arrow::array::{Int64Array, RecordBatch};
use datafusion::arrow::datatypes::{DataType, Field, Schema};
use datafusion::prelude::SessionContext;
use pgrx::prelude::*;
use std::sync::Arc;

/// RAII guard emulating C's `HOLD_INTERRUPTS()`/`RESUME_INTERRUPTS()` (they are macros over
/// `InterruptHoldoffCount`, not callable functions). Holds interrupts for the lifetime of the synchronous
/// `block_on` so a mid-flight `proc_exit` cannot drop the tokio runtime and crash the backend.
struct HeldInterrupts;
impl HeldInterrupts {
    fn hold() -> Self {
        unsafe {
            pg_sys::InterruptHoldoffCount += 1;
        }
        HeldInterrupts
    }
}
impl Drop for HeldInterrupts {
    fn drop(&mut self) {
        unsafe {
            pg_sys::InterruptHoldoffCount -= 1;
        }
    }
}

/// M98 smoke — run a DataFusion aggregate over a 3-row Arrow batch inside this backend and return the row count
/// (3). Proves the linked DataFusion executes + the async runtime works + Arrow round-trips, all in-process in a
/// PG backend. `theodb.enable_df_probe` is not needed — this is a plain `#[pg_extern]`, GUC-free, test-only in
/// spirit (it does nothing a production path calls yet).
#[pg_extern]
fn theodb_df_probe() -> i64 {
    // A current-thread tokio runtime — no IO/timers needed for an in-memory compute query, so no `enable_all()`.
    let rt = match tokio::runtime::Builder::new_current_thread().build() {
        Ok(rt) => rt,
        Err(e) => error!("theodb_df_probe: tokio runtime: {e}"),
    };
    let held = HeldInterrupts::hold();
    let result: Result<usize, datafusion::error::DataFusionError> = rt.block_on(async {
        let ctx = SessionContext::new();
        let schema = Arc::new(Schema::new(vec![Field::new("x", DataType::Int64, false)]));
        let batch =
            RecordBatch::try_new(schema, vec![Arc::new(Int64Array::from(vec![1i64, 2, 3]))])?;
        let df = ctx.read_batch(batch)?;
        // `count()` plans + executes a vectorized aggregate over the batch → the row count.
        df.count().await
    });
    // H2: restore the interrupt holdoff (and drop the tokio runtime) BEFORE the error path — so `error!`'s
    // ereport/panic unwinds with interrupts already resumed, not relying on `panic = "unwind"` running Drop.
    drop(held);
    drop(rt);
    match result {
        Ok(n) => n as i64,
        Err(e) => error!("theodb_df_probe: DataFusion: {e}"),
    }
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    /// M98 T3.1 — the DataFusion-inside-a-backend smoke: a `SELECT theodb_df_probe()` returns the count (3) that
    /// DataFusion computed over a 3-row Arrow batch. Proves the seam links + runs end-to-end (coexistence at
    /// runtime), the pillar GATE's runtime half.
    #[pg_test]
    fn m98_datafusion_runs_in_backend() {
        let n = Spi::get_one::<i64>("SELECT theodb_df_probe()").unwrap().unwrap();
        assert_eq!(n, 3, "DataFusion aggregate over a 3-row Arrow batch must return 3 (got {n})");
    }

    /// M98 T2.1 — a pure in-process DataFusion link probe (no PG): proves the crate links DataFusion + Arrow +
    /// the async runtime independent of the backend. (Runs as a plain unit test under `cargo test`.)
    #[test]
    fn m98_datafusion_links_in_process() {
        use datafusion::arrow::array::{Int64Array, RecordBatch};
        use datafusion::arrow::datatypes::{DataType, Field, Schema};
        use datafusion::prelude::SessionContext;
        use std::sync::Arc;
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let n = rt.block_on(async {
            let ctx = SessionContext::new();
            let schema = Arc::new(Schema::new(vec![Field::new("x", DataType::Int64, false)]));
            let batch =
                RecordBatch::try_new(schema, vec![Arc::new(Int64Array::from(vec![1i64, 2, 3, 4]))])
                    .unwrap();
            ctx.read_batch(batch).unwrap().count().await.unwrap()
        });
        assert_eq!(n, 4, "in-process DataFusion count must return 4");
    }
}
