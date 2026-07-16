//! M101 Phase A — heap-authoritative in-memory Arrow columnar cache.
//!
//! A `theodb_columnarize(table, cols)` pragma builds an in-memory Arrow `RecordBatch` from a HEAP table's projected
//! columns (via SPI over the heap's committed rows), which the M100 DataFusion executor aggregates. The heap stays
//! the source of truth: the cache is a derived read-only replica. Phase A proves the heap→Arrow build + aggregate;
//! invalidate-on-write + the snapshot-compatibility gate (the MVCC substrate) are Phase B, and the planner
//! `CustomScan` wiring is Phase C. Own-code glue (Rule 9); Apache-2.0 `arrow`/`datafusion` the adopted engine.
#![allow(non_snake_case)]

use super::df_executor::{build_arrow, run_aggs_on_batch, AggSpec};
use datafusion::arrow::record_batch::RecordBatch;
use pgrx::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

thread_local! {
    /// Per-backend Arrow cache: relation OID → the cached RecordBatch. (Shared-memory residency is a follow-up;
    /// per-backend is the simplest MVCC-safe slice — each backend builds under its own snapshot.)
    static CACHE: RefCell<HashMap<u32, RecordBatch>> = RefCell::new(HashMap::new());
}

/// Extract one SPI cell (row column `col1`, 1-based) as the byte layout `build_arrow` consumes (fixed: attlen LE
/// bytes; text: raw UTF-8). The PG builtin type OID drives both the SPI get and the Arrow mapping.
fn encode_cell(
    row: &pgrx::spi::SpiHeapTupleData,
    col1: usize,
    typid: u32,
) -> Result<Option<Vec<u8>>, String> {
    Ok(match typid {
        21 => row.get::<i16>(col1).map_err(|e| format!("{e:?}"))?.map(|v| v.to_le_bytes().to_vec()),
        23 => row.get::<i32>(col1).map_err(|e| format!("{e:?}"))?.map(|v| v.to_le_bytes().to_vec()),
        20 => row.get::<i64>(col1).map_err(|e| format!("{e:?}"))?.map(|v| v.to_le_bytes().to_vec()),
        700 => row.get::<f32>(col1).map_err(|e| format!("{e:?}"))?.map(|v| v.to_le_bytes().to_vec()),
        701 => row.get::<f64>(col1).map_err(|e| format!("{e:?}"))?.map(|v| v.to_le_bytes().to_vec()),
        16 => row.get::<bool>(col1).map_err(|e| format!("{e:?}"))?.map(|v| vec![v as u8]),
        25 | 1042 | 1043 => row.get::<String>(col1).map_err(|e| format!("{e:?}"))?.map(|v| v.into_bytes()),
        other => return Err(format!("arrow_cache: unsupported column type oid {other}")),
    })
}

