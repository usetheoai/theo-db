//! api/IO layer (plan M21 T2.1): the Spi-bound orchestration behind `theodb.hnsw_knn` / `theodb.ivfflat_knn`.
//!
//! Keeps `crate::ann` pure (algorithm only). This module validates arguments at the boundary (Unbreakable
//! Rule 8 — typed 22023 via `crate::pg::err_input`, with lower AND upper caps, EC-2), reads the corpus from a
//! caller-named table column via `Spi` (skipping NULL vectors, EC-1), builds the chosen own index once, and
//! answers a flattened batch of queries. Coexistence (ADR D1): it only READS the vector column via `::real[]`
//! — it never touches pgvector's index or storage.
use crate::ann::{HnswIndex, IvfflatIndex, Metric};
use crate::pg::err_input;
use pgrx::prelude::*;

/// Which own index to build.
pub(crate) enum Algo {
    Hnsw,
    Ivfflat,
}

/// Per-call parameters (already the SQL defaults when the caller omits them).
pub(crate) struct Params {
    pub k: i32,
    pub m: i32,
    pub ef_construction: i32,
    pub ef_search: i32,
    pub lists: i32,
    pub probes: i32,
    pub seed: i64,
}

// pgvector-mirrored caps (EC-2 — prevent unbounded allocation). Sources: pgvector hnsw.h:50-58, ivfflat.h:51-54.
const M_MAX: i32 = 100;
const EF_MAX: i32 = 1000;
const LIST_MAX: i32 = 32768;

fn require(cond: bool, msg: &str) {
    if !cond {
        err_input(msg);
    }
}

