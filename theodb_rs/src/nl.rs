//! SPI-orchestration adapter (blueprint M19): safe natural-language → SQL with layered anti-prompt-injection
//! guards, ported from the plpython3u `ai.nl_to_sql`/`ai.nl_query` (sql/60) — the LAST plpython3u in the
//! surface. Unlike the portable `embed`/`chat` modules (all ABI access via `crate::pg`), this module talks to
//! Postgres directly through `Spi`/pgrx (the L4 EXPLAIN call) — that SPI use IS the accepted ADR-C boundary.
//!
//! Defense (does NOT trust the LLM):
//! - L1 — prompt constraint (hardening; the system prompt is byte-identical so the stub routes on it).
//! - L2 — static validation on a comment-stripped/lowercased copy: single statement, SELECT/WITH-only, a fixed banned-keyword denylist, no `DO $$`/`CALL`. Stdlib scanning (no regex crate, ADR-B); tokenizing on `[a-z0-9_]+` runs is byte-equivalent to the plpython3u `\b…\b` denylist.
//! - L4 — PARSER-GRADE relation allowlist via `EXPLAIN (FORMAT JSON)` (Postgres's planner enumerates every relation — comma-joins/quoted-idents/CTEs included). A Rust SQL parser would diverge from the planner and reopen that vulnerability class (ADR-A), so L4 delegates to Postgres via SPI.
//! - L3 — read-only sandbox execution in `nl_query` (transaction_read_only + statement_timeout → 25006).
//!
//! Generate-vs-execute split: `nl_to_sql` returns the validated SQL (does NOT execute); `nl_query` runs it.
//! Every rejection is SQLSTATE 22023 (verbatim messages); a write reaching execution is 25006.
use pgrx::prelude::*;
use serde_json::Value;

