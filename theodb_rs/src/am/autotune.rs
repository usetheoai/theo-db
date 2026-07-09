//! Domain layer (M67): the deterministic `ef_search` recommender + the per-index scan-stats collector.
//!
//! **Recommend, do NOT auto-tune online** (blueprint ADR-0026-a): almost no production vector DB mutates a
//! live recall knob — it oscillates and collides with the user's `SET`. The rung-1 that solves the real pain
//! (the manual knob) is a deterministic recommender: given a sample of query vectors + a recall target, find
//! the MINIMUM `ef_search` that reaches the target on a sampled EXACT ground truth. Recall(ef) is monotone
//! non-decreasing (Malkov & Yashunin — the ef+1 candidate list is a superset of ef's), so a doubling+bisection
//! is sound (no local maxima). The operator applies the suggestion with `SET theodb_hnsw.ef_search`.
//!
//! Recall estimation without production GT is unreliable; the honest base-of-truth is a SAMPLED exact scan
//! (seqscan brute force over the sample), exactly what Qdrant/Milvus/Elastic ship for retrieval-quality checks.
use std::collections::HashSet;

use crate::pg::err_input;

const MAX_EF: i32 = 1000; // matches guc.rs MAX_EF_SEARCH (pgvector's hnsw.ef_search ceiling)

/// The exact top-k (seqscan brute force) row set for one query vector — the recall ground truth. Uses `ctid`
/// as the stable row identifier so the recommender needs no knowledge of the table's PK column.
fn exact_topk(tbl: &str, col: &str, qvec: &str, k: i32) -> HashSet<String> {
    pgrx::Spi::run("SET LOCAL enable_indexscan=off; SET LOCAL enable_bitmapscan=off; SET LOCAL enable_seqscan=on")
        .unwrap_or_else(|e| err_input(&format!("recommend_ef: GT setup failed: {e:?}")));
    let sql = format!(
        "SELECT ctid::text FROM {tbl} ORDER BY \"{col}\" <=> '{qvec}'::vector LIMIT {k}"
    );
    pgrx::Spi::connect(|c| {
        c.select(&sql, None, &[])
            .unwrap_or_else(|e| err_input(&format!("recommend_ef: exact scan failed: {e:?}")))
            .filter_map(|r| r.get::<String>(1).unwrap())
            .collect()
    })
}

/// Mean recall@k of the index scan at `ef` over the sample, against the precomputed exact GT sets.
fn recall_at_ef(tbl: &str, col: &str, samples: &[&str], gts: &[HashSet<String>], k: i32, ef: i32) -> f64 {
    pgrx::Spi::run(&format!(
        "SET LOCAL enable_seqscan=off; SET LOCAL enable_bitmapscan=off; SET LOCAL enable_indexscan=on; \
         SET LOCAL theodb_hnsw.ef_search = {ef}"
    ))
    .unwrap_or_else(|e| err_input(&format!("recommend_ef: ann setup failed: {e:?}")));
    let mut sum = 0.0;
    let mut n = 0;
    for (q, gt) in samples.iter().zip(gts.iter()) {
        if gt.is_empty() {
            continue; // a query with no neighbours is not a recall data point
        }
        let sql = format!("SELECT ctid::text FROM {tbl} ORDER BY \"{col}\" <=> '{q}'::vector LIMIT {k}");
        let ann: HashSet<String> = pgrx::Spi::connect(|c| {
            c.select(&sql, None, &[])
                .unwrap_or_else(|e| err_input(&format!("recommend_ef: ann scan failed: {e:?}")))
                .filter_map(|r| r.get::<String>(1).unwrap())
                .collect()
        });
        let hits = ann.intersection(gt).count();
        sum += hits as f64 / gt.len() as f64;
        n += 1;
    }
    if n == 0 {
        return 1.0; // no measurable query → vacuously satisfied (empty index / sample)
    }
    sum / n as f64
}

