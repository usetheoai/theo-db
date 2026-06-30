//! Domain layer (blueprint M19, ADR-C): hybrid search (FTS + vector) fused by Reciprocal Rank Fusion,
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

use crate::pg::{err_input, err_unsupported};

/// The RRF fusion template. `%1$I`=id_col, `%2$I`=vector_col, `%3$s`=tbl (regclass::text, already quoted),
/// `%4$I`=content_tsv_col — substituted by Postgres `format()` (injection-safe). The `$1..$5` placeholders
/// are LITERAL here (format only touches `%` specifiers) and bind at execution: $1=qvec(text→::vector),
/// $2=query_text, $3=per_leg_limit, $4=k, $5=result_limit. Byte-faithful to sql/40:76-106 (the only change
/// vs the plpgsql original is `$1::vector`, since qvec crosses the Rust boundary as text).
const FUSION_TEMPLATE: &str = r#"WITH vec AS (
    SELECT %1$I AS _id,
           RANK() OVER (ORDER BY %2$I <=> $1::vector) AS rank
    FROM %3$s
    WHERE %2$I IS NOT NULL AND $1 IS NOT NULL
    ORDER BY %2$I <=> $1::vector
    LIMIT $3
),
fts AS (
    SELECT %1$I AS _id,
           RANK() OVER (ORDER BY ts_rank_cd(%4$I, plainto_tsquery('english', $2)) DESC) AS rank
    FROM %3$s
    WHERE $2 IS NOT NULL AND %4$I @@ plainto_tsquery('english', $2)
    ORDER BY ts_rank_cd(%4$I, plainto_tsquery('english', $2)) DESC
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
fn run_rrf(
    tbl_text: &str,
    id_col: &str,
    content_tsv_col: &str,
    vector_col: &str,
    query_text: Option<&str>,
    query_vector_text: Option<&str>,
    k: i32,
    per_leg_limit: i32,
    result_limit: i32,
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

    // Resolve the vector leg's query vector (as text): explicit query_vector wins; else embed query_text.
    let qvec: Option<String> = match query_vector_text {
        Some(v) => Some(v.to_string()),
        None => match query_text {
            Some(qt) => {
                // Fail-fast seam guard (audit #3/#8): theodb.embed is a late-bound cross-extension call with
                // no pg_depend edge — dropping theodb_rs would surface as a cryptic 42883 mid-query. Check the
                // exact 2-arg signature (model has a DEFAULT) and turn absence into a clear 0A000.
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
                Spi::get_one_with_args::<String>(
                    "SELECT theodb.embed($1)::text",
                    &[qt.into()],
                )
                .unwrap_or_else(|e| err_input(&format!("ai.hybrid_search_rrf: embed failed: {e:?}")))
            }
            None => None, // unreachable: rejected above
        },
    };

    // Build the fusion SQL with Postgres-native %I quoting (injection-safe) — one format() call over SPI.
    // The template is dollar-quoted ($fq$…$fq$); its literal $1..$5 survive and bind at execution.
    let build_q = format!("SELECT format($fq${FUSION_TEMPLATE}$fq$, $1, $2, $3, $4)");
    let built = Spi::get_one_with_args::<String>(
        &build_q,
        &[id_col.into(), vector_col.into(), tbl_text.into(), content_tsv_col.into()],
    )
    .ok()
    .flatten()
    .unwrap_or_else(|| err_input("ai.hybrid_search_rrf: could not build fusion query"));

    // Execute the fused query and collect (id, score). SELECT-only → read-only Spi::connect.
    Spi::connect(|client| {
        let table = client
            .select(
                built.as_str(),
                None,
                &[
                    qvec.clone().into(),
                    query_text.into(),
                    per_leg_limit.into(),
                    k.into(),
                    result_limit.into(),
                ],
            )
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

#[pg_schema]
mod theodb_rs {
    use super::run_rrf;
    use crate::pg::err_input;
    use pgrx::prelude::*;

    /// api-surface: the RRF hybrid-search entrypoint (the SQL `ai.hybrid_search_rrf`). The public wrapper
    /// passes `tbl::text` (regclass→quoted name) and `query_vector::text`; this returns the fused table.
    #[pg_extern]
    #[allow(clippy::too_many_arguments)]
    fn _hybrid_search_rrf(
        tbl_text: &str,
        id_col: &str,
        content_tsv_col: &str,
        vector_col: &str,
        query_text: Option<&str>,
        query_vector_text: Option<&str>,
        k: i32,
        per_leg_limit: i32,
        result_limit: i32,
    ) -> TableIterator<'static, (name!(id, String), name!(score, f32))> {
        let rows = run_rrf(
            tbl_text, id_col, content_tsv_col, vector_col, query_text, query_vector_text, k,
            per_leg_limit, result_limit,
        );
        TableIterator::new(rows)
    }

    /// api-surface: the literal spec-06 JSON surface (the SQL `ai.hybrid_search(jsonb)`). Parses the config
    /// and delegates to the SAME `run_rrf` (one fusion source of truth). Required keys missing → 22023.
    #[pg_extern]
    fn _hybrid_search_json(
        config: pgrx::JsonB,
    ) -> TableIterator<'static, (name!(id, String), name!(score, f32))> {
        let cfg = config.0;
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

        // Resolve the bare table name to a regclass::text (same as the plpgsql `(config->>'table')::regclass`
        // then `tbl::text`) — Postgres quotes it safely; a non-existent relation raises naturally.
        let tbl_text = Spi::get_one_with_args::<String>("SELECT ($1)::regclass::text", &[table.into()])
            .ok()
            .flatten()
            .unwrap_or_else(|| err_input("ai.hybrid_search: table does not resolve to a relation"));

        let rows = run_rrf(
            &tbl_text, id_col, content_tsv_col, vector_col, query_text, query_vector, k,
            per_leg_limit, result_limit,
        );
        TableIterator::new(rows)
    }
}