use crate::pg::err_input;

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
    for s in allowed.iter().flatten() {
        let t = s.trim().to_lowercase();
        if !t.is_empty() && !allow.contains(&t) {
            allow.push(t);
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

    // L2 — static validation (single-statement, SELECT/WITH-only, banned tokens, no procedural blocks).
    if let Err(e) = l2_validate(&sql) {
        err_input(&e);
    }
    // L4 — parser-grade relation allowlist via EXPLAIN (SPI planner boundary).
    l4_validate_relations(&sql, &allow);

    sql
}

/// L2 static validation on a comment-stripped, lowercased copy of the generated SQL. PURE (no SPI, no
/// divergence) so the security-boundary composition is unit-testable without the LLM/oracle (M25). Returns
/// `Err(message)` on the first violation; the caller maps it to the typed 22023 error. Byte-identical checks
/// + messages to the previous inline logic.
fn l2_validate(sql: &str) -> Result<(), String> {
    let low = strip_sql_comments(sql).to_lowercase();
    let low = low.trim().to_string();

    // L2(a) — single statement (no ';' except an optional trailing one).
    let trimmed = low.trim_end().trim_end_matches(';');
    if trimmed.contains(';') {
        return Err("ai.nl_to_sql: multiple statements are not allowed".to_string());
    }
    // L2(b) — SELECT/WITH only.
    if !starts_with_keyword(&low, "select") && !starts_with_keyword(&low, "with") {
        let head: String = sql.chars().take(60).collect();
        return Err(format!(
            "ai.nl_to_sql: only SELECT/WITH queries are allowed (got: {head})"
        ));
    }
    // L2(c) — banned tokens (leftmost word-token equal to a banned keyword).
    if let Some(tok) = first_banned_token(&low) {
        return Err(format!("ai.nl_to_sql: banned token '{tok}' in generated SQL"));
    }
    // procedural blocks: `do $$` (optional whitespace) or `call`.
    if has_do_block(&low) || has_word(&low, "call") {
        return Err("ai.nl_to_sql: procedural blocks are not allowed".to_string());
    }
    Ok(())
}

/// PURE relation-allowlist check: every planned relation must be in `allow` (schema-qualified, or bare under
/// `public`). Unit-testable without SPI (M25). Returns `Err(message)` on the first disallowed relation.
fn relation_allowed(rels: &[(String, String)], allow: &[String]) -> Result<(), String> {
    for (schema, name) in rels {
        let qualified = format!("{schema}.{name}");
        let ok = allow.iter().any(|a| a == &qualified)
            || (schema == "public" && allow.iter().any(|a| a == name));
        if !ok {
            return Err(format!(
                "ai.nl_to_sql: relation '{qualified}' is not in the allowlist"
            ));
        }
    }
    Ok(())
}

/// L4 — parser-grade relation allowlist via EXPLAIN (FORMAT JSON). The `sql` passed L2 (a single SELECT/WITH
/// with no ';'), so interpolating it into one EXPLAIN command cannot break out. Runs EXPLAIN via SPI (it PLANS,
/// does not execute); an un-plannable query longjmps out under SPI, trapped by PgTryBuilder and re-raised as the
/// contracted 22023 "did not plan (rejected)" (parity with the plpython3u `try/except`). Fail-closed either way.
fn l4_validate_relations(sql: &str, allow: &[String]) {
    let explain = format!("EXPLAIN (FORMAT JSON, VERBOSE false) {sql}");
    let plan_opt: Option<Value> = PgTryBuilder::new(|| {
        Spi::get_one::<pgrx::Json>(&explain).ok().flatten().map(|j| j.0)
    })
    .catch_others(|_| None)
    .execute();
    let plan: Value = match plan_opt {
        Some(j) => j,
        None => err_input("ai.nl_to_sql: query did not plan (rejected)"),
    };
    let mut rels: Vec<(String, String)> = Vec::new();
    collect_relations(&plan, &mut rels);
    if let Err(e) = relation_allowed(&rels, allow) {
        err_input(&e);
    }
}

// NOTE (M19 ADR-F): `ai.nl_query` (L3 read-only sandbox execution) stays a thin plpgsql keeper in sql/60,
// NOT a Rust function. L3 is transaction-control (`SET LOCAL transaction_read_only`) + dynamic `EXECUTE` —
// inherently SQL/plpgsql operations (the M18 precedent: the chunked import PROCEDURE stayed plpgsql, ADR-D).
// Critically, it MUST call `ai.nl_to_sql` at the SQL level (`validated := ai.nl_to_sql(...)`) so nl_to_sql's
// L4 `EXPLAIN`-over-SPI runs in a clean execution context; calling the Rust `nl_to_sql` nested from a Rust
// `nl_query` frame makes the nested EXPLAIN-SPI fail. The anti-injection core (L1/L2/L4) is 100% Rust here.

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

// Rust unit tests for the pure L2/L4 helpers (plan T1.1: "a Rust unit test per L2 rule"). These assert the
// byte-faithful parity with the plpython3u `\b…\b` regex semantics WITHOUT a DB — a refactor that weakens the
// anti-injection scan fails here at `cargo pgrx test`, not only in the slow Python image oracle. The
// end-to-end cross-language parity stays proven by benchmarks/tests/test_nl_sql.py (35).
#[cfg(any(test, feature = "pg_test"))]
#[pg_schema]
mod tests {
    use super::*;

    #[pg_test]
    fn first_banned_token_matches_whole_tokens_only() {
        // whole-token banned keywords are caught (leftmost wins)…
        assert_eq!(first_banned_token("select drop from t").as_deref(), Some("drop"));
        assert_eq!(first_banned_token("insert into x").as_deref(), Some("insert"));
        // …the pg_ls_ bare-prefix sibling is caught…
        assert_eq!(first_banned_token("select pg_ls_dir('.')").as_deref(), Some("pg_ls_dir"));
        // …a banned word embedded in a larger identifier is NOT a separate token (\b semantics)…
        assert_eq!(first_banned_token("select dropped_at from t"), None);
        assert_eq!(first_banned_token("select created from t"), None); // 'created' != 'create'
        // …and a benign read query has no banned token.
        assert_eq!(first_banned_token("select count(*) from documents"), None);
    }

    #[pg_test]
    fn has_do_block_and_has_word_detect_procedural() {
        assert!(has_do_block("do $$ begin end $$")); // do then $$
        assert!(has_do_block("select 1; do$$")); // no space variant
        assert!(!has_do_block("select doc from t")); // 'doc' is not a 'do' token
        assert!(!has_do_block("select 1")); // no do-block
        assert!(has_word("call foo()", "call"));
        assert!(!has_word("select recall from t", "call")); // 'recall' is not 'call'
    }

    #[pg_test]
    fn starts_with_keyword_is_word_bounded() {
        assert!(starts_with_keyword("  select 1", "select"));
        assert!(starts_with_keyword("with x as (select 1) select * from x", "with"));
        assert!(!starts_with_keyword("selection from t", "select")); // 'selection' is not 'select'
        assert!(!starts_with_keyword("update t set x=1", "select"));
    }

    #[pg_test]
    fn strip_sql_comments_removes_line_and_block() {
        // a banned token hidden behind a line comment is removed from the L2 copy (so it cannot smuggle)…
        assert!(!strip_sql_comments("select 1 -- drop table t\n from t").contains("drop"));
        // …and a DOTALL block comment too.
        assert!(!strip_sql_comments("select /* drop */ 1").contains("drop"));
        assert!(strip_sql_comments("select 1 from t").contains("from t")); // non-comment text preserved
    }

    #[pg_test]
    fn nl_fence_strip_unwraps_markdown_fence() {
        assert_eq!(nl_fence_strip("```sql\nSELECT 1```"), "SELECT 1");
        assert_eq!(nl_fence_strip("```\nSELECT 1\n```"), "SELECT 1");
        assert_eq!(nl_fence_strip("SELECT 1"), "SELECT 1"); // no fence → unchanged (trimmed)
    }

    #[pg_test]
    fn collect_relations_walks_the_plan_tree() {
        let plan: Value = serde_json::json!([
            {"Plan": {"Node Type": "Seq Scan", "Schema": "public", "Relation Name": "documents",
                      "Plans": [{"Node Type": "Seq Scan", "Schema": "secret", "Relation Name": "pg_authid"}]}}
        ]);
        let mut rels: Vec<(String, String)> = Vec::new();
        collect_relations(&plan, &mut rels);
        assert!(rels.contains(&("public".to_string(), "documents".to_string())));
        assert!(rels.contains(&("secret".to_string(), "pg_authid".to_string()))); // nested relation found
    }

    // M25 — the L2 security-boundary COMPOSITION, unit-tested without the LLM/oracle (previously untested).
    // Negative cases assert the SPECIFIC typed message (testing.md § 4.1) — asserting only .is_err() would
    // false-pass if the check under test were deleted but a *different* guard still tripped (e.g. the banned
    // 'drop' token catching a dropped multistatement check).
    #[pg_test]
    fn l2_validate_rejects_multistatement() {
        // No banned token here, so ONLY the multistatement guard can reject — isolates the check under test.
        let e = l2_validate("SELECT 1; SELECT 2").unwrap_err();
        assert!(e.contains("multiple statements are not allowed"), "got: {e}");
    }

    #[pg_test]
    fn l2_validate_rejects_non_select() {
        // DELETE is not a banned token nor a multistatement — ONLY the SELECT/WITH-only guard can reject.
        let e = l2_validate("DELETE FROM documents").unwrap_err();
        assert!(e.contains("only SELECT/WITH queries are allowed"), "got: {e}");
        // UPDATE/INSERT likewise fail on the SELECT/WITH-only guard (they are not in BANNED).
        assert!(l2_validate("UPDATE t SET x = 1").unwrap_err().contains("only SELECT/WITH"));
        assert!(l2_validate("INSERT INTO t VALUES (1)").unwrap_err().contains("only SELECT/WITH"));
    }

    #[pg_test]
    fn l2_validate_rejects_banned_token_and_procedural_block() {
        // The banned-token wiring: a SELECT that reaches a file-read builtin trips the banned scan (not the
        // SELECT/WITH guard) — proves l2_validate still calls first_banned_token.
        let e = l2_validate("SELECT pg_read_file('/etc/passwd')").unwrap_err();
        assert!(e.contains("banned token"), "got: {e}");
        // The procedural-block wiring: a DO $$ block smuggled after a SELECT (no ';', so it clears the
        // multi-statement guard L2a; "do" is not banned, so it clears L2c) reaches has_do_block (L2d) — proves
        // that call survived extraction. (A bare "do $$…" would be caught earlier by the SELECT/WITH-only guard,
        // and "…; do $$…" by the multi-statement guard; neither would exercise has_do_block itself.)
        let e2 = l2_validate("select 1 do $$ perform 1 $$").unwrap_err();
        assert!(e2.contains("procedural blocks are not allowed"), "got: {e2}");
    }

    #[pg_test]
    fn l2_validate_accepts_select_and_with() {
        assert!(l2_validate("SELECT id FROM documents WHERE x > 1").is_ok());
        assert!(l2_validate("WITH t AS (SELECT 1) SELECT * FROM t").is_ok());
    }

    #[pg_test]
    fn l2_validate_accepts_single_trailing_semicolon() {
        // Boundary: exactly one trailing ';' is valid (an interior ';' is not — covered by the reject test).
        assert!(l2_validate("SELECT 1;").is_ok());
    }

    // M25 — the relation-allowlist logic, unit-tested without SPI/EXPLAIN.
    #[pg_test]
    fn relation_allowed_enforces_allowlist() {
        let allow = vec!["public.documents".to_string()];
        assert!(relation_allowed(&[("public".into(), "documents".into())], &allow).is_ok());
        // bare name under public matches an allowlisted bare entry
        assert!(relation_allowed(&[("public".into(), "documents".into())], &["documents".to_string()]).is_ok());
        // a relation outside the allowlist is rejected (e.g. a system catalog the model tried to reach)
        assert!(relation_allowed(&[("secret".into(), "pg_authid".into())], &allow).is_err());
    }

    #[pg_test]
    fn relation_allowed_bare_entry_does_not_authorize_other_schema() {
        // Security branch (the `schema == "public"` guard): a BARE allowlist entry `documents` must NOT
        // authorize a same-named table planted in another schema (e.g. `secret.documents`). An attacker who
        // creates `secret.documents` must still be rejected — the bare match is scoped to `public` only.
        let bare = vec!["documents".to_string()];
        assert!(relation_allowed(&[("secret".into(), "documents".into())], &bare).is_err());
        // …and the qualified form `secret.documents` is not implied by the bare `public` entry either.
        assert!(relation_allowed(&[("secret".into(), "documents".into())], &["public.documents".to_string()]).is_err());
    }
}
