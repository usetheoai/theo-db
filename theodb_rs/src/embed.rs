//! Domain layer (blueprint ADR-1): the embedding call — portable business logic. Talks to the
//! configurable model endpoint over HTTP (minreq, native-tls) and returns a pgvector text literal.
//! All Postgres specifics (typed errors, GUC reads) are delegated to `crate::pg` — this module touches
//! the pgrx ABI only through those helpers, which concentrates the PG coupling at one boundary (ADR-1).
//!
//! Two entry points share one HTTP/parse path (`post_json` + `resolve_cfg` + `format_embedding`, DRY):
//!   * `run`       — one input → one vector (per-row `theodb.embed`).
//!   * `run_batch` — N inputs → N vectors in ONE round-trip (`theodb.embed_batch`); collapses the
//!                   embed N+1 the system-design audit flagged CRITICAL. The endpoint natively accepts
//!                   `input: string[]` (OpenAI shape); embeddings are mapped back by `data[].index`.
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

    let (endpoint, mdl, api_key) = resolve_cfg("theodb.embed", model);
    let payload = serde_json::json!({ "input": content, "model": mdl }).to_string();
    let body = post_json("theodb.embed", &endpoint, payload, api_key.as_deref());

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
            truncate(&body.to_string(), 200)
        )),
    };

    format_embedding(emb, "theodb.embed")
}

/// Generate embeddings for a whole array in ONE HTTP round-trip (the N→1 fix for the audit's
/// CRITICAL embed N+1). Returns N pgvector text literals aligned to the input order; the SQL wrapper
/// `theodb.embed_batch` casts each `::vector`. N-in/N-out is enforced (count + index): a size mismatch
/// or a malformed `data[]` is a typed 38000, never a silent misalignment.
pub(crate) fn run_batch(items: &[Option<&str>], model: Option<&str>) -> Vec<String> {
    // Empty-but-valid input: no HTTP call, empty result (the SQL wrapper COALESCEs to ARRAY[]::vector[]).
    if items.is_empty() {
        return Vec::new();
    }

    // NULL element breaks the N-in/N-out alignment (mirror ai.generate_batch) -> fail-fast 22023,
    // BEFORE any GUC read or HTTP call.
    let mut inputs: Vec<&str> = Vec::with_capacity(items.len());
    for it in items {
        match it {
            Some(s) => inputs.push(s),
            None => err_input("theodb.embed_batch: array elements must not be NULL"),
        }
    }

    let (endpoint, mdl, api_key) = resolve_cfg("theodb.embed_batch", model);
    let payload = serde_json::json!({ "input": inputs, "model": mdl }).to_string();
    let body = post_json("theodb.embed_batch", &endpoint, payload, api_key.as_deref());

    let data = match body.get("data").and_then(|d| d.as_array()) {
        Some(a) => a,
        None => err_external(&format!(
            "theodb.embed_batch: unexpected embedding response shape: {}",
            truncate(&body.to_string(), 200)
        )),
    };
    // N-in/N-out count invariant.
    if data.len() != items.len() {
        err_external(&format!(
            "theodb.embed_batch: batch size mismatch: requested {} embeddings, endpoint returned {}",
            items.len(),
            data.len()
        ));
    }

    // Map each embedding back by its `index` (OpenAI guarantees it), NOT array position — so an
    // out-of-order response is still placed correctly. A missing/out-of-range/duplicate index is a
    // malformed response (38000), not a silent misalignment.
    let mut out: Vec<Option<String>> = vec![None; items.len()];
    for (pos, item) in data.iter().enumerate() {
        let idx = item
            .get("index")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(pos);
        if idx >= items.len() {
            err_external(
                "theodb.embed_batch: unexpected embedding response shape: index out of range",
            );
        }
        if out[idx].is_some() {
            err_external(
                "theodb.embed_batch: unexpected embedding response shape: duplicate index",
            );
        }
        let emb = match item.get("embedding").and_then(|e| e.as_array()) {
            Some(a) => a,
            None => err_external(
                "theodb.embed_batch: unexpected embedding response shape: missing embedding array",
            ),
        };
        out[idx] = Some(format_embedding(emb, "theodb.embed_batch"));
    }

    // Bijection guaranteed (N data items, N slots, in-range, no duplicate) — every slot is filled.
    out.into_iter()
        .map(|o| o.unwrap_or_else(|| err_external("theodb.embed_batch: missing embedding for an index")))
        .collect()
}

