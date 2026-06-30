//! Domain layer (blueprint M19): safe natural-language → SQL with layered anti-prompt-injection guards,
//! ported from the plpython3u `ai.nl_to_sql`/`ai.nl_query` (sql/60) — the LAST plpython3u in the surface.
//!
//! Defense (does NOT trust the LLM):
//!   * L1 — prompt constraint (hardening; the system prompt is byte-identical so the stub routes on it).
//!   * L2 — static validation on a comment-stripped/lowercased copy: single statement, SELECT/WITH-only,
//!          a fixed banned-keyword denylist, no `DO $$`/`CALL`. Stdlib scanning (no regex crate, ADR-B);
//!          tokenizing on `[a-z0-9_]+` runs is byte-equivalent to the plpython3u `\b…\b` denylist.
//!   * L4 — PARSER-GRADE relation allowlist via `EXPLAIN (FORMAT JSON)` (Postgres's planner enumerates every
//!          relation — comma-joins/quoted-idents/CTEs included). A Rust SQL parser would diverge from the
//!          planner and reopen that vulnerability class (ADR-A), so L4 delegates to Postgres via SPI.
//!   * L3 — read-only sandbox execution in `nl_query` (transaction_read_only + statement_timeout → 25006).
//! Generate-vs-execute split: `nl_to_sql` returns the validated SQL (does NOT execute); `nl_query` runs it.
//! Every rejection is SQLSTATE 22023 (verbatim messages); a write reaching execution is 25006.
use pgrx::prelude::*;
use serde_json::Value;

use crate::pg::{err_input, guc};

/// The banned-token denylist (verbatim from sql/60:74-78). A generated SQL token equal to any of these is
/// rejected (file/exfil/DDL/DML family). `pg_ls_` is the bare-prefix sibling the first cut missed.
const BANNED: &[&str] = &[
    "drop", "insert", "update", "delete", "alter", "truncate", "grant", "revoke", "create", "copy",
    "merge", "reindex", "vacuum", "pg_read_file", "pg_read_binary_file", "pg_stat_file", "pg_ls_dir",
    "pg_ls_waldir", "pg_ls_logdir", "pg_ls_tmpdir", "pg_ls_archive_statusdir", "pg_ls_", "lo_import",
    "lo_export", "lo_get", "lo_put", "dblink", "pg_sleep", "set_config", "current_setting",
    "pg_terminate_backend", "pg_cancel_backend", "pg_read_server_files",
];

