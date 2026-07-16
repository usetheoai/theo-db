//! Domain layer (blueprint M18 ADR-1 / P3-P4): the chat-completions surface — portable business logic for
//! the generative `ai.*` functions. Talks to `theodb.llm_endpoint` via the shared `crate::http` client and
//! parses the model's text reply into each function's typed result. PG specifics (typed errors, GUCs) are
//! delegated to `crate::pg` (ADR-1 — PG coupling at one boundary).
//!
//! Parity contract (oracle: `benchmarks/tests/test_ai_sql.py` + stub `tools/chat_server.py`):
//!   * the system/user prompts are BYTE-IDENTICAL to the plpython3u originals (the stub routes on them);
//!   * input/parse errors -> SQLSTATE 22023; endpoint/response failures -> SQLSTATE 38000;
//!   * the parsers replicate the plpython3u logic exactly (first-token bool/label, first-number rank,
//!     markdown-fence-strip + JSON-array batch with JSON-null -> SQL NULL).
use std::cell::Cell;

use serde_json::{json, Value};

use crate::http::{post_json, truncate};
use crate::pg::{err_external, err_input, guc};

// M102 — inference round-trip counter (the wiring-triad runtime metric for the AI-operator surface). Every
// `chat()` call is ONE round-trip (real HTTP OR the deterministic test model), so this counts round-trips
// honestly for both the hermetic result-equivalence test and the real-AI benchmark. Thread-local: one backend.
thread_local! {
    static AI_CALLS: Cell<u64> = const { Cell::new(0) };
}

/// Round-trips issued by this backend since the last reset (M102 observability / the "1 call for N rows" proof).
pub(crate) fn ai_call_count() -> u64 {
    AI_CALLS.with(|c| c.get())
}

/// Reset the round-trip counter (test/benchmark harness).
pub(crate) fn ai_call_reset() {
    AI_CALLS.with(|c| c.set(0));
}

/// Parse a model reply into a boolean via the SAME first-token rule as `ai.if` (yes/true/1/y/t vs no/false/0/n/f).
/// `None` when the reply is not a recognizable boolean — the caller decides whether that is an error or a NULL.
pub(crate) fn parse_bool(out: &str) -> Option<bool> {
    let first = first_token(out, |c| c.is_ascii_alphanumeric());
    match first.as_str() {
        "yes" | "true" | "1" | "y" | "t" => Some(true),
        "no" | "false" | "0" | "n" | "f" => Some(false),
        _ => None,
    }
}

/// M102 — the deterministic test model (`SET theodb.llm_test_model = 'parity'`): a hermetic, HTTP-free inference
/// used ONLY by tests/benchmarks so result-equivalence of the batched operator vs the per-row path is provable
/// without a flaky/paid live LLM (ADR D3). Rule: extract the LAST integer of each item; answer "yes" iff even.
/// Format-aware — a batched call (system asks for a JSON array) returns a JSON array; a single call returns one
/// word — so the batched operator and the per-row `ai.if` agree item-for-item. Production leaves the GUC unset.
fn test_model_reply(kind: &str, prompt: &str, system: Option<&str>) -> String {
    if kind != "parity" {
        err_input(&format!(
            "ai._chat: unknown theodb.llm_test_model '{kind}' (only 'parity' is defined)"
        ));
    }
    let batched = system.is_some_and(|s| s.contains("JSON array"));
    if batched {
        // The batched user message is "1. <item>\n2. <item>\n…" (see `ai_generate_batch`).
        let answers: Vec<String> = prompt
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|line| {
                let item = line.splitn(2, ". ").nth(1).unwrap_or(line);
                if last_int_is_even(item) { "yes".to_string() } else { "no".to_string() }
            })
            .collect();
        serde_json::to_string(&answers).expect("Vec<String> serializes")
    } else if last_int_is_even(prompt) {
        "yes".to_string()
    } else {
        "no".to_string()
    }
}

/// The last maximal run of ASCII digits in `s`, parsed as u128, is even. No digit → false ("no").
fn last_int_is_even(s: &str) -> bool {
    let b = s.as_bytes();
    let mut end = b.len();
    // find the end of the last digit run
    while end > 0 && !b[end - 1].is_ascii_digit() {
        end -= 1;
    }
    if end == 0 {
        return false;
    }
    let mut start = end;
    while start > 0 && b[start - 1].is_ascii_digit() {
        start -= 1;
    }
    s[start..end].parse::<u128>().map(|n| n % 2 == 0).unwrap_or(false)
}

