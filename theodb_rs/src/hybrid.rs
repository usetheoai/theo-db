//! SPI-orchestration adapter (blueprint M19, ADR-C): hybrid search (FTS + vector) fused by Reciprocal Rank Fusion,
//! ported from the plpgsql `ai.hybrid_search_rrf`/`ai.hybrid_search` (sql/40). The Rust function is the
//! ENTRYPOINT and owns orchestration; the RRF fusion itself stays ONE SQL string (RANK per leg → FULL
//! OUTER JOIN → summed COALESCE(1/(k+rank))), run via SPI. We do NOT reimplement RRF/RANK/FULL-OUTER-JOIN
//! in Rust — that would fork the fusion math (ADR-C / D2: one fusion source of truth). Identifier args are
//! quoted with Postgres-native `format('%I', …)` over SPI (NOT hand-rolled), so the injection-safety of the
//! plpgsql version is preserved byte-for-byte (sql/40 used the same `%I`/`%s`/`$n` discipline).
//!
//! Legs: vector (`<=>` cosine, pgvector) and FTS (`ts_rank_cd` over a tsvector column, 'english' pinned).
//! A doc matched by only one retriever still surfaces (FULL OUTER JOIN + COALESCE). Deterministic tie-break
//! `score DESC, id ASC` matches the offline Python twin `rrf_fuse`. Fail-fast typed errors: k/per_leg_limit/
//! result_limit must be > 0 and at least one of query_text/query_vector must be present (22023); the
//! `theodb.embed` seam is guarded with 0A000 when theodb_rs (and thus embed) was dropped.
use pgrx::prelude::*;
use serde_json::Value;

use crate::pg::{err_input, err_unsupported};

/// The RRF fusion template (default `ts_rank_cd` lexical leg). `%1$I`=id_col, `%2$I`=vector_col,
/// `%3$s`=tbl (regclass::text, already quoted), `%4$I`=lexical column (tsvector), `%5$s`=filter_sql —
/// substituted by Postgres `format()` (injection-safe). The `$1..$6` placeholders are LITERAL here (format
/// only touches `%` specifiers) and bind at execution: $1=qvec(text→::vector), $2=query_text,
/// $3=per_leg_limit, $4=k, $5=result_limit, $6=language(regconfig). Byte-faithful to sql/40:76-106.
const FUSION_TEMPLATE_TSRANK: &str = r#"WITH vec AS (
    SELECT %1$I AS _id,
           RANK() OVER (ORDER BY %2$I <=> $1::vector) AS rank
    FROM %3$s
    WHERE %2$I IS NOT NULL AND $1 IS NOT NULL AND (%5$s)
    ORDER BY %2$I <=> $1::vector
    LIMIT $3
),
fts AS (
    SELECT %1$I AS _id,
           RANK() OVER (ORDER BY ts_rank_cd(%4$I, plainto_tsquery($6::regconfig, $2)) DESC) AS rank
    FROM %3$s
    WHERE $2 IS NOT NULL AND %4$I @@ plainto_tsquery($6::regconfig, $2) AND (%5$s)
    ORDER BY ts_rank_cd(%4$I, plainto_tsquery($6::regconfig, $2)) DESC
    LIMIT $3
)
SELECT COALESCE(vec._id, fts._id)::text AS id,
       (COALESCE(1.0 / ($4 + vec.rank), 0.0)
      + COALESCE(1.0 / ($4 + fts.rank), 0.0))::real AS score
FROM vec
FULL OUTER JOIN fts ON vec._id = fts._id
ORDER BY score DESC, id ASC
LIMIT $5"#;

