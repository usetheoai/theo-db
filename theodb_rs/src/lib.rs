//! TheoDB's own PostgreSQL extension in Rust (pgrx) — M17 (ROADMAP-v2 / ADR 0006).
//!
//! Rewrites `theodb.embed` (formerly plpython3u + urllib, `sql/30-theodb-embed.sql`) in Rust with
//! **proven parity**: same signature, same vector output, same typed SQLSTATEs as the baseline.
//! Mirrors AlloyDB's embedding() pattern — the DB calls a configurable model endpoint (it does NOT
//! ship a model). Configuration via the same session GUCs:
//!   SET theodb.embedding_endpoint = 'http://host:8088/v1/embeddings';   -- required
//!   SET theodb.embedding_model    = 'BAAI/bge-small-en-v1.5';           -- optional
//!   SET theodb.embedding_api_key  = '...';                              -- optional bearer token
//!
//! Error parity (the oracle `benchmarks/tests/test_embed_sql.py` is authoritative):
//!   * input errors (NULL content, unset endpoint, non-http(s) scheme)        -> SQLSTATE 22023
//!   * HTTP / response failures (connect/timeout/non-2xx/bad body/shape/empty) -> SQLSTATE 38000
//!
//! Security parity: only http(s):// endpoints; redirects are NOT followed (SSRF: a 30x to an internal
//! host or cloud metadata is refused); the function is REVOKEd from PUBLIC (install SQL below).
use pgrx::prelude::*;

::pgrx::pg_module_magic!();

// theodb_rs owns its OWN schema `theodb_rs` (so it never tries to CREATE the `theodb` schema, which
// is owned by the umbrella `theodb` extension — PG forbids a second extension from CREATE-IF-NOT-EXISTS
// on a schema it does not own). The public `theodb.embed` wrapper is created INTO the existing `theodb`
// schema via extension_sql (creating an object in another extension's schema is allowed; only CREATE
// SCHEMA conflicts). theodb_rs `requires = 'theodb'` so the `theodb` schema exists first.
#[pg_schema]
mod theodb_rs {
    use pgrx::prelude::*;
    use serde_json::Value;

    /// Raise SQLSTATE 22023 (invalid_parameter_value) for input/config errors. Diverges.
    fn err_input(msg: &str) -> ! {
        pgrx::pg_sys::panic::ErrorReport::new(
            PgSqlErrorCode::ERRCODE_INVALID_PARAMETER_VALUE,
            msg.to_string(),
            "theodb.embed",
        )
        .report(PgLogLevel::ERROR);
        unreachable!()
    }

    /// Raise SQLSTATE 38000 (external_routine_exception) for HTTP/response failures. Diverges.
    fn err_external(msg: &str) -> ! {
        pgrx::pg_sys::panic::ErrorReport::new(
            PgSqlErrorCode::ERRCODE_EXTERNAL_ROUTINE_EXCEPTION,
            msg.to_string(),
            "theodb.embed",
        )
        .report(PgLogLevel::ERROR);
        unreachable!()
    }

    /// Rust implementation of the embedding call (installed as `theodb_rs._embed_text`). Returns the
    /// embedding as a pgvector text literal `"[x,y,z]"` (exactly as the plpython3u baseline did); the
    /// SQL wrapper `theodb.embed` casts it to `vector`. Internal: REVOKEd from PUBLIC alongside the wrapper.
    #[pg_extern]
    fn _embed_text(content: Option<&str>, model: Option<&str>) -> String {
        let content = match content {
            Some(c) => c,
            None => err_input("theodb.embed: content must not be NULL"),
        };

        let endpoint = match guc("theodb.embedding_endpoint") {
            Some(e) => e,
            None => err_input(
                "theodb.embed: theodb.embedding_endpoint is not set — \
                 SET theodb.embedding_endpoint = 'http://host:port/v1/embeddings'",
            ),
        };

        // SSRF hardening: only http(s); refuse file://, ftp://, gopher://, etc. (the GUC is session-settable).
        if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
            err_input("theodb.embed: endpoint must be http(s)://");
        }