/// One configurable chat-completions round-trip + parse of `choices[0].message.content`. The single HTTP
/// source of truth for the generative `ai.*` (exposed as the SQL function `ai._chat`). Byte-identical message
/// construction to the plpython3u original: `model` + `messages` (system only when truthy, then user).
pub(crate) fn chat(prompt: Option<&str>, system: Option<&str>, model: Option<&str>) -> String {
    let prompt = match prompt {
        Some(p) => p,
        None => err_input("ai._chat: prompt must not be NULL"),
    };
    // M102 — one round-trip per chat() (counted for BOTH the real HTTP path and the test model, so the
    // benchmark's round-trip count is honest). The deterministic test model short-circuits before any HTTP.
    AI_CALLS.with(|c| c.set(c.get() + 1));
    if let Some(kind) = guc("theodb.llm_test_model") {
        return test_model_reply(&kind, prompt, system);
    }
    let (endpoint, mdl, api_key) = resolve_chat_cfg(model);

    let mut messages: Vec<Value> = Vec::new();
    // `if system:` — a NULL or empty-string system adds no system message (Python truthiness parity).
    if let Some(s) = system.filter(|s| !s.is_empty()) {
        messages.push(json!({ "role": "system", "content": s }));
    }
    messages.push(json!({ "role": "user", "content": prompt }));
    let payload = json!({ "model": mdl, "messages": messages }).to_string();

    let body = post_json("ai._chat", &endpoint, payload, api_key.as_deref());

    // Parity with the plpython3u original: a missing choices/choices[0]/message/content key (or empty
    // choices) is "unexpected chat response shape"; a present-but-null OR empty-string content is the
    // distinct "empty completion" (the plpython3u `content is None or content == ""` branch, 38000).
    match body
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|m| m.get("message"))
        .and_then(|m| m.get("content"))
    {
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        Some(Value::String(_)) | Some(Value::Null) => {
            err_external("ai._chat: endpoint returned an empty completion")
        }
        _ => err_external(&format!(
            "ai._chat: unexpected chat response shape: {}",
            truncate(&body.to_string(), 200)
        )),
    }
}

/// `ai.if` — natural-language condition -> boolean. First-token match (not startswith); unparseable -> 22023.
pub(crate) fn ai_if(prompt: Option<&str>, model: Option<&str>) -> bool {
    let out = chat(prompt, Some("Answer with exactly one word: yes or no."), model);
    match parse_bool(&out) {
        Some(b) => b,
        None => err_input(&format!(
            "ai.if: unparseable boolean from model: {}",
            truncate(&out, 50)
        )),
    }
}

/// `ai.analyze_sentiment` — content -> one of {positive,negative,neutral}. First-token match; else 22023.
pub(crate) fn ai_sentiment(content: Option<&str>, model: Option<&str>) -> String {
    let out = chat(
        content,
        Some("Classify the sentiment of the text. Reply with exactly one of: positive, negative, neutral."),
        model,
    );
    let first = first_token(&out, |c| c.is_ascii_alphabetic()); // re.split(r"[^a-z]+", ...)[0]
    if matches!(first.as_str(), "positive" | "negative" | "neutral") {
        return first;
    }
    err_input(&format!(
        "ai.analyze_sentiment: model did not return a known label: {}",
        truncate(&out, 50)
    ));
}

/// `ai.rank` — natural-language scoring -> real. Parses the FIRST number anywhere (no clamp); none -> 22023.
pub(crate) fn ai_rank(prompt: Option<&str>, model: Option<&str>) -> f32 {
    let out = chat(
        prompt,
        Some("Reply with a single number between 0 and 1 and nothing else."),
        model,
    );
    match first_number(&out) {
        Some(v) => v as f32,
        None => err_input(&format!(
            "ai.rank: model did not return a number: {}",
            truncate(&out, 50)
        )),
    }
}

/// `ai.generate_batch` — answer N prompts in ONE round-trip. Empty -> [] (no call); NULL element -> 22023;
/// the model must return a JSON array of exactly N (string|null) items (markdown fence tolerated).
pub(crate) fn ai_generate_batch(prompts: &[Option<&str>], model: Option<&str>) -> Vec<Option<String>> {
    let n = prompts.len();
    if n == 0 {
        return Vec::new();
    }
    if prompts.iter().any(|p| p.is_none()) {
        err_input(
            "ai.generate_batch: prompts must not contain NULL elements (breaks the N-in/N-out alignment)",
        );
    }

    let user = prompts
        .iter()
        .enumerate()
        .map(|(i, p)| format!("{}. {}", i + 1, p.unwrap()))
        .collect::<Vec<_>>()
        .join("\n");
    let system = format!(
        "You are given {n} numbered items. Respond with ONLY a JSON array of exactly {n} strings — the \
         answer to each item, in order. No prose, no markdown."
    );

    let out = chat(Some(&user), Some(&system), model);
    parse_batch(&out, n)
}