/// Generate (via the configurable model) + statically validate the question into ONE read-only SELECT over
/// `allowed`. Returns the validated SQL; raises 22023 on any violation. Does NOT execute the query.
pub(crate) fn nl_to_sql(question: Option<&str>, allowed: &[Option<&str>], model: Option<&str>) -> String {
    let question = match question {
        Some(q) if !q.trim().is_empty() => q,
        _ => err_input("ai.nl_to_sql: question must not be empty"),
    };

    // Allowlist normalization: bare ('documents') or schema-qualified ('public.documents'), strip+lower.
    let mut allow: Vec<String> = Vec::new();
    for r in allowed {
        if let Some(s) = r {
            let t = s.trim().to_lowercase();
            if !t.is_empty() && !allow.contains(&t) {
                allow.push(t);
            }
        }
    }
    if allow.is_empty() {
        err_input("ai.nl_to_sql: allowed_relations must be a non-empty list");
    }

    // L1 — constrain the model (byte-identical to sql/60:44-49; the stub routes on this text).
    let mut sorted = allow.clone();
    sorted.sort();
    let system = format!(
        "You translate a question into exactly ONE read-only PostgreSQL SELECT query. \
         You may reference ONLY these relations: {}. \
         Output ONLY the SQL — no prose, no markdown, no trailing semicolon. \
         Use SELECT or WITH only. Never modify data.",
        sorted.join(", ")
    );
    // Same crate → call the chat domain directly (the plpython3u went through SPI `ai._chat`; the result and
    // the prompt are identical, so parity holds and we avoid a needless SPI hop).
    let raw = crate::chat::chat(Some(question), Some(&system), model);

    let sql = nl_fence_strip(raw.trim());

    // L2 — static validation on a comment-stripped, lowercased copy.
    let low = strip_sql_comments(&sql).to_lowercase();
    let low = low.trim().to_string();

    // L2(a) — single statement (no ';' except an optional trailing one).
    let trimmed = low.trim_end().trim_end_matches(';');
    if trimmed.contains(';') {
        err_input("ai.nl_to_sql: multiple statements are not allowed");
    }
    // L2(b) — SELECT/WITH only.
    if !starts_with_keyword(&low, "select") && !starts_with_keyword(&low, "with") {
        let head: String = sql.chars().take(60).collect();
        err_input(&format!(
            "ai.nl_to_sql: only SELECT/WITH queries are allowed (got: {head})"
        ));
    }
    // L2(c) — banned tokens (leftmost word-token equal to a banned keyword).
    if let Some(tok) = first_banned_token(&low) {
        err_input(&format!("ai.nl_to_sql: banned token '{tok}' in generated SQL"));
    }
    // procedural blocks: `do $$` (optional whitespace) or `call`.
    if has_do_block(&low) || has_word(&low, "call") {
        err_input("ai.nl_to_sql: procedural blocks are not allowed");
    }

    // L4 — parser-grade relation allowlist via EXPLAIN (FORMAT JSON). The validated `sql` is a single
    // SELECT/WITH with no ';' (L2(a)), so interpolating it into one EXPLAIN command cannot break out.
    let explain = format!("EXPLAIN (FORMAT JSON, VERBOSE false) {sql}");
    // A planner error (unknown relation / syntax) raises a Postgres ERROR (longjmp), not a Rust panic —
    // catch it with PgTryBuilder and fail closed (22023). We do not continue the aborted work; we raise our
    // own typed error immediately, so the outcome is a clean rejection regardless of txn state.
    let plan_text: Option<String> = pgrx::PgTryBuilder::new(|| Spi::get_one::<String>(&explain).ok().flatten())
        .catch_others(|_| None)
        .execute();
    let plan_text = match plan_text {
        Some(s) => s,
        None => err_input("ai.nl_to_sql: query did not plan (rejected)"),
    };
    let plan: Value = match serde_json::from_str(&plan_text) {
        Ok(v) => v,
        Err(_) => err_input("ai.nl_to_sql: query did not plan (rejected): bad plan JSON"),
    };
    let mut rels: Vec<(String, String)> = Vec::new();
    collect_relations(&plan, &mut rels);
    for (schema, name) in &rels {
        let qualified = format!("{schema}.{name}");
        let ok = allow.iter().any(|a| a == &qualified)
            || (schema == "public" && allow.iter().any(|a| a == name));
        if !ok {
            err_input(&format!(
                "ai.nl_to_sql: relation '{qualified}' is not in the allowlist"
            ));
        }
    }

    sql
}

/// Validate (via `nl_to_sql`) then EXECUTE in the read-only sandbox (L3). Returns jsonb rows. A write that
/// reaches execution raises SQLSTATE 25006; a runaway query is aborted by statement_timeout (57014).
pub(crate) fn nl_query(
    question: Option<&str>,
    allowed: &[Option<&str>],
    model: Option<&str>,
    max_rows: i32,
) -> pgrx::JsonB {
    if max_rows <= 0 {
        err_input(&format!("ai.nl_query: max_rows must be > 0 (got {max_rows})"));
    }
    // L1 + L2 + L4 (generate-time) — runs read-write (EXPLAIN plans only) BEFORE the read-only sandbox.
    let validated = nl_to_sql(question, allowed, model);

    // L3 — PostgreSQL-native read-only sandbox (SET LOCAL, transaction-scoped). A write → 25006. Matches the
    // plpython3u set_config(..., is_local=true) semantics; not restored (PG forbids RW-after-query, 25001) —
    // the normal single-statement `SELECT ai.nl_query(...)` ends the txn immediately, so no leak.
    Spi::run("SET LOCAL transaction_read_only = on").expect("set transaction_read_only");
    Spi::run("SET LOCAL statement_timeout = 5000").expect("set statement_timeout");

    // Outer LIMIT on the wrapper (not injected into the subquery) so a generated `… LIMIT n` does not produce
    // `LIMIT n LIMIT m`. `validated` is L4-validated; `max_rows` is a positive int (safe to interpolate).
    let wrapped = format!(
        "SELECT coalesce(jsonb_agg(row_to_json(t)), '[]'::jsonb) FROM ({validated}) t LIMIT {max_rows}"
    );
    Spi::get_one::<pgrx::JsonB>(&wrapped)
        .expect("nl_query execution")
        .unwrap_or_else(|| pgrx::JsonB(serde_json::json!([])))
}

