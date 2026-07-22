//! M103 — vector index co-resident with scalar/analytical columns in the columnar substrate (Lance-inspired).
//!
//! The summit (rung M-ε): the IVF vector index (partition-id + raw f32 vector as columns) co-resides with the
//! scalar `label` + the analytical columns in ONE columnar layout, so a scalar-prefiltered top-k vector search +
//! an analytical projection compose in one column-pruned scan. The correctness GATE (ADR D2) is byte-identity: at
//! full probe the columnar filtered top-k equals the EXACT filtered brute-force top-k, proving the co-resident
//! layout loses NO recall. Both paths rerank with the SAME exact kernel (`vec::l2_dist_from_bytes`) and order with
//! the SAME tie-break as the M90-M95 scan (`am/scan.rs::Scored`: `d.total_cmp` then `tid.cmp`) — byte-identity by
//! construction (ADR D1). Own code (Lance is a file-format design study only — D1 license gate).
//!
//! Honest ceiling (ADR D4): a cost/scale/composability win. Recall is EQUAL by construction (the GATE), never a
//! claim; NO QPS-vs-ScaNN claim (the M73/M74 paradigm ceiling is untouched by co-residence).

use std::collections::HashSet;

use pgrx::prelude::*;

use crate::ann::{IvfflatIndex, Metric};
use crate::vec::{l2_dist_from_bytes, l2_distance};

/// Little-endian raw f32 bytes — the on-column encoding of a vector (the `vec` kernels' contract).
fn f32_to_le_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