/// Resolve the chat endpoint (with SSRF http(s) guard), model, and optional api key from the `theodb.llm_*`
/// session GUCs (mirrors `embed::resolve_cfg` for the chat triple).
fn resolve_chat_cfg(model: Option<&str>) -> (String, String, Option<String>) {
    let endpoint = match guc("theodb.llm_endpoint") {
        Some(e) => e,
        None => err_input(
            "ai._chat: theodb.llm_endpoint is not set — \
             SET theodb.llm_endpoint = 'https://host/v1/chat/completions'",
        ),
    };
    if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
        err_input("ai._chat: endpoint must be http(s)://");
    }
    // `model or _cfg("theodb.llm_model") or "default"` — empty string is falsy in Python, so an explicit
    // `''` model argument falls back to the GUC/default (parity; not kept as the literal empty model).
    let mdl: String = model
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| guc("theodb.llm_model"))
        .unwrap_or_else(|| "default".to_string());
    let api_key = guc("theodb.llm_api_key");
    (endpoint, mdl, api_key)
}

/// First token of `s` (trimmed + lowercased) over the allowed char-class — equivalent to
/// `re.split(r"[^<class>]+", s.strip().lower(), 1)[0]`: the leading run of allowed chars (empty if `s`
/// starts with a disallowed char after trim).
fn first_token(s: &str, allowed: fn(char) -> bool) -> String {
    s.trim()
        .chars()
        .map(|c| c.to_ascii_lowercase())
        .take_while(|c| allowed(*c))
        .collect()
}

/// The first number matching `-?\d+(?:\.\d+)?` anywhere in `s` (re.search parity), parsed as f64.
fn first_number(s: &str) -> Option<f64> {
    let b = s.as_bytes();
    let n = b.len();
    let mut i = 0;
    while i < n {
        let c = b[i];
        // A number starts at a digit, or a '-' immediately followed by a digit.
        let neg = c == b'-' && i + 1 < n && b[i + 1].is_ascii_digit();
        if !(neg || c.is_ascii_digit()) {
            i += 1;
            continue;
        }
        let start = i;
        let mut j = if neg { i + 1 } else { i };
        while j < n && b[j].is_ascii_digit() {
            j += 1;
        }
        if j + 1 < n && b[j] == b'.' && b[j + 1].is_ascii_digit() {
            j += 1;
            while j < n && b[j].is_ascii_digit() {
                j += 1;
            }
        }
        return s[start..j].parse::<f64>().ok();
    }
    None
}

/// Strip an optional leading ```` ```lang ```` fence and trailing ```` ``` ```` fence (the plpython3u
/// `re.sub(r"^```[A-Za-z0-9_-]*\s*", "")` + `re.sub(r"\s*```$", "")` on the stripped string).
fn strip_fence(s: &str) -> String {
    let mut t = s.trim();
    if let Some(rest) = t.strip_prefix("```") {
        let rest = rest.trim_start_matches(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        t = rest.trim_start();
    }
    if let Some(stripped) = t.strip_suffix("```") {
        t = stripped.trim_end();
    }
    t.trim().to_string()
}

/// Parse the batch completion into exactly `n` (string|null) items. Invalid JSON / wrong length /
/// non-string element -> 22023 (parity with the plpython3u parser). JSON `null` -> SQL NULL element.
fn parse_batch(out: &str, n: usize) -> Vec<Option<String>> {
    let s = strip_fence(out);
    let arr: Value = match serde_json::from_str(&s) {
        Ok(v) => v,
        Err(_) => err_input(&format!(
            "ai.generate_batch: model did not return valid JSON: {}",
            truncate(out, 80)
        )),
    };
    let items = match arr.as_array() {
        Some(a) if a.len() == n => a,
        _ => err_input(&format!(
            "ai.generate_batch: expected a JSON array of {} items, got {}",
            n,
            truncate(&arr.to_string(), 80)
        )),
    };
    let mut result = Vec::with_capacity(n);
    for x in items {
        match x {
            Value::Null => result.push(None),
            Value::String(t) => result.push(Some(t.clone())),
            other => err_input(&format!(
                "ai.generate_batch: model returned a non-string array element: {}",
                truncate(&other.to_string(), 80)
            )),
        }
    }
    result
}

// M25 — unit tests for the pure parsers (parity with the vec/nl/sbq test discipline; previously untested).
#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use super::*;
    use pgrx::prelude::*;

    #[pg_test]
    fn first_number_extracts_leading_number() {
        assert_eq!(first_number("score: 4.5 out of 5"), Some(4.5));
        assert_eq!(first_number("-3 degrees"), Some(-3.0));
        assert_eq!(first_number("v2.0 release"), Some(2.0));
        assert_eq!(first_number("no digits here"), None);
    }

    #[pg_test]
    fn strip_fence_removes_markdown_fence() {
        assert_eq!(strip_fence("```json\n[1,2]\n```"), "[1,2]");
        assert_eq!(strip_fence("```\nhello\n```"), "hello");
        assert_eq!(strip_fence("plain text"), "plain text");
    }

    #[pg_test]
    fn parse_batch_splits_json_array_with_nulls() {
        let r = parse_batch("[\"a\", null, \"c\"]", 3);
        assert_eq!(r, vec![Some("a".to_string()), None, Some("c".to_string())]);
    }
}
