//! TheoDB's own PostgreSQL extension in Rust (pgrx) — M17 (ROADMAP-v2 / ADR 0006).
//!
//! `theodb.embed` (formerly plpython3u + urllib, `sql/30-theodb-embed.sql`) rewritten in Rust at
//! **proven parity**: same signature, same vector output, same typed SQLSTATEs as the baseline.
//! Mirrors AlloyDB's embedding() pattern — the DB calls a configurable model endpoint (it does NOT
//! ship a model). Configuration via the same session GUCs:
//!   SET theodb.embedding_endpoint = 'http://host:8088/v1/embeddings';   -- required
//!   SET theodb.embedding_model    = 'BAAI/bge-small-en-v1.5';           -- optional
//!   SET theodb.embedding_api_key  = '...';                              -- optional bearer token
//!
//! Layering (blueprint ADR-1 — 3 boundaries): `pg` (Postgres/pgrx glue: typed errors + GUCs) ·
//! `embed` (portable domain logic: the HTTP call + parse) · this file (api-surface: the `#[pg_extern]`
//! entrypoint + the `theodb.embed` SQL wrapper). `lib.rs` is the composition/api root + module map.
use pgrx::prelude::*;

::pgrx::pg_module_magic!();

mod chat;
mod embed;
mod http;
mod hybrid;
mod migrate;
mod nl;
mod pg;

// theodb_rs owns its OWN schema `theodb_rs` (so it never tries to CREATE the `theodb` schema, which
// is owned by the umbrella `theodb` extension — PG forbids a second extension from CREATE-IF-NOT-EXISTS
// on a schema it does not own). The public `theodb.embed` wrapper is created INTO the existing `theodb`
// schema via extension_sql (creating an object in another extension's schema is allowed; only CREATE
// SCHEMA conflicts). theodb_rs `requires = 'theodb'` so the `theodb` schema exists first.
#[pg_schema]
mod theodb_rs {
    use pgrx::prelude::*;

    /// api-surface: the `theodb_rs._embed_text(content, model)` entrypoint. A thin delegate to the
    /// domain logic in `crate::embed` — the SQL wrapper `theodb.embed` casts its text output to `vector`.
    /// Internal: REVOKEd from PUBLIC alongside the wrapper.
    #[pg_extern]
    fn _embed_text(content: Option<&str>, model: Option<&str>) -> String {
        crate::embed::run(content, model)
    }

    /// api-surface: the `theodb_rs._embed_batch_text(content[], model)` entrypoint — N inputs → N
    /// pgvector text literals in ONE HTTP round-trip (the N→1 fix for the audit's CRITICAL embed N+1).
    /// A thin delegate to `crate::embed::run_batch`; the SQL wrapper `theodb.embed_batch` casts each
    /// element `::vector`. Internal: REVOKEd from PUBLIC alongside the wrapper.
    #[pg_extern]
    fn _embed_batch_text(content: Vec<Option<String>>, model: Option<&str>) -> Vec<String> {
        // text[] maps to Vec<Option<String>> (nullable elements); borrow each as &str for the domain
        // layer (which detects NULL elements -> 22023). `content` lives through the call.
        let refs: Vec<Option<&str>> = content.iter().map(|o| o.as_deref()).collect();
        crate::embed::run_batch(&refs, model)
    }

    // ── M18: the generative ai.* surface (was plpython3u in sql/50) ──────────────────────────────────
    // Thin delegates to `crate::chat`; the public `ai.*` SQL wrappers (below) carry the documented names,
    // return types, VOLATILE, and REVOKE. `ai._chat` stays SQL-callable so ai.generate/summarize/the
    // aggregate finalfunc / M19 nl_to_sql keep working.

    /// `theodb_rs._ai_chat` — one chat-completions round-trip (the SQL `ai._chat`).
    #[pg_extern]
    fn _ai_chat(prompt: Option<&str>, system: Option<&str>, model: Option<&str>) -> String {
        crate::chat::chat(prompt, system, model)
    }

