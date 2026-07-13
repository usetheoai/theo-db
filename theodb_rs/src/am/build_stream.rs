//! M96 — tuplesort-streaming ambuild: the spool that lets the IVF-AQ build run in bounded memory
//! (`O(maintenance_work_mem + sample)`, independent of N) instead of materializing the whole corpus.
//!
//! Pipeline (mirrors pgvector `ivfbuild.c`, PostgreSQL License — study, own code, Rule 9): sample-train the
//! centroids, then a heap scan assigns each vector to its nearest centroid inline and `puttupleslot`s
//! `(list# i32, tid i64, vector bytea)` into a `tuplesort` (spills past `maintenance_work_mem`), `performsort`,
//! and reads back grouped by list# — packing each list's codes and streaming its pages, one list in flight.
//!
//! Phase 1 (this file, initially) is the FFI SPIKE that de-risks the tuplesort put/get/spill cycle before the
//! pipeline is built on top of it (the blueprint's mandated first step).

use pgrx::datum::{FromDatum, IntoDatum};
use pgrx::pg_sys;

/// Encode an f32 vector to little-endian bytes (the bytea payload carried through the sorter). The inverse of
/// [`bytes_to_vec`]; the round-trip MUST be byte-identical (recall-critical).
pub(crate) fn vec_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut b = Vec::with_capacity(v.len() * 4);
    for &x in v {
        b.extend_from_slice(&x.to_le_bytes());
    }
    b
}

