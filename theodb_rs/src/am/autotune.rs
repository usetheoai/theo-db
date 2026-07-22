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
use std::cell::Cell;
use std::collections::HashSet;

use pgrx::prelude::*;

use crate::pg::err_input;

const MAX_EF: i32 = 1000; // matches guc.rs MAX_EF_SEARCH (pgvector's hnsw.ef_search ceiling)

// Backend-local observation counter: the HNSW `traverse` adds its already-computed `pages_read` here every
// scan (a cheap in-memory add — does NOT touch the scan's correctness or crash-safety, no page write). The
// recommender/collector resets it, runs a scan, and reads the accumulated pages to persist real `pages_read`.
thread_local! {
    static SCAN_PAGES_READ: Cell<i64> = const { Cell::new(0) };
    static SCAN_CANDIDATES: Cell<i64> = const { Cell::new(0) }; // M68 — candidates navigated in the beam
}

/// Called by the HNSW scan (`hnsw_page::traverse`) with the pages it read — accumulates for the collector.
pub(crate) fn bump_scan_pages(reads: i64) {
    SCAN_PAGES_READ.with(|c| c.set(c.get() + reads));
}

/// M68 — called by the HNSW scan with the candidates it navigated (the `visited` count from ground_search).
pub(crate) fn bump_scan_candidates(candidates: i64) {
    SCAN_CANDIDATES.with(|c| c.set(c.get() + candidates));
}

/// Reset the backend-local accumulators (before a measured scan window).
fn reset_scan_counters() {
    SCAN_PAGES_READ.with(|c| c.set(0));
    SCAN_CANDIDATES.with(|c| c.set(0));
}

/// Read (and keep) the accumulated pages-read since the last reset.
fn read_scan_pages() -> i64 {
    SCAN_PAGES_READ.with(|c| c.get())
}

/// Read (and keep) the accumulated candidates-seen since the last reset.
fn read_scan_candidates() -> i64 {
    SCAN_CANDIDATES.with(|c| c.get())
}

// M67 — the per-index scan-stats catalog. A HEAP table keyed by the index OID, OUTSIDE the index pages
// (ADR-0026-b: writing scan stats into the index pages via GenericXLog would violate partial-read +
// the M35 graph immutability; a heap catalog keeps the scan read-only, the IndexAmRoutine contract intact).
// Molded on `theodb.vectorizer_worker_stats`. The catalog is written only by the sampled collector
// (`record_scan_stat`, invoked from `scan_stats`) — NOT per hot-path query. Every hot-path scan DOES bump the
// backend-local thread_local counters (cheap in-memory add), but the SPI write into this catalog is sampled.
pgrx::extension_sql!(
    r#"
CREATE TABLE IF NOT EXISTS theodb._index_scan_stats (
    relid         oid  PRIMARY KEY,
    n_scans       bigint      NOT NULL DEFAULT 0,
    sum_pages_read bigint     NOT NULL DEFAULT 0,
    sum_candidates bigint     NOT NULL DEFAULT 0,
    sum_latency_us bigint     NOT NULL DEFAULT 0,
    last_ef       int,
    last_updated  timestamptz NOT NULL DEFAULT now()
);
"#,
    name = "theodb_index_scan_stats_schema",
    requires = ["theodb_schema_bootstrap"], // M70: schema theodb criado pelo theodb_rs (flip ADR-D1)
);

