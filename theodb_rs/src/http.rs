//! Shared HTTP client (blueprint M18 ADR D2 / P2): the single send + bounded-retry + SSRF + parse core
//! used by BOTH `embed` (embeddings endpoint) and `chat` (chat-completions endpoint). Extracted from
//! `embed.rs` so the retry/SSRF policy lives in one place (DRY). Noun-neutral messages (`"{fn_name}:
//! endpoint call failed: …"`) so each caller reports under its own function name; both oracles assert
//! only the `"call failed"` substring + SQLSTATE, so the noun is free.
//!
//! Posture (must not regress): http(s)-only is enforced by the CALLER (resolve_*); here we never follow a
//! 30x (`with_max_redirects(0)`); connect/timeout/non-2xx/non-JSON -> 38000; the api key lives only in the
//! `Authorization` header, never in an error/warning string (no-leak even after retries are exhausted).
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::pg::{err_external, guc, warn};

/// Max retries for the recoverable class (3 attempts total). Bounded so a down endpoint can never hang
/// beyond `(MAX_RETRIES + 1) × timeout` (`error-handling.md` — retry with backoff, never unbounded).
pub(crate) const MAX_RETRIES: u32 = 2;

// M104 (Q3) — a PER-BACKEND circuit breaker (Nygard / MS "Circuit Breaker" / resilience4j closed→open→half-open).
// Postgres is process-per-backend, so the breaker is `thread_local` (no shared memory, no cross-backend coordination
// — a deliberate, documented non-goal: the per-row `ai.*` surface that the audit flagged runs in ONE backend, so a
// per-backend breaker fully removes the "every row waits out the full retry budget against a dead endpoint" cost).
// After K consecutive failures the endpoint's breaker OPENS: further calls fail FAST (38000, no TCP attempt) for
// `open_ms`, then a single HALF-OPEN probe decides re-close vs re-open. Wraps the send loop; does not touch the
// SSRF/redirect=0/api-key-in-header/38000 posture.
const BREAKER_K: u32 = 5; // consecutive failures to trip (audit suggestion; simpler than a rate window, adequate per-backend)
const BREAKER_OPEN_MS_DEFAULT: u64 = 30_000; // one HTTP timeout; GUC `theodb.http_breaker_open_ms`

#[derive(Clone, Copy)]
enum BreakerState {
    Closed,
    Open { until: Instant },
    HalfOpen,
}

thread_local! {
    static BREAKERS: RefCell<HashMap<String, (BreakerState, u32)>> = RefCell::new(HashMap::new());
}

fn breaker_open_ms() -> u64 {
    guc("theodb.http_breaker_open_ms").and_then(|s| s.parse().ok()).unwrap_or(BREAKER_OPEN_MS_DEFAULT)
}

/// Gate a call: `Ok` = proceed (Closed, or Half-Open probe); `Err` = the breaker is Open → fail fast, no TCP.
fn breaker_allow(endpoint: &str) -> Result<(), ()> {
    BREAKERS.with(|b| {
        let mut m = b.borrow_mut();
        let (state, _) = m.entry(endpoint.to_string()).or_insert((BreakerState::Closed, 0));
        match *state {
            BreakerState::Closed | BreakerState::HalfOpen => Ok(()),
            BreakerState::Open { until } => {
                if Instant::now() >= until {
                    *state = BreakerState::HalfOpen; // allow ONE probe
                    Ok(())
                } else {
                    Err(())
                }
            }
        }
    })
}

