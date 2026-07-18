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
    /// Per-backend Arrow cache: relation OID → (the cached RecordBatch, the `columnar.cache_state.generation` it was
    /// built at). Reused IFF the current generation is unchanged (no write since the build); otherwise rebuilt under
    /// the reader's own snapshot — which makes the cache snapshot-correct by construction (it materializes exactly
    /// what the reader's snapshot sees). Shared-memory residency is a follow-up; per-backend is the MVCC-safe slice.
    static CACHE: RefCell<HashMap<u32, (RecordBatch, i64)>> = RefCell::new(HashMap::new());
}

// The MVCC substrate (M101 Phase B): a shared `columnar.cache_state` catalog carries the invalidation generation
// per cached table; a statement trigger installed by `columnarize` bumps it on any INSERT/UPDATE/DELETE/TRUNCATE
// (within the writing xact). A read reuses its per-backend cache only when its built generation matches the current
// generation — so a write forces a rebuild under the reader's snapshot (heap-authoritative; the cache never carries
// per-row xmin/xmax visibility — that would re-implement MVCC, the M99 D2 trap).
extension_sql!(
    r#"
CREATE SCHEMA IF NOT EXISTS columnar;
CREATE TABLE IF NOT EXISTS columnar.cache_state (
    relid      oid       PRIMARY KEY,
    generation bigint    NOT NULL DEFAULT 0,
    cols       text[]    NOT NULL
);
CREATE OR REPLACE FUNCTION columnar._invalidate() RETURNS trigger LANGUAGE plpgsql AS $fn$
BEGIN
    UPDATE columnar.cache_state SET generation = generation + 1 WHERE relid = TG_RELID;
    RETURN NULL;
END;
$fn$;
"#,
    name = "theodb_arrow_cache_state",
    requires = ["theodb_columnar_catalog"], // reuse the `columnar` schema created by the M99 columnar catalog
);

/// Read the current invalidation generation + the columnarized columns for `rel_oid`, or None if not columnarized.
///
/// MVCC-LOAD-BEARING: this MUST stay a read-only SPI (`c.select`, not a mutating query). A read-only SPI in a
/// still-immutable xact runs under the reader's ActiveSnapshot, so the generation read here and the seqscan in
/// `build_cache` are CO-SNAPSHOT — that is exactly what makes `built_generation == current_generation` a correct
/// "the committed set I see is the set the cache captured" test (RR-safe). Switching to a mutating SPI (fresh
/// per-statement snapshot) or reading the cache in an already-mutated xact would silently break the RR guarantee
/// (council-index-storage M101 review).
unsafe fn cache_state(rel_oid: pg_sys::Oid) -> Result<Option<(i64, Vec<String>)>, String> {
    Spi::connect(|c| {
        let t = c
            .select(
                "SELECT generation, cols FROM columnar.cache_state WHERE relid = $1",
                None,
                &[rel_oid.into()],
            )
            .map_err(|e| format!("arrow_cache: cache_state read: {e:?}"))?;
        if t.is_empty() {
            return Ok(None);
        }
        let r = t.first();
        let cur_gen = r.get::<i64>(1).map_err(|e| format!("{e:?}"))?.ok_or("null generation")?;
        let cols = r.get::<Vec<String>>(2).map_err(|e| format!("{e:?}"))?.ok_or("null cols")?;
        Ok(Some((cur_gen, cols)))
    })
}