/// Strip an optional leading ```` ```lang ```` fence + trailing ```` ``` ```` — the nl variant
/// (`^```[a-zA-Z]*\n?` / `\n?```$`, sql/60:57-58), DISTINCT from `chat::strip_fence`.
fn nl_fence_strip(s: &str) -> String {
    let mut t = s;
    if let Some(rest) = t.strip_prefix("```") {
        let rest = rest.trim_start_matches(|c: char| c.is_ascii_alphabetic());
        t = rest.strip_prefix('\n').unwrap_or(rest);
    }
    // trailing optional \n then ```
    let mut owned = t.to_string();
    if let Some(stripped) = owned.strip_suffix("```") {
        owned = stripped.strip_suffix('\n').unwrap_or(stripped).to_string();
    }
    owned.trim().to_string()
}

/// Remove `-- line` comments and `/* block */` comments (DOTALL), each → a space (sql/60:61-62).
fn strip_sql_comments(s: &str) -> String {
    // line comments: from "--" to end-of-line.
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let n = bytes.len();
    let mut i = 0;
    while i < n {
        if i + 1 < n && bytes[i] == b'-' && bytes[i + 1] == b'-' {
            out.push(' ');
            i += 2;
            while i < n && bytes[i] != b'\n' {
                i += 1;
            }
        } else if i + 1 < n && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            out.push(' ');
            i += 2;
            while i + 1 < n && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(n); // skip the closing */
        } else {
            // push the full UTF-8 char starting at i.
            let ch_len = utf8_len(bytes[i]);
            out.push_str(&s[i..(i + ch_len).min(n)]);
            i += ch_len;
        }
    }
    out
}

fn utf8_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b >> 5 == 0b110 {
        2
    } else if b >> 4 == 0b1110 {
        3
    } else {
        4
    }
}

/// `^\s*<kw>\b` on an already-lowercased string: leading whitespace, then `kw`, then a non-word char or end.
fn starts_with_keyword(low: &str, kw: &str) -> bool {
    let t = low.trim_start();
    if let Some(rest) = t.strip_prefix(kw) {
        rest.chars().next().map(|c| !is_word_char(c)).unwrap_or(true)
    } else {
        false
    }
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The leftmost word-token (maximal `[a-z0-9_]+` run) that equals a banned keyword — byte-equivalent to the
/// plpython3u `\b(<keywords>)\b` search on the lowercased copy.
fn first_banned_token(low: &str) -> Option<String> {
    let bytes = low.as_bytes();
    let n = bytes.len();
    let mut i = 0;
    while i < n {
        if is_word_char(bytes[i] as char) {
            let start = i;
            while i < n && is_word_char(bytes[i] as char) {
                i += 1;
            }
            let tok = &low[start..i];
            if BANNED.contains(&tok) {
                return Some(tok.to_string());
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Whole-word presence of `word` (e.g. `call`).
fn has_word(low: &str, word: &str) -> bool {
    low.split(|c: char| !is_word_char(c)).any(|t| t == word)
}

/// `\bdo\b\s*\$\$` — a `do` token followed (after optional whitespace) by `$$`.
fn has_do_block(low: &str) -> bool {
    let bytes = low.as_bytes();
    let n = bytes.len();
    let mut i = 0;
    while i + 1 < n {
        // find a "do" token
        let is_start = i == 0 || !is_word_char(bytes[i - 1] as char);
        if is_start
            && bytes[i] == b'd'
            && bytes[i + 1] == b'o'
            && (i + 2 == n || !is_word_char(bytes[i + 2] as char))
        {
            let mut j = i + 2;
            while j < n && (bytes[j] as char).is_whitespace() {
                j += 1;
            }
            if j + 1 < n && bytes[j] == b'$' && bytes[j + 1] == b'$' {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// Recursively collect every `(schema, relation)` from an EXPLAIN (FORMAT JSON) plan tree (sql/60:97-107).
fn collect_relations(node: &Value, acc: &mut Vec<(String, String)>) {
    match node {
        Value::Object(map) => {
            if let Some(Value::String(name)) = map.get("Relation Name") {
                let schema = map
                    .get("Schema")
                    .and_then(|s| s.as_str())
                    .unwrap_or("public")
                    .to_lowercase();
                let pair = (schema, name.to_lowercase());
                if !acc.contains(&pair) {
                    acc.push(pair);
                }
            }
            for v in map.values() {
                collect_relations(v, acc);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                collect_relations(v, acc);
            }
        }
        _ => {}
    }
}
