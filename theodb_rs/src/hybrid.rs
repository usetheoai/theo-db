//! SPI-orchestration adapter (blueprint M19, ADR-C): hybrid search (FTS + vector) fused by Reciprocal Rank Fusion,
//! ported from the plpgsql `ai.hybrid_search_rrf`/`ai.hybrid_search` (sql/40). The Rust function is the
//! ENTRYPOINT and owns orchestration; the RRF fusion itself stays ONE SQL string (RANK per leg → FULL
//! OUTER JOIN → summed COALESCE(1/(k+rank))), run via SPI. We do NOT reimplement RRF/RANK/FULL-OUTER-JOIN
//! in Rust — that would fork the fusion math (ADR-C / D2: one fusion source of truth). IDENTIFIER args
//! (id/vector/tsvector/text columns) are quoted with Postgres-native `format('%I', …)` over SPI (NOT
//! hand-rolled), and VALUE args (qvec/query_text/language) are execution binds (`$1..$6`) — both are
//! injection-safe. The ONE exception is `filter_sql` (M53): it is RAW caller-privilege SQL interpolated
//! as `%5$s` (a bare boolean predicate), NOT `%I`-quoted and NOT parametrized. Its safety is *syntactic
//! confinement* under SECURITY INVOKER (read-only SPI, no privilege boundary crossed — the predicate can
//! do nothing the caller could not do in a plain query), NOT injection-proofing. NEVER build `filter_sql`
//! from untrusted input, NEVER wrap this function in SECURITY DEFINER, NEVER GRANT it to a role you intend
//! to isolate (a subquery in the predicate reads with the caller's privileges by design).
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
/// `%3$s`=tbl (regclass::text, already quoted), `%4$I`=lexical column (tsvector), `%5$s`=filter_sql,
/// `%6$s`=vec_weight, `%7$s`=text_weight (M106: per-leg RRF weights, validated finite ≥ 0 and formatted as
/// numeric literals — injection-safe; default 1.0 each = the pre-M106 unweighted fusion) — all substituted by
/// Postgres `format()`. The `$1..$6` placeholders are LITERAL here (format only touches `%` specifiers) and
/// bind at execution: $1=qvec(text→::vector), $2=query_text, $3=per_leg_limit, $4=k, $5=result_limit,
/// $6=language(regconfig). Byte-faithful to sql/40:76-106 when both weights are 1.0.
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
       (%6$s * COALESCE(1.0 / ($4 + vec.rank), 0.0)
      + %7$s * COALESCE(1.0 / ($4 + fts.rank), 0.0))::real AS score
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
       (%6$s * COALESCE(1.0 / ($4 + vec.rank), 0.0)
      + %7$s * COALESCE(1.0 / ($4 + fts.rank), 0.0))::real AS score
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
    vec_weight: f64,
    text_weight: f64,
) -> Vec<(String, f32)> {
    // Fail-fast, typed (Rule 8) — mirror sql/40:45-57.
    if k <= 0 {
        err_input(&format!("ai.hybrid_search_rrf: k must be > 0 (got {k})"));
    }
    // M106 — weighted RRF: each leg's reciprocal-rank term is scaled by its weight. Weights must be finite
    // and non-negative (a negative weight would invert the fusion; NaN/inf would poison the numeric literal).
    // Default 1.0 each ⇒ byte-identical to the pre-M106 unweighted fusion. 0.0 disables a leg (valid).
    for (name, w) in [("vector_weight", vec_weight), ("text_weight", text_weight)] {
        if !w.is_finite() || w < 0.0 {
            err_input(&format!(
                "ai.hybrid_search: {name} must be a finite number >= 0 (got {w})"
            ));
        }
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
    // M53: the relational filter is inlined into BOTH legs' WHERE, wrapped in parens (`AND (<filter>)`).
    // SECURITY INVOKER + read-only SPI + REVOKE-from-PUBLIC mean NO privilege boundary is crossed — a
    // subquery in the predicate reads only what the caller could already read (this is by design, and is
    // why filter_sql must never be built from untrusted input; see the module docstring). The guard here is
    // defense-in-depth *syntactic confinement*, NOT injection-proofing: reject a statement terminator and
    // SQL comment sequences so the predicate cannot neutralize the closing paren / trailing `ORDER BY`/
    // `LIMIT` / the second CTE and break out of `( ... )`. It is honestly a blacklist, not a parser — a
    // read-only subquery still composes (caller-privilege). Absent filter ⇒ `true` (no-op, byte-identical
    // to the pre-M53 legs). A structured filter API (column/op/value, %I + bind) is the fail-closed
    // follow-up (backlog).
    let filter = filter_sql.unwrap_or("true");
    if filter.contains(';') || filter.contains("--") || filter.contains("/*") || filter.contains("*/") {
        err_input(
            "ai.hybrid_search_rrf: filter_sql must be a single boolean predicate (no ';', comment, or chaining) — it is raw caller-privilege SQL, never build it from untrusted input",
        );
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
    // M106: the two weights are rendered as fixed-precision decimal literals (validated finite ≥ 0 above, so
    // the string is pure numeric — injection-safe) and passed as the `%6$s`/`%7$s` format args. The `+ 0.0`
    // normalizes IEEE-754 negative zero (`-0.0` passes `>= 0.0`) to `+0.0`, so the literal never carries a
    // stray leading `-` (keeps the "unsigned decimal literal" invariant; semantically `-0.0` == `0.0`).
    let vec_w_lit = format!("{:.6}", vec_weight + 0.0);
    let text_w_lit = format!("{:.6}", text_weight + 0.0);
    let build_q = format!("SELECT format($fq${template}$fq$, $1, $2, $3, $4, $5, $6, $7)");
    let built = Spi::get_one_with_args::<String>(
        &build_q,
        &[
            id_col.into(), vector_col.into(), tbl_text.into(), lexical_col.into(), filter.into(),
            vec_w_lit.into(), text_w_lit.into(),
        ],
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
    // M106 — per-leg RRF weights (default 1.0 = unweighted). Accepts int or float JSON; run_rrf validates
    // finite ≥ 0. This honors the `vector_weight`/`text_weight` keys the docs promised (audit gap 06).
    let as_f64 = |k: &str, d: f64| cfg.get(k).and_then(|v| v.as_f64()).unwrap_or(d);
    let vec_weight = as_f64("vector_weight", 1.0);
    let text_weight = as_f64("text_weight", 1.0);

    // Resolve the bare table name to a regclass::text (same as the plpgsql `(config->>'table')::regclass`
    // then `tbl::text`) — Postgres quotes it safely; a non-existent relation raises naturally.
    let tbl_text = Spi::get_one_with_args::<String>("SELECT ($1)::regclass::text", &[table.into()])
        .ok()
        .flatten()
        .unwrap_or_else(|| err_input("ai.hybrid_search: table does not resolve to a relation"));

    run_rrf(
        &tbl_text, id_col, content_tsv_col, vector_col, query_text, query_vector, k, per_leg_limit,
        result_limit, language, filter_sql, lexical_engine, content_text_col, vec_weight, text_weight,
    )
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    // Two SINGLE-LEG docs so the weight cleanly decides the winner: `dv` is ONLY in the vector leg (has an
    // embedding, no 'database' term → absent from FTS), `df` is ONLY in the FTS leg (`embedding IS NULL` →
    // excluded by the vector leg's `IS NOT NULL` guard, has 'database'). Each contributes w_leg/(k+1), so the
    // ranking is decided purely by which leg's weight is larger. Query supplies query_vector (no embed needed).
    fn seed() {
        Spi::run(
            "CREATE TABLE hybw (doc_id text PRIMARY KEY, body text, text_tsv tsvector, embedding vector(3))",
        )
        .unwrap();
        Spi::run(
            "INSERT INTO hybw VALUES \
             ('dv','unrelated lexical words', to_tsvector('english','unrelated lexical words'), '[1,0,0]'), \
             ('df','database database database', to_tsvector('english','database database database'), NULL)",
        )
        .unwrap();
    }

    fn top(vector_weight: &str, text_weight: &str) -> String {
        seed();
        let sql = format!(
            "SELECT id FROM ai.hybrid_search(jsonb_build_object(\
             'table','hybw','id_col','doc_id','content_tsv_col','text_tsv','vector_col','embedding',\
             'query_text','database','query_vector','[1,0,0]','result_limit',5,\
             'vector_weight',{vector_weight},'text_weight',{text_weight})) LIMIT 1"
        );
        Spi::get_one::<String>(&sql).unwrap().unwrap()
    }

    // M106: upweighting the vector leg lifts its top doc (dv) to #1; upweighting the text leg flips it to df.
    #[pg_test]
    fn m106_vector_weight_lifts_vector_leg_top() {
        assert_eq!(top("3", "1"), "dv", "vector_weight=3 must rank the vector-leg winner first");
    }

    #[pg_test]
    fn m106_text_weight_flips_ranking_to_fts_leg_top() {
        assert_eq!(top("1", "3"), "df", "text_weight=3 must flip the ranking to the FTS-leg winner");
    }

    // M106 (review LOW): IEEE-754 `-0.0` passes the `>= 0` guard; it must be treated as `0.0` (disable the
    // leg) and MUST NOT emit a stray `-` into the SQL literal. `-0.0` on the vector leg ⇒ FTS-leg doc wins.
    #[pg_test]
    fn m106_negative_zero_weight_behaves_as_zero() {
        assert_eq!(top("-0.0", "1"), "df", "vector_weight=-0.0 disables the vector leg (behaves as 0.0)");
    }

    // M106: a negative weight is rejected with a typed 22023 (fail-fast).
    #[pg_test]
    fn m106_negative_weight_rejected() {
        seed();
        let r = std::panic::catch_unwind(|| {
            Spi::run(
                "SELECT id FROM ai.hybrid_search(jsonb_build_object(\
                 'table','hybw','id_col','doc_id','content_tsv_col','text_tsv','vector_col','embedding',\
                 'query_text','database','vector_weight',-1))",
            )
        });
        assert!(r.is_err(), "a negative vector_weight must raise (not silently proceed)");
    }
}