    /// `theodb_rs._ai_if` — natural-language condition -> boolean (the SQL `ai.if`).
    #[pg_extern]
    fn _ai_if(prompt: Option<&str>, model: Option<&str>) -> bool {
        crate::chat::ai_if(prompt, model)
    }

    /// `theodb_rs._ai_sentiment` — content -> {positive,negative,neutral} (the SQL `ai.analyze_sentiment`).
    #[pg_extern]
    fn _ai_sentiment(content: Option<&str>, model: Option<&str>) -> String {
        crate::chat::ai_sentiment(content, model)
    }

    /// `theodb_rs._ai_rank` — natural-language scoring -> real (the SQL `ai.rank`).
    #[pg_extern]
    fn _ai_rank(prompt: Option<&str>, model: Option<&str>) -> f32 {
        crate::chat::ai_rank(prompt, model)
    }

    /// `theodb_rs._ai_generate_batch` — N prompts -> N answers in ONE round-trip (the SQL `ai.generate_batch`).
    /// NULL array -> 22023; NULL element -> 22023; empty -> empty (no call); JSON null -> SQL NULL element.
    #[pg_extern]
    fn _ai_generate_batch(
        prompts: Option<Vec<Option<String>>>,
        model: Option<&str>,
    ) -> Vec<Option<String>> {
        let prompts = match prompts {
            Some(p) => p,
            None => crate::pg::err_input("ai.generate_batch: prompts must not be NULL"),
        };
        let refs: Vec<Option<&str>> = prompts.iter().map(|o| o.as_deref()).collect();
        crate::chat::ai_generate_batch(&refs, model)
    }

    // ── M19: NL→SQL (the last plpython3u) — anti-injection L1/L2/L4 + L3 sandbox, now Rust ───────────────
    /// `theodb_rs._nl_to_sql` — validate a question into ONE read-only SELECT over the allowlist (the SQL
    /// `ai.nl_to_sql`). Returns the validated SQL; raises 22023 on any violation. Does NOT execute.
    #[pg_extern]
    fn _nl_to_sql(question: Option<&str>, allowed_relations: Vec<Option<String>>, model: Option<&str>) -> String {
        let refs: Vec<Option<&str>> = allowed_relations.iter().map(|o| o.as_deref()).collect();
        crate::nl::nl_to_sql(question, &refs, model)
    }

    // ── M19: hybrid search (RRF) — Rust entrypoints orchestrating the fusion SQL via SPI (crate::hybrid) ──
    /// `theodb_rs._hybrid_search_rrf` — the RRF hybrid-search entrypoint (the SQL `ai.hybrid_search_rrf`).
    /// The public wrapper passes `tbl::text` (regclass→quoted name) and `query_vector::text`.
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
        TableIterator::new(crate::hybrid::run_rrf(
            tbl_text, id_col, content_tsv_col, vector_col, query_text, query_vector_text, k,
            per_leg_limit, result_limit,
        ))
    }

    /// `theodb_rs._hybrid_search_json` — the literal spec-06 JSON surface (the SQL `ai.hybrid_search(jsonb)`).
    /// Delegates to the SAME fusion as `_hybrid_search_rrf` (one fusion source of truth). Missing keys → 22023.
    #[pg_extern]
    fn _hybrid_search_json(
        config: pgrx::JsonB,
    ) -> TableIterator<'static, (name!(id, String), name!(score, f32))> {
        TableIterator::new(crate::hybrid::run_rrf_json(config.0))
    }

    // ── M19: Pinecone import — Rust loop + %I-quoted INSERT via SPI (crate::migrate) ─────────────────────
    /// `theodb_rs._import_pinecone` — the Pinecone import entrypoint (the SQL `theodb.import_pinecone`).
    /// The public wrapper passes `target::text` (regclass→quoted name). Returns the count of inserted records.
    #[pg_extern]
    fn _import_pinecone(
        target_text: &str,
        export: pgrx::JsonB,
        id_col: &str,
        embedding_col: &str,
        metadata_col: &str,
    ) -> i32 {
        crate::migrate::import(target_text, export.0, id_col, embedding_col, metadata_col)
    }
}

