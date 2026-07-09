//! Domain layer (M65): the cross-encoder rerank call — portable business logic. Talks to the
//! configurable reranker endpoint over HTTP (reusing `crate::http`, the same client `embed`/`chat` use)
//! and returns N relevance scores aligned to the input `documents` order. All Postgres specifics (typed
//! errors, GUC reads) are delegated to `crate::pg`, concentrating the PG coupling at one boundary.
//!
//! Mirrors `embed::run_batch` (blueprint ADR-0024-b — reuse the proven pattern, do NOT reinvent the HTTP
//! client / config path, Rule 9). The wire contract is the cross-encoder rerank shape converged on by
//! Cohere / Jina / Voyage / BGE-TEI / vLLM:
//!   request : `{ "query": string, "documents": string[], "model": string }`
//!   response: `{ "results": [ { "index": int, "relevance_score": float } , ... ] }`  (order not assumed)
//!
//! Error parity with the rest of the `ai.*` surface:
//!   * input errors (NULL query/doc, unset endpoint, non-http(s) scheme)          -> SQLSTATE 22023
//!   * HTTP / response failures (connect/timeout/non-2xx/bad body/shape/N-mismatch) -> SQLSTATE 38000
use serde_json::Value;

use crate::http::{post_json, truncate};
use crate::pg::{err_external, err_input, guc};

/// Rerank `documents` against `query` via the `theodb.rerank_endpoint` GUC and return N relevance
/// scores aligned to the input order (score[i] = relevance of documents[i]). This is the body invoked
/// by `theodb_rs._ai_rerank`; the SQL wrapper `ai.rerank` turns it into `TABLE(idx, score)`.
///
/// N-in/N-out is enforced by count AND by `results[].index`: a size mismatch, an out-of-range index,
/// or a duplicate index is a typed 38000, never a silent misalignment (mirror `embed::run_batch`).
pub(crate) fn run(query: Option<&str>, documents: &[Option<&str>], model: Option<&str>) -> Vec<f32> {
    let query = match query {
        Some(q) => q,
        None => err_input("ai.rerank: query must not be NULL"),
    };

    // Empty-but-valid documents: no HTTP call, empty result (the SQL wrapper yields zero rows).
    if documents.is_empty() {
        return Vec::new();
    }

    // NULL element breaks the N-in/N-out alignment (mirror embed_batch) -> fail-fast 22023,
    // BEFORE any GUC read or HTTP call.
    let mut docs: Vec<&str> = Vec::with_capacity(documents.len());
    for d in documents {
        match d {
            Some(s) => docs.push(s),
            None => err_input("ai.rerank: document array elements must not be NULL"),
        }
    }

    let (endpoint, mdl, api_key) = resolve_cfg("ai.rerank", model);
    let payload = serde_json::json!({ "query": query, "documents": docs, "model": mdl }).to_string();
    let body = post_json("ai.rerank", &endpoint, payload, api_key.as_deref());

    parse_rerank_results(&body, documents.len())
}

/// Parse the reranker response `{ "results": [ { index, relevance_score } ] }` into N scores aligned to
/// the input order. Pure (no HTTP / no pgrx) so it is unit-testable in isolation (mirror `chat::parse_batch`).
/// N-in/N-out invariant: exactly `n` results, every `index` in `[0, n)`, no duplicate — else 38000.
fn parse_rerank_results(body: &Value, n: usize) -> Vec<f32> {
    let results = match body.get("results").and_then(|r| r.as_array()) {
        Some(a) => a,
        None => err_external(&format!(
            "ai.rerank: unexpected rerank response shape: {}",
            truncate(&body.to_string(), 200)
        )),
    };
    if results.len() != n {
        err_external(&format!(
            "ai.rerank: rerank size mismatch: requested {} documents, endpoint returned {} results",
            n,
            results.len()
        ));
    }

    // Map each score back by its `index` (NOT array position) so an out-of-order response is still placed
    // correctly. Missing/out-of-range/duplicate index or non-numeric score is a malformed response (38000).
    let mut out: Vec<Option<f32>> = vec![None; n];
    for (pos, item) in results.iter().enumerate() {
        let idx = item
            .get("index")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(pos);
        if idx >= n {
            err_external("ai.rerank: unexpected rerank response shape: index out of range");
        }
        if out[idx].is_some() {
            err_external("ai.rerank: unexpected rerank response shape: duplicate index");
        }
        let score = match item.get("relevance_score").and_then(|s| s.as_f64()) {
            Some(f) => f as f32,
            None => err_external(
                "ai.rerank: unexpected rerank response shape: missing or non-numeric relevance_score",
            ),
        };
        out[idx] = Some(score);
    }

    // Bijection guaranteed (n results, n slots, in-range, no duplicate) — every slot filled.
    out.into_iter()
        .map(|o| o.unwrap_or_else(|| err_external("ai.rerank: missing score for an index")))
        .collect()
}

