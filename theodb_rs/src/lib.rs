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

    /// `theodb_rs._nl_query` — validate (via nl_to_sql) then execute in the read-only sandbox (L3); returns
    /// jsonb rows. A write reaching execution raises 25006 (the SQL `ai.nl_query`).
    #[pg_extern]
    fn _nl_query(
        question: Option<&str>,
        allowed_relations: Vec<Option<String>>,
        model: Option<&str>,
        max_rows: default!(i32, 100),
    ) -> pgrx::JsonB {
        let refs: Vec<Option<&str>> = allowed_relations.iter().map(|o| o.as_deref()).collect();
        crate::nl::nl_query(question, &refs, model, max_rows)
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

CREATE FUNCTION ai.nl_query(question text, allowed_relations text[], model text DEFAULT NULL, max_rows int DEFAULT 100)
RETURNS jsonb LANGUAGE sql VOLATILE
AS $$ SELECT theodb_rs._nl_query(question, allowed_relations, model, max_rows) $$;

COMMENT ON FUNCTION ai.nl_to_sql(text, text[], text) IS
  'Translate a natural-language question into ONE validated read-only SELECT over allowed_relations (via the '
  'configurable model). Defense: L2 static validation (single statement, SELECT/WITH-only, banned-function '
  'denylist) + L4 parser-grade relation allowlist (EXPLAIN enumerates every planned relation). Fail-fast '
  '22023. Does NOT execute. Implemented in Rust (theodb_rs, M19). Not granted to PUBLIC.';
COMMENT ON FUNCTION ai.nl_query(text, text[], text, int) IS
  'Generate+validate (ai.nl_to_sql) then execute the SELECT in a PostgreSQL-native read-only sandbox '
  '(transaction_read_only + statement_timeout -> 25006 on any write). Returns jsonb rows. Rust (theodb_rs, M19). '
  'Not granted to PUBLIC.';

REVOKE ALL ON FUNCTION ai.nl_to_sql(text, text[], text) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai.nl_query(text, text[], text, int) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._nl_to_sql(text, text[], text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._nl_query(text, text[], text, int) FROM PUBLIC;
"#,
    name = "theodb_nl_wrappers",
    requires = [_nl_to_sql, _nl_query],
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