/// Record one scan observation for a relation (aggregated). Sampled by design — the collector calls this,
/// not every hot-path scan (ADR-0026-b: a per-scan SPI write on the read path is too costly).
fn record_scan_stat(relid: i64, pages_read: i64, candidates: i64, latency_us: i64, ef: i32) {
    Spi::run_with_args(
        "INSERT INTO theodb._index_scan_stats (relid, n_scans, sum_pages_read, sum_candidates, sum_latency_us, last_ef) \
         VALUES ($1, 1, $2, $3, $4, $5) \
         ON CONFLICT (relid) DO UPDATE SET \
           n_scans = theodb._index_scan_stats.n_scans + 1, \
           sum_pages_read = theodb._index_scan_stats.sum_pages_read + EXCLUDED.sum_pages_read, \
           sum_candidates = theodb._index_scan_stats.sum_candidates + EXCLUDED.sum_candidates, \
           sum_latency_us = theodb._index_scan_stats.sum_latency_us + EXCLUDED.sum_latency_us, \
           last_ef = EXCLUDED.last_ef, last_updated = now()",
        &[relid.into(), pages_read.into(), candidates.into(), latency_us.into(), ef.into()],
    )
    .unwrap_or_else(|e| err_input(&format!("theodb.record_scan_stat: {e:?}")));
}

/// The exact top-k (seqscan brute force) row set for one query vector — the recall ground truth. Uses `ctid`
/// as the stable row identifier so the recommender needs no knowledge of the table's PK column.
fn exact_topk(tbl: &str, col: &str, qvec: &str, k: i32) -> HashSet<String> {
    pgrx::Spi::run("SET LOCAL enable_indexscan=off; SET LOCAL enable_bitmapscan=off; SET LOCAL enable_seqscan=on")
        .unwrap_or_else(|e| err_input(&format!("recommend_ef: GT setup failed: {e:?}")));
    let sql =
        format!("SELECT ctid::text FROM {tbl} ORDER BY \"{col}\" <=> '{qvec}'::vector LIMIT {k}");
    pgrx::Spi::connect(|c| {
        c.select(&sql, None, &[])
            .unwrap_or_else(|e| err_input(&format!("recommend_ef: exact scan failed: {e:?}")))
            .filter_map(|r| r.get::<String>(1).unwrap())
            .collect()
    })
}