/// Resolve the endpoint (with SSRF http(s) guard), model, and optional api key from session GUCs.
/// Mirrors `embed::resolve_cfg` but reads the `theodb.rerank_*` GUC family (free session GUCs, no
/// GucRegistry — ADR-0024-b).
fn resolve_cfg(fn_name: &str, model: Option<&str>) -> (String, String, Option<String>) {
    let endpoint = match guc("theodb.rerank_endpoint") {
        Some(e) => e,
        None => err_input(&format!(
            "{fn_name}: theodb.rerank_endpoint is not set — \
             SET theodb.rerank_endpoint = 'http://host:port/rerank'"
        )),
    };

    // SSRF hardening: only http(s); refuse file://, ftp://, gopher://, etc. (the GUC is session-settable).
    if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
        err_input(&format!("{fn_name}: endpoint must be http(s)://"));
    }

    let mdl: String = model
        .map(|s| s.to_string())
        .or_else(|| guc("theodb.rerank_model"))
        .unwrap_or_else(|| "default".to_string());
    let api_key = guc("theodb.rerank_api_key");

    (endpoint, mdl, api_key)
}

// M65 — unit tests for the pure rerank-response parser (parity with the embed/chat parser test discipline).
#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use super::*;
    use pgrx::prelude::*;
    use serde_json::json;

    #[pg_test]
    fn parse_aligns_scores_by_index() {
        // Out-of-order results must still map to the input order via `index`.
        let body = json!({"results": [
            {"index": 1, "relevance_score": 0.2},
            {"index": 0, "relevance_score": 0.9},
        ]});
        assert_eq!(parse_rerank_results(&body, 2), vec![0.9_f32, 0.2_f32]);
    }

    #[pg_test(error = "ai.rerank: rerank size mismatch: requested 3 documents, endpoint returned 2 results")]
    fn parse_size_mismatch_fails_typed() {
        let body = json!({"results": [
            {"index": 0, "relevance_score": 0.1},
            {"index": 1, "relevance_score": 0.2},
        ]});
        let _ = parse_rerank_results(&body, 3);
    }

    #[pg_test(error = "ai.rerank: unexpected rerank response shape: index out of range")]
    fn parse_index_out_of_range_fails_typed() {
        let body = json!({"results": [{"index": 5, "relevance_score": 0.1}]});
        let _ = parse_rerank_results(&body, 1);
    }

    #[pg_test(error = "ai.rerank: unexpected rerank response shape: duplicate index")]
    fn parse_duplicate_index_fails_typed() {
        let body = json!({"results": [
            {"index": 0, "relevance_score": 0.1},
            {"index": 0, "relevance_score": 0.2},
        ]});
        let _ = parse_rerank_results(&body, 2);
    }

    #[pg_test(error = "ai.rerank: unexpected rerank response shape: missing or non-numeric relevance_score")]
    fn parse_non_numeric_score_fails_typed() {
        let body = json!({"results": [{"index": 0, "relevance_score": "high"}]});
        let _ = parse_rerank_results(&body, 1);
    }

    #[pg_test(error = "ai.rerank: unexpected rerank response shape: {\"data\":[]}")]
    fn parse_missing_results_key_fails_typed() {
        let body = json!({"data": []});
        let _ = parse_rerank_results(&body, 1);
    }
}