/// A SQL identifier we will interpolate into the read query MUST match this (defense against injection — the
/// table itself arrives pre-resolved as `regclass::text`, but column names are caller text). Deliberately a
/// strict ASCII allowlist (not `%I`-quoting like M19): it is injection-proof and column names that are SQL
/// keywords / contain special chars are out of scope for an ANN embedding column — fail-fast 22023 instead.
fn valid_ident(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 63
        && name.bytes().next().is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Read `(id, vector)` rows from `src_table` (already a safe `regclass::text` name). Skips NULL vectors
/// (EC-1). Validates every vector has dimension `qdim` (fail-fast 22023 on inconsistency).
///
/// MEMORY: reads the whole corpus into a `Vec<(i64, Vec<f32>)>` (row count × vector dim × 4 bytes). This is
/// the measurement-first SQL-callable scope (ADR D1) — for very large tables the caller should pre-filter the
/// `src_table` (e.g. a view); a persisted, streaming on-disk access method is the deferred M21b follow-up.
fn read_corpus(src_table: &str, embed_col: &str, id_col: &str, qdim: usize) -> Vec<(i64, Vec<f32>)> {
    // id_col must be an integer-typed column (EC-6) — clean 22023 instead of a raw cast error deep in the read.
    let type_sql = format!(
        "SELECT format_type(a.atttypid, NULL) FROM pg_attribute a \
         WHERE a.attrelid = {rel}::regclass AND a.attname = '{col}' AND a.attnum > 0 AND NOT a.attisdropped",
        rel = quote_literal(src_table),
        col = id_col
    );
    match Spi::get_one::<String>(&type_sql) {
        Ok(Some(t)) if matches!(t.as_str(), "smallint" | "integer" | "bigint") => {}
        Ok(Some(t)) => err_input(&format!("theodb ann: id_col '{id_col}' must be an integer column, is '{t}'")),
        _ => err_input(&format!("theodb ann: id_col '{id_col}' not found on {src_table}")),
    }

    let sql = format!("SELECT ({id_col})::bigint, ({embed_col})::real[] FROM {src_table}");
    let mut corpus: Vec<(i64, Vec<f32>)> = Vec::new();
    Spi::connect(|client| {
        let table = client.select(&sql, None, &[]).unwrap_or_else(|e| {
            err_input(&format!("theodb ann: failed to read {src_table}.{embed_col}: {e}"))
        });
        for row in table {
            let id = match row.get::<i64>(1) {
                Ok(Some(v)) => v,
                _ => continue, // NULL id row — skip (cannot label it)
            };
            let v = match row.get::<Vec<f32>>(2) {
                Ok(Some(v)) => v,
                Ok(None) => continue,      // NULL vector — skip (EC-1, pgvector index semantics)
                Err(e) => err_input(&format!("theodb ann: bad vector in {embed_col}: {e}")),
            };
            if v.len() != qdim {
                err_input(&format!(
                    "theodb ann: vector dimension {} != query dimension {qdim} (row id {id})",
                    v.len()
                ));
            }
            corpus.push((id, v));
        }
    });
    corpus
}

/// Quote a string as a SQL literal (single-quote escaped) for the catalog lookup.
fn quote_literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Validate args, read the corpus once, build the chosen index, answer every query in the flattened `queries`
/// (`qdim`-sized chunks). Returns `(query_idx, id, distance)` rows ordered by distance within each query.
#[allow(clippy::too_many_arguments)]
pub(crate) fn knn(
    algo: Algo,
    src_table: &str,
    embed_col: &str,
    id_col: &str,
    metric_s: &str,
    queries: &[f32],
    qdim: i32,
    p: Params,
) -> Vec<(i32, i64, f64)> {
    // --- boundary validation (Rule 8; typed 22023) ---
    let metric = Metric::parse(metric_s)
        .unwrap_or_else(|| err_input(&format!("theodb ann: unknown metric '{metric_s}' (use l2|cosine|ip)")));
    require(valid_ident(embed_col), "theodb ann: embed_col is not a valid identifier");
    require(valid_ident(id_col), "theodb ann: id_col is not a valid identifier");
    require(qdim >= 1, "theodb ann: qdim must be >= 1");
    require(p.k >= 1, "theodb ann: k must be >= 1");
    // Per-algorithm caps (EC-2): only the knobs the chosen index actually uses are validated, so an unused
    // knob on the other algorithm's path never spuriously rejects a valid call.
    match algo {
        Algo::Hnsw => {
            require(p.m >= 2 && p.m <= M_MAX, "theodb ann: m must be in [2, 100]");
            require(
                p.ef_construction >= p.k && p.ef_construction <= EF_MAX,
                "theodb ann: ef_construction must be in [k, 1000]",
            );
            require(p.ef_search >= p.k && p.ef_search <= EF_MAX, "theodb ann: ef_search must be in [k, 1000]");
        }
        Algo::Ivfflat => {
            require(p.lists >= 1 && p.lists <= LIST_MAX, "theodb ann: lists must be in [1, 32768]");
            require(p.probes >= 1 && p.probes <= LIST_MAX, "theodb ann: probes must be in [1, 32768]");
        }
    }

    // Empty queries → 0 rows, no Spi read / build (EC-5).
    if queries.is_empty() {
        return Vec::new();
    }
    let qd = qdim as usize;
    if !queries.len().is_multiple_of(qd) {
        err_input(&format!(
            "theodb ann: queries length {} is not a multiple of qdim {qd}",
            queries.len()
        ));
    }
    let seed = p.seed as u64;
    let corpus = read_corpus(src_table, embed_col, id_col, qd);

    let k = p.k as usize;
    let mut out: Vec<(i32, i64, f64)> = Vec::new();
    match algo {
        Algo::Hnsw => {
            let idx = HnswIndex::build(&corpus, p.m as usize, p.ef_construction as usize, metric, seed);
            for (qi, chunk) in queries.chunks(qd).enumerate() {
                for (id, dist) in idx.search(chunk, k, p.ef_search as usize) {
                    out.push((qi as i32, id, dist));
                }
            }
        }
        Algo::Ivfflat => {
            let idx = IvfflatIndex::build(&corpus, p.lists as usize, metric, seed);
            for (qi, chunk) in queries.chunks(qd).enumerate() {
                for (id, dist) in idx.search(chunk, k, p.probes as usize) {
                    out.push((qi as i32, id, dist));
                }
            }
        }
    }
    out
}