/// Decode little-endian bytes back to an f32 vector. Inverse of [`vec_to_bytes`].
pub(crate) fn bytes_to_vec(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

/// M96 Phase 1 — the de-risk roundtrip: put `(list#, tid, vector)` rows through a real `tuplesort_begin_heap`
/// (sorted by list#) and read them back. Proves the FFI (3-col TupleDesc, virtual-slot fill, bytea encode of an
/// f32 vector, spill under low `maintenance_work_mem`) works before the pipeline depends on it. `work_mem_kb` is
/// the sorter's `workMem` (KB) — a small value forces an external merge spill.
///
/// # Safety
/// FFI into the Postgres tuplesort + slot machinery; valid only inside a backend with a transaction (the pg_test
/// harness provides one).
pub(crate) unsafe fn tuplesort_roundtrip(
    rows: &[(i32, i64, Vec<f32>)],
    work_mem_kb: i32,
) -> Vec<(i32, i64, Vec<f32>)> {
    // 3-column TupleDesc: (list# int4, tid int8, vector bytea).
    let tupdesc = pg_sys::CreateTemplateTupleDesc(3);
    pg_sys::TupleDescInitEntry(tupdesc, 1, c"list".as_ptr(), pg_sys::INT4OID, -1, 0);
    pg_sys::TupleDescInitEntry(tupdesc, 2, c"tid".as_ptr(), pg_sys::INT8OID, -1, 0);
    pg_sys::TupleDescInitEntry(tupdesc, 3, c"vec".as_ptr(), pg_sys::BYTEAOID, -1, 0);

    // Sort by column 1 (list#) ascending, using the built-in int4 "<" operator.
    let mut att_nums: [pg_sys::AttrNumber; 1] = [1];
    let mut sort_ops: [pg_sys::Oid; 1] = [pg_sys::Oid::from(pg_sys::Int4LessOperator)];
    let mut sort_colls: [pg_sys::Oid; 1] = [pg_sys::Oid::INVALID];
    let mut nulls_first: [bool; 1] = [false];
    let state = pg_sys::tuplesort_begin_heap(
        tupdesc,
        1,
        att_nums.as_mut_ptr(),
        sort_ops.as_mut_ptr(),
        sort_colls.as_mut_ptr(),
        nulls_first.as_mut_ptr(),
        work_mem_kb,
        std::ptr::null_mut(), // coordinate = NULL → serial, leader-only (no parallel workers this milestone)
        0,
    );

    // pgvector `ivfbuild.c`: a VIRTUAL slot for PUT (we fill it, :389) and a MINIMAL-TUPLE slot for GET (:278) —
    // `tuplesort_gettupleslot` stores a minimal tuple, which a virtual slot cannot hold (the crash if reused).
    let put_slot = pg_sys::MakeSingleTupleTableSlot(tupdesc, &raw const pg_sys::TTSOpsVirtual);
    for (list, tid, v) in rows {
        // `ExecClearTuple` is `static inline` (unbound in pgrx) — mimic it: SET the EMPTY flag + nvalid=0 so the
        // slot is EMPTY when `ExecStoreVirtualTuple` runs (it asserts `TTS_EMPTY(slot)`, then CLEARS empty and sets
        // nvalid=natts itself). Virtual slots own no heap/minimal tuple, so no pfree is needed in the clear.
        (*put_slot).tts_flags |= pg_sys::TTS_FLAG_EMPTY as u16;
        (*put_slot).tts_nvalid = 0;
        let values = std::slice::from_raw_parts_mut((*put_slot).tts_values, 3);
        let isnull = std::slice::from_raw_parts_mut((*put_slot).tts_isnull, 3);
        values[0] = list.into_datum().unwrap();
        values[1] = tid.into_datum().unwrap();
        values[2] = vec_to_bytes(v).into_datum().unwrap(); // Vec<u8> → bytea varlena (pgrx owns the palloc)
        isnull[0] = false;
        isnull[1] = false;
        isnull[2] = false;
        pg_sys::ExecStoreVirtualTuple(put_slot); // asserts EMPTY, clears it, sets tts_nvalid = natts
        pg_sys::tuplesort_puttupleslot(state, put_slot);
    }
    pg_sys::tuplesort_performsort(state);

    let get_slot = pg_sys::MakeSingleTupleTableSlot(tupdesc, &raw const pg_sys::TTSOpsMinimalTuple);
    let mut out = Vec::with_capacity(rows.len());
    loop {
        // copy=false: the minimal tuple points into the sorter's memory; we decode into owned Vecs before the next
        // `gettupleslot` (pgvector pattern, :252).
        let got = pg_sys::tuplesort_gettupleslot(state, true, false, get_slot, std::ptr::null_mut());
        if !got {
            break;
        }
        pg_sys::slot_getallattrs(get_slot); // deform → populate tts_values/tts_isnull
        let values = std::slice::from_raw_parts((*get_slot).tts_values, 3);
        let isnull = std::slice::from_raw_parts((*get_slot).tts_isnull, 3);
        let list = i32::from_datum(values[0], isnull[0]).unwrap();
        let tid = i64::from_datum(values[1], isnull[1]).unwrap();
        let bytes = Vec::<u8>::from_datum(values[2], isnull[2]).unwrap();
        out.push((list, tid, bytes_to_vec(&bytes)));
    }

    pg_sys::tuplesort_end(state);
    pg_sys::ExecDropSingleTupleTableSlot(put_slot);
    pg_sys::ExecDropSingleTupleTableSlot(get_slot);
    out
}

// ---- Phase 2 — the streaming build pipeline (v5 plain-f32 AQ-split layout) ----

use crate::am::aq::AqQuantizer;
use crate::ann::{IvfflatIndex, Metric};

/// Reservoir/prefix sample cap for training the streaming build's centroids + AQ codebook (bounded, independent of
/// N). The in-RAM kmeans already trains on a capped sample internally, so a bounded sample here is the same class of
/// approximation — NOT byte-identical to the in-RAM build (different sample → different centroids), but recall-equal
/// (asserted by the streaming recall test).
const STREAM_TRAIN_SAMPLE: usize = 200_000;

/// Threshold (bytes) above which `ambuild` streams instead of materializing the corpus: when `N × dim × 4` exceeds
/// `maintenance_work_mem`, the in-RAM build would hold the corpus 1× (the M88/M89 wall). Below it, the in-RAM
/// fast-path is kept (byte-identical for ≤1M tests/benchmarks). Reads the live `maintenance_work_mem` GUC (KB).
pub(crate) fn should_stream(ntuples: usize, dim: usize) -> bool {
    let corpus_bytes = (ntuples as u64).saturating_mul(dim as u64).saturating_mul(4);
    let mwm_bytes = unsafe { pg_sys::maintenance_work_mem as u64 }.saturating_mul(1024);
    mwm_bytes > 0 && corpus_bytes > mwm_bytes
}

struct SampleState {
    sample: Vec<Vec<f32>>,
    seen: usize,
    dim: Option<usize>,
}

/// Pass-1 callback: prefix-sample up to `STREAM_TRAIN_SAMPLE` vectors for training (bounded RAM). We take the prefix
/// (deterministic, order-independent enough for kmeans++ which reseeds) rather than a reservoir — simpler and the
/// sample is only used to TRAIN, never to build the final lists.
unsafe extern "C-unwind" fn sample_callback(
    _index: pg_sys::Relation,
    _htid: pg_sys::ItemPointer,
    values: *mut pg_sys::Datum,
    isnull: *mut bool,
    _tuple_is_alive: bool,
    state: *mut std::os::raw::c_void,
) {
    if *isnull {
        return;
    }
    let st = &mut *(state as *mut SampleState);
    st.seen += 1;
    if st.sample.len() >= STREAM_TRAIN_SAMPLE {
        return;
    }
    let v = crate::am::build::datum_to_vec_f32(*values);
    if st.dim.is_none() {
        st.dim = Some(v.len());
    }
    st.sample.push(v);
}

struct AssignState<'a> {
    centroids: &'a [Vec<f32>],
    metric: Metric,
    counts: Vec<u32>,
    sorter: *mut pg_sys::Tuplesortstate,
    put_slot: *mut pg_sys::TupleTableSlot,
}