/// Record a call outcome: success → Close + reset; failure → count, trip to Open at `K` consecutive (or immediately
/// re-open a failed Half-Open probe).
fn breaker_record(endpoint: &str, success: bool) {
    BREAKERS.with(|b| {
        let mut m = b.borrow_mut();
        let entry = m.entry(endpoint.to_string()).or_insert((BreakerState::Closed, 0));
        if success {
            *entry = (BreakerState::Closed, 0);
        } else {
            let fails = entry.1 + 1;
            let open = matches!(entry.0, BreakerState::HalfOpen) || fails >= BREAKER_K;
            if open {
                *entry = (BreakerState::Open { until: Instant::now() + Duration::from_millis(breaker_open_ms()) }, fails);
            } else {
                *entry = (BreakerState::Closed, fails);
            }
        }
    });
}
/// Per-request timeout (seconds). Total wall-clock is bounded by `(MAX_RETRIES + 1) × HTTP_TIMEOUT_SECS`.
const HTTP_TIMEOUT_SECS: u64 = 30;

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
    // M104 (Q3): if this backend's breaker for `endpoint` is OPEN, fail FAST (no TCP, no retry budget) so a per-row
    // surface over a dead endpoint costs ~K probes, not N × (MAX_RETRIES+1) × timeout.
    if breaker_allow(endpoint).is_err() {
        err_external(&format!(
            "{fn_name}: endpoint call failed: circuit open (endpoint failing; retry after {}s)",
            breaker_open_ms() / 1000
        ));
    }
    let mut attempt: u32 = 0;
    let resp = loop {
        let mut req = minreq::post(endpoint)
            .with_header("Content-Type", "application/json")
            .with_body(payload.clone()) // body is consumed per send -> clone for retryability
            .with_timeout(HTTP_TIMEOUT_SECS)
            // SSRF: never follow a 30x to an internal host / cloud metadata (parity with the
            // plpython3u _NoRedirect handler). minreq follows up to 100 redirects by default.
            .with_max_redirects(0);
        if let Some(key) = api_key {
            req = req.with_header("Authorization", format!("Bearer {key}"));
        }

        match req.send() {
            Ok(r) if (200..300).contains(&r.status_code) => {
                breaker_record(endpoint, true); // endpoint reachable + healthy → close/reset the breaker
                break r;
            }
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
            Ok(r) => {
                // A 5xx / 429 is the ENDPOINT failing → count it (may trip the breaker); a 4xx is OUR request
                // being rejected (endpoint reachable) → do NOT trip the breaker on our own bad prompts.
                let endpoint_failing = r.status_code >= 500 || r.status_code == 429;
                breaker_record(endpoint, !endpoint_failing);
                err_external(&format!(
                    "{fn_name}: endpoint call failed: HTTP status {}",
                    r.status_code
                ))
            }
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
            Err(e) => {
                breaker_record(endpoint, false); // connection/timeout exhausted → the endpoint is down
                err_external(&format!("{fn_name}: endpoint call failed: {e}"))
            }
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

/// Test-only: is this backend's breaker OPEN for `endpoint`? (observes the state machine deterministically).
#[cfg(any(test, feature = "pg_test"))]
pub(crate) fn breaker_is_open(endpoint: &str) -> bool {
    BREAKERS.with(|b| matches!(b.borrow().get(endpoint).map(|e| e.0), Some(BreakerState::Open { .. })))
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use super::*;
    use pgrx::prelude::*;

    // M104 (Q3) — the per-backend breaker OPENS after K consecutive failures and then fails FAST (no TCP, no retry
    // budget). A per-row surface over a dead endpoint pays ~K probes instead of N × (retries × timeout).
    #[pg_test]
    fn m104_breaker_opens_after_k_failures_then_fails_fast() {
        let ep = "http://127.0.0.1:1/dead"; // port 1: connection refused (fast) — a stand-in for a down endpoint
        // K failing calls trip the breaker (each caught — post_json diverges via ereport on failure).
        for _ in 0..BREAKER_K {
            PgTryBuilder::new(|| post_json("breaker_test", ep, "{}".to_string(), None))
            .catch_others(|_| Value::Null)
            .execute();
        }
        assert!(breaker_is_open(ep), "breaker OPEN after {BREAKER_K} consecutive failures");
        // when OPEN, the call fails FAST (<100ms — no TCP attempt, no ~500ms retry+backoff of a real refused call).
        let t = std::time::Instant::now();
        PgTryBuilder::new(|| post_json("breaker_test", ep, "{}".to_string(), None))
        .catch_others(|_| Value::Null)
        .execute();
        let ms = t.elapsed().as_millis();
        assert!(ms < 100, "open breaker fails FAST ({ms}ms < 100), short-circuiting the send/retry budget");
    }

    // M104 — a 4xx (our bad request) does NOT trip the breaker (endpoint reachable); only 5xx/timeout do. We can't
    // hit a live 4xx hermetically, so assert the state-machine directly: a HalfOpen success closes it.
    #[pg_test]
    fn m104_breaker_success_closes() {
        let ep = "http://127.0.0.1:1/close";
        for _ in 0..BREAKER_K {
            PgTryBuilder::new(|| post_json("bt", ep, "{}".to_string(), None)).catch_others(|_| Value::Null).execute();
        }
        assert!(breaker_is_open(ep), "open after K failures");
        breaker_record(ep, true); // a success (e.g. after open_ms half-open probe) closes + resets
        assert!(!breaker_is_open(ep), "a success closes the breaker (reset to Closed)");
    }
}
