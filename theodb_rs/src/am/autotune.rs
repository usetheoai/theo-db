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

/// Resolve a caller-supplied relation name through `regclass`, returning the catalog's own rendering.
///
/// #172, THIRD axis — found by the `/review` of the very milestone that fixed the other two. The first pass
/// closed `qvec` (`quote_literal`) and `col` (`valid_ident`) and I stated the function was safe. It was not:
/// `tbl` stayed a raw `{tbl}` splice in all three query builders, and the SAME `1/0` oracle proves it executes:
///
/// ```text
/// SELECT theodb_rs._scan_stats(0::oid, '(SELECT 1/0 AS ctid, NULL::vector AS e) s', 'e', '[1,2]', 10, 5);
/// -- assembled: SELECT ctid FROM (SELECT 1/0 AS ctid, …) s ORDER BY "e" <=> …  =>  ERROR: division by zero
/// ```
///
/// Fixing two of three axes and declaring victory is worse than fixing none, because it retires the suspicion.
/// The lesson is mechanical, not moral: patch the *shape* (every interpolation in the builder), never the
/// *payload* that happened to be probed.
///
/// `regclass` — not `%I` — for the same reason as `graph.rs::build_csr`: it validates that the name parses AND
/// resolves through `search_path`, raising 42P01 before any SQL is assembled, and `regclassout` re-renders the
/// identifier from `pg_class`, so what reaches the query comes from the catalog rather than from the caller.
/// `%I` is lexical only and would mangle a schema-qualified `s1.t` into one quoted identifier.
fn resolve_relation(tbl: &str, ctx: &str) -> String {
    Spi::get_one_with_args::<String>("SELECT ($1)::regclass::text", &[tbl.into()])
        .ok()
        .flatten()
        .unwrap_or_else(|| err_input(&format!("{ctx}: relation {tbl:?} does not resolve")))
}

// Backend-local observation counter: the HNSW `traverse` adds its already-computed `pages_read` here every
// scan (a cheap in-memory add — does NOT touch the scan's correctness or crash-safety, no page write). The
// recommender/collector resets it, runs a scan, and reads the accumulated pages to persist real `pages_read`.
thread_local! {
    static SCAN_PAGES_READ: Cell<i64> = const { Cell::new(0) };
    static SCAN_CANDIDATES: Cell<i64> = const { Cell::new(0) }; // M68 — candidates navigated in the beam
}

/// Accumulate the pages a scan segment read. PRIVATE on purpose — see `record_scan_observation`.
fn bump_scan_pages(reads: i64) {
    SCAN_PAGES_READ.with(|c| c.set(c.get() + reads));
}

/// M68 — accumulate the candidates a scan segment navigated (the `visited` count from ground_search).
/// PRIVATE on purpose — see `record_scan_observation`.
fn bump_scan_candidates(candidates: i64) {
    SCAN_CANDIDATES.with(|c| c.set(c.get() + candidates));
}