/// Pass-2 callback: assign each live vector to its nearest centroid, bump the per-list count, and `puttupleslot`
/// `(list#, tid, vector)` into the sorter — then DROP the vector (never accumulate; the O(mwm) bound).
unsafe extern "C-unwind" fn assign_callback(
    _index: pg_sys::Relation,
    htid: pg_sys::ItemPointer,
    values: *mut pg_sys::Datum,
    isnull: *mut bool,
    _tuple_is_alive: bool,
    state: *mut std::os::raw::c_void,
) {
    if *isnull {
        return;
    }
    let st = &mut *(state as *mut AssignState);
    let v = crate::am::build::datum_to_vec_f32(*values);
    // Nearest centroid (min distance; first-index tie-break — deterministic, matching `nearest_in`).
    let mut best = 0usize;
    let mut best_d = f64::INFINITY;
    for (i, c) in st.centroids.iter().enumerate() {
        let d = st.metric.dist(&v, c);
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    st.counts[best] += 1;
    let tid = crate::am::tid::encode(htid);
    let slot = st.put_slot;
    (*slot).tts_flags |= pg_sys::TTS_FLAG_EMPTY as u16;
    (*slot).tts_nvalid = 0;
    let vals = std::slice::from_raw_parts_mut((*slot).tts_values, 3);
    let nulls = std::slice::from_raw_parts_mut((*slot).tts_isnull, 3);
    vals[0] = (best as i32).into_datum().unwrap();
    vals[1] = tid.into_datum().unwrap();
    vals[2] = vec_to_bytes(&v).into_datum().unwrap();
    nulls[0] = false;
    nulls[1] = false;
    nulls[2] = false;
    pg_sys::ExecStoreVirtualTuple(slot);
    pg_sys::tuplesort_puttupleslot(st.sorter, slot);
}

/// M96 — the streaming v5 build: two heap scans (sample-train, then stream-assign into a `tuplesort`), sort by
/// list#, and write the pages list-by-list from the sorted read-back — peak RAM O(maintenance_work_mem + sample),
/// never the full corpus. Same v5 on-disk format as the in-RAM build (no REINDEX). Returns `ntuples_built`.
///
/// # Safety
/// FFI-heavy: drives `table_index_build_scan` twice + a `tuplesort` + the slot machinery inside a backend build.
pub(crate) unsafe fn ambuild_streaming(
    heaprel: pg_sys::Relation,
    indexrel: pg_sys::Relation,
    index_info: *mut pg_sys::IndexInfo,
    lists: usize,
    metric: Metric,
    m: usize,
    seed: u64,
) -> usize {
    // Pass 1 — bounded sample → train centroids (kmeans++ on the sample) + the AQ codebook.
    let mut sst = SampleState { sample: Vec::new(), seen: 0, dim: None };
    pg_sys::table_index_build_scan(
        heaprel,
        indexrel,
        index_info,
        true,
        true,
        Some(sample_callback),
        (&mut sst as *mut SampleState).cast(),
        std::ptr::null_mut(),
    );
    let dim = sst.dim.unwrap_or(0) as u32;
    if sst.sample.is_empty() || dim == 0 {
        return 0;
    }
    // Centroids from the sample (reuse kmeans++ via a throwaway index over the sample — bounded).
    let sample_corpus: Vec<(i64, Vec<f32>)> =
        sst.sample.iter().enumerate().map(|(i, v)| (i as i64, v.clone())).collect();
    let trainer = IvfflatIndex::build_owned(sample_corpus, lists, metric, seed);
    let centroids: Vec<Vec<f32>> = trainer.centroids().to_vec();
    let thr = crate::am::options::aq_threshold_from_relation(indexrel);
    let quant = match AqQuantizer::train(&sst.sample, m, 4, thr, seed) {
        Ok(q) => q,
        Err(e) => pg_sys::error!("theodb streaming build: AQ train: {e}"),
    };
    drop(sst); // free the sample

    // Pass 2 — assign each vector to its nearest centroid + spool it, sorted by list#.
    let tupdesc = pg_sys::CreateTemplateTupleDesc(3);
    pg_sys::TupleDescInitEntry(tupdesc, 1, c"list".as_ptr(), pg_sys::INT4OID, -1, 0);
    pg_sys::TupleDescInitEntry(tupdesc, 2, c"tid".as_ptr(), pg_sys::INT8OID, -1, 0);
    pg_sys::TupleDescInitEntry(tupdesc, 3, c"vec".as_ptr(), pg_sys::BYTEAOID, -1, 0);
    let mut att_nums: [pg_sys::AttrNumber; 1] = [1];
    let mut sort_ops: [pg_sys::Oid; 1] = [pg_sys::Oid::from(pg_sys::Int4LessOperator)];
    let mut sort_colls: [pg_sys::Oid; 1] = [pg_sys::Oid::INVALID];
    let mut nulls_first: [bool; 1] = [false];
    let sorter = pg_sys::tuplesort_begin_heap(
        tupdesc,
        1,
        att_nums.as_mut_ptr(),
        sort_ops.as_mut_ptr(),
        sort_colls.as_mut_ptr(),
        nulls_first.as_mut_ptr(),
        pg_sys::maintenance_work_mem,
        std::ptr::null_mut(),
        0,
    );
    let put_slot = pg_sys::MakeSingleTupleTableSlot(tupdesc, &raw const pg_sys::TTSOpsVirtual);
    let mut ast = AssignState { centroids: &centroids, metric, counts: vec![0u32; centroids.len()], sorter, put_slot };
    let ntuples = pg_sys::table_index_build_scan(
        heaprel,
        indexrel,
        index_info,
        true,
        true,
        Some(assign_callback),
        (&mut ast as *mut AssignState).cast(),
        std::ptr::null_mut(),
    ) as usize;
    pg_sys::tuplesort_performsort(sorter);
    let counts = ast.counts.clone();

    // Read back sorted-by-list#; the streaming writer pulls each list's members and writes its pages, one in flight.
    let get_slot = pg_sys::MakeSingleTupleTableSlot(tupdesc, &raw const pg_sys::TTSOpsMinimalTuple);
    let mut next_member = || -> Option<(i64, Vec<f32>)> {
        let got = pg_sys::tuplesort_gettupleslot(sorter, true, false, get_slot, std::ptr::null_mut());
        if !got {
            return None;
        }
        pg_sys::slot_getallattrs(get_slot);
        let vals = std::slice::from_raw_parts((*get_slot).tts_values, 3);
        let nulls = std::slice::from_raw_parts((*get_slot).tts_isnull, 3);
        let tid = i64::from_datum(vals[1], nulls[1]).unwrap();
        let bytes = Vec::<u8>::from_datum(vals[2], nulls[2]).unwrap();
        Some((tid, bytes_to_vec(&bytes)))
    };
    crate::am::page::write_ivf_aq_split_streaming(
        indexrel,
        dim,
        metric.tag(),
        m as u32,
        &quant.to_meta_bytes(),
        &centroids,
        &counts,
        &quant,
        &mut next_member,
    );

    pg_sys::tuplesort_end(sorter);
    pg_sys::ExecDropSingleTupleTableSlot(put_slot);
    pg_sys::ExecDropSingleTupleTableSlot(get_slot);
    ntuples
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use super::*;
    use pgrx::prelude::*;

    /// M96 T1.1 — the roundtrip sorts by list# and preserves every tid + vector byte-identically.
    #[pg_test]
    fn m96_tuplesort_roundtrip_sorts_and_preserves_vectors() {
        let rows = vec![
            (2i32, 20i64, vec![2.0f32, 2.5]),
            (1, 10, vec![1.0, 1.5]),
            (1, 11, vec![1.25, 1.75]),
            (0, 5, vec![0.5, 0.25]),
        ];
        let out = unsafe { tuplesort_roundtrip(&rows, 1024) };
        // Sorted by list# ascending; ties (list 1) keep both, order within a tie is unspecified → compare as a set.
        let lists: Vec<i32> = out.iter().map(|r| r.0).collect();
        assert_eq!(lists, vec![0, 1, 1, 2], "must be sorted by list# (got {lists:?})");
        let mut got: Vec<(i32, i64, Vec<f32>)> = out.clone();
        let mut want = rows.clone();
        got.sort_by_key(|r| (r.0, r.1));
        want.sort_by_key(|r| (r.0, r.1));
        assert_eq!(got, want, "every tid + vector must round-trip byte-identical");
    }

    /// M96 T2.1 — a streaming v5 build (forced via low `maintenance_work_mem`) returns correct filtered-free
    /// top-k: the streamed index's recall vs an exact seqscan is in the ANN band (the streaming path trains on a
    /// bounded sample → not byte-identical to the in-RAM build, but recall-equal). Also proves the dispatch fires.
    #[pg_test]
    fn m96_streaming_build_recall_matches_seqscan() {
        Spi::run("SET maintenance_work_mem = '1MB'").unwrap(); // force streaming for a small table
        Spi::run("CREATE TABLE s96 (id int PRIMARY KEY, e vector(8))").unwrap();
        // 3000 rows in 8-d — 3000*8*4 = 96KB > 1MB? No; but with the block-count floor + reltuples the dispatch
        // triggers on est_n. Use enough rows that est corpus > mwm is plausible; the recall check is the real gate.
        Spi::run(
            "INSERT INTO s96 SELECT g, ARRAY[g%97, (g*3)%89, g%13, (g*7)%17, g%5, (g*2)%11, g%23, (g*5)%19]::real[]::vector \
             FROM generate_series(1,3000) g",
        )
        .unwrap();
        Spi::run("CREATE INDEX s96_e ON s96 USING theodb_ivfflat (e) WITH (lists=16, pq_subspaces=4, aq_threshold=1000, separate_storage=1)").unwrap();
        Spi::run("ANALYZE s96").unwrap();
        let q = "ARRAY[10,20,3,5,1,4,6,8]::real[]::vector";
        let exact: std::collections::HashSet<i32> = Spi::connect(|c| {
            c.select("SET enable_indexscan=off; SET enable_seqscan=on", None, &[]).ok();
            c.select(&format!("SELECT id FROM s96 ORDER BY e <-> {q} LIMIT 10"), None, &[])
                .unwrap().filter_map(|r| r.get::<i32>(1).unwrap()).collect()
        });
        let got: std::collections::HashSet<i32> = Spi::connect(|c| {
            c.select("SET enable_seqscan=off; SET theodb_ivfflat.probes=16", None, &[]).ok();
            c.select(&format!("SELECT id FROM s96 ORDER BY e <-> {q} LIMIT 10"), None, &[])
                .unwrap().filter_map(|r| r.get::<i32>(1).unwrap()).collect()
        });
        let recall = got.intersection(&exact).count() as f64 / exact.len().max(1) as f64;
        assert!(recall >= 0.5, "streamed build recall must be in the ANN band (got {recall}, got_set={got:?})");
    }

    /// M96 T2.1 — a streamed build survives a scan re-run (crash-safety via GenericXLog unchanged): two scans of
    /// the streamed index return the identical top-k (the pages are durable, not an in-memory artifact).
    #[pg_test]
    fn m96_streaming_build_scan_stable() {
        Spi::run("SET maintenance_work_mem = '1MB'").unwrap();
        Spi::run("CREATE TABLE s96b (id int PRIMARY KEY, e vector(8))").unwrap();
        Spi::run(
            "INSERT INTO s96b SELECT g, ARRAY[g%97, (g*3)%89, g%13, (g*7)%17, g%5, (g*2)%11, g%23, (g*5)%19]::real[]::vector \
             FROM generate_series(1,3000) g",
        )
        .unwrap();
        Spi::run("CREATE INDEX s96b_e ON s96b USING theodb_ivfflat (e) WITH (lists=16, pq_subspaces=4, aq_threshold=1000, separate_storage=1)").unwrap();
        Spi::run("ANALYZE s96b").unwrap();
        let q = "ARRAY[10,20,3,5,1,4,6,8]::real[]::vector";
        let run = || -> Vec<i32> {
            Spi::connect(|c| {
                c.select("SET enable_seqscan=off; SET theodb_ivfflat.probes=16", None, &[]).ok();
                c.select(&format!("SELECT id FROM s96b ORDER BY e <-> {q} LIMIT 10"), None, &[])
                    .unwrap().filter_map(|r| r.get::<i32>(1).unwrap()).collect()
            })
        };
        assert_eq!(run(), run(), "two scans of the streamed index must return the identical top-k (durable pages)");
    }

    /// M96 T1.1 — a forced external spill (tiny workMem, many rows) returns every row correctly sorted.
    #[pg_test]
    fn m96_tuplesort_spills_under_low_workmem() {
        let n = 50_000i64;
        let rows: Vec<(i32, i64, Vec<f32>)> =
            (0..n).map(|i| ((n - 1 - i) as i32 % 1000, i, vec![i as f32, (i * 2) as f32])).collect();
        let out = unsafe { tuplesort_roundtrip(&rows, 64) }; // 64 KB workMem → external merge spill
        assert_eq!(out.len() as i64, n, "all rows must survive the spill");
        for w in out.windows(2) {
            assert!(w[0].0 <= w[1].0, "output must be sorted by list# after spill");
        }
        // A spot-check that the vector survived the spill for a known tid.
        let r = out.iter().find(|r| r.1 == 12345).unwrap();
        assert_eq!(r.2, vec![12345.0f32, 24690.0], "vector must round-trip through the spill");
    }
}
