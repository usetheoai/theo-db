//! Domain layer (blueprint ADR-1): the embedding call — portable business logic. Talks to the
//! configurable model endpoint over HTTP (minreq, native-tls) and returns a pgvector text literal.
//! All Postgres specifics (typed errors, GUC reads) are delegated to `crate::pg` — this module touches
//! the pgrx ABI only through those helpers, which concentrates the PG coupling at one boundary (ADR-1).
//!
//! Error parity with the plpython3u baseline (oracle: `benchmarks/tests/test_embed_sql.py`):
//!   * input errors (NULL content, unset endpoint, non-http(s) scheme)        -> SQLSTATE 22023
//!   * HTTP / response failures (connect/timeout/non-2xx/bad body/shape/empty) -> SQLSTATE 38000
use serde_json::Value;

use crate::pg::{err_external, err_input, guc};

/// Generate the embedding for `content` via the `theodb.embedding_endpoint` GUC and return it as a
/// pgvector text literal `"[x,y,z]"` (exactly as the plpython3u baseline did); the SQL wrapper
/// `theodb.embed` casts it to `vector`. This is the body invoked by `theodb_rs._embed_text`.
pub(crate) fn run(content: Option<&str>, model: Option<&str>) -> String {
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

/// Truncate a string to `n` chars (UTF-8-safe) for bounded error messages.
fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}