/// Build the Arrow cache batch for `rel_oid`'s `cols` from the heap (SPI seqscan over committed rows).
unsafe fn build_cache(rel_oid: pg_sys::Oid, cols: &[String]) -> Result<RecordBatch, String> {
    // Resolve (name, typid) per requested column from the live tuple descriptor.
    let rel = pg_sys::relation_open(rel_oid, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
    let tupdesc = (*rel).rd_att;
    let natts = (*tupdesc).natts as usize;
    let mut meta: Vec<(String, u32)> = Vec::new();
    for name in cols {
        let idx = (0..natts)
            .find(|&i| {
                std::ffi::CStr::from_ptr((*(*tupdesc).attrs.as_ptr().add(i)).attname.data.as_ptr()).to_string_lossy()
                    == name.as_str()
            })
            .ok_or_else(|| format!("arrow_cache: column '{name}' not found"))?;
        let typid = (*(*tupdesc).attrs.as_ptr().add(idx)).atttypid.to_u32();
        meta.push((name.clone(), typid));
    }
    let relname = std::ffi::CStr::from_ptr(pg_sys::get_rel_name(rel_oid)).to_string_lossy().into_owned();
    let nsp = pg_sys::get_namespace_name(pg_sys::get_rel_namespace(rel_oid));
    let nspname = std::ffi::CStr::from_ptr(nsp).to_string_lossy().into_owned();
    pg_sys::relation_close(rel, pg_sys::AccessShareLock as pg_sys::LOCKMODE);

    let collist = cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
    let sql = format!("SELECT {collist} FROM \"{nspname}\".\"{relname}\"");
    Spi::connect(|c| {
        let t = c.select(&sql, None, &[]).map_err(|e| format!("arrow_cache: cache build select: {e:?}"))?;
        let ncol = meta.len();
        let mut columns: Vec<Vec<Option<Vec<u8>>>> = vec![Vec::new(); ncol];
        for row in t {
            for (i, (_, typid)) in meta.iter().enumerate() {
                columns[i].push(encode_cell(&row, i + 1, *typid)?);
            }
        }
        let arrow_cols: Vec<(String, u32, Vec<Option<Vec<u8>>>)> = meta
            .iter()
            .enumerate()
            .map(|(i, (name, typid))| (name.clone(), *typid, std::mem::take(&mut columns[i])))
            .collect();
        let (schema, arrays) = build_arrow(&arrow_cols)?;
        RecordBatch::try_new(Arc::new(schema), arrays).map_err(|e| format!("arrow_cache: batch: {e}"))
    })
}

/// Pragma: build (or rebuild) the Arrow cache for a heap table's columns.
#[pg_extern]
fn theodb_columnarize(table: pg_sys::Oid, cols: Vec<String>) -> bool {
    unsafe {
        match build_cache(table, &cols) {
            Ok(batch) => {
                CACHE.with(|c| c.borrow_mut().insert(table.to_u32(), batch));
                true
            }
            Err(e) => error!("{e}"),
        }
    }
}

#[cfg(any(test, feature = "pg_test"))]
#[pg_extern]
fn theodb_cache_agg(table: pg_sys::Oid, num_col: String) -> String {
    unsafe {
        let batch = CACHE.with(|c| c.borrow().get(&table.to_u32()).cloned());
        let Some(batch) = batch else { error!("arrow_cache: no cache for this table (call theodb_columnarize first)") };
        let res = run_aggs_on_batch(batch, &[AggSpec::CountStar, AggSpec::SumFloat8(num_col)]);
        match res {
            Ok(r) => {
                let cnt = i64::from_datum(r[0].0, r[0].1).unwrap_or(0);
                let sm = f64::from_datum(r[1].0, r[1].1).unwrap_or(0.0);
                format!("count={cnt};sum={sm:.4}")
            }
            Err(e) => error!("{e}"),
        }
    }
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    /// M101 Phase A — the Arrow cache built from a HEAP table (via SPI) aggregates result-identical to the heap.
    /// Proves the heap→Arrow build + the DataFusion aggregate over the cache batch (the substrate before the MVCC
    /// invalidation machinery of Phase B).
    #[pg_test]
    fn m101_cache_agg_matches_heap() {
        Spi::run("CREATE TABLE m101_h (id int, measure float8)").unwrap();
        Spi::run("INSERT INTO m101_h SELECT g, (g * 1.5)::float8 FROM generate_series(1, 50000) g").unwrap();
        let oid = Spi::get_one::<pg_sys::Oid>("SELECT 'm101_h'::regclass::oid").unwrap().unwrap();
        let built = Spi::get_one_with_args::<bool>(
            "SELECT theodb_columnarize($1, ARRAY['measure'])",
            &[oid.into()],
        )
        .unwrap()
        .unwrap();
        assert!(built, "the cache must build");

        let cache = Spi::get_one_with_args::<String>("SELECT theodb_cache_agg($1, 'measure')", &[oid.into()])
            .unwrap()
            .unwrap();
        let hc = Spi::get_one::<i64>("SELECT count(*) FROM m101_h").unwrap().unwrap();
        let hs = Spi::get_one::<f64>("SELECT sum(measure) FROM m101_h").unwrap().unwrap();
        assert_eq!(cache, format!("count={hc};sum={hs:.4}"), "the cache aggregate must match the heap");
        Spi::run("DROP TABLE m101_h").unwrap();
    }
}