/// M53 item 2: the BM25 lexical-leg variant. The `vec` CTE and the RRF tail are byte-identical to
/// `FUSION_TEMPLATE_TSRANK`; only the `fts` CTE changes. `%4$I` is the TEXT column indexed `USING bm25`.
/// The pg_textsearch `<@>` operator returns the NEGATED BM25 score (distance: smaller = better match), so
/// the leg orders ASC (best-first) — symmetric to the vector `<=>` leg — NOT `DESC`. There is no `@@`
/// match-filter (BM25 is a top-k ranker; `LIMIT $3` via Block-Max WAND selects the top matches), which is
/// byte-faithful to the offline twin `db.bm25_query`. `$6` (language) is NOT referenced here — the analyzer
/// is fixed at index build time (`text_config`), so `language` is inert for BM25; run_rrf binds only $1..$5
/// for this template. `%5$s` composes the M53 filter_sql identically to the ts_rank_cd leg.
const FUSION_TEMPLATE_BM25: &str = r#"WITH vec AS (
    SELECT %1$I AS _id,
           RANK() OVER (ORDER BY %2$I <=> $1::vector) AS rank
    FROM %3$s
    WHERE %2$I IS NOT NULL AND $1 IS NOT NULL AND (%5$s)
    ORDER BY %2$I <=> $1::vector
    LIMIT $3
),
fts AS (
    SELECT %1$I AS _id,
           RANK() OVER (ORDER BY %4$I <@> $2) AS rank
    FROM %3$s
    WHERE $2 IS NOT NULL AND %4$I IS NOT NULL AND (%5$s)
    ORDER BY %4$I <@> $2
    LIMIT $3
)
SELECT COALESCE(vec._id, fts._id)::text AS id,
       (COALESCE(1.0 / ($4 + vec.rank), 0.0)
      + COALESCE(1.0 / ($4 + fts.rank), 0.0))::real AS score
FROM vec
FULL OUTER JOIN fts ON vec._id = fts._id
ORDER BY score DESC, id ASC
LIMIT $5"#;