/// Recommend the MINIMUM `ef_search` that reaches `recall_target` on the sample. Doubling to find a bracket
/// `[prev, ef]` where recall crosses the target, then bisection for the least ef inside it (recall monotone).
/// Returns `MAX_EF` when the target is unreachable within the ceiling (honest: the caller sees the ceiling).
pub(crate) fn recommend_ef(tbl: &str, col: &str, samples: &[&str], target: f64, k: i32) -> i32 {
    if !(target > 0.0 && target <= 1.0) {
        err_input("theodb.recommend_ef: recall_target must be in (0, 1]");
    }
    if k <= 0 {
        err_input("theodb.recommend_ef: k must be > 0");
    }
    if samples.is_empty() {
        err_input("theodb.recommend_ef: sample must not be empty");
    }
    let gts: Vec<HashSet<String>> = samples.iter().map(|q| exact_topk(tbl, col, q, k)).collect();

    // Doubling: grow ef until recall(ef) >= target, tracking the previous ef for the bracket.
    let mut prev = k.max(1);
    let mut ef = prev;
    loop {
        if recall_at_ef(tbl, col, samples, &gts, k, ef) >= target {
            break;
        }
        if ef >= MAX_EF {
            return MAX_EF; // target unreachable within the ceiling
        }
        prev = ef;
        ef = (ef * 2).min(MAX_EF);
    }
    // Bisection over [prev, ef] for the least ef that still reaches the target.
    let (mut lo, mut hi) = (prev, ef);
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if recall_at_ef(tbl, col, samples, &gts, k, mid) >= target {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}

// M67 — pg_tests for the recommender (module name = schema `tests`, the codebase convention; NOT `pg_*`).
#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use super::*;
    use pgrx::prelude::*;

    /// Seed a small clustered table + theodb_hnsw index and return a few query vectors (as text literals).
    fn seed(tbl: &str, n: i32) -> Vec<String> {
        Spi::run(&format!("CREATE TEMP TABLE {tbl} (id int PRIMARY KEY, e vector(8))")).unwrap();
        for i in 0..n {
            let center = (i % 5) as f32;
            let v: Vec<String> = (0..8)
                .map(|j| format!("{:.3}", 1.0 + center + 0.02 * (((i * 7 + j * 3) % 11) as f32 - 5.0)))
                .collect();
            Spi::run(&format!("INSERT INTO {tbl} VALUES ({i}, '[{}]')", v.join(","))).unwrap();
        }
        Spi::run(&format!("CREATE INDEX {tbl}_idx ON {tbl} USING theodb_hnsw (e theodb_hnsw_cosine_ops)")).unwrap();
        // 3 probes = the emb of rows 0,1,2.
        (0..3)
            .map(|i| Spi::get_one::<String>(&format!("SELECT e::text FROM {tbl} WHERE id={i}")).unwrap().unwrap())
            .collect()
    }

    #[pg_test]
    fn recommend_ef_is_monotone_and_bounded() {
        let probes = seed("rec1", 80);
        let refs: Vec<&str> = probes.iter().map(|s| s.as_str()).collect();
        // A low target needs no more ef than a high target (monotone: recall(ef) non-decreasing).
        let ef_low = recommend_ef("rec1", "e", &refs, 0.5, 5);
        let ef_high = recommend_ef("rec1", "e", &refs, 1.0, 5);
        assert!(ef_low >= 5 && ef_low <= MAX_EF, "ef_low {ef_low} in range");
        assert!(ef_high >= ef_low, "a higher recall target needs at least as much ef ({ef_high} >= {ef_low})");
        assert!(ef_high <= MAX_EF, "ef bounded by the ceiling");
    }

    #[pg_test(error = "theodb.recommend_ef: recall_target must be in (0, 1]")]
    fn recommend_ef_rejects_bad_target() {
        let probes = seed("rec2", 20);
        let refs: Vec<&str> = probes.iter().map(|s| s.as_str()).collect();
        let _ = recommend_ef("rec2", "e", &refs, 1.5, 5);
    }

    #[pg_test(error = "theodb.recommend_ef: sample must not be empty")]
    fn recommend_ef_rejects_empty_sample() {
        seed("rec3", 20);
        let _ = recommend_ef("rec3", "e", &[], 0.9, 5);
    }
}
