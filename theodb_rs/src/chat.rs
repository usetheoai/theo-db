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
use serde_json::{json, Value};

use crate::http::{post_json, truncate};
use crate::pg::{err_external, err_input, guc};

/// One configurable chat-completions round-trip + parse of `choices[0].message.content`. The single HTTP
/// source of truth for the generative `ai.*` (exposed as the SQL function `ai._chat`). Byte-identical message
/// construction to the plpython3u original: `model` + `messages` (system only when truthy, then user).
pub(crate) fn chat(prompt: Option<&str>, system: Option<&str>, model: Option<&str>) -> String {
    let prompt = match prompt {
        Some(p) => p,
        None => err_input("ai._chat: prompt must not be NULL"),
    };
    let (endpoint, mdl, api_key) = resolve_chat_cfg(model);

    let mut messages: Vec<Value> = Vec::new();
    // `if system:` — a NULL or empty-string system adds no system message (Python truthiness parity).
    if let Some(s) = system.filter(|s| !s.is_empty()) {
        messages.push(json!({ "role": "system", "content": s }));
    }
    messages.push(json!({ "role": "user", "content": prompt }));
    let payload = json!({ "model": mdl, "messages": messages }).to_string();

    let body = post_json("ai._chat", &endpoint, payload, api_key.as_deref());

    let content = body
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|m| m.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or_else(|| {
            err_external(&format!(
                "ai._chat: unexpected chat response shape: {}",
                truncate(&body.to_string(), 200)
            ))
        });
    if content.is_empty() {
        err_external("ai._chat: endpoint returned an empty completion");
    }
    content.to_string()
}

/// `ai.if` — natural-language condition -> boolean. First-token match (not startswith); unparseable -> 22023.
pub(crate) fn ai_if(prompt: Option<&str>, model: Option<&str>) -> bool {
    let out = chat(prompt, Some("Answer with exactly one word: yes or no."), model);
    let first = first_token(&out, |c| c.is_ascii_alphanumeric()); // re.split(r"[^a-z0-9]+", ...)[0]
    if matches!(first.as_str(), "yes" | "true" | "1" | "y" | "t") {
        return true;
    }
    if matches!(first.as_str(), "no" | "false" | "0" | "n" | "f") {
        return false;
    }
    err_input(&format!(
        "ai.if: unparseable boolean from model: {}",
        truncate(&out, 50)
    ));
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
    let mdl: String = model
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