/// The nearest centroid index (min L2, FIRST-index tie-break — matching the IVF `nearest_in` determinism). Used to
/// assign each row a `part_id` column and to pick the probed partitions for a query.
fn nearest_centroid(centroids: &[Vec<f32>], v: &[f32]) -> usize {
    let mut best = 0usize;
    let mut best_d = f64::INFINITY;
    for (i, c) in centroids.iter().enumerate() {
        let d = l2_distance(c, v);
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    best
}

/// The `probes` nearest centroid indices for a query (the probed partition set). Ordered by the SAME (dist, index)
/// tie-break so the probe set is deterministic; `probes >= nlist` selects every partition (full probe).
fn probed_partitions(centroids: &[Vec<f32>], query: &[f32], probes: usize) -> HashSet<i32> {
    let mut d: Vec<(usize, f64)> =
        centroids.iter().enumerate().map(|(i, c)| (i, l2_distance(c, query))).collect();
    d.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    d.into_iter().take(probes.max(1)).map(|(i, _)| i as i32).collect()
}

/// Top-k selection with the EXACT M90-M95 tie-break (`am/scan.rs::Scored`): order by `f64::total_cmp` on the
/// distance, then by `i64` tid ascending. Shared by the filtered scan AND the byte-identity oracle so the two
/// produce a byte-identical `(tid, dist)` sequence (ADR D1).
fn topk(mut scored: Vec<(i64, f64)>, k: usize) -> Vec<(i64, f64)> {
    scored.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    scored.truncate(k);
    scored
}

/// The co-resident vector-index-as-columns: parallel arrays are exactly the columnar columns a `theodb_columnar`
/// table stores (`tid int8`, `part_id int4`, `label int4`, `vec bytea`), plus the IVF centroids (a small sidecar).
pub(crate) struct ColumnarVIndex {
    pub(crate) dim: usize,
    pub(crate) tid: Vec<i64>,
    pub(crate) part_id: Vec<i32>,
    pub(crate) label: Vec<i32>,
    pub(crate) vec_raw: Vec<Vec<u8>>,
    pub(crate) centroids: Vec<Vec<f32>>,
}

impl ColumnarVIndex {
    /// Build the co-resident layout from `(tid, label, vector)` rows: IVF k-means centroids (reuse
    /// `IvfflatIndex::build_owned`), a per-row `part_id`, and the raw f32 vector column. `nlist` partitions.
    pub(crate) fn build(rows: &[(i64, i32, Vec<f32>)], nlist: usize, seed: u64) -> Self {
        let dim = rows.first().map(|r| r.2.len()).unwrap_or(0);
        let corpus: Vec<(i64, Vec<f32>)> = rows.iter().map(|(t, _, v)| (*t, v.clone())).collect();
        let centroids = if corpus.is_empty() {
            Vec::new()
        } else {
            IvfflatIndex::build_owned(corpus, nlist.max(1), Metric::L2, seed).centroids().to_vec()
        };
        let mut cv = ColumnarVIndex {
            dim,
            tid: Vec::with_capacity(rows.len()),
            part_id: Vec::with_capacity(rows.len()),
            label: Vec::with_capacity(rows.len()),
            vec_raw: Vec::with_capacity(rows.len()),
            centroids,
        };
        for (t, lab, v) in rows {
            let pid =
                if cv.centroids.is_empty() { 0 } else { nearest_centroid(&cv.centroids, v) as i32 };
            cv.tid.push(*t);
            cv.part_id.push(pid);
            cv.label.push(*lab);
            cv.vec_raw.push(f32_to_le_bytes(v));
        }
        cv
    }

    /// The byte-identity ORACLE (ADR D2): the exact filtered brute-force top-k — every `label`-matching row,
    /// reranked with the exact kernel, ordered by the exact tie-break. This is the canonical correct answer.
    pub(crate) fn exact_filtered(&self, query: &[f32], k: usize, label: i32) -> Vec<(i64, f64)> {
        let scored: Vec<(i64, f64)> = (0..self.tid.len())
            .filter(|&i| self.label[i] == label)
            .map(|i| (self.tid[i], l2_dist_from_bytes(query, &self.vec_raw[i])))
            .collect();
        topk(scored, k)
    }

    /// The co-resident filtered top-k: scalar prefilter (label mask) → IVF probe restriction (part_id in the
    /// probed set) → exact rerank → top-k. At `probes >= nlist` (full probe) the candidate set equals every
    /// masked row, so the result is byte-identical to `exact_filtered` — the recall-equivalence GATE. When the
    /// index was reconstructed from stored columns WITHOUT the centroid sidecar (`from_columns`), the scan runs
    /// at full probe over every masked row (the GATE path) — the honest columnar round-trip.
    pub(crate) fn knn_filtered(
        &self,
        query: &[f32],
        k: usize,
        probes: usize,
        label: i32,
    ) -> Vec<(i64, f64)> {
        let candidates: Vec<usize> = if self.centroids.is_empty() {
            // no centroid sidecar (reconstructed from columnar columns) → full masked scan (= full probe).
            (0..self.tid.len()).filter(|&i| self.label[i] == label).collect()
        } else {
            let probed =
                probed_partitions(&self.centroids, query, probes.min(self.centroids.len()));
            (0..self.tid.len())
                .filter(|&i| self.label[i] == label && probed.contains(&self.part_id[i]))
                .collect()
        };
        let scored: Vec<(i64, f64)> = candidates
            .into_iter()
            .map(|i| (self.tid[i], l2_dist_from_bytes(query, &self.vec_raw[i])))
            .collect();
        topk(scored, k)
    }

    /// Reconstruct the co-resident index from the columns read back out of a `theodb_columnar` table (no centroid
    /// sidecar — the query path runs at full probe, the GATE). `dim` is inferred from the first vector column.
    pub(crate) fn from_columns(
        tid: Vec<i64>,
        part_id: Vec<i32>,
        label: Vec<i32>,
        vec_raw: Vec<Vec<u8>>,
    ) -> Self {
        let dim = vec_raw.first().map(|b| b.len() / 4).unwrap_or(0);
        ColumnarVIndex { dim, tid, part_id, label, vec_raw, centroids: Vec::new() }
    }

    /// Guard: the query dimension must match the index dimension (the raw-vector kernel would otherwise assert
    /// across the C boundary). Returns the mismatch so the caller raises a typed error (Rule 8).
    pub(crate) fn check_query_dim(&self, query: &[f32]) -> Result<(), String> {
        if self.dim != 0 && query.len() != self.dim {
            return Err(format!(
                "vindex: query dim {} != index dim {} (raw-vector rerank requires matching dims)",
                query.len(),
                self.dim
            ));
        }
        Ok(())
    }
}

/// Parse `dim*4` little-endian bytes back into an f32 vector (the inverse of `f32_to_le_bytes`).
fn le_bytes_to_f32(raw: &[u8]) -> Vec<f32> {
    raw.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

#[pgrx::pg_schema]
mod theodb_rs {
    use pgrx::prelude::*;

    /// `theodb_rs._f32vec_to_bytea` — encode a float4[] as the little-endian raw-vector bytes stored in the `vec`
    /// column of a co-resident columnar index (lets SQL populate the vector column). NULL element → 22023.
    #[pg_extern]
    fn _f32vec_to_bytea(v: Vec<Option<f32>>) -> Vec<u8> {
        let flat: Vec<f32> = v
            .into_iter()
            .map(|x| {
                x.unwrap_or_else(|| {
                    crate::pg::err_input("vindex: vector must not contain NULL elements")
                })
            })
            .collect();
        super::f32_to_le_bytes(&flat)
    }

    /// `theodb_rs._vindex_assign` — IVF k-means over the given raw-vector columns; returns the per-row `part_id`
    /// (the IVF partition, materialized as a column — DoD 1). NULL vec → 22023.
    #[pg_extern]
    fn _vindex_assign(vecs: pgrx::Array<'_, &[u8]>, nlist: i32) -> Vec<i32> {
        let rows: Vec<(i64, i32, Vec<f32>)> = vecs
            .iter()
            .enumerate()
            .map(|(i, o)| {
                let raw = o.unwrap_or_else(|| crate::pg::err_input("vindex: vec must not be NULL"));
                (i as i64, 0, super::le_bytes_to_f32(raw))
            })
            .collect();
        let idx = super::ColumnarVIndex::build(&rows, nlist.max(1) as usize, 42);
        idx.part_id
    }

    /// `theodb_rs._vindex_decode_bytes` — decode the named columns of a `theodb_columnar` table and return the
    /// total decoded byte count. Isolates the column-DECODE cost from the vector rerank so the benchmark can
    /// measure the actual pruning win (decode of the index columns vs all columns) without the L2 confound.
    #[pg_extern]
    fn _vindex_decode_bytes(idx: pg_sys::Oid, cols: pgrx::Array<'_, &str>) -> i64 {
        let names: Vec<String> = cols.iter().flatten().map(|s| s.to_string()).collect();
        unsafe { super::decode_bytes_impl(idx, &names) }
            .unwrap_or_else(|e| crate::pg::err_input(&e))
    }

    /// `theodb_rs._vindex_knn_columnar` — the co-resident filtered top-k over a `theodb_columnar` table. Reads
    /// ONLY the index columns (`tid`, `part_id`, `label`, `vec`) via a COLUMN-PRUNED decode (skips the analytical
    /// columns), reconstructs the index, and runs the scalar-prefiltered vector search at full probe (the GATE
    /// path). Returns `(tid, dist)` ordered by the exact M90-M95 tie-break.
    #[pg_extern]
    fn _vindex_knn_columnar(
        idx: pg_sys::Oid,
        query: Vec<Option<f32>>,
        k: i32,
        _probes: i32,
        label: i32,
    ) -> TableIterator<'static, (name!(tid, i64), name!(dist, f64))> {
        let q: Vec<f32> = query
            .into_iter()
            .map(|x| {
                x.unwrap_or_else(|| {
                    crate::pg::err_input("vindex: query must not contain NULL elements")
                })
            })
            .collect();
        let results = unsafe { super::knn_columnar_impl(idx, &q, k.max(0) as usize, label) }
            .unwrap_or_else(|e| crate::pg::err_input(&e));
        TableIterator::new(results.into_iter())
    }
}

/// Read the pruned index columns out of a `theodb_columnar` table and run the filtered top-k. Column pruning: only
/// (`tid`, `part_id`, `label`, `vec`) are decoded — the analytical columns' zstd is never touched (DoD 4).
unsafe fn knn_columnar_impl(
    idx: pg_sys::Oid,
    query: &[f32],
    k: usize,
    label: i32,
) -> Result<Vec<(i64, f64)>, String> {
    let rel = pg_sys::relation_open(idx, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
    let out = (|| {
        let want = ["tid", "part_id", "label", "vec"];
        let mut proj = Vec::with_capacity(4);
        for name in want {
            proj.push(crate::am::columnar::column_index(rel, name).ok_or_else(|| {
                format!("vindex: column '{name}' not found in the columnar index")
            })?);
        }
        let cols = crate::am::columnar::decode_columns(rel, Some(&proj), &[], false)?;
        // cols is in projection order: tid, part_id, label, vec
        let by = |i: usize| &cols[i].2;
        let n = by(0).len();
        let (mut tid, mut part_id, mut lbl, mut vec_raw) = (
            Vec::with_capacity(n),
            Vec::with_capacity(n),
            Vec::with_capacity(n),
            Vec::with_capacity(n),
        );
        for r in 0..n {
            let t = by(0)[r].as_ref().ok_or("vindex: NULL tid")?;
            let p = by(1)[r].as_ref().ok_or("vindex: NULL part_id")?;
            let l = by(2)[r].as_ref().ok_or("vindex: NULL label")?;
            let v = by(3)[r].as_ref().ok_or("vindex: NULL vec")?;
            tid.push(i64::from_le_bytes(v8(t)?));
            part_id.push(i32::from_le_bytes(v4(p)?));
            lbl.push(i32::from_le_bytes(v4(l)?));
            vec_raw.push(v.clone());
        }
        let cv = ColumnarVIndex::from_columns(tid, part_id, lbl, vec_raw);
        cv.check_query_dim(query)?;
        Ok(cv.knn_filtered(query, k, usize::MAX, label))
    })();
    pg_sys::relation_close(rel, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
    out
}

/// Decode only the named columns and return the total decoded byte count (isolates the column-decode cost from the
/// vector rerank — the benchmark control that quantifies the pruning win, decode-of-index-cols vs decode-of-all).
unsafe fn decode_bytes_impl(idx: pg_sys::Oid, names: &[String]) -> Result<i64, String> {
    let rel = pg_sys::relation_open(idx, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
    let out = (|| {
        let mut proj = Vec::with_capacity(names.len());
        for name in names {
            proj.push(
                crate::am::columnar::column_index(rel, name)
                    .ok_or_else(|| format!("vindex: column '{name}' not found"))?,
            );
        }
        let cols = crate::am::columnar::decode_columns(rel, Some(&proj), &[], false)?;
        let total: i64 = cols
            .iter()
            .flat_map(|(_, _, vals)| vals.iter())
            .map(|v| v.as_ref().map(|b| b.len() as i64).unwrap_or(0))
            .sum();
        Ok(total)
    })();
    pg_sys::relation_close(rel, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
    out
}

fn v8(b: &[u8]) -> Result<[u8; 8], String> {
    b.get(..8)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| "vindex: int8 column < 8 bytes".to_string())
}
fn v4(b: &[u8]) -> Result<[u8; 4], String> {
    b.get(..4)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| "vindex: int4 column < 4 bytes".to_string())
}

// SQL wrappers: the public `theodb.vindex_*`. REVOKEd from PUBLIC (parity with the other theodb.* compute funcs
// that read a relation). `vindex_knn_columnar` reads ONLY the 4 index columns (column pruning — DoD 4).
extension_sql!(
    r#"
CREATE FUNCTION theodb.f32vec_to_bytea(v float4[]) RETURNS bytea LANGUAGE sql IMMUTABLE
AS $$ SELECT theodb_rs._f32vec_to_bytea(v) $$;

CREATE FUNCTION theodb.vindex_assign(vecs bytea[], nlist int) RETURNS int[] LANGUAGE sql VOLATILE
AS $$ SELECT theodb_rs._vindex_assign(vecs, nlist) $$;

CREATE FUNCTION theodb.vindex_knn_columnar(idx regclass, query float4[], k int, probes int, label int)
RETURNS TABLE(tid bigint, dist float8) LANGUAGE sql VOLATILE
AS $$ SELECT * FROM theodb_rs._vindex_knn_columnar(idx::oid, query, k, probes, label) $$;

CREATE FUNCTION theodb.vindex_decode_bytes(idx regclass, cols text[]) RETURNS bigint LANGUAGE sql VOLATILE
AS $$ SELECT theodb_rs._vindex_decode_bytes(idx::oid, cols) $$;

COMMENT ON FUNCTION theodb.vindex_knn_columnar(regclass, float4[], int, int, int) IS
  'M103 — co-resident filtered vector top-k over a theodb_columnar index: reads ONLY the tid/part_id/label/vec '
  'columns (column pruning), scalar prefilter + exact rerank, byte-identical to the exact filtered search. The '
  'probes argument is RESERVED — the columnar path currently runs full-probe (the byte-identity GATE); reduced-'
  'probe over columnar-stored centroids is a documented follow-up.';

REVOKE ALL ON FUNCTION theodb.vindex_knn_columnar(regclass, float4[], int, int, int) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.vindex_assign(bytea[], int) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.vindex_decode_bytes(regclass, text[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._vindex_knn_columnar(oid, float4[], int, int, int) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._vindex_assign(bytea[], int) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._vindex_decode_bytes(oid, text[]) FROM PUBLIC;
"#,
    name = "theodb_vindex",
    requires = [
        _f32vec_to_bytea,
        _vindex_assign,
        _vindex_knn_columnar,
        _vindex_decode_bytes,
        "theodb_schema_bootstrap"
    ],
);

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use super::*;
    use pgrx::prelude::*;

    // A deterministic dataset: dim-4 vectors, two labels, seeded — no RNG in the test itself.
    fn dataset() -> Vec<(i64, i32, Vec<f32>)> {
        let mut rows = Vec::new();
        for i in 0..200i64 {
            let f = i as f32;
            // spread across the space; label = parity so the mask splits the set
            let v = vec![f, (f * 0.5) % 13.0, (f * 0.25) % 7.0, (200.0 - f)];
            rows.push((i, (i % 2) as i32, v));
        }
        rows
    }

    #[pg_test]
    fn m103_full_probe_byte_identical_to_exact_filtered() {
        let rows = dataset();
        let idx = ColumnarVIndex::build(&rows, 16, 42);
        let query = vec![37.0f32, 3.0, 2.0, 160.0];
        let k = 10;
        for label in [0, 1] {
            let exact = idx.exact_filtered(&query, k, label);
            // full probe = every partition
            let filtered = idx.knn_filtered(&query, k, idx.centroids.len(), label);
            assert_eq!(
                filtered, exact,
                "GATE: full-probe columnar filtered top-k must be byte-identical to exact filtered brute-force (label {label})"
            );
        }
    }

    #[pg_test]
    fn m103_mask_returns_only_label_matching_rows() {
        let rows = dataset();
        let idx = ColumnarVIndex::build(&rows, 16, 42);
        let query = vec![10.0f32, 1.0, 1.0, 190.0];
        let got = idx.knn_filtered(&query, 20, idx.centroids.len(), 0);
        // every returned tid is an even id (label 0)
        for (tid, _) in &got {
            assert_eq!(tid % 2, 0, "the scalar prefilter must admit only label-0 (even) rows");
        }
    }

    #[pg_test]
    fn m103_reduced_probe_is_subset_recall_of_exact() {
        // at reduced probes the result is a (recall <= 1.0) approximation of exact — every returned row is a real
        // label-matching row and the distances are non-decreasing (correctly ordered), but it may miss some.
        let rows = dataset();
        let idx = ColumnarVIndex::build(&rows, 16, 42);
        let query = vec![50.0f32, 4.0, 3.0, 150.0];
        let got = idx.knn_filtered(&query, 10, 2, 1);
        let mut prev = f64::NEG_INFINITY;
        for (tid, d) in &got {
            assert_eq!(tid % 2, 1, "label-1 only");
            assert!(*d >= prev, "distances non-decreasing (correct order)");
            prev = *d;
        }
    }

    #[pg_test]
    fn m103_empty_mask_returns_empty() {
        let rows = dataset();
        let idx = ColumnarVIndex::build(&rows, 16, 42);
        let query = vec![1.0f32, 1.0, 1.0, 1.0];
        let got = idx.knn_filtered(&query, 10, idx.centroids.len(), 99); // no row has label 99
        assert!(got.is_empty(), "an empty mask yields an empty top-k, not an error");
    }

    // ---- END-TO-END: vector index stored AS columnar columns co-resident with an analytical column ----

    #[pg_test]
    fn m103_columnar_coresident_filtered_topk_matches_exact_and_composes() {
        // co-resident columnar index: (tid, part_id, label, vec) + an analytical column `payload` in ONE table.
        Spi::run(
            "CREATE TABLE m103_idx (tid int8, part_id int4, label int4, vec bytea, payload float8) USING theodb_columnar",
        )
        .unwrap();
        // build vectors; part_id is the REAL IVF partition (theodb.vindex_assign), materialized as a column (DoD 1).
        Spi::run(
            "CREATE TEMP TABLE src AS
             SELECT g AS tid, (g % 2) AS label,
                    theodb.f32vec_to_bytea(ARRAY[g::float4, (g*0.5)::float4, (g*0.25)::float4, (300-g)::float4]) AS vec,
                    (g * 1.5)::float8 AS payload
             FROM generate_series(1, 150) g",
        )
        .unwrap();
        // assign IVF partitions over the vector column, then load the co-resident columnar index.
        Spi::run(
            "INSERT INTO m103_idx
             SELECT s.tid, a.part_id, s.label, s.vec, s.payload
             FROM (SELECT array_agg(vec ORDER BY tid) AS vecs FROM src) v,
                  LATERAL unnest(theodb.vindex_assign(v.vecs, 8)) WITH ORDINALITY AS a(part_id, ord)
             JOIN src s ON s.tid = a.ord",
        )
        .unwrap();

        // co-resident filtered top-k (reads only the 4 index columns — payload is pruned)
        let query = "ARRAY[37,18.5,9.25,263]::float4[]";
        let got = Spi::get_one::<i64>(&format!(
            "SELECT count(*) FROM theodb.vindex_knn_columnar('m103_idx'::regclass, {query}, 5, 8, 0)"
        ))
        .unwrap()
        .unwrap();
        assert_eq!(got, 5, "returns k=5 filtered rows");

        // every returned tid is a label-0 (even) row — the scalar prefilter over the columnar-stored `label`
        let non_label0 = Spi::get_one::<i64>(&format!(
            "SELECT count(*) FROM theodb.vindex_knn_columnar('m103_idx'::regclass, {query}, 5, 8, 0) WHERE tid % 2 <> 0"
        ))
        .unwrap()
        .unwrap();
        assert_eq!(non_label0, 0, "the columnar-stored label prefilter admits only label-0 rows");

        // distances are non-decreasing (correct order from the exact rerank over the columnar-stored vec column)
        let disordered = Spi::get_one::<i64>(&format!(
            "SELECT count(*) FROM (
               SELECT dist, lag(dist) OVER (ORDER BY dist) AS prev
               FROM theodb.vindex_knn_columnar('m103_idx'::regclass, {query}, 5, 8, 0)
             ) t WHERE prev IS NOT NULL AND dist < prev"
        ))
        .unwrap()
        .unwrap();
        assert_eq!(disordered, 0, "top-k ordered by exact distance (the M90-M95 tie-break)");

        // DoD 2 composition: aggregate the analytical `payload` over the filtered top-k row-ids in one query
        let avg = Spi::get_one::<f64>(&format!(
            "SELECT avg(i.payload) FROM theodb.vindex_knn_columnar('m103_idx'::regclass, {query}, 5, 8, 0) knn
             JOIN m103_idx i ON i.tid = knn.tid"
        ))
        .unwrap();
        assert!(
            avg.is_some(),
            "the filtered top-k composes with an analytical aggregation in one plan"
        );
    }
}
