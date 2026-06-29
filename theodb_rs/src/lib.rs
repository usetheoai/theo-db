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

mod embed;
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
