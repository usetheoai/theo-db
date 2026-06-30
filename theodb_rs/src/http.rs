//! Shared HTTP client (blueprint M18 ADR D2 / P2): the single send + bounded-retry + SSRF + parse core
//! used by BOTH `embed` (embeddings endpoint) and `chat` (chat-completions endpoint). Extracted from
//! `embed.rs` so the retry/SSRF policy lives in one place (DRY). Noun-neutral messages (`"{fn_name}:
//! endpoint call failed: …"`) so each caller reports under its own function name; both oracles assert
//! only the `"call failed"` substring + SQLSTATE, so the noun is free.
//!
//! Posture (must not regress): http(s)-only is enforced by the CALLER (resolve_*); here we never follow a
//! 30x (`with_max_redirects(0)`); connect/timeout/non-2xx/non-JSON -> 38000; the api key lives only in the
//! `Authorization` header, never in an error/warning string (no-leak even after retries are exhausted).
use serde_json::Value;

use crate::pg::{err_external, warn};

/// Max retries for the recoverable class (3 attempts total). Bounded so a down endpoint can never hang
/// beyond `(MAX_RETRIES + 1) × timeout` (`error-handling.md` — retry with backoff, never unbounded).
pub(crate) const MAX_RETRIES: u32 = 2;

/// The recoverable HTTP status class (transient): too-many-requests + bad/unavailable gateway. Other 4xx
/// (400/401/403/404/422) and other 5xx (500/504) are irrecoverable -> fail-fast, NO retry (retrying would
/// mask bugs, Rule 8).
fn is_recoverable_status(status: i32) -> bool {
    matches!(status, 429 | 502 | 503)
}

/// Bounded exponential backoff with jitter (stdlib only — no `rand`/`backoff` crate, parsimony rung 5):
/// attempt 0 -> ~100ms, attempt 1 -> ~400ms, plus 0–49ms jitter from the clock to de-synchronize retries.
fn backoff(attempt: u32) {
    let base_ms = 100u64.saturating_mul(4u64.saturating_pow(attempt));
    let jitter_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::from(d.subsec_nanos()) % 50)
        .unwrap_or(0);
    std::thread::sleep(std::time::Duration::from_millis(base_ms + jitter_ms));
}

/// POST a JSON payload to `endpoint` and return the parsed response body. A bounded recoverable-class
/// retry (connect/timeout + 429/502/503) wraps the send/status; non-recoverable status, exhausted retries,
/// and non-JSON bodies fail fast with SQLSTATE 38000 ("call failed"). `fn_name` prefixes every message.
pub(crate) fn post_json(fn_name: &str, endpoint: &str, payload: String, api_key: Option<&str>) -> Value {
    let mut attempt: u32 = 0;
    let resp = loop {
        let mut req = minreq::post(endpoint)
            .with_header("Content-Type", "application/json")
            .with_body(payload.clone()) // body is consumed per send -> clone for retryability
            .with_timeout(30)
            // SSRF: never follow a 30x to an internal host / cloud metadata (parity with the
            // plpython3u _NoRedirect handler). minreq follows up to 100 redirects by default.
            .with_max_redirects(0);
        if let Some(key) = api_key {
            req = req.with_header("Authorization", format!("Bearer {key}"));
        }

        match req.send() {
            Ok(r) if (200..300).contains(&r.status_code) => break r,
            Ok(r) if is_recoverable_status(r.status_code) && attempt < MAX_RETRIES => {
                warn(&format!(
                    "{fn_name}: endpoint returned HTTP {}; retrying ({}/{})",
                    r.status_code,
                    attempt + 1,
                    MAX_RETRIES
                ));
                backoff(attempt);
                attempt += 1;
                continue;
            }
            // Non-2xx, non-recoverable, OR retries exhausted -> fail-fast with the status (38000).
            Ok(r) => err_external(&format!(
                "{fn_name}: endpoint call failed: HTTP status {}",
                r.status_code
            )),
            // Connect/timeout/redirect errors are recoverable until the cap, then fail-fast (38000).
            Err(e) if attempt < MAX_RETRIES => {
                warn(&format!(
                    "{fn_name}: endpoint connection error; retrying ({}/{}): {e}",
                    attempt + 1,
                    MAX_RETRIES
                ));
                backoff(attempt);
                attempt += 1;
                continue;
            }
            Err(e) => err_external(&format!("{fn_name}: endpoint call failed: {e}")),
        }
    };

    // Parse is NOT retried — a 200 with a non-JSON body is a "call failed" (parity: plpython3u catches
    // JSONDecodeError in the same branch as URLError).
    let body_str = match resp.as_str() {
        Ok(s) => s,
        Err(e) => err_external(&format!("{fn_name}: endpoint call failed: {e}")),
    };
    match serde_json::from_str(body_str) {
        Ok(v) => v,
        Err(e) => err_external(&format!("{fn_name}: endpoint call failed: {e}")),
    }
}

/// Truncate a string to `n` chars (UTF-8-safe) for bounded error messages.
pub(crate) fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}