/// Record the COMPLETE observation of one scan segment — pages read AND candidates navigated — in a single
/// call. Every scan path MUST report through here rather than bumping the two counters by hand.
///
/// This exists because reporting only half was a real, shipped defect (B-015): the M118 resume path
/// (`hnsw_page::resumable_init` / `resumable_next`) duplicated `traverse`'s greedy descent, accumulated its own
/// `reads`, and never called either bump — while being the DEFAULT path for every V1 exact-f32 index. The
/// measurement that isolated it toggled the kill-switch on one query: `theodb_hnsw.resume = off` reported
/// `pages_read=112 candidates_seen=38`; `= on` reported `0` and `0`, with the same plan and the same 5 rows.
/// `theodb.explain_scan`, `theodb.scan_stats` and the `theodb._index_scan_stats` collector therefore reported
/// zero for the most common scan in the product, and the ef_search recommender consumed those zeros.
///
/// A pair of calls is two chances to forget one; this is one. The two `bump_*` helpers are PRIVATE to this
/// module precisely so a future scan path cannot reach half the pair from outside: the type system now refuses
/// the shape of the defect, instead of a comment asking nicely. Note this still does NOT make instrumentation
/// automatic for a path that reports nothing at all — only a test can do that (see
/// `scan_stats_instruments_the_resume_path`), which is why the fix ships with one.
pub(crate) fn record_scan_observation(reads: i64, candidates: i64) {
    bump_scan_pages(reads);
    bump_scan_candidates(candidates);
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
    // #172: `qvec` é texto livre do chamador — `quote_literal` escapa aspas/backslashes, então o payload não
    // consegue fechar o literal e emendar SQL. `col` já foi validado por `valid_ident` na fronteira pública.
    let sql = format!(
        "SELECT ctid::text FROM {tbl} ORDER BY \"{col}\" <=> {lit}::vector LIMIT {k}",
        lit = pgrx::spi::quote_literal(qvec)
    );
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
        // #172: idem `exact_topk` — o vetor de amostra é texto livre e vai por `quote_literal`.
        let sql = format!(
            "SELECT ctid::text FROM {tbl} ORDER BY \"{col}\" <=> {lit}::vector LIMIT {k}",
            lit = pgrx::spi::quote_literal(q)
        );
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
    let tbl = resolve_relation(tbl, "theodb.scan_stats");
    // #172 — fail-closed na fronteira (rules/error-handling.md): `col` era interpolado com aspas manuais
    // (`\"{col}\"`), então um `"` no valor quebrava a citação e emendava SQL arbitrário — MEDIDO com o oráculo
    // `1/0`. Allowlist ASCII estrita, o mesmo `valid_ident` já usado em ann_query/pq/sbq (Rule 9).
    if !crate::ann_query::valid_ident(col) {
        err_input(
            "theodb.scan_stats: vector_col must be a plain identifier ([A-Za-z_][A-Za-z0-9_]*, ≤63)",
        );
    }
    // B-021 — fail-fast quando NENHUM índice da tabela responde ao operador que este diagnóstico usa.
    //
    // A consulta abaixo é fixa em `<=>` (cosine), mas o opclass DEFAULT do AM é `theodb_hnsw_l2_ops`. Quem
    // cria o índice sem nomear o opclass fica com um índice que não serve `<=>`: medido, o plano cai em
    // `Limit → Sort → Seq Scan` e o diagnóstico devolvia `pages_read=0 candidates_seen=0` — indistinguível
    // de "o índice foi usado e leu zero páginas". O zero silencioso é a pior resposta possível de um
    // instrumento de diagnóstico, e é a mesma família de falso-negativo que o B-015 corrigiu do outro lado.
    //
    // Erro tipado e não WARNING porque o número que viria em seguida seria de um seqscan, não do índice —
    // reportá-lo sob o nome `explain_scan` seria medir uma coisa e rotular outra (Regra 8: falhe alto, cedo
    // e claro). A checagem é por FAMÍLIA de operador (`pg_amop`), então cobre qualquer AM servido pelo nosso
    // handler, incluindo o alias pgvector.
    let serves_cosine: bool = Spi::get_one(&format!(
        "SELECT EXISTS (
           SELECT 1 FROM pg_index i
             JOIN pg_opclass oc ON oc.oid = i.indclass[0]
             JOIN pg_amop ao ON ao.amopfamily = oc.opcfamily AND ao.amoppurpose = 'o'
             JOIN pg_operator op ON op.oid = ao.amopopr
           WHERE i.indrelid = {lit}::regclass AND op.oprname = '<=>')",
        lit = pgrx::spi::quote_literal(&tbl)
    ))
    .unwrap_or(Some(true))
    .unwrap_or(true);
    if !serves_cosine {
        err_input(&format!(
            "theodb.scan_stats: no index on {tbl} answers the cosine operator `<=>` — this diagnostic \
             always probes with `<=>`, and the access method's DEFAULT opclass is l2. Rebuild with \
             `USING theodb_hnsw ({col} theodb_hnsw_cosine_ops)` (or the pgvector-compatible \
             `vector_cosine_ops`), or the numbers below would come from a sequential scan, not the index."
        ));
    }
    Spi::run(&format!(
        "SET LOCAL enable_seqscan=off; SET LOCAL enable_bitmapscan=off; SET LOCAL enable_indexscan=on; \
         SET LOCAL theodb_hnsw.ef_search = {ef}"
    ))
    .unwrap_or_else(|e| err_input(&format!("theodb.scan_stats: setup failed: {e:?}")));
    reset_scan_counters();
    // #172: `qvec` é texto livre do chamador → `quote_literal`; `col` validado por `valid_ident` acima.
    let sql = format!(
        "SELECT ctid FROM {tbl} ORDER BY \"{col}\" <=> {lit}::vector LIMIT {k}",
        lit = pgrx::spi::quote_literal(qvec)
    );
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
    let tbl = resolve_relation(tbl, "theodb.recommend_ef");
    // #172 — mesmo gate fail-closed do `scan_stats`: `col` chega como texto livre e era interpolado com aspas
    // manuais. MEDIDO: `recommend_ef('t','e" <=> ''[1,2]''::vector LIMIT (SELECT 1/0) --', …)` executava a
    // subquery injetada (ERROR: division by zero).
    if !crate::ann_query::valid_ident(col) {
        err_input(
            "theodb.recommend_ef: vector_col must be a plain identifier ([A-Za-z_][A-Za-z0-9_]*, ≤63)",
        );
    }
    if k <= 0 {
        err_input("theodb.recommend_ef: k must be > 0");
    }
    if samples.is_empty() {
        err_input("theodb.recommend_ef: sample must not be empty");
    }
    let gts: Vec<HashSet<String>> = samples.iter().map(|q| exact_topk(&tbl, col, q, k)).collect();

    // Doubling: grow ef until recall(ef) >= target, tracking the previous ef for the bracket.
    let mut prev = k.max(1);
    let mut ef = prev;
    loop {
        if recall_at_ef(&tbl, col, samples, &gts, k, ef) >= target {
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
        if recall_at_ef(&tbl, col, samples, &gts, k, mid) >= target {
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

    /// B-015 regression — the collector must observe the scan on BOTH scan paths, not just one.
    ///
    /// The sibling test above asserts `pages > 0` without pinning WHICH path produced it, and that gap shipped:
    /// the M118 resume path (`hnsw_page::resumable_init` / `resumable_next`) is the DEFAULT for every V1
    /// exact-f32 index and reported nothing, so `explain_scan`/`scan_stats`/`_index_scan_stats` read zero for
    /// the product's most common scan while the ef_search recommender consumed those zeros. Flipping the
    /// kill-switch on one query was what isolated it: `resume = off` → `pages=112 cand=38`; `resume = on` →
    /// `0`/`0`, same plan, same 5 rows.
    ///
    /// So this asserts the A/B directly. Were the fix reverted, the `on` half fails while the `off` half still
    /// passes — which is precisely the asymmetry the sibling test could not see.
    #[pg_test]
    fn scan_stats_instruments_the_resume_path() {
        // `sst3`, not `sst2` — `scan_stats_rejects_bad_ef` already seeds `sst2`, and two tests sharing a
        // fixture name is the order-dependence `rules/testing.md § 3` forbids.
        let probes = seed("sst3", 60);

        // The M118 path — the default, and the one that was blind.
        Spi::run("SET theodb_hnsw.resume = on").unwrap();
        let (pages_on, cand_on, _lat, results_on) = scan_stats(0, "sst3", "e", &probes[0], 40, 5);
        assert!(results_on > 0, "the resume-path scan returned rows (got {results_on})");
        assert!(
            pages_on > 0,
            "resume ON must observe pages_read — the M118 path reports through \
             record_scan_observation like traverse (got {pages_on})"
        );
        assert!(
            cand_on > 0,
            "resume ON must observe candidates_seen — ResumableGround::candidates_seen() was an \
             accessor with no caller until B-015 (got {cand_on})"
        );

        // The M52 re-search path, which always reported. Kept in the same test so the two are compared on one
        // binary and one dataset: an assertion that only ever ran on one path is what let this defect ship.
        Spi::run("SET theodb_hnsw.resume = off").unwrap();
        let (pages_off, cand_off, _lat, _r) = scan_stats(0, "sst3", "e", &probes[0], 40, 5);
        assert!(pages_off > 0, "resume OFF still observes pages_read (got {pages_off})");
        assert!(cand_off > 0, "resume OFF still observes candidates_seen (got {cand_off})");

        Spi::run("SET theodb_hnsw.resume = on").unwrap();
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

    /// B-021 regression — an index whose opclass does NOT answer `<=>` must fail loudly, never return zeros.
    ///
    /// The AM's DEFAULT opclass is `theodb_hnsw_l2_ops`, and this diagnostic always probes with `<=>`
    /// (cosine). Measured before the fix: the plan fell back to `Limit → Sort → Seq Scan` and `scan_stats`
    /// answered `pages_read=0 candidates_seen=0` — a number indistinguishable from "the index was used and
    /// read no pages". A diagnostic that reports a sequential scan under the name `explain_scan` is worse
    /// than one that refuses, which is why this is a typed error and not a warning.
    ///
    /// The error is captured rather than matched with `#[pg_test(error = …)]`: the message interpolates the
    /// resolved relation name, and a TEMP table's `regclass::text` depends on the session's `pg_temp` schema
    /// — pinning it would make the test fragile for a reason that has nothing to do with the behaviour.
    #[pg_test]
    fn scan_stats_refuses_an_index_that_does_not_answer_cosine() {
        Spi::run("CREATE TEMP TABLE l2only (id int PRIMARY KEY, e vector(8))").unwrap();
        Spi::run("INSERT INTO l2only VALUES (1, '[1,2,3,4,5,6,7,8]')").unwrap();
        // Opclass L2 — o DEFAULT do AM, e justamente o que NÃO serve `<=>`.
        Spi::run("CREATE INDEX l2only_idx ON l2only USING theodb_hnsw (e theodb_hnsw_l2_ops)")
            .unwrap();

        let refused = PgTryBuilder::new(|| {
            scan_stats(0, "l2only", "e", "[1,2,3,4,5,6,7,8]", 40, 5);
            false // chegou aqui ⇒ NÃO recusou: o defeito (zeros silenciosos) está de volta
        })
        .catch_others(|_| true)
        .execute();

        assert!(
            refused,
            "scan_stats must REFUSE an index whose opclass does not answer `<=>`; returning zeros silently \
             is the falso-negativo this guard exists to kill"
        );
    }

    /// B-021 regression — `explain_scan` must find the index by the HANDLER that serves it, not by the NAME
    /// of the access method.
    ///
    /// The sibling test above only ever creates the index with `USING theodb_hnsw`, so it could never see the
    /// gap that shipped: `sql/vector--0.6.0.sql` registers a pgvector-compatible alias (`CREATE ACCESS METHOD
    /// hnsw ... HANDLER theodb_hnsw_amhandler`), and an alias is a SECOND row in `pg_am` — measured, `hnsw`
    /// OID 17174 vs `theodb_hnsw` OID 16568. The old lookup matched `a.amname = 'theodb_hnsw'`, so every index
    /// created the pgvector way answered `(no theodb_hnsw index on this table)` — and that is EVERY index the
    /// `theo-rag` creates, i.e. the diagnostic was blind exactly where the dogfood exercises it.
    ///
    /// The alias is created here rather than by installing the `vector` extension: the invariant under test is
    /// "any AM served by our handler is found", and a locally-created AM exercises it without coupling this
    /// test to the shim's packaging. `#[pg_test]` runs inside a transaction, so the AM disappears on rollback.
    #[pg_test]
    fn explain_scan_finds_index_created_through_an_am_alias() {
        let probes = seed("esc_alias_src", 60);
        let q = &probes[0];

        // Um AM com OUTRO nome e o MESMO handler own-code — a forma exata do shim pgvector.
        //
        // A opclass precisa ser declarada JUNTO: ela é por-AM, e criar o access method não herda as
        // opclasses do irmão (medido — sem esta linha o CREATE INDEX falha com
        // `operator class "theodb_hnsw_cosine_ops" does not exist for access method "hnsw_alias_test"`).
        // É exatamente o que `sql/vector--0.6.0.sql` faz para o alias `hnsw`, reproduzido no mínimo.
        Spi::run("CREATE ACCESS METHOD hnsw_alias_test TYPE INDEX HANDLER theodb_hnsw_amhandler")
            .unwrap();
        Spi::run(
            "CREATE OPERATOR CLASS alias_cosine_ops FOR TYPE vector USING hnsw_alias_test AS \
               OPERATOR 1 <=> (vector, vector) FOR ORDER BY float_ops, \
               FUNCTION 1 theodb_metric_cosine()",
        )
        .unwrap();
        Spi::run("CREATE TEMP TABLE esc_alias (id int PRIMARY KEY, e vector(8))").unwrap();
        Spi::run("INSERT INTO esc_alias SELECT id, e FROM esc_alias_src").unwrap();
        Spi::run(
            "CREATE INDEX esc_alias_idx ON esc_alias USING hnsw_alias_test (e alias_cosine_ops)",
        )
        .unwrap();

        let idx: String = Spi::connect(|c| {
            let sql = format!(
                "SELECT index_name FROM theodb.explain_scan('esc_alias'::regclass, 'e', '{q}', 40, 5)"
            );
            c.select(&sql, None, &[]).unwrap().first().get::<String>(1).unwrap().unwrap_or_default()
        });

        assert_eq!(
            idx, "esc_alias_idx",
            "explain_scan must resolve an index whose AM is an ALIAS sharing our handler; \
             resolving by `amname` returns the not-found sentinel instead (got '{idx}')"
        );
    }
}