/// Orchestrate one RRF hybrid search and return the fused rows. `tbl_text` is a `regclass::text` (already
/// correctly quoted by Postgres). `query_vector_text` is the pgvector value rendered as text (or None).
/// Diverges (typed error) on any guard violation; otherwise returns `(id, score)` ordered by fused score.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_rrf(
    tbl_text: &str,
    id_col: &str,
    content_tsv_col: &str,
    vector_col: &str,
    query_text: Option<&str>,
    query_vector_text: Option<&str>,
    k: i32,
    per_leg_limit: i32,
    result_limit: i32,
    language: &str,
    filter_sql: Option<&str>,
    lexical_engine: &str,
    content_text_col: Option<&str>,
) -> Vec<(String, f32)> {
    // Fail-fast, typed (Rule 8) — mirror sql/40:45-57.
    if k <= 0 {
        err_input(&format!("ai.hybrid_search_rrf: k must be > 0 (got {k})"));
    }
    if per_leg_limit <= 0 {
        err_input(&format!(
            "ai.hybrid_search_rrf: per_leg_limit must be > 0 (got {per_leg_limit})"
        ));
    }
    if result_limit <= 0 {
        err_input(&format!(
            "ai.hybrid_search_rrf: result_limit must be > 0 (got {result_limit})"
        ));
    }
    if query_text.is_none() && query_vector_text.is_none() {
        err_input("ai.hybrid_search_rrf: provide query_text and/or query_vector");
    }
    // M53: the relational filter is inlined into BOTH legs' WHERE, syntactically confined in parens
    // (`AND (<filter>)`). The whole function is SECURITY INVOKER (runs with the CALLER's privilege — no
    // privilege boundary is crossed), so the filter can do nothing the caller could not do in a plain query.
    // The one hard confinement guard: reject a statement terminator, so the filter cannot chain a second
    // statement out of the CTE's WHERE. Absent filter ⇒ `true` (no-op, byte-identical to the pre-M53 legs).
    let filter = filter_sql.unwrap_or("true");
    if filter.contains(';') {
        err_input("ai.hybrid_search_rrf: filter_sql must be a single boolean predicate (no ';')");
    }

    // M53 item 2: select the lexical leg (ts_rank_cd default | bm25 opt-in). The `%4$I` format slot means
    // "the lexical column, as the engine interprets it": a tsvector for ts_rank_cd, the raw TEXT column
    // (indexed `USING bm25`) for bm25. Fail-fast typed (Rule 8) on an invalid engine or a bm25 leg missing
    // its text column — NEVER silently fall back (a silent fallback would let a caller measure ts_rank_cd
    // while believing it is BM25). The `bm25` path additionally requires the pg_textsearch extension —
    // absent on the shipped image, so it surfaces a clear 0A000 (mirrors the embed-seam guard) rather than a
    // cryptic 42883 mid-query. `language` ($6) is inert for bm25 (analyzer fixed at index build).
    let (template, lexical_col, binds_language) = match lexical_engine {
        "ts_rank_cd" => (FUSION_TEMPLATE_TSRANK, content_tsv_col, true),
        "bm25" => {
            let text_col = content_text_col.unwrap_or_else(|| {
                err_input(
                    "ai.hybrid_search_rrf: lexical_engine='bm25' requires content_text_col (the TEXT column indexed USING bm25)",
                )
            });
            let missing = Spi::get_one::<bool>(
                "SELECT NOT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pg_textsearch')",
            )
            .ok()
            .flatten()
            .unwrap_or(true);
            if missing {
                err_unsupported(
                    "ai.hybrid_search_rrf: lexical_engine='bm25' requires the pg_textsearch extension (CREATE EXTENSION pg_textsearch, shared_preload_libraries=pg_textsearch) — not present on the shipped image; use lexical_engine='ts_rank_cd' (default)",
                );
            }
            (FUSION_TEMPLATE_BM25, text_col, false)
        }
        other => err_input(&format!(
            "ai.hybrid_search_rrf: lexical_engine must be 'ts_rank_cd' or 'bm25' (got '{other}')"
        )),
    };

    // Resolve the vector leg's query vector (explicit wins; else embed query_text). Extracted (M25).
    let qvec: Option<String> = resolve_query_vector(query_text, query_vector_text);

    // Build the fusion SQL with Postgres-native %I quoting (injection-safe) — one format() call over SPI.
    // `%1$I..%4$I` = identifiers (quoted), `%3$s` = regclass text, `%5$s` = the confined filter predicate.
    // The template is dollar-quoted ($fq$…$fq$); its literal $1..$6 survive and bind at execution.
    let build_q = format!("SELECT format($fq${template}$fq$, $1, $2, $3, $4, $5)");
    let built = Spi::get_one_with_args::<String>(
        &build_q,
        &[id_col.into(), vector_col.into(), tbl_text.into(), lexical_col.into(), filter.into()],
    )
    .ok()
    .flatten()
    .unwrap_or_else(|| err_input("ai.hybrid_search_rrf: could not build fusion query"));

    // Execute the fused query and collect (id, score). SELECT-only → read-only Spi::connect. The bm25
    // template does NOT reference $6, so bind only $1..$5 for it (binding an unreferenced param would be
    // rejected by SPI); the ts_rank_cd template references $6 (language::regconfig).
    Spi::connect(|client| {
        let mut exec_args: Vec<pgrx::datum::DatumWithOid> = vec![
            qvec.clone().into(),
            query_text.into(),
            per_leg_limit.into(),
            k.into(),
            result_limit.into(),
        ];
        if binds_language {
            exec_args.push(language.into());
        }
        let table = client
            .select(built.as_str(), None, &exec_args)
            .unwrap_or_else(|e| err_input(&format!("ai.hybrid_search_rrf: fusion query failed: {e:?}")));
        let mut out: Vec<(String, f32)> = Vec::with_capacity(table.len());
        for row in table {
            let id = row.get::<String>(1).ok().flatten();
            let score = row.get::<f32>(2).ok().flatten();
            if let (Some(id), Some(score)) = (id, score) {
                out.push((id, score));
            }
        }
        out
    })
}