// SQL wrapper: the public `theodb.embed(content, model DEFAULT NULL) RETURNS vector`. Casts the Rust
// function's text output to `vector` via pgvector's input function (plan ADR D5 — no extra crate).
// Both functions are REVOKEd from PUBLIC (least-privilege parity with sql/30:80).
extension_sql!(
    r#"
CREATE FUNCTION theodb.embed(content text, model text DEFAULT NULL)
RETURNS vector
LANGUAGE sql
AS $$ SELECT theodb_rs._embed_text(content, model)::vector $$;

COMMENT ON FUNCTION theodb.embed(text, text) IS
  'Generate an embedding for content via the configurable model endpoint (theodb.embedding_endpoint). '
  'Returns a pgvector value. Implemented in Rust (theodb_rs extension, M17). '
  'SECURITY: server-side outbound HTTP to the configured endpoint (http(s) only, no redirects); '
  'not granted to PUBLIC. theodb.embedding_api_key is a session GUC (visible to SHOW / captured by '
  'log_statement) — set it per-session out of band, not in logged DDL. '
  'CALL IS SYNCHRONOUS: one blocking HTTP round-trip per row.';

REVOKE ALL ON FUNCTION theodb.embed(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._embed_text(text, text) FROM PUBLIC;
"#,
    name = "theodb_embed_wrapper",
    // Reference the pgrx function as a bare-ident PositioningRef::FullPath (matches the extern by
    // `unaliased_name`, like pgvectorscale's `requires = [smallint_array_overlap]`) so the wrapper's
    // SQL-language body, which validates `theodb_rs._embed_text` at CREATE time, is emitted AFTER it.
    requires = [_embed_text],
);

// SQL wrapper: the public `theodb.embed_batch(content text[], model DEFAULT NULL) RETURNS vector[]`.
// Casts each Rust text-literal output `::vector` and preserves the input order via WITH ORDINALITY.
// COALESCE makes an empty input array return an empty `vector[]` (NOT NULL) — the array_agg over zero
// rows is NULL otherwise (edge-case EC-1). REVOKEd from PUBLIC (least-privilege parity with embed).
extension_sql!(
    r#"
CREATE FUNCTION theodb.embed_batch(content text[], model text DEFAULT NULL)
RETURNS vector[]
LANGUAGE sql
AS $$
  SELECT COALESCE(array_agg(t::vector ORDER BY ord), ARRAY[]::vector[])
  FROM unnest(theodb_rs._embed_batch_text(content, model)) WITH ORDINALITY AS u(t, ord)
$$;

COMMENT ON FUNCTION theodb.embed_batch(text[], text) IS
  'Generate embeddings for an array of inputs in ONE HTTP round-trip (collapses the per-row embed N+1). '
  'Returns vector[] aligned to the input order. Implemented in Rust (theodb_rs, audit-remediation). '
  'Mirrors ai.generate_batch N-in/N-out: a size mismatch is a typed error, NULL elements are rejected, '
  'an empty array returns an empty vector[] with no HTTP call. Not granted to PUBLIC.';

REVOKE ALL ON FUNCTION theodb.embed_batch(text[], text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._embed_batch_text(text[], text) FROM PUBLIC;
"#,
    name = "theodb_embed_batch_wrapper",
    requires = [_embed_batch_text],
);

// SQL wrappers: the public generative `ai.*` surface (M18 — was plpython3u in sql/50, now Rust). Created
// INTO the existing `ai` schema (owned by the `theodb` umbrella; theodb_rs `requires = theodb` so it exists
// first). Exact public signatures / RETURNS / VOLATILE preserved; every function REVOKEd from PUBLIC (it
// makes server-side outbound HTTP). `ai._chat` stays SQL-callable (ai.generate/summarize/the aggregate
// finalfunc + M19 nl_to_sql depend on it by name).
extension_sql!(
    r#"
CREATE FUNCTION ai._chat(prompt text, system text DEFAULT NULL, model text DEFAULT NULL)
RETURNS text LANGUAGE sql VOLATILE
AS $$ SELECT theodb_rs._ai_chat(prompt, system, model) $$;

CREATE FUNCTION ai."if"(prompt text, model text DEFAULT NULL)
RETURNS boolean LANGUAGE sql VOLATILE
AS $$ SELECT theodb_rs._ai_if(prompt, model) $$;

CREATE FUNCTION ai.analyze_sentiment(content text, model text DEFAULT NULL)
RETURNS text LANGUAGE sql VOLATILE
AS $$ SELECT theodb_rs._ai_sentiment(content, model) $$;

CREATE FUNCTION ai.rank(prompt text, model text DEFAULT NULL)
RETURNS real LANGUAGE sql VOLATILE
AS $$ SELECT theodb_rs._ai_rank(prompt, model) $$;

CREATE FUNCTION ai.generate_batch(prompts text[], model text DEFAULT NULL)
RETURNS text[] LANGUAGE sql VOLATILE
AS $$ SELECT theodb_rs._ai_generate_batch(prompts, model) $$;

COMMENT ON FUNCTION ai._chat(text, text, text) IS
  'PRIVATE: one configurable chat-completions round-trip (theodb.llm_endpoint, http(s)-only, no redirects) '
  '+ parse of choices[0].message.content. Single HTTP source of truth for the public ai.* functions. '
  'Implemented in Rust (theodb_rs, M18). Not granted to PUBLIC. theodb.llm_api_key is a session GUC.';

REVOKE ALL ON FUNCTION ai._chat(text, text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai."if"(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai.analyze_sentiment(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai.rank(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai.generate_batch(text[], text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._ai_chat(text, text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._ai_if(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._ai_sentiment(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._ai_rank(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._ai_generate_batch(text[], text) FROM PUBLIC;
"#,
    name = "theodb_ai_wrappers",
    requires = [_ai_chat, _ai_if, _ai_sentiment, _ai_rank, _ai_generate_batch],
);

// SQL wrappers: the public NL→SQL surface (M19 — `ai.nl_to_sql` was the last plpython3u, now Rust). Created
// INTO the existing `ai` schema; exact public signatures preserved; REVOKEd from PUBLIC (they make an
// outbound LLM call via ai._chat and ai.nl_query executes dynamic SQL). The M12 config layer (sql/61) and
// `ai.nl_query` resolve `ai.nl_to_sql` by name.
extension_sql!(
    r#"
CREATE FUNCTION ai.nl_to_sql(question text, allowed_relations text[], model text DEFAULT NULL)
RETURNS text LANGUAGE sql VOLATILE
AS $$ SELECT theodb_rs._nl_to_sql(question, allowed_relations, model) $$;

COMMENT ON FUNCTION ai.nl_to_sql(text, text[], text) IS
  'Translate a natural-language question into ONE validated read-only SELECT over allowed_relations (via the '
  'configurable model). Defense: L2 static validation (single statement, SELECT/WITH-only, banned-function '
  'denylist) + L4 parser-grade relation allowlist (EXPLAIN enumerates every planned relation). Fail-fast '
  '22023. Does NOT execute. Implemented in Rust (theodb_rs, M19). Not granted to PUBLIC.';

REVOKE ALL ON FUNCTION ai.nl_to_sql(text, text[], text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._nl_to_sql(text, text[], text) FROM PUBLIC;
"#,
    name = "theodb_nl_wrappers",
    requires = [_nl_to_sql],
);

// SQL wrappers: the public hybrid-search surface (M19 — was plpgsql in sql/40, now Rust). Created INTO the
// existing `ai` schema. The public ai.hybrid_search_rrf keeps its exact named parameters + DEFAULTs (callers
// use tbl => …, query_vector => … named notation) and the `regclass`/`vector` types pgrx cannot express
// natively — the thin SQL wrapper bridges them to the text-typed Rust entrypoint (tbl::text, query_vector::text).
// RETURNS TABLE(id text, score real) preserved. Both REVOKEd from PUBLIC (the embed leg makes outbound HTTP).
// NOTE (M19 scope decision): hybrid now co-resides with theodb.embed in theodb_rs, so the former cross-extension
// seam guard scenario (drop theodb_rs, hybrid survives → 0A000) no longer applies — dropping theodb_rs removes
// both (the function itself → 42883). The defensive 0A000 guard remains for an individually-dropped embed.
extension_sql!(
    r#"
CREATE FUNCTION ai.hybrid_search_rrf(
    tbl              regclass,
    id_col           text,
    content_tsv_col  text,
    vector_col       text,
    query_text       text    DEFAULT NULL,
    query_vector     vector  DEFAULT NULL,
    k                int     DEFAULT 60,
    per_leg_limit    int     DEFAULT 20,
    result_limit     int     DEFAULT 5
)
RETURNS TABLE(id text, score real)
LANGUAGE sql STABLE
AS $$
  SELECT id, score FROM theodb_rs._hybrid_search_rrf(
    tbl::text, id_col, content_tsv_col, vector_col, query_text, query_vector::text,
    k, per_leg_limit, result_limit)
$$;

CREATE FUNCTION ai.hybrid_search(config jsonb)
RETURNS TABLE(id text, score real)
LANGUAGE sql STABLE
AS $$ SELECT id, score FROM theodb_rs._hybrid_search_json(config) $$;

COMMENT ON FUNCTION ai.hybrid_search_rrf(regclass, text, text, text, text, vector, int, int, int) IS
  'Hybrid search: fuse a PostgreSQL FTS leg (ts_rank_cd over a tsvector column) and a pgvector leg (<=>) via '
  'Reciprocal Rank Fusion (score = sum 1/(k+rank), k default 60 — Cormack et al. 2009). Empty legs handled by '
  'FULL OUTER JOIN + COALESCE. query_text feeds FTS and, when query_vector is NULL, is embedded via '
  'theodb.embed. Implemented in Rust (theodb_rs, M19) — orchestrates ONE fusion SQL via SPI (one fusion '
  'source of truth). Identifier args are %I-quoted (injection-safe). Not granted to PUBLIC.';

COMMENT ON FUNCTION ai.hybrid_search(jsonb) IS
  'Literal spec-06 JSON surface over ai.hybrid_search_rrf (one fusion definition). Implemented in Rust '
  '(theodb_rs, M19). Fail-fast 22023 on missing required keys (table, id_col, content_tsv_col, vector_col). '
  'Not granted to PUBLIC.';

REVOKE ALL ON FUNCTION ai.hybrid_search_rrf(regclass, text, text, text, text, vector, int, int, int) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai.hybrid_search(jsonb) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._hybrid_search_rrf(text, text, text, text, text, text, int, int, int) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._hybrid_search_json(jsonb) FROM PUBLIC;
"#,
    name = "theodb_hybrid_wrappers",
    requires = [_hybrid_search_rrf, _hybrid_search_json],
);

// SQL wrapper: the public migration helper `theodb.import_pinecone` (M19 — was plpgsql in sql/80, now Rust).
// The chunked PROCEDURE theodb.import_pinecone_chunked STAYS plpgsql (ADR-D — only a plpgsql PROCEDURE can
// COMMIT per batch). Created INTO the existing `theodb` schema; exact signature + DEFAULTs preserved; REVOKEd
// from PUBLIC (writes to caller-owned tables). The thin wrapper bridges `regclass` to the text-typed Rust fn.
extension_sql!(
    r#"
CREATE FUNCTION theodb.import_pinecone(
    target        regclass,
    export        jsonb,
    id_col        text DEFAULT 'id',
    embedding_col text DEFAULT 'embedding',
    metadata_col  text DEFAULT 'metadata'
) RETURNS integer
LANGUAGE sql
AS $$ SELECT theodb_rs._import_pinecone(target::text, export, id_col, embedding_col, metadata_col) $$;

COMMENT ON FUNCTION theodb.import_pinecone(regclass, jsonb, text, text, text) IS
  'Import a Pinecone export (JSON array of {id,values,metadata}) into a TheoDB table (id, embedding vector, '
  'metadata jsonb). Native jsonb (serde); safe dynamic SQL (%I-quoted, regclass-validated, parameter-bound). '
  'Implemented in Rust (theodb_rs, M19). Fail-fast 22023 on a non-array export or a record missing id/values. '
  'For large/atomic-vs-chunked imports see theodb.import_pinecone_chunked (PROCEDURE). Not granted to PUBLIC.';

REVOKE ALL ON FUNCTION theodb.import_pinecone(regclass, jsonb, text, text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._import_pinecone(text, jsonb, text, text, text) FROM PUBLIC;
"#,
    name = "theodb_import_wrapper",
    requires = [_import_pinecone],
);

// Rust-side unit tests for the input-validation guards (no network needed). The cross-language
// parity + HTTP behaviors are proven by the Python oracle (benchmarks/tests/test_embed_sql.py)
// against the rebuilt image.
#[cfg(any(test, feature = "pg_test"))]
#[pg_schema]
mod tests {
    use pgrx::prelude::*;

    #[pg_test(error = "must not be NULL")]
    fn embed_null_content_rejected() {
        Spi::run("SET theodb.embedding_endpoint = 'http://127.0.0.1:1/v1/embeddings'").unwrap();
        let _ = Spi::get_one::<String>("SELECT theodb_rs._embed_text(NULL, NULL)");
    }

    #[pg_test(error = "embedding_endpoint is not set")]
    fn embed_unset_endpoint_rejected() {
        // Ensure the GUC is unset for this test (ignore error if it was never set).
        Spi::run("RESET theodb.embedding_endpoint").ok();
        let _ = Spi::get_one::<String>("SELECT theodb_rs._embed_text('x', NULL)");
    }

    #[pg_test(error = "http(s)")]
    fn embed_non_http_scheme_rejected() {
        Spi::run("SET theodb.embedding_endpoint = 'file:///etc/passwd'").unwrap();
        let _ = Spi::get_one::<String>("SELECT theodb_rs._embed_text('x', NULL)");
    }

    #[pg_test(error = "call failed")]
    fn embed_unreachable_endpoint_fails_typed() {
        // Port 1 is unreachable -> connect error -> "call failed" (38000).
        Spi::run("SET theodb.embedding_endpoint = 'http://127.0.0.1:1/v1/embeddings'").unwrap();
        let _ = Spi::get_one::<String>("SELECT theodb_rs._embed_text('x', NULL)");
    }

    #[pg_test(error = "must not be NULL")]
    fn embed_batch_rejects_null_element() {
        // A NULL element breaks N-in/N-out alignment -> 22023, BEFORE any GUC/HTTP (endpoint set but unused).
        Spi::run("SET theodb.embedding_endpoint = 'http://127.0.0.1:1/v1/embeddings'").unwrap();
        Spi::run("SELECT theodb_rs._embed_batch_text(ARRAY['x', NULL]::text[], NULL)").unwrap();
    }

    #[pg_test]
    fn embed_batch_empty_makes_no_call() {
        // Empty input -> empty result with NO HTTP call (endpoint is unreachable; if it were called this
        // would error). Proves the no-HTTP short-circuit.
        Spi::run("SET theodb.embedding_endpoint = 'http://127.0.0.1:1/v1/embeddings'").unwrap();
        let n = Spi::get_one::<i64>(
            "SELECT cardinality(theodb_rs._embed_batch_text(ARRAY[]::text[], NULL))",
        )
        .unwrap();
        assert_eq!(n, Some(0));
    }
}

#[cfg(test)]
pub mod pg_test {
    pub fn setup(_options: Vec<&str>) {}

    pub fn postgresql_conf_options() -> Vec<&'static str> {
        vec![]
    }
}