        let mdl: String = model
            .map(|s| s.to_string())
            .or_else(|| guc("theodb.embedding_model"))
            .unwrap_or_else(|| "default".to_string());
        let api_key = guc("theodb.embedding_api_key");

        let payload = serde_json::json!({ "input": content, "model": mdl }).to_string();

        let mut req = minreq::post(endpoint.as_str())
            .with_header("Content-Type", "application/json")
            .with_body(payload)
            .with_timeout(30)
            // SSRF: never follow a 30x to an internal host / cloud metadata (parity with the
            // plpython3u _NoRedirect handler). minreq follows up to 100 redirects by default.
            .with_max_redirects(0);
        if let Some(key) = api_key {
            req = req.with_header("Authorization", format!("Bearer {key}"));
        }

        // URL/connect/timeout/redirect errors -> "call failed" (38000), like the baseline.
        let resp = match req.send() {
            Ok(r) => r,
            Err(e) => err_external(&format!("theodb.embed: embedding endpoint call failed: {e}")),
        };
        if !(200..300).contains(&resp.status_code) {
            err_external(&format!(
                "theodb.embed: embedding endpoint call failed: HTTP status {}",
                resp.status_code
            ));
        }

        // A 200 with a non-JSON body is a "call failed" (parity: plpython3u catches JSONDecodeError
        // in the same branch as URLError).
        let body_str = match resp.as_str() {
            Ok(s) => s,
            Err(e) => err_external(&format!("theodb.embed: embedding endpoint call failed: {e}")),
        };
        let body: Value = match serde_json::from_str(body_str) {
            Ok(v) => v,
            Err(e) => err_external(&format!("theodb.embed: embedding endpoint call failed: {e}")),
        };

        // Valid JSON but the expected data[0].embedding array is missing -> "unexpected ... shape".
        let emb = match body
            .get("data")
            .and_then(|d| d.get(0))
            .and_then(|e| e.get("embedding"))
            .and_then(|e| e.as_array())
        {
            Some(a) => a,
            None => err_external(&format!(
                "theodb.embed: unexpected embedding response shape: {}",
                truncate(body_str, 200)
            )),
        };

        if emb.is_empty() {
            err_external("theodb.embed: endpoint returned an empty embedding");
        }

        // Format as a pgvector text literal "[x,y,z]" (the SQL wrapper casts ::vector).
        let mut out = String::with_capacity(emb.len() * 8 + 2);
        out.push('[');
        for (i, v) in emb.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            // Non-numeric entries are a malformed response (38000), not a silent 0.
            match v.as_f64() {
                Some(f) => out.push_str(&f.to_string()),
                None => err_external(
                    "theodb.embed: unexpected embedding response shape: non-numeric vector element",
                ),
            }
        }
        out.push(']');
        out
    }

    /// Read a session GUC by its (trusted, literal) name. Mirrors the plpython3u
    /// `current_setting(name, true)` call — returns None when unset/empty.
    fn guc(name: &str) -> Option<String> {
        // `name` is a hardcoded literal (no user input), exactly as the plpython3u version formatted it.
        Spi::get_one::<String>(&format!("SELECT current_setting('{name}', true)"))
            .ok()
            .flatten()
            .filter(|s| !s.is_empty())
    }

    fn truncate(s: &str, n: usize) -> String {
        s.chars().take(n).collect()
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
  'not granted to PUBLIC. CALL IS SYNCHRONOUS: one blocking HTTP round-trip per row.';

REVOKE ALL ON FUNCTION theodb.embed(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._embed_text(text, text) FROM PUBLIC;
"#,
    name = "theodb_embed_wrapper",
    // Reference the pgrx function as a bare-ident PositioningRef::FullPath (matches the extern by
    // `unaliased_name`, like pgvectorscale's `requires = [smallint_array_overlap]`) so the wrapper's
    // SQL-language body, which validates `theodb_rs._embed_text` at CREATE time, is emitted AFTER it.
    requires = [_embed_text],
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
}

#[cfg(test)]
pub mod pg_test {
    pub fn setup(_options: Vec<&str>) {}

    pub fn postgresql_conf_options() -> Vec<&'static str> {
        vec![]
    }
}