/// Resolve the endpoint (with SSRF http(s) guard), model, and optional api key from session GUCs.
/// `fn_name` prefixes the typed-error messages so `embed` and `embed_batch` report under their own name
/// (parity with the per-row messages the oracle pins).
fn resolve_cfg(fn_name: &str, model: Option<&str>) -> (String, String, Option<String>) {
    let endpoint = match guc("theodb.embedding_endpoint") {
        Some(e) => e,
        None => err_input(&format!(
            "{fn_name}: theodb.embedding_endpoint is not set — \
             SET theodb.embedding_endpoint = 'http://host:port/v1/embeddings'"
        )),
    };

    // SSRF hardening: only http(s); refuse file://, ftp://, gopher://, etc. (the GUC is session-settable).
    if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
        err_input(&format!("{fn_name}: endpoint must be http(s)://"));
    }

    let mdl: String = model
        .map(|s| s.to_string())
        .or_else(|| guc("theodb.embedding_model"))
        .unwrap_or_else(|| "default".to_string());
    let api_key = guc("theodb.embedding_api_key");

    (endpoint, mdl, api_key)
}

/// The single HTTP source of truth shared by `run` + `run_batch` (DRY) — POST a JSON payload to the
/// embeddings endpoint and return the parsed response body. SSRF posture (http(s)-only resolved by the
/// caller; no redirects) and typed-error parity (connect/timeout/non-2xx/non-JSON -> 38000 "call failed")
/// are preserved here. Phase 3 (recoverable-class retry) wraps the send/status here so BOTH entry points
/// inherit it in one place.
fn post_json(fn_name: &str, endpoint: &str, payload: String, api_key: Option<&str>) -> Value {
    let mut req = minreq::post(endpoint)
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
        Err(e) => err_external(&format!("{fn_name}: embedding endpoint call failed: {e}")),
    };
    if !(200..300).contains(&resp.status_code) {
        err_external(&format!(
            "{fn_name}: embedding endpoint call failed: HTTP status {}",
            resp.status_code
        ));
    }

    // A 200 with a non-JSON body is a "call failed" (parity: plpython3u catches JSONDecodeError
    // in the same branch as URLError).
    let body_str = match resp.as_str() {
        Ok(s) => s,
        Err(e) => err_external(&format!("{fn_name}: embedding endpoint call failed: {e}")),
    };
    match serde_json::from_str(body_str) {
        Ok(v) => v,
        Err(e) => err_external(&format!("{fn_name}: embedding endpoint call failed: {e}")),
    }
}

/// Format a JSON embedding array as a pgvector text literal `"[x,y,z]"` (the SQL wrapper casts `::vector`).
/// An empty array or a non-numeric element is a malformed response (38000), not a silent zero/empty.
fn format_embedding(emb: &[Value], fn_name: &str) -> String {
    if emb.is_empty() {
        err_external(&format!("{fn_name}: endpoint returned an empty embedding"));
    }
    let mut out = String::with_capacity(emb.len() * 8 + 2);
    out.push('[');
    for (i, v) in emb.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        // Non-numeric entries are a malformed response (38000), not a silent 0.
        match v.as_f64() {
            Some(f) => out.push_str(&f.to_string()),
            None => err_external(&format!(
                "{fn_name}: unexpected embedding response shape: non-numeric vector element"
            )),
        }
    }
    out.push(']');
    out
}

/// Truncate a string to `n` chars (UTF-8-safe) for bounded error messages.
fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}