/// Resolve the vector leg's query vector (as text): an explicit `query_vector` wins; otherwise embed
/// `query_text` via `theodb.embed`. Extracted from `run_rrf` (M25). Includes the embed-seam fail-fast guard
/// (audit #3/#8: `theodb.embed` is a late-bound cross-extension call with no `pg_depend` edge — dropping
/// theodb_rs would surface as a cryptic 42883 mid-query; check the exact 2-arg signature and turn absence into
/// a clear 0A000). Diverges (typed) on embed failure. Returns `None` only if both inputs are absent
/// (unreachable — `run_rrf` rejects that upstream).
fn resolve_query_vector(query_text: Option<&str>, query_vector_text: Option<&str>) -> Option<String> {
    match query_vector_text {
        Some(v) => Some(v.to_string()),
        None => match query_text {
            Some(qt) => {
                let missing = Spi::get_one::<bool>(
                    "SELECT to_regprocedure('theodb.embed(text, text)') IS NULL",
                )
                .ok()
                .flatten()
                .unwrap_or(true);
                if missing {
                    err_unsupported(
                        "ai.hybrid_search_rrf: theodb.embed is unavailable — install the theodb_rs extension (CREATE EXTENSION theodb_rs), or pass query_vector explicitly",
                    );
                }
                Spi::get_one_with_args::<String>("SELECT theodb.embed($1)::text", &[qt.into()])
                    .unwrap_or_else(|e| err_input(&format!("ai.hybrid_search_rrf: embed failed: {e:?}")))
            }
            None => None,
        },
    }
}
/// Parse the `ai.hybrid_search(jsonb)` config and delegate to `run_rrf` (one fusion source of truth).
/// Required keys missing → 22023. Returns the fused rows. Called by the `#[pg_extern]` in `lib.rs`.
pub(crate) fn run_rrf_json(cfg: Value) -> Vec<(String, f32)> {
    let get_str = |k: &str| cfg.get(k).and_then(|v| v.as_str());
    let (table, id_col, content_tsv_col, vector_col) =
        match (get_str("table"), get_str("id_col"), get_str("content_tsv_col"), get_str("vector_col")) {
            (Some(t), Some(i), Some(c), Some(v)) => (t, i, c, v),
            _ => err_input(
                "ai.hybrid_search: config must include table, id_col, content_tsv_col, vector_col",
            ),
        };
    let query_text = get_str("query_text");
    let query_vector = get_str("query_vector");
    let as_i32 = |k: &str, d: i32| cfg.get(k).and_then(|v| v.as_i64()).map(|n| n as i32).unwrap_or(d);
    let k = as_i32("k", 60);
    let per_leg_limit = as_i32("per_leg_limit", 20);
    let result_limit = as_i32("result_limit", 5);
    // M53: optional `language` (FTS regconfig, default english) + `filter_sql` (relational WHERE predicate)
    // + `lexical_engine` (ts_rank_cd default | bm25) + `content_text_col` (bm25 TEXT column).
    let language = get_str("language").unwrap_or("english");
    let filter_sql = get_str("filter_sql");
    let lexical_engine = get_str("lexical_engine").unwrap_or("ts_rank_cd");
    let content_text_col = get_str("content_text_col");

    // Resolve the bare table name to a regclass::text (same as the plpgsql `(config->>'table')::regclass`
    // then `tbl::text`) — Postgres quotes it safely; a non-existent relation raises naturally.
    let tbl_text = Spi::get_one_with_args::<String>("SELECT ($1)::regclass::text", &[table.into()])
        .ok()
        .flatten()
        .unwrap_or_else(|| err_input("ai.hybrid_search: table does not resolve to a relation"));

    run_rrf(
        &tbl_text, id_col, content_tsv_col, vector_col, query_text, query_vector, k, per_leg_limit,
        result_limit, language, filter_sql, lexical_engine, content_text_col,
    )
}