/// Get the cache batch for `rel_oid`, rebuilding it under the CURRENT snapshot when the invalidation generation has
/// advanced since the backend's cache was built (or the cache is absent). Snapshot-correct by construction: a
/// rebuild runs the SPI seqscan under the reader's active snapshot, so it materializes exactly the reader's view.
unsafe fn get_or_build(rel_oid: pg_sys::Oid) -> Result<RecordBatch, String> {
    let oid = rel_oid.to_u32();
    let (cur_gen, cols) = cache_state(rel_oid)?.ok_or("arrow_cache: table not columnarized")?;
    if let Some((batch, built_gen)) = CACHE.with(|c| c.borrow().get(&oid).cloned()) {
        if built_gen == cur_gen {
            return Ok(batch); // no write since the build → the committed set is unchanged → reuse
        }
    }
    let batch = build_cache(rel_oid, &cols)?;
    // M104 — bound the per-backend cache: evict entries until under the entry cap before inserting a new table, so
    // a backend that columnarizes many tables cannot grow the cache without limit (the audit's unbounded-cache
    // finding). `theodb.arrow_cache_max_entries` (default 16). Eviction is arbitrary (HashMap order) — a coarse
    // size bound, not strict LRU; the generation gate makes a re-fetch of an evicted table simply rebuild.
    let cap = crate::am::guc::arrow_cache_max_entries();
    CACHE.with(|c| {
        let mut m = c.borrow_mut();
        while m.len() >= cap && !m.contains_key(&oid) {
            if let Some(&k) = m.keys().next() {
                m.remove(&k);
            } else {
                break;
            }
        }
        m.insert(oid, (batch.clone(), cur_gen));
    });
    Ok(batch)
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

/// Does this backend hold a cache for `relid_u32` whose batch schema contains every column in `cols`? A cheap
/// thread-local check (no SPI) the planner hook uses to decide admission — the exec-time `get_or_build` then does the
/// generation check + snapshot-correct rebuild.
pub(super) fn has_cached_columns(relid_u32: u32, cols: &[String]) -> bool {
    CACHE.with(|c| {
        c.borrow().get(&relid_u32).is_some_and(|(batch, _)| {
            let schema = batch.schema();
            cols.iter().all(|name| schema.field_with_name(name).is_ok())
        })
    })
}

/// Run the aggregates over the heap-authoritative Arrow cache (rebuilding under the current snapshot if a write has
/// invalidated it) — the exec-time entry point the planner `CustomScan` (M101 Phase C) drives for a cached heap table.
pub(super) unsafe fn run_cache_aggs(
    rel_oid: pg_sys::Oid,
    aggs: &[AggSpec],
) -> Result<Vec<(pg_sys::Datum, bool)>, String> {
    let batch = get_or_build(rel_oid)?;
    run_aggs_on_batch(batch, aggs, None)
}

/// Pragma: register a heap table's columns for the Arrow cache — records `columnar.cache_state`, installs the
/// invalidate-on-write statement trigger, and builds the backend's cache. Rebuilds (bumps the generation) if called
/// again. Returns true.
#[pg_extern]
fn theodb_columnarize(table: pg_sys::Oid, cols: Vec<String>) -> bool {
    unsafe {
        let res = (|| -> Result<(), String> {
            // Register / refresh the cache_state row (a re-columnarize bumps the generation so every backend rebuilds).
            Spi::run_with_args(
                "INSERT INTO columnar.cache_state (relid, generation, cols) VALUES ($1, 0, $2) \
                 ON CONFLICT (relid) DO UPDATE SET cols = EXCLUDED.cols, generation = columnar.cache_state.generation + 1",
                &[table.into(), cols.clone().into()],
            )
            .map_err(|e| format!("arrow_cache: cache_state upsert: {e:?}"))?;
            // Install the invalidate-on-write statement trigger on the heap table (idempotent).
            let relname = std::ffi::CStr::from_ptr(pg_sys::get_rel_name(table)).to_string_lossy().into_owned();
            let nspname = std::ffi::CStr::from_ptr(pg_sys::get_namespace_name(pg_sys::get_rel_namespace(table)))
                .to_string_lossy()
                .into_owned();
            let qual = format!("\"{nspname}\".\"{relname}\"");
            Spi::run(&format!("DROP TRIGGER IF EXISTS columnar_invalidate ON {qual}"))
                .map_err(|e| format!("arrow_cache: drop trigger: {e:?}"))?;
            Spi::run(&format!(
                "CREATE TRIGGER columnar_invalidate AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON {qual} \
                 FOR EACH STATEMENT EXECUTE FUNCTION columnar._invalidate()"
            ))
            .map_err(|e| format!("arrow_cache: create trigger: {e:?}"))?;
            // Prime this backend's cache.
            get_or_build(table).map(|_| ())
        })();
        match res {
            Ok(()) => true,
            Err(e) => error!("{e}"),
        }
    }
}

/// Prime THIS backend's Arrow cache from the already-registered `columnar.cache_state` (built under the current
/// snapshot), WITHOUT bumping the invalidation generation. Used to build a reader's cache after another session
/// installed the cache_state/trigger (the per-backend cache is not shared) — e.g. in the isolation harness.
#[pg_extern]
fn theodb_cache_refresh(table: pg_sys::Oid) -> bool {
    unsafe {
        match get_or_build(table) {
            Ok(_) => true,
            Err(e) => error!("{e}"),
        }
    }
}

#[cfg(any(test, feature = "pg_test"))]
#[pg_extern]
fn theodb_cache_agg(table: pg_sys::Oid, num_col: String) -> String {
    unsafe {
        let batch = match get_or_build(table) {
            Ok(b) => b,
            Err(e) => error!("{e}"),
        };
        let res = run_aggs_on_batch(batch, &[AggSpec::CountStar, AggSpec::SumFloat8(num_col)], None);
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

    /// M101 Phase B — invalidate-on-write: after the cache is built, a write bumps the `columnar.cache_state`
    /// generation (the statement trigger), so the next read REBUILDS under the current snapshot and reflects the new
    /// row — the cache never returns a stale answer. Proves the invalidation + rebuild-on-stale substrate (the full
    /// cross-xact snapshot correctness is the Phase D isolation permutations).
    #[pg_test]
    fn m101_write_invalidates_cache() {
        Spi::run("CREATE TABLE m101_iv (id int, measure float8)").unwrap();
        Spi::run("INSERT INTO m101_iv SELECT g, (g * 2.0)::float8 FROM generate_series(1, 10000) g").unwrap();
        let oid = Spi::get_one::<pg_sys::Oid>("SELECT 'm101_iv'::regclass::oid").unwrap().unwrap();
        Spi::get_one_with_args::<bool>("SELECT theodb_columnarize($1, ARRAY['measure'])", &[oid.into()])
            .unwrap()
            .unwrap();

        // Cache reflects the 10000 rows.
        let before = Spi::get_one_with_args::<String>("SELECT theodb_cache_agg($1, 'measure')", &[oid.into()])
            .unwrap()
            .unwrap();
        assert!(before.starts_with("count=10000;"), "cache before write: {before}");

        // A write bumps the generation via the trigger → the next cache read must rebuild and see 10001 rows.
        Spi::run("INSERT INTO m101_iv VALUES (10001, 5.0)").unwrap();
        let after = Spi::get_one_with_args::<String>("SELECT theodb_cache_agg($1, 'measure')", &[oid.into()])
            .unwrap()
            .unwrap();
        let hc = Spi::get_one::<i64>("SELECT count(*) FROM m101_iv").unwrap().unwrap();
        let hs = Spi::get_one::<f64>("SELECT sum(measure) FROM m101_iv").unwrap().unwrap();
        assert_eq!(hc, 10001, "the heap now has 10001 rows");
        assert_eq!(after, format!("count={hc};sum={hs:.4}"), "the cache must rebuild and match the heap after the write");
        assert_ne!(before, after, "the cache result must change after the write (not stale)");
        Spi::run("DROP TABLE m101_iv").unwrap();
    }

    /// M101 Phase C — a heap table WITH a usable Arrow cache is planned as the vectorized `CustomScan` (EXPLAIN shows
    /// the node) and is result-identical to the native heap aggregate; after a write it stays correct (the cache
    /// rebuilds snapshot-correctly at exec). Wires the M101 cache into the M100 planner CustomScan path.
    #[pg_test]
    fn m101_heap_cache_customscan_matches_heap() {
        Spi::run("SET theodb.enable_columnar_agg = on").unwrap();
        Spi::run("CREATE TABLE m101_hc (id int, measure float8)").unwrap();
        Spi::run("INSERT INTO m101_hc SELECT g, (g * 1.5)::float8 FROM generate_series(1, 20000) g").unwrap();
        let oid = Spi::get_one::<pg_sys::Oid>("SELECT 'm101_hc'::regclass::oid").unwrap().unwrap();
        Spi::get_one_with_args::<bool>("SELECT theodb_columnarize($1, ARRAY['measure'])", &[oid.into()])
            .unwrap()
            .unwrap();

        // EXPLAIN: the heap aggregate with a cache is our CustomScan.
        let top = Spi::get_one::<String>("EXPLAIN (COSTS OFF) SELECT count(*), sum(measure) FROM m101_hc")
            .unwrap()
            .unwrap();
        assert!(top.contains("Custom Scan"), "heap-with-cache aggregate must be a CustomScan: {top}");

        // Result-equivalence (the aggregate runs over the Arrow cache).
        let cc = Spi::get_one::<i64>("SELECT count(*) FROM m101_hc").unwrap().unwrap();
        assert_eq!(cc, 20000, "cache count matches");
        let cs = Spi::get_one::<f64>("SELECT sum(measure) FROM m101_hc").unwrap().unwrap();
        let expect = (1..=20000i64).map(|g| g as f64 * 1.5).sum::<f64>();
        assert!((cs - expect).abs() < 1e-2, "cache sum matches the heap ({cs} vs {expect})");

        // After a write, the CustomScan stays correct (get_or_build rebuilds the invalidated cache at exec).
        Spi::run("INSERT INTO m101_hc VALUES (20001, 3.0)").unwrap();
        let cc2 = Spi::get_one::<i64>("SELECT count(*) FROM m101_hc").unwrap().unwrap();
        assert_eq!(cc2, 20001, "after the write, the count reflects the new row (cache rebuilt)");
        Spi::run("DROP TABLE m101_hc").unwrap();
    }
}