/// Mean recall@k of the index scan at `ef` over the sample, against the precomputed exact GT sets.
fn recall_at_ef(
    tbl: &str,
    col: &str,
    samples: &[&str],
    gts: &[HashSet<String>],
    k: i32,
    ef: i32,
) -> f64 {
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
        let sql =
            format!("SELECT ctid::text FROM {tbl} ORDER BY \"{col}\" <=> '{q}'::vector LIMIT {k}");
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

/// Measure one index scan: run `SELECT ctid FROM tbl ORDER BY col <=> q LIMIT k` at `ef` and return the
/// observed `(pages_read, latency_us, results)`. `pages_read` comes from the backend-local accumulator the
/// HNSW `traverse` feeds — the real per-scan cost, not an estimate (M67 collector, DoD bullet 1). Read-only.
pub(crate) fn scan_stats(
    relid: i64,
    tbl: &str,
    col: &str,
    qvec: &str,
    ef: i32,
    k: i32,
) -> (i64, i64, i64, i64) {
    if ef <= 0 || k <= 0 {
        err_input("theodb.scan_stats: ef and k must be > 0");
    }
    Spi::run(&format!(
        "SET LOCAL enable_seqscan=off; SET LOCAL enable_bitmapscan=off; SET LOCAL enable_indexscan=on; \
         SET LOCAL theodb_hnsw.ef_search = {ef}"
    ))
    .unwrap_or_else(|e| err_input(&format!("theodb.scan_stats: setup failed: {e:?}")));
    reset_scan_counters();
    let sql = format!("SELECT ctid FROM {tbl} ORDER BY \"{col}\" <=> '{qvec}'::vector LIMIT {k}");
    let t0 = std::time::Instant::now();
    let results: i64 = Spi::get_one(&format!("SELECT count(*) FROM ({sql}) s"))
        .unwrap_or_else(|e| err_input(&format!("theodb.scan_stats: scan failed: {e:?}")))
        .unwrap_or(0);
    let latency_us = t0.elapsed().as_micros() as i64;
    let (pages_read, candidates) = (read_scan_pages(), read_scan_candidates());
    record_scan_stat(relid, pages_read, candidates, latency_us, ef); // persist the observation (the collector)
    (pages_read, candidates, latency_us, results)
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

    /// Seed a small clustered table + theodb_hnsw index and return a few query vectors (as text literals).
    fn seed(tbl: &str, n: i32) -> Vec<String> {
        Spi::run(&format!("CREATE TEMP TABLE {tbl} (id int PRIMARY KEY, e vector(8))")).unwrap();
        for i in 0..n {
            let center = (i % 5) as f32;
            let v: Vec<String> = (0..8)
                .map(|j| {
                    format!("{:.3}", 1.0 + center + 0.02 * (((i * 7 + j * 3) % 11) as f32 - 5.0))
                })
                .collect();
            Spi::run(&format!("INSERT INTO {tbl} VALUES ({i}, '[{}]')", v.join(","))).unwrap();
        }
        Spi::run(&format!(
            "CREATE INDEX {tbl}_idx ON {tbl} USING theodb_hnsw (e theodb_hnsw_cosine_ops)"
        ))
        .unwrap();
        // 3 probes = the emb of rows 0,1,2.
        (0..3)
            .map(|i| {
                Spi::get_one::<String>(&format!("SELECT e::text FROM {tbl} WHERE id={i}"))
                    .unwrap()
                    .unwrap()
            })
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
        assert!(
            ef_high >= ef_low,
            "a higher recall target needs at least as much ef ({ef_high} >= {ef_low})"
        );
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

    #[pg_test]
    fn scan_stats_records_real_pages_read() {
        let probes = seed("sst1", 60);
        // Measure one scan → real pages_read + candidates > 0 (the HNSW traverse fed the backend-local counters).
        let (pages, candidates, _latency, results) = scan_stats(0, "sst1", "e", &probes[0], 40, 5);
        assert!(pages > 0, "the collector observes real pages_read from the scan (got {pages})");
        assert!(
            candidates > 0,
            "the collector observes real candidates_seen from the scan (got {candidates})"
        );
        assert!(results > 0, "the scan returned rows (got {results})");
        // The observation was persisted into the catalog keyed by relid 0 (this test's synthetic key).
        let (n, cand): (i64, i64) = Spi::connect(|c| {
            let r = c
                .select(
                    "SELECT n_scans, sum_candidates FROM theodb._index_scan_stats WHERE relid = 0",
                    None,
                    &[],
                )
                .unwrap()
                .first();
            (r.get::<i64>(1).unwrap().unwrap_or(0), r.get::<i64>(2).unwrap().unwrap_or(0))
        });
        assert_eq!(n, 1, "the collector persisted one observation into theodb._index_scan_stats");
        assert!(cand > 0, "candidates_seen persisted into the catalog (got {cand})");
    }

    #[pg_test(error = "theodb.scan_stats: ef and k must be > 0")]
    fn scan_stats_rejects_bad_ef() {
        let probes = seed("sst2", 20);
        let _ = scan_stats(0, "sst2", "e", &probes[0], 0, 5);
    }

    #[pg_test]
    fn explain_scan_shows_index_and_candidates() {
        let probes = seed("esc1", 60);
        // theodb.explain_scan resolves the theodb_hnsw index name + reports pages_read/candidates for the scan.
        let (idx, pages, cand): (String, i64, i64) = Spi::connect(|c| {
            let sql = format!(
                "SELECT index_name, pages_read, candidates_seen FROM theodb.explain_scan('esc1'::regclass, 'e', '{}', 40, 5)",
                probes[0]
            );
            let r = c.select(&sql, None, &[]).unwrap().first();
            (
                r.get::<String>(1).unwrap().unwrap_or_default(),
                r.get::<i64>(2).unwrap().unwrap_or(0),
                r.get::<i64>(3).unwrap().unwrap_or(0),
            )
        });
        assert!(
            idx.contains("esc1"),
            "explain_scan shows the theodb_hnsw index name (got '{idx}')"
        );
        assert!(pages > 0, "explain_scan shows real pages_read (got {pages})");
        assert!(cand > 0, "explain_scan shows candidates_seen (got {cand})");
    }
}
