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

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use super::*;


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
